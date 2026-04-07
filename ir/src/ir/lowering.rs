use std::collections::HashMap;

use arena::Key;
use smol_str::SmolStr;

use crate::ast::{self, ConstValue};
use crate::ir::Context;
use crate::ir::objects::{AnyObject, Module};
use crate::ir::types::{
    AnyTypeKey, ArrayType, ConstraintKey, ConstraintType, FunctionType, ModuleKey, ModuleTag,
    PrimitiveType, StructType, TupleType,
};

#[derive(Default)]
pub struct GenericContext {
    scopes: Vec<Vec<(SmolStr, ConstraintKey)>>,
}

impl Context {
    pub(crate) fn lower_import_stage(&mut self) {
        let map: HashMap<Vec<SmolStr>, Key<ModuleTag>> =
            HashMap::from_iter(self.ast.keys().map(|k| {
                let mut module = Module::default();
                module.path = k.clone();
                let type_key = self.types.modules.push(module);
                (k.clone(), type_key)
            }));

        for (key, module) in &self.ast {
            let ir_module = self.types.modules.get_mut_unchecked(map.get(key).unwrap());
            for obj in module.objects.iter() {
                match &obj.inner {
                    ast::Object::Import { ident, alias } => {
                        let key: Vec<SmolStr> =
                            ident.inner.path.iter().map(|a| a.inner.clone()).collect();
                        let ident = match &alias.0 {
                            Some(name) => name.inner.inner.clone(),
                            None => ident.inner.path.last().unwrap().inner.clone(),
                        };
                        let ty_key = match map.get(&key) {
                            Some(k) => *k,
                            None => todo!("lamo"),
                        };
                        let obj = AnyObject::Import { module: ty_key };
                        ir_module.symbols.insert(ident, obj);
                    }
                    _ => (),
                }
            }
        }
    }

    pub fn resolve_const_path(
        &mut self,
        path: &[SmolStr],
        mut module: ModuleKey,
        generic_context: &mut GenericContext,
    ) -> Option<&AnyObject> {
        for next_stop in path {
            let current = self.types.modules.get_mut_unchecked(&module);
            let next = match current.symbols.get(next_stop) {
                Some(next) => next,
                None => match current.hoisted_symbols.remove(next_stop) {
                    Some(hoisted) => match hoisted {
                        ast::Object::Const {
                            docs,
                            ident,
                            ty,
                            expression,
                        } => {
                            let ty = ty.lower(self, module, generic_context);
                            let value = expression.const_eval(self, generic_context)?;
                            let obj = AnyObject::Const { value, ty };
                            let current = self.types.modules.get_mut_unchecked(&module);
                            current.symbols.insert(ident.inner.clone(), obj);
                            return current.symbols.get(&ident.inner);
                        }
                        _ => todo!(),
                    },
                    None => return None,
                },
            };
        }

        todo!()
    }

    pub fn lower_module(&mut self, path: Vec<SmolStr>) {
        /*self.types
            .module_references
            .push_unique(ModuleRefType { key: path.clone() });
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
                ast::Object::Import { ident, alias } => {
                    let key = ident.inner.path.iter().map(|a| a.inner.clone()).collect();
                    let ident = match &alias.0 {
                        Some(name) => name.inner.inner.clone(),
                        None => ident.inner.path.last().unwrap().inner.clone(),
                    };
                    let ty = ModuleRefType { key: key };
                    let ty_key = self.types.module_references.push_unique(ty);
                    let obj = AnyObject::Import { module: ty_key };
                }
            }
        }*/
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

impl ast::Expression {
    pub fn const_eval(
        &self,
        ctx: &mut Context,
        generic_context: &mut GenericContext,
    ) -> Option<ConstValue> {
        match self {
            ast::Expression::Value(value) => {
                if !value.postfix.is_empty() {
                    return None;
                }
                Some(match &value.literal.inner {
                    ast::Literal::Identifier(identifier_path) => todo!(),
                    ast::Literal::Structure(span, spans) => todo!(),
                    ast::Literal::Number(number) => ConstValue::Number(number.clone()),
                    ast::Literal::String(smol_str) => ConstValue::String(smol_str.clone()),
                    ast::Literal::Char(c) => ConstValue::Char(*c),
                    ast::Literal::Array(spans) => todo!(),
                    ast::Literal::Tuple(spans) => todo!(),
                })
            }
            ast::Expression::Binary { l, r, op } => todo!(),
        }
    }
}

impl ast::Type {
    pub fn lower(
        &self,
        ctx: &mut Context,
        module: ModuleKey,
        generic_context: &mut GenericContext,
    ) -> AnyTypeKey {
        let Self { literal } = &self;
        match &literal.inner {
            ast::TypeLiteral::Path(identifier_path, generics) => {
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
                let path: Vec<SmolStr> = identifier_path
                    .path
                    .iter()
                    .map(|p| p.inner.clone())
                    .collect();
                let resolved = ctx.resolve_const_path(&path, todo!(), generic_context);
                match generics {
                    Some(generics) => {
                        for generic in &generics.inner {
                            let _ = generic.lower(ctx, module, generic_context);
                        }
                        todo!()
                    }
                    None => {
                        todo!("resolve actual path the usual way")
                    }
                }
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
            ast::TypeLiteral::Enum(variants) => {
                todo!()
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
