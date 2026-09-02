use std::ops::Deref;

use arena::Arena;
use arena_scope::stack::Stack;

use crate::{
    ast::{
        self, Body, ConstValue, Expression, Function, Number, NumberValue, Postfix, Span,
        SpanIndex, Type,
    },
    const_stage::{
        ConstValueKey, Constants, Context, Diagnostic, Error, Errors, Warning, Warnings,
        lowering::{ConstEvalResult, apply_generic_arguments},
        objects::{AnyObject, AnyObjectKey, FunctionObj, FunctionObjKey, InitState, IrCache},
        types::{AnyTypeKey, ModuleKey, PrimitiveType, RefType},
    },
    ir::{
        Addr, BasicBlock, BlockCtx, ControlFrame, ControlFrameKind, FunctionIr, FunctionIrKey,
        Instruction, Terminator, Value, ValueKey, Variable,
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
            IrCache::Single(InitState::Uninitialized) => {
                let ir = self.ir_cache.push(FunctionIr::new(fun, *key));
                fun.data.ir = IrCache::Single(InitState::Progress(ir));
                ir
            }
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
        let ast_obj = match &fun.ast_object {
            Some(o) => o,
            None => return Ok(ir_key),
        };
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

        let falls_through = match self
            .ast
            .get(&module.path)
            .unwrap()
            .objects
            .get_unchecked(ast_obj)
            .clone()
            .deref()
        {
            ast::Object::Function(Function { body, .. }) => {
                self.lower_block(&ir_key, &mut block_ctx, &body, &mod_key)?
            }
            _ => unreachable!(),
        };
        let ir = self.ir_cache.get_mut_unchecked(&ir_key);
        ir.variables.shrink();
        ir.values.shrink();

        let fun = self.objects.functions.get_unchecked(key);
        match (falls_through, &ir.blocks.get_mut_unchecked().terminator) {
            (false, _) | (true, Some(_)) => (),
            (true, None) => match fun.data.return_type.get_done() {
                AnyTypeKey::Void => {
                    ir.blocks
                        .get_mut_unchecked()
                        .terminate(Terminator::Return(None), false);
                }
                _ => Err(Error {
                    inner: Errors::Todo("Either you forgot return or I have to improve my thing"),
                    module: mod_key,
                    span: SpanIndex::default(),
                })?,
            },
        }

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
    ) -> Result<bool, Error> {
        Ok(match block.deref() {
            Body::Block(block) => {
                block_ctx.variables.push();
                let mut falls_through = true;
                let mut it = block.iter();
                while let Some(st) = it.next() {
                    let current = self
                        .ir_cache
                        .get_unchecked(ir)
                        .blocks
                        .current_key()
                        .unwrap();
                    if self
                        .ir_cache
                        .get_unchecked(ir)
                        .blocks
                        .arena()
                        .get_unchecked(&current)
                        .value
                        .terminator
                        .is_some()
                    {
                        self.dead_code(module, &mut it);
                        falls_through = false;
                        break;
                    }
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
                                            &self.constants,
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
                                needs_address: false,
                            };
                            let ir = self.ir_cache.get_mut_unchecked(ir);
                            let dst = ir.variables.push(var);
                            ir.blocks
                                .get_mut_unchecked()
                                .extend([Instruction::StoreVar { dst, src }], st.location);
                            block_ctx.variables.insert(ident.deref().clone(), dst);
                        }
                        ast::Statement::Return { expression } => {
                            let returns = self.ir_cache.get_mut_unchecked(ir).returns;
                            self.dead_code(module, &mut it);
                            let val = match (returns, expression) {
                                (Some(returns), Some(expr)) => {
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
                                (Some(returns), None) => match returns {
                                    AnyTypeKey::Void => None,
                                    _ => Err(Error {
                                        inner: Errors::ExpectedReturnExpression(block_ctx.source),
                                        module: *module,
                                        span: st.location,
                                    })?,
                                },
                                (None, Some(expr)) => match returns {
                                    _ => Err(Error {
                                        inner: Errors::InvalidExpression,
                                        module: *module,
                                        span: expr.location,
                                    })?,
                                },
                                (None, None) => None,
                            };
                            let ir = self.ir_cache.get_mut_unchecked(ir);
                            ir.blocks
                                .get_mut_unchecked()
                                .terminate(Terminator::Return(val), false);
                            return Ok(false);
                        }
                        ast::Statement::Invoke { .. } => (),
                        ast::Statement::Expr { expression } => {
                            self.lower_expression(ir, block_ctx, expression, &None)?;
                        }
                        ast::Statement::Loop { label, body } => {
                            let current = self
                                .ir_cache
                                .get_unchecked(ir)
                                .blocks
                                .current_key()
                                .unwrap();
                            let ir_obj = self.ir_cache.get_mut_unchecked(ir);
                            let break_block = ir_obj.blocks.push(BasicBlock::default());
                            let continue_block = ir_obj.blocks.push(BasicBlock::default());
                            ir_obj
                                .blocks
                                .arena_mut()
                                .get_mut_unchecked(&current)
                                .value
                                .terminate(Terminator::Jump(continue_block, None), false);

                            block_ctx.control_stack.push(ControlFrame {
                                kind: ControlFrameKind::Loop {
                                    break_block,
                                    continue_block,
                                },
                                label: label.as_ref().map(|s| s.deref().clone()),
                            });

                            ir_obj.blocks.restore(continue_block);
                            let body_falls_through =
                                self.lower_block(ir, block_ctx, body, module)?;
                            let body_end = self
                                .ir_cache
                                .get_unchecked(ir)
                                .blocks
                                .current_key()
                                .unwrap();
                            if body_falls_through
                                && self
                                    .ir_cache
                                    .get_unchecked(ir)
                                    .blocks
                                    .arena()
                                    .get_unchecked(&body_end)
                                    .value
                                    .terminator
                                    .is_none()
                            {
                                self.ir_cache
                                    .get_mut_unchecked(ir)
                                    .blocks
                                    .arena_mut()
                                    .get_mut_unchecked(&body_end)
                                    .value
                                    .terminate(Terminator::Jump(continue_block, None), false);
                            }

                            self.ir_cache
                                .get_mut_unchecked(ir)
                                .blocks
                                .restore(break_block);
                            block_ctx.control_stack.pop();
                        }
                        ast::Statement::Break { label } => {
                            let (label, span) = match label {
                                Some(l) => (Some(l.deref().clone()), l.location),
                                None => (None, st.location),
                            };
                            let break_block =
                                block_ctx.get_break_block(&label).map_err(|inner| Error {
                                    inner,
                                    module: *module,
                                    span,
                                })?;
                            let ir_obj = self.ir_cache.get_mut_unchecked(ir);
                            ir_obj
                                .blocks
                                .get_mut_unchecked()
                                .terminate(Terminator::Jump(break_block, None), false);
                            self.dead_code(module, &mut it);
                            falls_through = false;
                            break;
                        }
                        ast::Statement::Continue { label } => {
                            let (label, span) = match label {
                                Some(l) => (Some(l.deref().clone()), l.location),
                                None => (None, st.location),
                            };
                            let break_block =
                                block_ctx
                                    .get_continue_block(&label)
                                    .map_err(|inner| Error {
                                        inner,
                                        module: *module,
                                        span,
                                    })?;
                            let ir_obj = self.ir_cache.get_mut_unchecked(ir);
                            ir_obj
                                .blocks
                                .get_mut_unchecked()
                                .terminate(Terminator::Jump(break_block, None), false);
                            self.dead_code(module, &mut it);
                            falls_through = false;
                            break;
                        }
                        ast::Statement::If {
                            condition,
                            then_block,
                            else_if,
                            else_block,
                        } => {
                            let cond_expect = Some(AnyTypeKey::Primitive(PrimitiveType::Bool));
                            let mut condition_block = self
                                .ir_cache
                                .get_unchecked(ir)
                                .blocks
                                .current_key()
                                .unwrap();
                            let mut condition_value = {
                                let addr =
                                    self.lower_expression(ir, block_ctx, condition, &cond_expect)?;
                                self.load_addr(
                                    ir,
                                    block_ctx,
                                    addr,
                                    &cond_expect,
                                    condition.location,
                                )?
                            };

                            let final_block = self
                                .ir_cache
                                .get_mut_unchecked(ir)
                                .blocks
                                .push(BasicBlock::default());

                            let mut then_block_key = self
                                .ir_cache
                                .get_mut_unchecked(ir)
                                .blocks
                                .push(BasicBlock::default());
                            self.ir_cache
                                .get_mut_unchecked(ir)
                                .blocks
                                .restore(then_block_key);
                            let mut any_branch_falls_through =
                                self.lower_block(ir, block_ctx, then_block, module)?;
                            let then_end = self
                                .ir_cache
                                .get_unchecked(ir)
                                .blocks
                                .current_key()
                                .unwrap();
                            if any_branch_falls_through
                                && self
                                    .ir_cache
                                    .get_unchecked(ir)
                                    .blocks
                                    .arena()
                                    .get_unchecked(&then_end)
                                    .value
                                    .terminator
                                    .is_none()
                            {
                                self.ir_cache
                                    .get_mut_unchecked(ir)
                                    .blocks
                                    .arena_mut()
                                    .get_mut_unchecked(&then_end)
                                    .value
                                    .terminate(Terminator::Jump(final_block, None), false);
                            } else {
                                any_branch_falls_through = false;
                            }

                            for else_if in else_if {
                                let next_condition_block = self
                                    .ir_cache
                                    .get_mut_unchecked(ir)
                                    .blocks
                                    .push(BasicBlock::default());
                                let next_then_block = self
                                    .ir_cache
                                    .get_mut_unchecked(ir)
                                    .blocks
                                    .push(BasicBlock::default());

                                self.ir_cache
                                    .get_mut_unchecked(ir)
                                    .blocks
                                    .restore(condition_block);
                                self.ir_cache
                                    .get_mut_unchecked(ir)
                                    .blocks
                                    .arena_mut()
                                    .get_mut_unchecked(&condition_block)
                                    .value
                                    .terminate(
                                        Terminator::Branch {
                                            condition: condition_value,
                                            then_block: then_block_key,
                                            else_block: next_condition_block,
                                        },
                                        false,
                                    );

                                self.ir_cache
                                    .get_mut_unchecked(ir)
                                    .blocks
                                    .restore(next_condition_block);
                                let addr = self.lower_expression(
                                    ir,
                                    block_ctx,
                                    &else_if.condition,
                                    &cond_expect,
                                )?;
                                condition_value = self.load_addr(
                                    ir,
                                    block_ctx,
                                    addr,
                                    &cond_expect,
                                    else_if.condition.location,
                                )?;

                                self.ir_cache
                                    .get_mut_unchecked(ir)
                                    .blocks
                                    .restore(next_then_block);
                                let branch_falls_through =
                                    self.lower_block(ir, block_ctx, &else_if.block, module)?;
                                let branch_end = self
                                    .ir_cache
                                    .get_unchecked(ir)
                                    .blocks
                                    .current_key()
                                    .unwrap();
                                if branch_falls_through
                                    && self
                                        .ir_cache
                                        .get_unchecked(ir)
                                        .blocks
                                        .arena()
                                        .get_unchecked(&branch_end)
                                        .value
                                        .terminator
                                        .is_none()
                                {
                                    self.ir_cache
                                        .get_mut_unchecked(ir)
                                        .blocks
                                        .arena_mut()
                                        .get_mut_unchecked(&branch_end)
                                        .value
                                        .terminate(Terminator::Jump(final_block, None), false);
                                }
                                any_branch_falls_through |= branch_falls_through;

                                condition_block = next_condition_block;
                                then_block_key = next_then_block;
                            }

                            let mut else_target = final_block;
                            let else_falls_through = match else_block {
                                Some(else_block) => {
                                    let else_block_key = self
                                        .ir_cache
                                        .get_mut_unchecked(ir)
                                        .blocks
                                        .push(BasicBlock::default());
                                    else_target = else_block_key;
                                    self.ir_cache
                                        .get_mut_unchecked(ir)
                                        .blocks
                                        .restore(else_block_key);
                                    let branch_falls_through =
                                        self.lower_block(ir, block_ctx, &else_block.block, module)?;
                                    let else_end = self
                                        .ir_cache
                                        .get_unchecked(ir)
                                        .blocks
                                        .current_key()
                                        .unwrap();
                                    if branch_falls_through
                                        && self
                                            .ir_cache
                                            .get_unchecked(ir)
                                            .blocks
                                            .arena()
                                            .get_unchecked(&else_end)
                                            .value
                                            .terminator
                                            .is_none()
                                    {
                                        self.ir_cache
                                            .get_mut_unchecked(ir)
                                            .blocks
                                            .arena_mut()
                                            .get_mut_unchecked(&else_end)
                                            .value
                                            .terminate(Terminator::Jump(final_block, None), false);
                                    }
                                    branch_falls_through
                                }
                                None => true,
                            };
                            any_branch_falls_through |= else_falls_through;

                            self.ir_cache
                                .get_mut_unchecked(ir)
                                .blocks
                                .restore(condition_block);
                            self.ir_cache
                                .get_mut_unchecked(ir)
                                .blocks
                                .arena_mut()
                                .get_mut_unchecked(&condition_block)
                                .value
                                .terminate(
                                    Terminator::Branch {
                                        condition: condition_value,
                                        then_block: then_block_key,
                                        else_block: else_target,
                                    },
                                    false,
                                );

                            self.ir_cache
                                .get_mut_unchecked(ir)
                                .blocks
                                .restore(final_block);
                            falls_through = any_branch_falls_through;
                            if !falls_through {
                                // Both sides terminate, so there is no continuation block.
                                // Stop this block immediately; callers inspect the returned
                                // flow result and therefore cannot accidentally emit into the
                                // unreachable merge block.
                                self.dead_code(module, &mut it);
                                break;
                            }
                        }
                        _ => todo!("{:?}", st),
                    }
                }
                block_ctx.variables.pop();
                falls_through
            }
            Body::Statement(expression) => {
                let returns = self.ir_cache.get_mut_unchecked(ir).returns.unwrap();
                let addr = self.lower_expression(ir, block_ctx, expression, &Some(returns))?;
                let val =
                    self.load_addr(ir, block_ctx, addr, &Some(returns), expression.location)?;
                let ir = self.ir_cache.get_mut_unchecked(ir);
                let this_block = ir.blocks.current_key().unwrap();
                match ir.blocks.arena().get_unchecked(&this_block).parent {
                    Some(_) => todo!("theres some work to do"),
                    None => ir
                        .blocks
                        .get_mut_unchecked()
                        .terminate(Terminator::Return(Some(val)), false),
                }
                false
            }
        })
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
                let key = self.constants.push(const_val);
                return Ok(Addr::Value(load_const(
                    self.ir_cache.get_mut_unchecked(ir),
                    expect,
                    module,
                    &key,
                    expr.location,
                    &self.constants,
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
                match op.deref() {
                    ast::Operator::Assign
                    | ast::Operator::ModAssign
                    | ast::Operator::DivAssign
                    | ast::Operator::MulAssign
                    | ast::Operator::SubAssign
                    | ast::Operator::AddAssign => {
                        let right_value =
                            self.load_addr(ir, block_ctx, right_addr, &None, r.location)?;
                        let result_type = right_ty;
                        match left_addr {
                            Addr::Var(key) => {
                                let ir = self.ir_cache.get_mut_unchecked(ir);
                                ir.blocks.get_mut_unchecked().extend(
                                    [Instruction::StoreVar {
                                        dst: key,
                                        src: right_value,
                                    }],
                                    op.location,
                                );
                                Ok(left_addr)
                            }
                            Addr::Value(key) => todo!(),
                            Addr::Object(any_object_key) => todo!(),
                            Addr::Function(key) => todo!(),
                            Addr::UnresolvedFunction(key) => todo!(),
                            Addr::MemoryRef { src, inner_ty } => todo!(),
                            Addr::Field { src, idx } => todo!(),
                            Addr::Never => todo!(),
                        }
                    }
                    ast::Operator::BitAnd
                    | ast::Operator::BitOr
                    | ast::Operator::Assign
                    | ast::Operator::Or => Err(Error {
                        inner: Errors::Todo("Operator unsupported"),
                        span: op.location,
                        module,
                    }),
                    _ => {
                        let left_value =
                            self.load_addr(ir, block_ctx, left_addr, &None, l.location)?;
                        let right_value =
                            self.load_addr(ir, block_ctx, right_addr, &None, r.location)?;
                        let result_ty =
                            op.result_type(left_ty, right_ty, &self.types)
                                .map_err(|inner| Error {
                                    inner,
                                    module,
                                    span: expr.location,
                                })?;

                        let ir = self.ir_cache.get_mut_unchecked(ir);
                        let dst = ir.values.push(Value::new(result_ty));
                        ir.blocks.get_mut_unchecked().extend(
                            [Instruction::BinOp {
                                op: *op.deref(),
                                l: left_value,
                                r: right_value,
                                dst,
                                ty: match left_ty {
                                    AnyTypeKey::Primitive(ty) => ty,
                                    _ => unreachable!(),
                                },
                            }],
                            op.location,
                        );
                        Ok(Addr::Value(dst))
                    }
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

                                    let (concrete_ty, _subs) = apply_generic_arguments(
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
                                        _ if ident.deref() == "_" => Addr::Never,
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
                                let var_obj = ir.variables.get_mut_unchecked(&var);
                                var_obj.needs_address = true;
                                var_obj.used = true;
                                let ty = self
                                    .types
                                    .references
                                    .push_unique(RefType { inner: var_obj.ty });
                                let dst = ir.values.push(Value::new(AnyTypeKey::Reference(ty)));
                                ir.blocks
                                    .get_mut_unchecked()
                                    .extend([Instruction::AddressOfVar { var, dst }], op.location);
                                Addr::Value(dst)
                            }
                            Addr::Value(val) => {
                                let ir = self.ir_cache.get_mut_unchecked(ir);
                                let val_obj = ir.values.get_mut_unchecked(&val);
                                val_obj.needs_address = true;
                                let ty = self
                                    .types
                                    .references
                                    .push_unique(RefType { inner: val_obj.ty });
                                let dst = ir.values.push(Value::new(AnyTypeKey::Reference(ty)));
                                ir.blocks
                                    .get_mut_unchecked()
                                    .extend([Instruction::AddressOfVal { val, dst }], op.location);
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
                                for (expr, (_ident, expect)) in
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
                                let dst = self_ir.values.push(Value::new(result));
                                self_ir.blocks.get_mut_unchecked().extend(
                                    [Instruction::Call {
                                        fun: ir_key,
                                        arguments: arg_values,
                                        result: dst,
                                    }],
                                    op.location,
                                );
                                Addr::Value(dst)
                            }
                            Addr::UnresolvedFunction(fun_key) => {
                                let obj = self.objects.functions.get_unchecked(&fun_key);
                                let _generics = obj.data.generics.clone();
                                let signature = *obj.data.type_of.get_done();
                                let params = self
                                    .types
                                    .functions
                                    .get_unchecked(&signature)
                                    .parameters
                                    .clone();
                                let _returns =
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
                                return Err(Error {
                                    inner: Errors::Todo("Generic arguments inference"),
                                    module,
                                    span: op.location,
                                });

                                // self.lower_function(&fun_key, &None)?;
                            }
                            _ => {
                                let _val =
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
                    let _ir = self.ir_cache.get_mut_unchecked(ir);
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
            Addr::Never => {
                let ir = self.ir_cache.get_mut_unchecked(ir);
                let blk = ir.blocks.get_mut_unchecked();
                blk.terminate(Terminator::Exit(11), true);
                blk.lock_instructions(true);

                Ok(ir.void)
            }
            Addr::Field { src, idx } => {
                src;
                idx;
                Err(Error {
                    inner: Errors::Todo("implement field access"),
                    module,
                    span,
                })
            }
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
                let dst = ir.values.push(Value::new(ty));
                let instr = ir.blocks.get_mut_unchecked();
                instr.extend([Instruction::LoadVar { src: key, dst }], span);
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
                        let dst = ir.values.push(Value::new(ty));
                        ir.blocks
                            .get_mut_unchecked()
                            .extend([Instruction::AddressOfObj { obj, dst }], span);
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
                        load_const(ir, expect, module, value, span, &self.constants)
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
                let dst = ir.values.push(Value::new(ty));
                ir.blocks
                    .get_mut_unchecked()
                    .extend([Instruction::AddressOfFun { fun, dst }], span);
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
            Addr::MemoryRef { src, inner_ty: _ } => {
                let ir = self.ir_cache.get_mut_unchecked(ir);
                let ty = ir.values.get_unchecked(&src).ty;
                let dst = ir.values.push(Value::new(ty));
                ir.blocks
                    .get_mut_unchecked()
                    .extend([Instruction::Deref { src, dst }], span);
                Ok(dst)
            }
        }
    }
}

impl FunctionIr {
    pub fn new(fun: &AnyObject<FunctionObj>, key: FunctionObjKey) -> FunctionIr {
        let mut instructions: Stack<BasicBlock> = Default::default();
        let blocks_entry = instructions.push(BasicBlock::default());
        let mut values = Arena::default();
        let void = values.push(Value::new(AnyTypeKey::Void));
        let mut variables = Arena::new();
        let mut parameters = Vec::new();
        for (ident, ty) in &fun.data.params {
            let ty = *ty.get_done();
            let value = values.push(Value::new(ty));
            let variable = Variable {
                identifier: ident.clone(),
                ty,
                value,
                mutated: false,
                used: false,
                needs_address: false,
            };
            let variable = variables.push(variable);
            parameters.push((ident.deref().clone(), variable));
        }

        let returns = *fun.data.return_type.get_done();
        let type_of = AnyTypeKey::Function(*fun.data.type_of.get_done());

        FunctionIr {
            source: Some(key),
            type_of: Some(type_of),
            returns: Some(returns),
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
        let void = values.push(Value::new(AnyTypeKey::Void));
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
            let value = values.push(Value::new(ty));
            let variable = Variable {
                identifier: ident.clone(),
                ty,
                value,
                mutated: false,
                used: false,
                needs_address: false,
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
        // ctx.types.substitutions.dirty.pop();
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
            let value = self.values.push(Value::new(ty));
            let variable = Variable {
                identifier: ident.clone(),
                ty,
                value,
                mutated: false,
                used: false,
                needs_address: false,
            };
            let variable = self.variables.push(variable);
            parameters.push((ident.deref().clone(), variable));
        }
        self.parameters = parameters;
        self.source = Some(fun_key);
        self.type_of = Some(AnyTypeKey::Function(*fun.data.type_of.get_done()));
        self.returns = match fun.data.return_type.get_done() {
            AnyTypeKey::Void => None,
            ty => Some(*ty),
        }
    }
}

fn load_const(
    ir: &mut FunctionIr,
    expect: &Option<AnyTypeKey>,
    module: arena::Key<crate::const_stage::types::ModuleTag>,
    const_key: &ConstValueKey,
    span: SpanIndex,
    constants: &Constants,
) -> Result<ValueKey, Diagnostic<Errors>> {
    let const_value = constants.data.get_unchecked(const_key);
    let ty = match expect {
        Some(ty) => *ty,
        None => const_value.type_of().map_err(|inner| Error {
            inner,
            module,
            span,
        })?,
    };
    let dst = ir.values.push(Value::new(ty));
    ir.blocks.get_mut_unchecked().extend(
        [Instruction::LoadConst {
            src: const_key.clone(),
            dst,
        }],
        span,
    );
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
            Addr::Never => Ok(AnyTypeKey::Never),
            Addr::Field { src, idx } => {
                let parent = src.type_of(ctx, ir)?;
                let dst_ty = parent
                    .field_by_idx(*idx, &ctx.types)
                    .expect("Field not found anymore");
                todo!()
            }
        }
    }
}
