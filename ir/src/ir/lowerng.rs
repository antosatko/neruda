use std::ops::Deref;

use arena::Arena;

use crate::{
    ast::{self, Body, Expression, Function, Span},
    const_stage::{
        Context, Diagnostic, Error, Errors, Warnings,
        objects::{FunctionObjKey, InitState},
        types::ModuleKey,
    },
    ir::{BlockCtx, FunctionIr, Value, Variable},
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

        let mut ir = FunctionIr {
            instructions: Vec::new(),
            //values: Arena::default(),
            variables: Arena::default(),
        };

        let mut block_ctx = BlockCtx { scopes: Vec::new() };

        match self
            .ast
            .get(&module.path)
            .unwrap()
            .objects
            .get_unchecked(&fun.ast_object)
            .clone()
            .deref()
        {
            ast::Object::Function(Function {
                ident,
                generics,
                parameters,
                return_type,
                body,
                docs,
            }) => self.lower_block(&mut ir, &mut block_ctx, body, &mod_key)?,
            _ => unreachable!(),
        }
        ir.instructions.shrink_to_fit();
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
                block_ctx.push_scope();
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
                                    let val = match expr.const_eval(self, *module, &None, &Some(ty))
                                    {
                                        Ok(val) => Value::Const(val),
                                        Err(err) => Err(err)?,
                                    };
                                    (val, ty)
                                }
                                (None, Some(expr)) => {
                                    let (ty, val) =
                                        match expr.const_eval(self, *module, &None, &None) {
                                            Ok(val) => (val.type_of(), Value::Const(val)),
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
                            block_ctx.declare_var(ident, module, key)?;
                        }
                        ast::Statement::Return { expression } => {
                            return Ok(());
                        }
                        _ => todo!("{:?}", st),
                    }
                }
                block_ctx.pop_scope();
            }
            Body::Statement(st) => {}
        }
        Ok(())
    }

    fn lower_expression(
        &mut self,
        ir: &mut FunctionIr,
        expr: &Span<Expression>,
        module: &ModuleKey,
    ) -> () {
    }
}
