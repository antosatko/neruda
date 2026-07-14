use std::ops::Deref;

use arena::Arena;
use arena_scope::stack::Stack;

use crate::{
    ast::{self, Body, Expression, Function, Postfix, Span, SpanIndex, Type},
    const_stage::{
        Context, Diagnostic, Error, Errors, Warning, Warnings,
        lowering::{ConstEvalResult, apply_generic_arguments},
        objects::{AnyObject, AnyObjectKey, FunctionObj, FunctionObjKey, InitState, IrCache},
        types::{AnyTypeKey, FunctionType, ModuleKey, RefType},
    },
    ir::{
        Addr, BasicBlock, BlockCtx, FunctionIr, FunctionIrKey, Instruction, Terminator, Value,
        ValueKey, Variable,
    },
};

impl Context {
    pub fn lower_ir(&mut self) -> Result<(), Error> {
        let fun_keys: Vec<FunctionObjKey> = self.objects.functions.iter_keys().collect();
        for key in fun_keys {
            if let IrCache::Single(_) = &self.objects.functions.get_unchecked(&key).data.ir {
                self.lower_function(&key, &None)?;
            }
        }

        Ok(())
    }

    #[track_caller]
    pub fn lower_function(
        &mut self,
        key: &FunctionObjKey,
        generic_arguments: &Option<Span<Vec<Span<Type>>>>,
    ) -> Result<FunctionIrKey, Error> {
        let fun = self.objects.functions.get_unchecked(key);
        let mod_key = fun.module;
        let backup_ir = FunctionIr::new_polymorphic(self, *key, generic_arguments)?;
        let ty = backup_ir.type_of.unwrap();
        dbg!("creating probably redundant ir");
        let fun = self.objects.functions.get_mut_unchecked(key);
        let ir_key = match &mut fun.data.ir {
            IrCache::Single(InitState::Progress(p)) => *p,
            IrCache::Single(InitState::Done(p)) => return Ok(*p),
            IrCache::Polymorphic(cache) => match generic_arguments {
                Some(_) => match cache.get(&ty) {
                    Some(InitState::Done(ir)) => return Ok(*ir),
                    _ => {
                        let key = self.ir_cache.push(backup_ir);
                        cache.insert(ty, InitState::Done(key));
                        key
                    }
                },
                None => unreachable!("generic inference is not my concern"),
            },
            _ => unreachable!("yup its just like that"),
        };
        let fun = self.objects.functions.get_unchecked(key);
        self.generic_ctx.restore(*fun.data.generic_scope.get_done());
        let module = self.types.modules.get_unchecked(&mod_key);

        let mut block_ctx = BlockCtx {
            variables: Default::default(),
            source: *key,
            control_stack: Vec::new(),
        };
        let ir = self.ir_cache.get_mut_unchecked(&ir_key);
        match ir.substitutions {
            Some(s) => self.types.substitutions.dirty.restore(s),
            None => (),
        }

        match self
            .ast
            .get(&module.path)
            .unwrap()
            .objects
            .get_unchecked(&fun.ast_object)
            .clone()
            .deref()
        {
            ast::Object::Function(Function { body, .. }) => {
                self.lower_block(&ir_key, &mut block_ctx, body, &mod_key)?
            }
            _ => unreachable!(),
        }
        let ir = self.ir_cache.get_mut_unchecked(&ir_key);
        ir.variables.shrink();
        ir.values.shrink();

        let fun = self.objects.functions.get_unchecked(key);
        if let IrCache::Single(_) = &fun.data.ir {
            for (var_key, var) in ir.variables.iter_pairs() {
                if !var.used && var.identifier.deref() != "_" {
                    self.diagnostics.warnings.push(Diagnostic {
                        inner: Warnings::VariableUnused {
                            ir: ir_key,
                            var: var_key,
                        },
                        module: mod_key,
                        span: var.identifier.location,
                    });
                }
            }
        }

        let fun = self.objects.functions.get_mut_unchecked(key);
        match &mut fun.data.ir {
            IrCache::Single(p) => p.mark_done(),
            IrCache::Polymorphic(cache) => {
                cache
                    .get_mut(&ty)
                    .map(|p| p.mark_done())
                    .expect("just checking");
            }
        };
        Ok(ir_key)
    }

    fn lower_block(
        &mut self,
        ir: &FunctionIrKey,
        block_ctx: &mut BlockCtx,
        block: &Span<Body>,
        module: &ModuleKey,
    ) -> Result<(), Error> {
        match block.deref() {
            Body::Block(block) => {
                block_ctx.variables.push();
                let mut it = block.iter();
                while let Some(st) = it.next() {
                    match st.deref() {
                        ast::Statement::Var {
                            ident,
                            ty,
                            expression,
                        } => {
                            let (value, ty) = match (ty, expression) {
                                (Some(ty), Some(expr)) => {
                                    let ty = ty.lower(self, *module)?;
                                    let val =
                                        self.lower_expression(ir, block_ctx, expr, &Some(ty))?;
                                    (val, ty)
                                }
                                (None, Some(expr)) => {
                                    let (ty, addr) =
                                        match self.lower_expression(ir, block_ctx, expr, &None) {
                                            Ok(addr) => (
                                                addr.type_of(self, ir).map_err(|inner| Error {
                                                    inner,
                                                    module: *module,
                                                    span: expr.location,
                                                })?,
                                                addr,
                                            ),
                                            Err(err) => Err(err)?,
                                        };
                                    (addr, ty)
                                }
                                (Some(ty), None) => {
                                    let ty_low = ty.lower(self, *module)?;
                                    let default =
                                        &ty_low.const_default(self).map_err(|e| Error {
                                            inner: e,
                                            module: *module,
                                            span: ty.location,
                                        })?;
                                    (
                                        Addr::Value(load_const(
                                            self.ir_cache.get_mut_unchecked(ir),
                                            &None,
                                            *module,
                                            default,
                                            ty.location,
                                        )?),
                                        ty_low,
                                    )
                                }
                                (None, None) => Err(Error {
                                    inner: Errors::FailedTypeInfer,
                                    module: *module,
                                    span: st.location,
                                })?,
                            };
                            let src = self.load_addr(
                                ir,
                                block_ctx,
                                value,
                                &Some(ty),
                                match expression {
                                    Some(e) => e.location,
                                    None => ident.location,
                                },
                            )?;
                            let var = Variable {
                                identifier: ident.clone(),
                                value: src,
                                ty,
                                used: false,
                                mutated: false,
                            };
                            let ir = self.ir_cache.get_mut_unchecked(ir);
                            let dst = ir.variables.push(var);
                            ir.blocks
                                .get_mut_unchecked()
                                .instructions
                                .push(st.map(|_| Instruction::StoreVar { dst, src }));
                            block_ctx.variables.insert(ident.deref().clone(), dst);
                        }
                        ast::Statement::Return { expression } => {
                            let returns = self.ir_cache.get_mut_unchecked(ir).returns.unwrap();
                            self.dead_code(module, &mut it);
                            let val = match expression {
                                Some(expr) => {
                                    let addr =
                                        self.lower_expression(ir, block_ctx, expr, &Some(returns))?;
                                    Some(self.load_addr(
                                        ir,
                                        block_ctx,
                                        addr,
                                        &Some(returns),
                                        expr.location,
                                    )?)
                                }
                                None => match returns {
                                    AnyTypeKey::Void => None,
                                    _ => Err(Error {
                                        inner: Errors::ExpectedReturnExpression(block_ctx.source),
                                        module: *module,
                                        span: st.location,
                                    })?,
                                },
                            };
                            let ir = self.ir_cache.get_mut_unchecked(ir);
                            ir.blocks.get_mut_unchecked().terminator =
                                Some(Terminator::Return(val));
                            return Ok(());
                        }
                        ast::Statement::Invoke { .. } => (),
                        ast::Statement::Expr { expression } => {
                            self.lower_expression(ir, block_ctx, expression, &None)?;
                        }
                        ast::Statement::Loop { label, body } => {
                            todo!("musim nejak ziskat klice bloku");
                        }
                        _ => todo!("{:?}", st),
                    }
                }
                block_ctx.variables.pop();
            }
            Body::Statement(expression) => {
                let returns = self.ir_cache.get_mut_unchecked(ir).returns.unwrap();
                let addr = self.lower_expression(ir, block_ctx, expression, &Some(returns))?;
                let val =
                    self.load_addr(ir, block_ctx, addr, &Some(returns), expression.location)?;
                let ir = self.ir_cache.get_mut_unchecked(ir);
                ir.blocks.get_mut_unchecked().terminator = Some(Terminator::Eval(val));
            }
        }
        Ok(())
    }

    fn dead_code(
        &mut self,
        module: &arena::Key<crate::const_stage::types::ModuleTag>,
        it: &mut std::slice::Iter<'_, Span<ast::Statement>>,
    ) {
        match it.next() {
            Some(next) => {
                let span = match it.last() {
                    Some(last) => {
                        let mut loc = next.location;
                        loc.len = last.location.len + last.location.index - next.location.index;
                        loc
                    }
                    None => next.location,
                };
                self.diagnostics.warnings.push(Warning {
                    inner: Warnings::DeadCode,
                    module: *module,
                    span,
                });
            }
            None => (),
        }
    }

    fn lower_expression(
        &mut self,
        ir: &FunctionIrKey,
        block_ctx: &mut BlockCtx,
        expr: &Span<Expression>,
        expect: &Option<AnyTypeKey>,
    ) -> Result<Addr, Error> {
        let module = self
            .objects
            .functions
            .get_unchecked(&block_ctx.source)
            .module;
        match expr.const_eval(self, module, &None, expect) {
            ConstEvalResult::Value(const_val) => {
                return Ok(Addr::Value(load_const(
                    self.ir_cache.get_mut_unchecked(ir),
                    expect,
                    module,
                    &const_val,
                    expr.location,
                )?));
            }
            ConstEvalResult::Error(err) => Err(err)?,
            ConstEvalResult::NotConst(_) => (),
        }
        match expr.deref() {
            Expression::Binary { l, r, op } => {
                let left_addr = self.lower_expression(ir, block_ctx, l, &None)?;
                let right_addr = self.lower_expression(ir, block_ctx, r, &None)?;
                let left_ty = left_addr.type_of(self, ir).map_err(|inner| Error {
                    inner,
                    module,
                    span: l.location,
                })?;
                let right_ty = right_addr.type_of(self, ir).map_err(|inner| Error {
                    inner,
                    module,
                    span: r.location,
                })?;
                let left_value = self.load_addr(ir, block_ctx, left_addr, &None, l.location)?;
                let right_value = self.load_addr(ir, block_ctx, right_addr, &None, r.location)?;
                match (left_ty, right_ty) {
                    (AnyTypeKey::Primitive(l_prim), AnyTypeKey::Primitive(r_prim))
                        if l_prim == r_prim =>
                    {
                        let ir = self.ir_cache.get_mut_unchecked(ir);
                        let dst = ir.values.push(Value { ty: left_ty });
                        ir.blocks.get_mut_unchecked().instructions.push(op.map(|_| {
                            Instruction::BinOp {
                                op: *op.deref(),
                                l: left_value,
                                r: right_value,
                                dst,
                            }
                        }));
                        Ok(Addr::Value(dst))
                    }
                    (AnyTypeKey::Primitive(_), AnyTypeKey::Primitive(_)) => Err(Error {
                        inner: Errors::TypeMismatch {
                            expected: left_ty,
                            got: right_ty,
                        },
                        module,
                        span: r.location,
                    }),
                    _ => Err(Error {
                        inner: Errors::Undefined("Binary operation on non primitive types"),
                        module,
                        span: l.location + r.location,
                    }),
                }
            }
            Expression::Value(val) => {
                let mut addr = match val.literal.deref() {
                    ast::Literal::Identifier(ident) => {
                        let path = &ident.path.path;
                        let generics = &ident.generics;
                        match self.resolve_const_path(&path, module, ident.path.location, generics)
                        {
                            Ok(any) => match (any, generics) {
                                (AnyObjectKey::Function(fun_key), Some(_)) => {
                                    let fun = self.objects.functions.get_unchecked(&fun_key);

                                    let (concrete_ty, subs) = apply_generic_arguments(
                                        self,
                                        module,
                                        ident.path.location,
                                        generics,
                                        *fun.data.generic_scope.get_done(),
                                        AnyTypeKey::Function(*fun.data.type_of.get_done()),
                                    )?;

                                    let fun = self.objects.functions.get_unchecked(&fun_key);
                                    match &fun.data.ir {
                                        IrCache::Single(_) => unreachable!(
                                            "Function is not polymorphic, this is already caught in the previous step"
                                        ),
                                        IrCache::Polymorphic(cache) => {
                                            match cache.get(&concrete_ty) {
                                                Some(ir) => Addr::Function(*ir.get_done()),
                                                _ => Addr::Function(
                                                    self.lower_function(&fun_key, generics)?,
                                                ),
                                            }
                                        }
                                    }
                                }
                                (AnyObjectKey::Function(fun_key), None) => {
                                    let fun = self.objects.functions.get_unchecked(&fun_key);
                                    match fun.data.ir {
                                        IrCache::Single(_) => {
                                            let ir_key = self.lower_function(&fun_key, &None)?;
                                            Addr::Function(ir_key)
                                        }
                                        IrCache::Polymorphic(_) => {
                                            Addr::UnresolvedFunction(fun_key)
                                        }
                                    }
                                }
                                (any, None) => Addr::Object(any),
                                _ => todo!("do smoething idk"),
                            },
                            Err(e) => {
                                if path.len() == 1 {
                                    let ident = path.first().unwrap();
                                    let ir = self.ir_cache.get_mut_unchecked(ir);
                                    match block_ctx.variables.get(ident) {
                                        _ if ident.deref() == "_" => Addr::Value(ir.void),
                                        Some(v) => Addr::Var(*v),
                                        None => match ir
                                            .parameters
                                            .iter()
                                            .find(|(i, _)| i == ident.deref())
                                        {
                                            Some((_, param)) => Addr::Var(*param),
                                            None => Err(Error {
                                                inner: Errors::VariableNotFound(
                                                    ident.deref().clone(),
                                                ),
                                                module,
                                                span: ident.location,
                                            })?,
                                        },
                                    }
                                } else {
                                    return Err(e);
                                }
                            }
                        }
                    }
                    a => todo!("{a:?}"),
                };
                for op in &val.postfix {
                    addr = match op.deref() {
                        Postfix::Ref => match addr {
                            Addr::Var(var) => {
                                let ir = self.ir_cache.get_mut_unchecked(ir);
                                let var_obj = ir.variables.get_unchecked(&var);
                                let ty = self
                                    .types
                                    .references
                                    .push_unique(RefType { inner: var_obj.ty });
                                let dst = ir.values.push(Value {
                                    ty: AnyTypeKey::Reference(ty),
                                });
                                ir.blocks
                                    .get_mut_unchecked()
                                    .instructions
                                    .push(op.map(|_| Instruction::AddressOfVar { var, dst }));
                                Addr::Value(dst)
                            }
                            Addr::Value(val) => {
                                let ir = self.ir_cache.get_mut_unchecked(ir);
                                let val_obj = ir.values.get_unchecked(&val);
                                let ty = self
                                    .types
                                    .references
                                    .push_unique(RefType { inner: val_obj.ty });
                                let dst = ir.values.push(Value {
                                    ty: AnyTypeKey::Reference(ty),
                                });
                                ir.blocks
                                    .get_mut_unchecked()
                                    .instructions
                                    .push(op.map(|_| Instruction::AddressOfVal { val, dst }));
                                Addr::Value(dst)
                            }
                            _ => Err(Error {
                                inner: Errors::Todo(
                                    "Referencing of anything that is not a variable nor value",
                                ),
                                module,
                                span: op.location,
                            })?,
                        },
                        Postfix::Deref => {
                            let src = self.load_addr(ir, block_ctx, addr, &None, op.location)?;

                            let ir = self.ir_cache.get_mut_unchecked(ir);
                            let val_obj = ir.values.get_unchecked(&src);
                            match val_obj.ty.unwrap_full(&self.types) {
                                AnyTypeKey::Reference(key) => {
                                    let ty = self.types.references.get_unchecked(&key).inner;
                                    Addr::MemoryRef { inner_ty: ty, src }
                                }
                                ty => Err(Error {
                                    inner: Errors::CouldNotDeref(ty),
                                    module,
                                    span: op.location,
                                })?,
                            }
                        }
                        Postfix::Call(arguments) => match addr {
                            Addr::Function(ir_key) => {
                                let callee = self.ir_cache.get_unchecked(&ir_key);
                                let result = callee.returns.unwrap_or(AnyTypeKey::Void);
                                let mut arg_values = Vec::with_capacity(arguments.capacity());
                                for (expr, (ident, expect)) in
                                    arguments.iter().zip(callee.parameters.clone())
                                {
                                    let expect = self
                                        .ir_cache
                                        .get_unchecked(&ir_key)
                                        .variables
                                        .get_unchecked(&expect)
                                        .ty;
                                    let addr =
                                        self.lower_expression(ir, block_ctx, expr, &Some(expect))?;
                                    let val = self.load_addr(
                                        ir,
                                        block_ctx,
                                        addr,
                                        &Some(expect),
                                        expr.location,
                                    )?;
                                    arg_values.push(val);
                                }
                                let self_ir = self.ir_cache.get_mut_unchecked(ir);
                                let dst = self_ir.values.push(Value { ty: result });
                                self_ir.blocks.get_mut_unchecked().instructions.push(op.map(
                                    |_| Instruction::Call {
                                        fun: ir_key,
                                        arguments: arg_values,
                                        result: dst,
                                    },
                                ));
                                Addr::Value(dst)
                            }
                            Addr::UnresolvedFunction(fun_key) => {
                                let obj = self.objects.functions.get_unchecked(&fun_key);
                                let generics = obj.data.generics.clone();
                                let signature = *obj.data.type_of.get_done();
                                let params = self
                                    .types
                                    .functions
                                    .get_unchecked(&signature)
                                    .parameters
                                    .clone();
                                let returns =
                                    self.types.functions.get_unchecked(&signature).returns;
                                let mut arg_values = Vec::with_capacity(arguments.capacity());
                                for (expr, expect) in arguments.iter().zip(params) {
                                    let addr =
                                        self.lower_expression(ir, block_ctx, expr, &Some(expect))?;
                                    let val = self.load_addr(
                                        ir,
                                        block_ctx,
                                        addr,
                                        &Some(expect),
                                        expr.location,
                                    )?;
                                    arg_values.push(val);
                                }

                                // let type_of = FunctionType {
                                //     parameters: arg_values
                                //         .iter()
                                //         .map(|v| {
                                //             self.ir_cache
                                //                 .get_unchecked(ir)
                                //                 .values
                                //                 .get_unchecked(v)
                                //                 .ty
                                //         })
                                //         .collect(),
                                //     returns,
                                // };
                                // let ty =
                                //     AnyTypeKey::Function(self.types.functions.push_unique(type_of));

                                self.lower_function(&fun_key, &None)?;

                                todo!()
                            }
                            _ => {
                                let val =
                                    self.load_addr(ir, block_ctx, addr, &None, op.location)?;
                                todo!()
                            }
                        },
                        _ => Err(Error {
                            inner: Errors::Todo("Postfix notation is not fully implemented"),
                            module,
                            span: op.location,
                        })?,
                    };
                }
                if let Some(expect) = expect {
                    let ty = addr.type_of(self, ir).map_err(|inner| Error {
                        inner,
                        module,
                        span: val.location,
                    })?;
                    let ir = self.ir_cache.get_mut_unchecked(ir);
                    ty.check(&mut self.types, expect).map_err(|inner| Error {
                        inner,
                        module,
                        span: val.location,
                    })?;
                }
                Ok(addr)
            }
        }
    }

    pub fn load_addr(
        &mut self,
        ir: &FunctionIrKey,
        block_ctx: &mut BlockCtx,
        addr: Addr,
        expect: &Option<AnyTypeKey>,
        span: SpanIndex,
    ) -> Result<ValueKey, Error> {
        let module = self
            .objects
            .functions
            .get_unchecked(&block_ctx.source)
            .module;
        match addr {
            Addr::Var(key) => {
                let ir = self.ir_cache.get_mut_unchecked(ir);
                let ty = ir.variables.get_unchecked(&key).ty;
                if let Some(expect) = expect {
                    ty.check(&mut self.types, expect).map_err(|inner| Error {
                        inner,
                        module,
                        span,
                    })?;
                }
                let dst = ir.values.push(Value { ty });
                let instr = ir.blocks.get_mut_unchecked();
                instr
                    .instructions
                    .push(Span::new(Instruction::LoadVar { src: key, dst }, span));
                let var = ir.variables.get_mut_unchecked(&key);
                var.used = true;
                Ok(dst)
            }
            Addr::Object(obj) => {
                let ty = obj.type_of(self).map_err(|inner| Error {
                    inner,
                    module,
                    span,
                })?;
                match expect {
                    Some(expect) => {
                        let ir = self.ir_cache.get_mut_unchecked(ir);
                        ty.check(&mut self.types, expect).map_err(|inner| Error {
                            inner,
                            module,
                            span,
                        })?;
                    }
                    None => (),
                }
                match obj {
                    AnyObjectKey::Function(_) => {
                        let ir = self.ir_cache.get_mut_unchecked(ir);
                        let dst = ir.values.push(Value { ty });
                        ir.blocks
                            .get_mut_unchecked()
                            .instructions
                            .push(Span::new(Instruction::AddressOfObj { obj, dst }, span));
                        Ok(dst)
                    }
                    AnyObjectKey::Const(key) => {
                        let value = self
                            .objects
                            .constants
                            .get_unchecked(&key)
                            .data
                            .value
                            .get_done();
                        let ir = self.ir_cache.get_mut_unchecked(ir);
                        load_const(ir, expect, module, value, span)
                    }
                    _ => Err(Error {
                        inner: Errors::Undefined("Referencing arbitrary objects"),
                        module,
                        span,
                    }),
                }
            }
            Addr::Value(val) => {
                let ir = self.ir_cache.get_mut_unchecked(ir);
                let ty = ir.values.get_unchecked(&val).ty;
                if let Some(expect) = expect {
                    ty.check(&mut self.types, expect).map_err(|inner| Error {
                        inner,
                        module,
                        span,
                    })?;
                }
                Ok(val)
            }
            Addr::Function(fun) => {
                let function = self.ir_cache.get_unchecked(&fun);
                let ty = function.type_of.unwrap();
                let ir = self.ir_cache.get_mut_unchecked(ir);
                let dst = ir.values.push(Value { ty });
                ir.blocks
                    .get_mut_unchecked()
                    .instructions
                    .push(Span::new(Instruction::AddressOfFun { fun, dst }, span));
                Ok(dst)
            }
            Addr::UnresolvedFunction(_fun_key) => match expect {
                Some(_ty) => Err(Error {
                    inner: Errors::Todo("Type reference inference"),
                    module,
                    span,
                }),
                None => Err(Error {
                    inner: Errors::UnresolvedFunctionReference,
                    module,
                    span,
                }),
            },
            Addr::MemoryRef { src, inner_ty } => {
                let ir = self.ir_cache.get_mut_unchecked(ir);
                let ty = ir.values.get_unchecked(&src).ty;
                let dst = ir.values.push(Value { ty });
                ir.blocks
                    .get_mut_unchecked()
                    .instructions
                    .push(Span::new(Instruction::Deref { src, dst }, span));
                Ok(dst)
            }
        }
    }
}

impl FunctionIr {
    pub fn new(fun: &AnyObject<FunctionObj>) -> FunctionIr {
        let mut instructions: Stack<BasicBlock> = Default::default();
        let blocks_entry = instructions.push(BasicBlock::default());
        let mut values = Arena::default();
        let void = values.push(Value {
            ty: AnyTypeKey::Void,
        });
        let mut variables = Arena::new();
        let mut parameters = Vec::new();
        for (ident, ty) in &fun.data.params {
            let ty = *ty.get_done();
            let value = values.push(Value { ty });
            let variable = Variable {
                identifier: ident.clone(),
                ty,
                value,
                mutated: false,
                used: false,
            };
            let variable = variables.push(variable);
            parameters.push((ident.deref().clone(), variable));
        }

        FunctionIr {
            source: None,
            type_of: None,
            returns: None,
            blocks: instructions,
            values,
            blocks_entry,
            variables,
            void,
            parameters,
            substitutions: None,
        }
    }

    pub fn new_polymorphic(
        ctx: &mut Context,
        fun_key: FunctionObjKey,
        generic_arguments: &Option<Span<Vec<Span<Type>>>>,
    ) -> Result<Self, Error> {
        let mut instructions: Stack<BasicBlock> = Default::default();
        let blocks_entry = instructions.push(BasicBlock::default());
        let mut values = Arena::default();
        let void = values.push(Value {
            ty: AnyTypeKey::Void,
        });
        let mut variables = Arena::new();
        let mut parameters = Vec::new();
        let fun = ctx.objects.functions.get_unchecked(&fun_key);
        let module = fun.module;
        let generics = *fun.data.generic_scope.get_done();
        let (type_of, substitutions) = apply_generic_arguments(
            ctx,
            module,
            SpanIndex::default(),
            generic_arguments,
            generics,
            AnyTypeKey::Function(*fun.data.type_of.get_done()),
        )?;
        match substitutions {
            Some(substitutions) => ctx.types.substitutions.dirty.restore(substitutions),
            None => (),
        }
        let fun = ctx.objects.functions.get_unchecked(&fun_key);
        for (ident, ty) in fun.data.params.clone() {
            let ty = ty
                .get_done()
                .substitute_many(&mut ctx.types)
                .map_err(|inner| Error {
                    inner,
                    module,
                    span: ident.location,
                })?;
            let value = values.push(Value { ty });
            let variable = Variable {
                identifier: ident.clone(),
                ty,
                value,
                mutated: false,
                used: false,
            };
            let variable = variables.push(variable);
            parameters.push((ident.deref().clone(), variable));
        }
        let fun = ctx.objects.functions.get_unchecked(&fun_key);
        let returns = Some(
            fun.data
                .return_type
                .get_done()
                .substitute_many(&mut ctx.types)
                .map_err(|inner| Error {
                    inner,
                    module,
                    span: todo!("mensi zmeny mozna?"),
                })?,
        );
        ctx.types.substitutions.dirty.pop();
        dbg!(&substitutions);

        Ok(FunctionIr {
            source: None,
            type_of: Some(type_of),
            returns,
            blocks: instructions,
            values,
            blocks_entry,
            variables,
            void,
            parameters,
            substitutions,
        })
    }

    pub fn const_stage_update(&mut self, fun: &AnyObject<FunctionObj>, fun_key: FunctionObjKey) {
        let mut parameters = Vec::new();
        for (ident, ty) in &fun.data.params {
            let ty = *ty.get_done();
            let value = self.values.push(Value { ty });
            let variable = Variable {
                identifier: ident.clone(),
                ty,
                value,
                mutated: false,
                used: false,
            };
            let variable = self.variables.push(variable);
            parameters.push((ident.deref().clone(), variable));
        }
        self.parameters = parameters;
        self.source = Some(fun_key);
        self.type_of = Some(AnyTypeKey::Function(*fun.data.type_of.get_done()));
        self.returns = Some(*fun.data.return_type.get_done());
    }
}

fn load_const(
    ir: &mut FunctionIr,
    expect: &Option<AnyTypeKey>,
    module: arena::Key<crate::const_stage::types::ModuleTag>,
    const_val: &ast::ConstValue,
    span: SpanIndex,
) -> Result<ValueKey, Diagnostic<Errors>> {
    let ty = match expect {
        Some(ty) => *ty,
        None => const_val.type_of().map_err(|inner| Error {
            inner,
            module,
            span,
        })?,
    };
    let dst = ir.values.push(Value { ty });
    ir.blocks.get_mut_unchecked().instructions.push(Span::new(
        Instruction::LoadConst {
            src: const_val.clone(),
            dst,
        },
        span,
    ));
    return Ok(dst);
}

impl Addr {
    pub fn type_of(&self, ctx: &Context, ir: &FunctionIrKey) -> Result<AnyTypeKey, Errors> {
        match self {
            Addr::Object(obj) => obj.type_of(ctx),
            Addr::Value(val) => Ok(ctx.ir_cache.get_unchecked(ir).values.get_unchecked(val).ty),
            Addr::MemoryRef { src: _, inner_ty } => Ok(*inner_ty),
            Addr::Var(var) => Ok(ctx
                .ir_cache
                .get_unchecked(ir)
                .variables
                .get_unchecked(var)
                .ty),
            Addr::Function(ir_key) => {
                let ir = ctx.ir_cache.get_unchecked(ir_key);
                Ok(ir.type_of.unwrap())
            }
            Addr::UnresolvedFunction(fun_key) => Ok(AnyTypeKey::Function(
                *ctx.objects
                    .functions
                    .get_unchecked(fun_key)
                    .data
                    .type_of
                    .get_done(),
            )),
        }
    }
}
