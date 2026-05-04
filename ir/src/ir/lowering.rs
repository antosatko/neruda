use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use arena::Key;
use smol_str::{SmolStr, ToSmolStr};

use crate::ast::{self, ConstValue, Number, NumberValue, SpanIndex};
use crate::ir::objects::{AnyObject, AnyObjectData, AnyObjectkey, FunctionData, InitState, Module};
use crate::ir::types::{
    AnyTypeKey, ArrayType, ConstraintKey, ConstraintType, EnumType, ModuleKey, ModuleTag,
    PrimitiveType, StructType, TraitType, TupleType,
};
use crate::ir::{Context, Diagnostic, Error, Errors};

#[derive(Default)]
pub struct GenericContext {
    scopes: Vec<Vec<(SmolStr, ConstraintKey)>>,
}

impl Context {
    pub(crate) fn lower_import_stage(&mut self) -> Result<(), Error> {
        let map: HashMap<Vec<SmolStr>, Key<ModuleTag>> =
            HashMap::from_iter(self.ast.iter().map(|(k, ast)| {
                let mut module = Module::new(Arc::clone(ast));
                module.path = k.clone();
                let type_key = self.types.modules.push(module);
                (k.clone(), type_key)
            }));

        for (key, module) in &self.ast {
            let module_key = map.get(key).unwrap();
            let ir_module = self.types.modules.get_mut_unchecked(module_key);
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
                        let obj = AnyObject::new(ident.clone(), obj, obj_key, *module_key);
                        let key = self.objects.push(obj);
                        ir_module.symbol_map.insert(ident, key);
                        continue;
                    }
                    ast::Object::Scheduler { .. } => continue,
                    ast::Object::Function(ast::Function { ident, .. }) => (
                        ident,
                        AnyObjectData::Function(FunctionData {
                            return_type: InitState::Uninitialized,
                            params: HashMap::new(),
                            generics: Vec::new(),
                        }),
                    ),
                    ast::Object::Component { ident, .. } => (
                        ident,
                        AnyObjectData::Component {
                            ty: InitState::Uninitialized,
                        },
                    ),
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
                            ty: InitState::Progress(unsafe { self.types.named.empty_alloc() }),
                            generics: Vec::new(),
                        },
                    ),
                    ast::Object::System { .. } => continue,
                    ast::Object::Const { ident, .. } => (
                        ident,
                        AnyObjectData::Const {
                            value: InitState::Uninitialized,
                            ty: InitState::Uninitialized,
                        },
                    ),
                };
                let ident = ident.inner.as_ref();
                let obj = AnyObject::new(ident.clone(), obj, obj_key, *module_key);
                let key = self.objects.push(obj);
                ir_module.symbol_map.insert(ident.clone(), key);
            }
        }
        Ok(())
    }

    pub fn lower_const_stage(&mut self) -> Result<(), Error> {
        let obj_keys: Vec<AnyObjectkey> = self.objects.iter_keys().collect();

        for obj_key in &obj_keys {
            let mut generic_ctx = GenericContext::default();
            let obj = self.objects.get_unchecked(obj_key);
            let mod_key = obj.module;
            let module = self.types.modules.get_unchecked(&mod_key);

            match self
                .ast
                .get(&module.path)
                .unwrap()
                .objects
                .get_unchecked(&obj.ast_object)
                .clone()
                .deref()
            {
                ast::Object::Type {
                    ident,
                    generics,
                    ty,
                    docs: _,
                } => {
                    generic_ctx.push_scope(&generics, self, &mod_key)?;
                    let obj = self.objects.get_mut_unchecked(obj_key);
                    let named_key = if let InitState::Done(ty) | InitState::Progress(ty) =
                        obj.type_state_mut_eager()
                    {
                        *ty
                    } else {
                        unreachable!("type should already be prepared")
                    };
                    self.types.named.get_mut_unchecked(&named_key).name = ident.inner.to_smolstr();
                    let key = match ty {
                        Some(t) => t.lower(self, mod_key, &mut generic_ctx)?,
                        _ => AnyTypeKey::Primitive(PrimitiveType::Void),
                    };
                    self.types.named.get_mut_unchecked(&named_key).repr = key;
                    let obj = self.objects.get_mut_unchecked(obj_key);
                    if let AnyObjectData::TypeAlias { ty, generics } = &mut obj.data {
                        ty.mark_done();
                        generics.extend(generic_ctx.scopes.last().unwrap().iter().cloned());
                    }
                    generic_ctx.pop_scope();
                }
                ast::Object::Component {
                    ty,
                    docs: _,
                    ident: _,
                } => {
                    let ty = match ty {
                        Some(ty) => ty.lower(self, mod_key, &mut generic_ctx)?,
                        None => AnyTypeKey::Primitive(PrimitiveType::Void),
                    };
                    let obj = self.objects.get_mut_unchecked(obj_key);
                    *obj.type_state_mut() = InitState::Done(ty);
                }
                ast::Object::Function(ast::Function {
                    ident: _,
                    generics,
                    parameters,
                    return_type,
                    body: _,
                    docs: _,
                }) => {
                    generic_ctx.push_scope(&generics, self, &mod_key)?;
                    let obj = self.objects.get_mut_unchecked(obj_key);
                    if let AnyObjectData::Function(fun) = &mut obj.data {
                        fun.return_type = InitState::Uninitialized
                    }
                    let return_type = match return_type {
                        Some(ty) => ty.lower(self, mod_key, &mut generic_ctx)?,
                        None => AnyTypeKey::Primitive(PrimitiveType::Void),
                    };
                    let obj = self.objects.get_mut_unchecked(obj_key);
                    if let AnyObjectData::Function(fun) = &mut obj.data {
                        fun.return_type = InitState::Done(return_type)
                    }
                    let mut params = HashMap::new();
                    for param in parameters {
                        let ident = param.ident.inner.deref().clone();
                        let ty = param.ty.lower(self, mod_key, &mut generic_ctx)?;
                        params.insert(ident, InitState::Done(ty));
                    }
                    let obj = self.objects.get_mut_unchecked(obj_key);
                    if let AnyObjectData::Function(fun) = &mut obj.data {
                        fun.params = params;
                    }

                    generic_ctx.pop_scope();
                }
                ast::Object::Const {
                    docs: _,
                    ident: _,
                    ty,
                    expression,
                } => {
                    self.lower_const(mod_key, &mut generic_ctx, obj_key, ty, expression)?;
                }
                _ => (),
            }
        }
        Ok(())
    }

    fn lower_const(
        &mut self,
        mod_key: Key<ModuleTag>,
        generic_ctx: &mut GenericContext,
        obj_key: &AnyObjectkey,
        ty: &ast::Span<ast::Type>,
        expression: &ast::Span<ast::Expression>,
    ) -> Result<(), Error> {
        let type_key = ty.lower(self, mod_key, generic_ctx)?;
        let obj = self.objects.get_mut_unchecked(obj_key);
        *obj.type_state_mut() = InitState::Done(type_key);
        let mut v = match expression.const_eval(self, mod_key, generic_ctx) {
            Ok(v) => v,
            Err(e) => Err(e)?,
        };
        if !self.type_check_const_value(&mut v, &type_key) {
            return Err(Diagnostic {
                span: expression.location,
                module: mod_key,
                inner: Errors::TypeMismatch {
                    expected: type_key,
                    got: AnyTypeKey::Primitive(v.type_of()),
                },
            });
        }
        let obj = self.objects.get_mut_unchecked(obj_key);
        match &mut obj.data {
            AnyObjectData::Const { value, .. } => *value = InitState::Done(v),
            _ => (),
        }
        Ok(())
    }

    pub fn resolve_const_path(
        &mut self,
        path: &[SmolStr],
        mod_key: ModuleKey,
        generic_context: &mut GenericContext,
        span: SpanIndex,
    ) -> Result<AnyObjectkey, Error> {
        if path.len() == 1 {
            match generic_context.find_generic(&path[0]) {
                Some(_) => todo!("toznam"),
                None => (),
            }
        }
        let mut current_mod_key = mod_key;
        for (i, next_stop) in path.iter().enumerate() {
            let is_last = i == path.len() - 1;
            let module = self.types.modules.get_unchecked(&current_mod_key);
            match module
                .symbol_map
                .get(next_stop)
                .map(|k| (k, self.objects.get_unchecked(k)))
            {
                Some((k, obj)) => match &obj.data {
                    AnyObjectData::Import { module } => current_mod_key = *module,
                    _ if is_last && next_stop == &obj.identifier => {
                        return Ok(k.clone());
                    }
                    _ => Err(Diagnostic {
                        inner: Errors::ObjectNotFound(path[..i].to_vec()),
                        span: module.ast.objects.get_unchecked(&obj.ast_object).location,
                        module: mod_key,
                    })?,
                },
                None => {
                    return Err(Diagnostic {
                        inner: Errors::ObjectNotFound(path[..i].to_vec()),
                        span,
                        module: mod_key,
                    })?;
                }
            }
        }
        Err(Diagnostic {
            inner: Errors::ObjectNotFound(path.to_vec()),
            span,
            module: mod_key,
        })
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
    ) -> Result<(), Error> {
        if let Some(generics) = generics {
            let mut scope = Vec::new();
            for generic in generics.inner.deref() {
                let mut constraints = Vec::new();
                for constr_path in &generic.constraints {
                    let module = ctx.types.modules.get_mut_unchecked(mod_key);
                    let obj_key = module
                        .symbol_map
                        .get(constr_path.path.first().as_ref().unwrap().inner.as_ref())
                        .unwrap()
                        .clone();
                    let obj = ctx.objects.get_unchecked(&obj_key);
                    let ty = match &obj.data {
                        AnyObjectData::Trait {
                            ty: InitState::Done(ty) | InitState::Progress(ty),
                        } => ty,
                        _ => {
                            return Err(Diagnostic {
                                inner: Errors::NonConstraintType(obj_key),
                                span: generic.identifier.location,
                                module: *mod_key,
                            });
                        }
                    };
                    constraints.push(*ty);
                }
                let key = match constraints.is_empty() {
                    true => ctx.auto_types.any_conr,
                    false => ctx.types.constraints.push(ConstraintType { constraints }),
                };
                scope.push((generic.identifier.inner.as_ref().clone(), key));
            }
            self.scopes.push(scope);
        } else {
            self.scopes.push(Vec::new());
        }
        Ok(())
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
        mod_key: ModuleKey,
        generic_context: &mut GenericContext,
    ) -> Result<ConstValue, Error> {
        match self {
            ast::Expression::Value(value) => {
                if !value.postfix.is_empty() {
                    return Err(Diagnostic {
                        span: value.location,
                        inner: Errors::NotConst,
                        module: mod_key,
                    });
                }
                match value.literal.inner.as_ref() {
                    ast::Literal::Identifier(identifier_path) => {
                        let path: Vec<SmolStr> = identifier_path
                            .path
                            .iter()
                            .map(|p| p.inner.as_ref().clone())
                            .collect();
                        match ctx.resolve_const_path(
                            &path,
                            mod_key,
                            generic_context,
                            identifier_path.location,
                        ) {
                            Ok(key) => {
                                let obj = ctx.objects.get_unchecked(&key);
                                match &obj.data {
                                    AnyObjectData::Const {
                                        value: InitState::Done(v),
                                        ty: _,
                                    } => return Ok(v.clone()),
                                    AnyObjectData::Const {
                                        value: InitState::Progress(_),
                                        ty: _,
                                    } => {
                                        panic!("we do not like circles around here")
                                    }
                                    AnyObjectData::Const { .. } => {
                                        let module = ctx.types.modules.get_unchecked(&mod_key);
                                        let (ty, expression) = match module
                                            .ast
                                            .objects
                                            .get_unchecked(&obj.ast_object)
                                            .inner
                                            .as_ref()
                                        {
                                            ast::Object::Const { ty, expression, .. } => {
                                                (ty.clone(), expression.clone())
                                            }
                                            _ => panic!(),
                                        };
                                        ctx.lower_const(
                                            mod_key,
                                            generic_context,
                                            &key,
                                            &ty,
                                            &expression,
                                        )
                                        .unwrap();
                                        let obj = ctx.objects.get_unchecked(&key);
                                        match &obj.data {
                                            AnyObjectData::Const {
                                                value: InitState::Done(v),
                                                ..
                                            } => Ok(v.clone()),
                                            _ => unreachable!(),
                                        }
                                    }
                                    _ => panic!("nope"),
                                }
                            }
                            Err(e) => return Err(e),
                        }
                    }
                    ast::Literal::Structure(_, _) => todo!(),
                    ast::Literal::Number(number) => Ok(ConstValue::Number(number.clone())),
                    ast::Literal::String(smol_str) => Ok(ConstValue::String(smol_str.clone())),
                    ast::Literal::Char(c) => Ok(ConstValue::Char(*c)),
                    ast::Literal::Array(_) => todo!(),
                    ast::Literal::Tuple(_) => todo!(),
                }
            }
            ast::Expression::Binary { l, r, op } => {
                let span = SpanIndex {
                    index: l.location.index,
                    len: r.location.len + r.location.index - l.location.index,
                };
                let l = l.const_eval(ctx, mod_key, generic_context)?;
                let r = r.const_eval(ctx, mod_key, generic_context)?;
                match op.const_apply(&l, &r, mod_key) {
                    Ok(v) => Ok(v),
                    Err(mut e) => {
                        e.span = span;
                        Err(e)
                    }
                }
            }
        }
    }
}

impl ast::Type {
    pub fn lower(
        &self,
        ctx: &mut Context,
        module: ModuleKey,
        generic_context: &mut GenericContext,
    ) -> Result<AnyTypeKey, Error> {
        let Self { literal, refs } = &self;
        if *refs.inner > 0 {
            todo!("do refs lamo")
        }
        match literal.inner.as_ref() {
            ast::TypeLiteral::Path(identifier_path, generic_arguments) => {
                if identifier_path.path.len() == 1
                    && let Some(ident) = identifier_path.path.first()
                {
                    if let Some(ty) = PrimitiveType::from_str(&ident.inner) {
                        return Ok(AnyTypeKey::Primitive(ty));
                    }
                    if let Some(ty) = generic_context.find_generic(ident) {
                        return Ok(AnyTypeKey::Constraint(*ty));
                    }
                }
                let path: Vec<SmolStr> = identifier_path
                    .path
                    .iter()
                    .map(|p| p.inner.as_ref().clone())
                    .collect();
                let resolved = ctx.resolve_const_path(
                    &path,
                    module,
                    generic_context,
                    identifier_path.location,
                )?;
                match &ctx.objects.get_unchecked(&resolved).data {
                    AnyObjectData::TypeAlias {
                        ty: InitState::Done(ty) | InitState::Progress(ty),
                        generics,
                    } => {
                        let ty = *ty;
                        let mut new = AnyTypeKey::Named(ty);
                        let gen_args = match generic_arguments {
                            Some(g) => g.inner.as_ref().clone(),
                            None => Vec::new(),
                        };
                        for (idx, (_, constraint)) in generics.clone().iter().enumerate() {
                            let substitution = gen_args[idx].lower(ctx, module, generic_context)?;
                            new = AnyTypeKey::Named(ty).substitute_named(
                                substitution,
                                constraint,
                                &mut ctx.types,
                            )?;
                        }
                        Ok(new)
                    }
                    _ => unreachable!("all paths are expected to end with a type alias"),
                }
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
                        param.ty.lower(ctx, module, generic_context)?,
                    ));
                }
                let key = ctx.types.structures.push_unique(StructType { parameters });
                Ok(AnyTypeKey::Struct(key))
            }
            ast::TypeLiteral::Enum(repr, ast_variants) => {
                let repr = match repr {
                    Some(repr) => match repr.lower(ctx, module, generic_context)? {
                        AnyTypeKey::Primitive(prim) => prim,
                        _ => PrimitiveType::I32,
                    },
                    None => PrimitiveType::I32,
                };
                let mut variants = Vec::new();
                for (ident, _) in ast_variants {
                    variants.push((ident.inner.as_ref().clone(), ConstValue::Bool(true)));
                }
                let key = ctx.types.enums.push_unique(EnumType { repr, variants });
                Ok(AnyTypeKey::Enum(key))
            }
            ast::TypeLiteral::Array(span, size) => {
                let ty = span.lower(ctx, module, generic_context)?;
                let size = match size {
                    Some(const_expr) => match const_expr.const_eval(ctx, module, generic_context) {
                        Ok(ConstValue::Number(n)) => match n.value {
                            NumberValue::Uint(n) | NumberValue::Any(n) => Some(n as _),
                            _ => todo!(),
                        },
                        Err(e) => Err(e)?,
                        _ => todo!(),
                    },
                    None => None,
                };
                let key = ctx.types.arrays.push_unique(ArrayType {
                    element_type: ty,
                    size: size,
                });
                Ok(AnyTypeKey::Array(key))
            }
            ast::TypeLiteral::Tuple(spans) => {
                let mut parameters = Vec::new();
                for ty in spans {
                    parameters.push(ty.lower(ctx, module, generic_context)?);
                }
                match parameters.is_empty() {
                    true => Ok(AnyTypeKey::Primitive(PrimitiveType::Void)),
                    _ => {
                        let key = ctx.types.tuples.push_unique(TupleType { parameters });
                        Ok(AnyTypeKey::Tuple(key))
                    }
                }
            }
        }
    }
}
