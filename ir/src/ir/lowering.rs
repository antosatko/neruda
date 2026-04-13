use std::collections::HashMap;
use std::ops::Deref;
use std::rc::Rc;
use std::sync::Arc;

use arena::Key;
use smol_str::SmolStr;

use crate::ast::{self, ConstValue, Number, NumberValue};
use crate::ir::Context;
use crate::ir::objects::{AnyObject, AnyObjectData, AnyObjectkey, InitState, Module};
use crate::ir::types::{
    AnyTypeKey, ArrayType, ConstraintKey, ConstraintType, EnumType, ModuleKey, ModuleTag,
    PrimitiveType, StructType, TraitType, TupleType,
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
            for (obj_key, obj) in module.objects.iter_pairs() {
                let (ident, obj) = match obj.inner.as_ref() {
                    ast::Object::Import { ident, alias } => {
                        let key: Vec<SmolStr> = ident
                            .inner
                            .path
                            .iter()
                            .map(|a| a.inner.as_ref().clone())
                            .collect();
                        let ident = match &alias.0 {
                            Some(name) => name.inner.inner.as_ref().clone(),
                            None => ident.inner.path.last().unwrap().inner.as_ref().clone(),
                        };
                        let ty_key = match map.get(&key) {
                            Some(k) => *k,
                            None => todo!("lamo"),
                        };
                        let obj = AnyObjectData::Import { module: ty_key };
                        let obj = AnyObject::new(ident.clone(), obj, obj_key);
                        let key = ir_module.objects.push(obj);
                        ir_module.symbol_map.insert(ident, key);
                        continue;
                    }
                    ast::Object::Scheduler { ident, .. } => continue,
                    ast::Object::Function(function) => continue,
                    ast::Object::Component { ident, .. } => continue,
                    ast::Object::TypeImpl { .. } => continue,
                    ast::Object::TraitImpl { .. } => continue,
                    ast::Object::Trait { ident, .. } => (
                        ident,
                        AnyObjectData::Trait {
                            ty: InitState::Progress(self.types.traits.push(TraitType {
                                ident: ident.inner.as_ref().clone(),
                            })),
                        },
                    ),
                    ast::Object::Type { ident, .. } => (
                        ident,
                        AnyObjectData::TypeAlias {
                            ty: InitState::Uninitialized,
                            generics: Vec::new(),
                        },
                    ),
                    ast::Object::System { ident, .. } => continue,
                    ast::Object::Const { ident, .. } => (
                        ident,
                        AnyObjectData::Const {
                            value: InitState::Uninitialized,
                            ty: InitState::Uninitialized,
                        },
                    ),
                };
                let ident = ident.inner.as_ref();
                let obj = AnyObject::new(ident.clone(), obj, obj_key);
                let key = ir_module.objects.push(obj);
                ir_module.symbol_map.insert(ident.clone(), key);
            }
        }
    }

    pub fn lower_const_stage(&mut self) {
        for mod_key in self.types.modules.iter_keys().collect::<Vec<ModuleKey>>() {
            let mut generic_ctx = GenericContext::default();
            let mod_type = self.types.modules.get_unchecked(&mod_key);
            let mod_ast = Arc::clone(self.ast.get(&mod_type.path).unwrap());

            let obj_keys: Vec<AnyObjectkey> = mod_type.objects.iter_keys().collect();

            for obj_key in &obj_keys {
                let module = self.types.modules.get_mut_unchecked(&mod_key);
                let obj = module.objects.get_unchecked(obj_key);

                match mod_ast.objects.get_unchecked(&obj.ast_object).deref() {
                    ast::Object::Type {
                        ident: _,
                        generics,
                        ty,
                        docs: _,
                    } => {
                        generic_ctx.push_scope(&generics, self, &mod_key);
                        let obj = self.obj_mut(mod_key, obj_key);
                        *obj.type_state_mut() = InitState::Progress(());
                        let key = ty
                            .as_ref()
                            .map(|t| t.lower(self, mod_key, &mut generic_ctx))
                            .unwrap_or(AnyTypeKey::Primitive(PrimitiveType::Void));
                        let obj = self.obj_mut(mod_key, obj_key);
                        *obj.type_state_mut() = InitState::Done(key);
                        generic_ctx.pop_scope();
                    }
                    ast::Object::Const {
                        docs: _,
                        ident: _,
                        ty,
                        expression,
                    } => {
                        let type_key = ty.lower(self, mod_key, &mut generic_ctx);
                        let obj = self.obj_mut(mod_key, obj_key);
                        *obj.type_state_mut() = InitState::Done(type_key);
                        let mut v = expression.const_eval(self, &mut generic_ctx).unwrap();
                        if !self.type_check_const_value(&mut v, &type_key) {
                            panic!("lala mas to blby")
                        }
                        let obj = self.obj_mut(mod_key, obj_key);
                        match &mut obj.data {
                            AnyObjectData::Const { value, .. } => *value = InitState::Done(v),
                            _ => (),
                        }
                    }
                    _ => (),
                }
            }
        }
    }

    fn obj_mut(
        &mut self,
        mod_key: Key<ModuleTag>,
        obj_key: &Key<super::objects::AnyObjectTag>,
    ) -> &mut AnyObject {
        let module = self.types.modules.get_mut_unchecked(&mod_key);
        let obj = module.objects.get_mut_unchecked(obj_key);
        obj
    }

    pub fn resolve_const_path(
        &mut self,
        path: &[SmolStr],
        mut module: ModuleKey,
        generic_context: &mut GenericContext,
    ) -> Option<&AnyObject> {
        for next_stop in path {
            /*let current = self.types.modules.get_mut_unchecked(&module);
            let next = match current.objects.get(next_stop) {
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
                            current.objects.insert(ident.inner.clone(), obj);
                            return current.objects.get(&ident.inner);
                        }
                        ast::Object::Type {
                            ident: _,
                            generics,
                            ty,
                            docs,
                        } => {
                            generic_context.push_scope(&generics, self);
                            let key = ty
                                .as_ref()
                                .map(|t| t.lower(self, module, generic_context))
                                .unwrap_or(AnyTypeKey::Primitive(PrimitiveType::Void));
                            let obj = AnyObject::TypeAlias {
                                ty: key,
                                generics: Vec::new(),
                            };
                            self.types
                                .modules
                                .get_mut_unchecked(&module)
                                .objects
                                .insert(next_stop.clone(), obj);
                            generic_context.pop_scope();
                            todo!()
                        }
                        _ => todo!(),
                    },
                    None => return None,
                },
            };*/
        }
        None
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
    fn type_check_const_value(&self, value: &mut ConstValue, ty: &AnyTypeKey) -> bool {
        match (ty, value) {
            (AnyTypeKey::Primitive(PrimitiveType::Char), ConstValue::Char(_)) => true,
            (
                AnyTypeKey::Primitive(ty),
                ConstValue::Number(Number {
                    size: None,
                    value: NumberValue::Any(_),
                }),
            ) if ty.is_numeric() => true,
            (
                AnyTypeKey::Primitive(ty),
                ConstValue::Number(Number {
                    size,
                    value: NumberValue::Any(_),
                }),
            ) if ty.number_size() == *size => true,

            (
                AnyTypeKey::Primitive(ty),
                ConstValue::Number(Number {
                    size,
                    value: NumberValue::Float(_),
                }),
            ) if *size == None => match ty.float_size() {
                Some(s) => {
                    *size = Some(s);
                    true
                }
                None => false,
            },
            (
                AnyTypeKey::Primitive(ty),
                ConstValue::Number(Number {
                    size: Some(size),
                    value: NumberValue::Float(_),
                }),
            ) => match ty.float_size() {
                Some(s) => *size == s,
                None => false,
            },

            (
                AnyTypeKey::Primitive(ty),
                ConstValue::Number(Number {
                    size,
                    value: NumberValue::Int(_),
                }),
            ) if *size == None => match ty.float_size() {
                Some(s) => {
                    *size = Some(s);
                    true
                }
                None => false,
            },
            (
                AnyTypeKey::Primitive(ty),
                ConstValue::Number(Number {
                    size: Some(size),
                    value: NumberValue::Int(_),
                }),
            ) => match ty.float_size() {
                Some(s) => *size == s,
                None => false,
            },

            (
                AnyTypeKey::Primitive(ty),
                ConstValue::Number(Number {
                    size,
                    value: NumberValue::Uint(_),
                }),
            ) if *size == None => match ty.float_size() {
                Some(s) => {
                    *size = Some(s);
                    true
                }
                None => false,
            },
            (
                AnyTypeKey::Primitive(ty),
                ConstValue::Number(Number {
                    size: Some(size),
                    value: NumberValue::Uint(_),
                }),
            ) => match ty.float_size() {
                Some(s) => *size == s,
                None => false,
            },
            (AnyTypeKey::Primitive(ty), ConstValue::Number(_)) if ty.is_numeric() => true,
            _ => false,
        }
    }
}

impl GenericContext {
    fn push_scope(
        &mut self,
        generics: &Option<ast::Span<Vec<ast::Span<ast::GenericParameter>>>>,
        ctx: &mut Context,
        mod_key: &ModuleKey,
    ) {
        if let Some(generics) = generics {
            let mut scope = Vec::new();
            for generic in generics.inner.deref() {
                let mut constraints = Vec::new();
                for constr_path in &generic.constraints {
                    let module = ctx.types.modules.get_mut_unchecked(mod_key);
                    let obj_key = module
                        .symbol_map
                        .get(constr_path.path.first().as_ref().unwrap().inner.as_ref())
                        .unwrap();
                    let obj = module.objects.get_unchecked(obj_key);
                    let ty = match &obj.data {
                        AnyObjectData::Trait {
                            ty: InitState::Done(ty) | InitState::Progress(ty),
                        } => ty,
                        _ => panic!("nějak si to vyřiď"),
                    };
                    constraints.push(*ty);
                }
                let key = match constraints.is_empty() {
                    true => ctx.auto_types.any_conr,
                    false => ctx
                        .types
                        .constraints
                        .push_unique(ConstraintType { constraints }),
                };
                scope.push((generic.identifier.inner.as_ref().clone(), key));
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
                Some(match value.literal.inner.as_ref() {
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
        match literal.inner.as_ref() {
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
                    .map(|p| p.inner.as_ref().clone())
                    .collect();
                /*let resolved = ctx
                    .resolve_const_path(&path, module, generic_context)
                    .unwrap();
                match generics {
                    Some(generics) => {
                        for generic in generics.inner.as_ref() {
                            let _ = generic.lower(ctx, module, generic_context);
                        }
                        todo!()
                    }
                    None => {
                        todo!("resolve actual path the usual way")
                    }
                }*/
                AnyTypeKey::Primitive(PrimitiveType::Void)
            }
            ast::TypeLiteral::Struct(spans) => {
                let mut parameters = Vec::new();
                for param in spans {
                    let ident = &param.ident.inner;
                    if parameters
                        .iter()
                        .any(|(p_ident, _)| p_ident == ident.as_ref())
                    {
                        panic!("duplicate parameter identifiers")
                    }
                    parameters.push((
                        ident.as_ref().clone(),
                        param.ty.lower(ctx, module, generic_context),
                    ));
                }
                let key = ctx.types.structures.push_unique(StructType { parameters });
                AnyTypeKey::Struct(key)
            }
            ast::TypeLiteral::Enum(repr, ast_variants) => {
                let repr = match repr {
                    Some(repr) => match repr.lower(ctx, module, generic_context) {
                        AnyTypeKey::Primitive(prim) => prim,
                        _ => PrimitiveType::I32,
                    },
                    None => PrimitiveType::I32,
                };
                let mut variants = Vec::new();
                for (ident, expr) in ast_variants {
                    variants.push((ident.inner.as_ref().clone(), ConstValue::Bool(true)));
                }
                let key = ctx.types.enums.push_unique(EnumType { repr, variants });
                AnyTypeKey::Enum(key)
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
