use smol_str::SmolStr;

use crate::ast;
use crate::ir::Context;
use crate::ir::types::{
    AnyTypeKey, ArrayType, ConstraintKey, ConstraintType, FunctionType, GenericType, PrimitiveType,
    StructType, TupleType,
};

#[derive(Default)]
pub struct GenericContext {
    scopes: Vec<Vec<(SmolStr, ConstraintKey)>>,
}

impl Context {
    pub fn lower_module(&mut self, module: &ast::Module) {
        let mut generic_ctx = GenericContext::default();
        for object in module.objects.iter() {
            match &object.inner {
                ast::Object::Scheduler {
                    ident,
                    resources,
                    systems,
                    init,
                    docs,
                } => (),
                ast::Object::Component { ident, ty, docs } => {
                    let key = ty
                        .as_ref()
                        .map(|t| t.lower(self, module, &mut generic_ctx))
                        .unwrap_or(AnyTypeKey::Primitive(PrimitiveType::Void));
                }
                ast::Object::Type {
                    ident,
                    generics,
                    ty,
                    docs,
                } => {
                    generic_ctx.push_scope(generics, self);
                    let key = ty
                        .as_ref()
                        .map(|t| t.lower(self, module, &mut generic_ctx))
                        .unwrap_or(AnyTypeKey::Primitive(PrimitiveType::Void));
                    generic_ctx.pop_scope();
                }
                ast::Object::System {
                    ident,
                    generics,
                    docs,
                    query,
                    before,
                    body,
                    after,
                } => {
                    generic_ctx.push_scope(generics, self);
                    generic_ctx.pop_scope();
                }
                ast::Object::Function {
                    ident,
                    generics,
                    parameters,
                    return_type,
                    body,
                    docs,
                } => {
                    generic_ctx.push_scope(generics, self);
                    let fn_type = FunctionType {
                        returns: return_type
                            .as_ref()
                            .map(|t| t.lower(self, module, &mut generic_ctx))
                            .unwrap_or(AnyTypeKey::Primitive(PrimitiveType::Void)),
                        parameters: parameters
                            .iter()
                            .map(|p| {
                                (
                                    p.ident.inner.clone(),
                                    p.ty.lower(self, module, &mut generic_ctx),
                                )
                            })
                            .collect(),
                    };
                    generic_ctx.pop_scope();
                    let key = self.types.functions.push_unique(fn_type);
                    let key = AnyTypeKey::Function(key);
                }
            }
        }
    }
}

impl GenericContext {
    fn push_scope(
        &mut self,
        generics: &Option<ast::Span<Vec<ast::Span<ast::GenericParameter>>>>,
        ctx: &mut Context,
    ) {
        if let Some(generics) = generics {
            let mut scope = Vec::new();
            let constraint = ConstraintType { constraints: () };
            let key = ctx.types.constraints.push_unique(constraint);
            for generic in &generics.inner {
                scope.push((generic.identifier.inner.clone(), key));
            }
            self.scopes.push(scope);
        } else {
            self.scopes.push(Vec::new());
        }
    }

    #[track_caller]
    fn pop_scope(&mut self) {
        assert!(self.scopes.pop().is_some());
    }

    fn find_generic(&self, ident: &SmolStr) -> Option<&ConstraintKey> {
        for scope in self.scopes.iter().rev() {
            if let Some(generic) = scope
                .iter()
                .find(|(g_ident, _)| ident == g_ident)
                .map(|(_, g)| g)
            {
                return Some(generic);
            }
        }
        None
    }
}

impl ast::Type {
    pub fn lower(
        &self,
        ctx: &mut Context,
        module: &ast::Module,
        generic_context: &mut GenericContext,
    ) -> AnyTypeKey {
        let Self { literal, generics } = &self;
        let unresolved = match &literal.inner {
            ast::TypeLiteral::Path(identifier_path) => {
                if identifier_path.path.len() == 1
                    && let Some(ident) = identifier_path.path.first()
                {
                    if let Some(ty) = PrimitiveType::from_str(&ident.inner) {
                        return AnyTypeKey::Primitive(ty);
                    }
                    if let Some(ty) = generic_context.find_generic(ident) {
                        return AnyTypeKey::Constraint(*ty);
                    }
                }
                todo!("resolve actual path")
            }
            ast::TypeLiteral::Struct(spans) => {
                let mut parameters = Vec::new();
                for param in spans {
                    let ident = &param.ident.inner;
                    if parameters.iter().any(|(p_ident, _)| p_ident == ident) {
                        panic!("duplicate parameter identifiers")
                    }
                    parameters.push((ident.clone(), param.ty.lower(ctx, module, generic_context)));
                }
                let key = ctx.types.structures.push_unique(StructType { parameters });
                AnyTypeKey::Struct(key)
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
        };
        match generics {
            None => unresolved,
            Some(generics) => {
                todo!("need to resolve generics")
            }
        }
    }
}
