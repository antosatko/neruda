use std::ops::Deref;

use arena::Arena;
use arena_scope::stack::Stack;

use crate::{
    ast::{self, Body, Expression, Function, Span, SpanIndex},
    const_stage::{
        Context, Diagnostic, Error, Errors, Warnings,
        objects::{AnyObjectKey, FunctionObjKey, InitState},
        types::{AnyTypeKey, ModuleKey, PrimitiveType},
    },
    ir::{
        Addr, BasicBlock, BlockCtx, FunctionIr, Instruction, Terminator, Value, ValueKey, Variable,
    },
};

impl Context {
    pub fn lower_ir(&mut self) -> Result<(), Error> {
        let fun_keys: Vec<FunctionObjKey> = self.objects.functions.iter_keys().collect();
        for key in fun_keys {
            self.lower_function(&key)?;
        }

        Ok(())
    }

    pub fn lower_function(&mut self, key: &FunctionObjKey) -> Result<(), Error> {
        let fun = self.objects.functions.get_unchecked(key);
        if fun.data.ir.is_done() {
            return Ok(());
        }
        let mod_key = fun.module;
        let module = self.types.modules.get_unchecked(&mod_key);
        let mut instructions: Stack<BasicBlock> = Default::default();
        let instructions_entry = instructions.push(BasicBlock::default());

        let mut ir = FunctionIr {
            blocks: instructions,
            values: Default::default(),
            blocks_entry: instructions_entry,
            variables: Arena::default(),
        };

        let mut block_ctx = BlockCtx {
            variables: Default::default(),
            source: *key,
        };

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
                self.lower_block(&mut ir, &mut block_ctx, body, &mod_key)?
            }
            _ => unreachable!(),
        }
        ir.variables.shrink();

        for (var_key, var) in ir.variables.iter_pairs() {
            if !var.used {
                self.diagnostics.warnings.push(Diagnostic {
                    inner: Warnings::VariableUnused {
                        function: *key,
                        var: var_key,
                    },
                    module: mod_key,
                    span: var.identifier.location,
                });
            }
        }

        let fun = self.objects.functions.get_mut_unchecked(key);
        fun.data.ir = InitState::Done(ir);
        Ok(())
    }

    fn lower_block(
        &mut self,
        ir: &mut FunctionIr,
        block_ctx: &mut BlockCtx,
        block: &Span<Body>,
        module: &ModuleKey,
    ) -> Result<(), Error> {
        match block.deref() {
            Body::Block(block) => {
                block_ctx.variables.push();
                for st in block {
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
                                    let (ty, val) =
                                        match self.lower_expression(ir, block_ctx, expr, &None) {
                                            Ok(val) => (ir.values.get_unchecked(&val).ty, val),
                                            Err(err) => Err(err)?,
                                        };
                                    (val, ty)
                                }
                                (Some(ty), None) => {
                                    let ty_low = ty.lower(self, *module)?;
                                    let default =
                                        ty_low.const_default(self).map_err(|e| Error {
                                            inner: e,
                                            module: *module,
                                            span: ty.location,
                                        })?;
                                    (
                                        load_const(ir, ty.location, &None, *module, default)?,
                                        ty_low,
                                    )
                                }
                                (None, None) => Err(Error {
                                    inner: Errors::FailedTypeInfer,
                                    module: *module,
                                    span: st.location,
                                })?,
                            };
                            let var = Variable {
                                identifier: ident.clone(),
                                value,
                                ty,
                                used: false,
                                constant: true,
                            };

                            let key = ir.variables.push(var);
                            block_ctx.variables.insert(ident.deref().clone(), key);
                        }
                        ast::Statement::Return { expression } => {
                            let returns = self
                                .objects
                                .functions
                                .get_unchecked(&block_ctx.source)
                                .data
                                .return_type
                                .get_done();
                            match expression {
                                Some(expr) => {
                                    self.lower_expression(ir, block_ctx, expr, &Some(*returns))?
                                }
                                None => match returns {
                                    AnyTypeKey::Primitive(PrimitiveType::Void) => return Ok(()),
                                    _ => Err(Error {
                                        inner: Errors::ExpectedReturnExpression(block_ctx.source),
                                        module: *module,
                                        span: st.location,
                                    })?,
                                },
                            };
                            return Ok(());
                        }
                        ast::Statement::Invoke { .. } => {
                            return Ok(());
                        }
                        _ => todo!("{:?}", st),
                    }
                }
                block_ctx.variables.pop();
            }
            Body::Statement(_) => {}
        }
        Ok(())
    }

    fn lower_expression(
        &mut self,
        ir: &mut FunctionIr,
        block_ctx: &mut BlockCtx,
        expr: &Span<Expression>,
        expect: &Option<AnyTypeKey>,
    ) -> Result<ValueKey, Error> {
        let module = self
            .objects
            .functions
            .get_unchecked(&block_ctx.source)
            .module;
        match expr.const_eval(self, module, &None, expect) {
            Ok(const_val) => {
                return load_const(ir, expr.location, expect, module, const_val);
            }
            Err(_) => (),
        }
        match expr.deref() {
            Expression::Binary { l, r, op } => {
                let left_value = self.lower_expression(ir, block_ctx, l, &None)?;
                let right_value = self.lower_expression(ir, block_ctx, r, &None)?;
                Ok(left_value)
            }
            Expression::Value(val) => {
                let mut addr = match val.literal.deref() {
                    ast::Literal::Identifier(ident) => {
                        let path = &ident.path.deref().path;
                        match self.resolve_const_path(&path, module, ident.path.location) {
                            Ok(any) => Addr::Object(any),
                            Err(e) => {
                                if path.len() == 1 {
                                    let ident = path.first().unwrap();
                                    match block_ctx.variables.get(ident) {
                                        Some(v) => Addr::Var(*v),
                                        None => Err(Error {
                                            inner: Errors::VariableNotFound(ident.deref().clone()),
                                            module,
                                            span: ident.location,
                                        })?,
                                    }
                                } else {
                                    return Err(e);
                                }
                            }
                        }
                    }
                    a => todo!("{a:?}"),
                };
                dbg!(&addr);
                self.load_addr(ir, block_ctx, addr, expect)
            }
        }
    }

    pub fn load_addr(
        &mut self,
        ir: &mut FunctionIr,
        block_ctx: &mut BlockCtx,
        addr: Addr,
        expect: &Option<AnyTypeKey>,
    ) -> Result<ValueKey, Error> {
        match addr {
            Addr::Var(key) => {
                let ty = ir.variables.get_unchecked(&key).ty;
                if let Some(expect) = expect {
                    ty.check(&self.types, expect).map_err(|inner| Error {
                        inner,
                        module: todo!(),
                        span: todo!(),
                    })?;
                }
                let dst = ir.values.push(Value { ty });
                let instr = ir.blocks.get_mut_unchecked();
                instr
                    .instructions
                    .push(Instruction::LoadVar { src: key, dst });
                let var = ir.variables.get_mut_unchecked(&key);
                var.used = true;
                Ok(dst)
            }
            Addr::Object(obj) => todo!(),
            Addr::Value(val) => Ok(val),
        }
    }
}

fn load_const(
    ir: &mut FunctionIr,
    span: SpanIndex,
    expect: &Option<AnyTypeKey>,
    module: arena::Key<crate::const_stage::types::ModuleTag>,
    const_val: ast::ConstValue,
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
    ir.blocks
        .get_mut_unchecked()
        .instructions
        .push(Instruction::LoadConst {
            src: const_val,
            dst,
        });
    return Ok(dst);
}
