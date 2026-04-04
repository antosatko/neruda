use smol_str::SmolStr;

use crate::ast;
use crate::ir::Context;
use crate::ir::types::{
    AnyTypeKey, ArrayType, ConstraintKey, ConstraintType, FunctionType, GenericType, PrimitiveType,
    StructType, TupleType,
};

impl Context {
    pub fn lower_module(&mut self, module: &ast::Module) {
        for object in module.objects.iter() {
            match &object.inner {
                ast::Object::Scheduler {
                    ident,
                    resources,
                    systems,
                    init,
                    docs,
                } => todo!(),
                ast::Object::Component { ident, ty, docs } => todo!(),
                ast::Object::Type {
                    ident,
                    generics,
                    ty,
                    docs,
                } => todo!(),
                ast::Object::System {
                    ident,
                    generics,
                    docs,
                    query,
                    before,
                    body,
                    after,
                } => todo!(),
                ast::Object::Function {
                    ident,
                    generics,
                    parameters,
                    return_type,
                    body,
                    docs,
                } => {
                    let fn_type = FunctionType {
                        returns: todo!(),
                        parameters: todo!(),
                    };
                }
            }
        }
    }
}

impl ast::Type {
    pub fn lower(
        &self,
        ctx: &mut Context,
        module: &ast::Module,
        generic_context: &mut Vec<Vec<(SmolStr, ConstraintKey)>>,
    ) -> AnyTypeKey {
        let Self { literal, generics } = &self;
        match &literal.inner {
            ast::TypeLiteral::Path(identifier_path) => {
                if identifier_path.path.len() == 1
                    && let Some(ident) = identifier_path.path.first()
                {
                    match PrimitiveType::from_str(&ident.inner) {
                        Some(ty) => return AnyTypeKey::Primitive(ty),
                        _ => (),
                    }
                }
                todo!()
            }
            ast::TypeLiteral::Struct((spans, generics_opt)) => {
                let mut parameters = Vec::new();
                let generic_parameters = match generics_opt {
                    Some(generics) => {
                        let mut generic_params = Vec::new();
                        for generic in generics.inner.iter().map(|g| &g.inner) {
                            let constraint_type = ConstraintType { constraints: () };
                            let key = ctx.types.constraints.push_unique(constraint_type);
                            if parameters.iter().any(|(g_ident, _)| g_ident == generic) {
                                panic!("duplicate generic identifiers")
                            }
                            generic_params.push((generic.clone(), key));
                        }
                        generic_context.push(generic_params);
                        for param in spans {
                            let ident = &param.ident.inner;
                            if parameters.iter().any(|(p_ident, _)| p_ident == ident) {
                                panic!("duplicate parameter identifiers")
                            }
                            parameters.push((
                                ident.clone(),
                                param.ty.lower(ctx, module, generic_context),
                            ));
                        }
                        Some(generic_context.pop().unwrap())
                    }
                    None => {
                        for param in spans {
                            let ident = &param.ident.inner;
                            if parameters.iter().any(|(p_ident, _)| p_ident == ident) {
                                panic!("duplicate parameter identifiers")
                            }
                            parameters.push((
                                ident.clone(),
                                param.ty.lower(ctx, module, generic_context),
                            ));
                        }
                        None
                    }
                };
                let key = ctx.types.structures.push_unique(StructType { parameters });
                let type_key = AnyTypeKey::Struct(key);
                match generic_parameters {
                    Some(generic_parameters) => {
                        let generic_key = ctx.types.generics.push_unique(GenericType {
                            inner: type_key,
                            generic_parameters,
                        });
                        AnyTypeKey::Generic(generic_key)
                    }
                    None => type_key,
                }
            }
            ast::TypeLiteral::Array(span, size) => {
                let ty = span.lower(ctx, module, generic_context);
                let key = ctx.types.arrays.push_unique(ArrayType {
                    element_type: ty,
                    size: *size,
                });
                AnyTypeKey::Array(key)
            }
            ast::TypeLiteral::Tuple(spans) => {
                let mut parameters = Vec::new();
                for ty in spans {
                    parameters.push(ty.lower(ctx, module, generic_context));
                }
                match parameters.is_empty() {
                    true => AnyTypeKey::Primitive(PrimitiveType::Void),
                    _ => {
                        let key = ctx.types.tuples.push_unique(TupleType { parameters });
                        AnyTypeKey::Tuple(key)
                    }
                }
            }
        }
    }
}
