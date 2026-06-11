use std::ops::Deref;

use arena::Arena;
use arena_scope::{ScopeTree, stack::Stack};
use smol_str::SmolStr;

use crate::{
    ast::{self, Body, Expression, Function, Span},
    const_stage::{
        Context, Diagnostic, Error, Errors, Warnings,
        objects::{AnyObjectKey, ConstObjKey, FunctionObjKey, InitState},
        types::{AnyTypeKey, ModuleKey, PrimitiveType},
    },
    ir::{Addr, BlockCtx, FunctionIr, Instruction, Value, Variable, VariableKey},
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
        let mut instructions: Stack<Vec<Instruction>> = Default::default();
        let instructions_entry = instructions.push(Vec::new());

        let mut ir = FunctionIr {
            instructions,
            instructions_entry,
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
                                            Ok(val) => (
                                                val.type_of(self, ir).map_err(|e| Error {
                                                    inner: e,
                                                    module: *module,
                                                    span: expr.location,
                                                })?,
                                                val,
                                            ),
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
                                    (Value::Const(default), ty_low)
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
    ) -> Result<Value, Error> {
        let module = self
            .objects
            .functions
            .get_unchecked(&block_ctx.source)
            .module;
        match expr.const_eval(self, module, &None, expect) {
            Ok(const_val) => return Ok(Value::Const(const_val)),
            Err(_) => (),
        }
        match expr.deref() {
            Expression::Binary { l, r, op } => {
                let left_value = self.lower_expression(ir, block_ctx, l, &None)?;
                let right_value = self.lower_expression(ir, block_ctx, r, &None)?;
                self.load_value(ir, block_ctx, left_value, expect)?;
                let ty = self.load_value(ir, block_ctx, right_value, expect)?;
                Ok(Value::Runtime(ty))
            }
            Expression::Value(val) => {
                let mut addr = match val.literal.deref() {
                    ast::Literal::Identifier(ident) => {
                        let path = &ident.path.deref().path;
                        match self.resolve_const_path(&path, module, ident.path.location) {
                            Ok(AnyObjectKey::Const(key)) => Addr::Const(key),
                            Ok(AnyObjectKey::Function(key)) => Addr::Function(key),
                            Ok(any) => Err(Error {
                                inner: Errors::ObjectInaccessibleInBlock(any),
                                module,
                                span: ident.path.location,
                            })?,
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
                    _ => todo!(),
                };
                dbg!(&addr);
                Ok(Value::Addr(addr))
            }
        }
    }

    pub fn load_value(
        &mut self,
        ir: &mut FunctionIr,
        block_ctx: &mut BlockCtx,
        value: Value,
        expect: &Option<AnyTypeKey>,
    ) -> Result<AnyTypeKey, Error> {
        match value {
            Value::Const(const_value) => Ok(const_value.type_of().unwrap()),
            Value::Runtime(ty) => Ok(ty),
            Value::Addr(addr) => self.load_addr(ir, block_ctx, addr, expect),
        }
    }

    pub fn load_addr(
        &mut self,
        ir: &mut FunctionIr,
        block_ctx: &mut BlockCtx,
        addr: Addr,
        expect: &Option<AnyTypeKey>,
    ) -> Result<AnyTypeKey, Error> {
        match addr {
            Addr::Var(key) => {
                let instr = ir.instructions.get_mut_unchecked();
                instr.push(Instruction::PushVar { src: key });
                let var = ir.variables.get_mut_unchecked(&key);
                var.used = true;
                let ty = var.ty;
                Ok(ty)
            }
            Addr::Function(key) => todo!(),
            Addr::Const(key) => {
                let obj = self.objects.constants.get_unchecked(&key);
                ir.instructions
                    .get_mut_unchecked()
                    .push(Instruction::PushConst {
                        src: obj.data.value.get_done().clone(),
                    });
                Ok(*obj.data.ty.get_done())
            }
        }
    }
}

impl Value {
    pub fn type_of(&self, ctx: &Context, ir: &FunctionIr) -> Result<AnyTypeKey, Errors> {
        match self {
            Self::Const(val) => val.type_of(),
            Self::Runtime(ty) => Ok(*ty),
            Self::Addr(addr) => match addr {
                Addr::Var(key) => Ok(ir.variables.get_unchecked(key).ty),
                Addr::Function(key) => Ok(*ctx
                    .objects
                    .functions
                    .get_unchecked(key)
                    .data
                    .type_of
                    .get_done()),
                Addr::Const(key) => {
                    Ok(*ctx.objects.constants.get_unchecked(key).data.ty.get_done())
                }
            },
        }
    }
}
