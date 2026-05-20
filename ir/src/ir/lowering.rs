use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use arena::Key;
use smol_str::{SmolStr, ToSmolStr};

use crate::ast::{self, ConstValue, Number, NumberValue, SpanIndex};
use crate::ir::objects::{
    AnyObject, AnyObjectKey, ComponentObj, ComponentObjKey, ConstObj, ConstObjKey, FunctionObj,
    ImportObj, InitState, Module, TraitObj, TypeAliasObj, TypeAliasObjKey,
};
use crate::ir::types::{
    AnyTypeKey, ArrayType, ConstraintKey, ConstraintType, EnumType, ModuleKey, ModuleTag,
    PrimitiveType, RefType, StructType, TraitType, TupleType,
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
                let (ident, key): (SmolStr, AnyObjectKey) = match obj.inner.as_ref() {
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
                        let obj = ImportObj { module: ty_key };
                        let obj = AnyObject::new(ident.clone(), obj, obj_key, *module_key);
                        let key = self.objects.imports.push(obj);
                        ir_module
                            .symbol_map
                            .insert(ident, AnyObjectKey::Import(key));
                        continue;
                    }
                    ast::Object::Scheduler { .. } => continue,
                    ast::Object::Function(ast::Function { ident, .. }) => (
                        ident.inner.as_ref().clone(),
                        self.objects
                            .functions
                            .push(AnyObject {
                                data: FunctionObj {
                                    return_type: InitState::Uninitialized,
                                    params: HashMap::new(),
                                    generics: Vec::new(),
                                },
                                identifier: ident.inner.as_ref().clone(),
                                ast_object: obj_key,
                                module: *module_key,
                            })
                            .into(),
                    ),
                    ast::Object::Component { ident, .. } => (
                        ident.inner.as_ref().clone(),
                        self.objects
                            .components
                            .push(AnyObject {
                                data: ComponentObj {
                                    ty: InitState::Uninitialized,
                                },
                                identifier: ident.inner.as_ref().clone(),
                                ast_object: obj_key,
                                module: *module_key,
                            })
                            .into(),
                    ),
                    ast::Object::TypeImpl { .. } => continue,
                    ast::Object::TraitImpl { .. } => continue,
                    ast::Object::Trait { ident, .. } => (
                        ident.inner.as_ref().clone(),
                        self.objects
                            .traits
                            .push(AnyObject {
                                data: TraitObj {
                                    ty: InitState::Progress(self.types.traits.push(TraitType {
                                        ident: ident.inner.as_ref().clone(),
                                    })),
                                },
                                identifier: ident.inner.as_ref().clone(),
                                ast_object: obj_key,
                                module: *module_key,
                            })
                            .into(),
                    ),
                    ast::Object::Type { ident, .. } => (
                        ident.inner.as_ref().clone(),
                        self.objects
                            .types
                            .push(AnyObject {
                                data: TypeAliasObj {
                                    ty: InitState::Progress(unsafe {
                                        self.types.named.empty_alloc()
                                    }),
                                    generics: Vec::new(),
                                    constants: HashMap::new(),
                                },
                                identifier: ident.inner.as_ref().clone(),
                                ast_object: obj_key,
                                module: *module_key,
                            })
                            .into(),
                    ),
                    ast::Object::System { .. } => continue,
                    ast::Object::Const { ident, .. } => (
                        ident.inner.as_ref().clone(),
                        self.objects
                            .constants
                            .push(AnyObject {
                                data: ConstObj {
                                    value: InitState::Uninitialized,
                                    ty: InitState::Uninitialized,
                                },
                                identifier: ident.inner.as_ref().clone(),
                                ast_object: obj_key,
                                module: *module_key,
                            })
                            .into(),
                    ),
                };
                ir_module.symbol_map.insert(ident, key);
            }
        }
        Ok(())
    }

    pub fn lower_const_stage(&mut self) -> Result<(), Error> {
        let mut generic_ctx = GenericContext::default();
        let type_keys: Vec<TypeAliasObjKey> = self.objects.types.iter_keys().collect();
        for type_key in type_keys {
            self.lower_type_alias_with_key(&mut generic_ctx, type_key)?;
        }

        let const_keys: Vec<ConstObjKey> = self.objects.constants.iter_keys().collect();
        for const_key in const_keys {
            self.lower_const_with_key(&mut generic_ctx, const_key)?;
        }

        let component_keys: Vec<ComponentObjKey> = self.objects.components.iter_keys().collect();
        for component_key in component_keys {
            self.lower_component_with_key(&mut generic_ctx, component_key)?;
        }
        /*
        let obj_keys: Vec<AnyObjectKey> = self.objects.iter_keys().collect();

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
                    if let FunctionObj(fun) = &mut obj.data {
                        fun.return_type = InitState::Uninitialized
                    }
                    let return_type = match return_type {
                        Some(ty) => ty.lower(self, mod_key, &mut generic_ctx)?,
                        None => AnyTypeKey::Primitive(PrimitiveType::Void),
                    };
                    let obj = self.objects.get_mut_unchecked(obj_key);
                    if let FunctionObj(fun) = &mut obj.data {
                        fun.return_type = InitState::Done(return_type)
                    }
                    let mut params = HashMap::new();
                    for param in parameters {
                        let ident = param.ident.inner.deref().clone();
                        let ty = param.ty.lower(self, mod_key, &mut generic_ctx)?;
                        params.insert(ident, InitState::Done(ty));
                    }
                    let obj = self.objects.get_mut_unchecked(obj_key);
                    if let FunctionObj(fun) = &mut obj.data {
                        fun.params = params;
                    }

                    generic_ctx.pop_scope();
                }
                _ => (),
            }
        }*/
        Ok(())
    }

    fn lower_component_with_key(
        &mut self,
        generic_ctx: &mut GenericContext,
        component_key: Key<super::objects::ComponentObjTag>,
    ) -> Result<(), Diagnostic<Errors>> {
        let this = self.objects.components.get_unchecked(&component_key);
        let ast_key = this.ast_object;
        let mod_key = this.module;
        let module_path = &self.types.modules.get_unchecked(&mod_key).path;
        Ok(
            if let ast::Object::Component {
                ty,
                docs: _,
                ident: _,
            } = self
                .ast
                .get(module_path)
                .unwrap()
                .objects
                .get_unchecked(&ast_key)
                .clone()
                .deref()
            {
                let ty = match ty {
                    Some(ty) => ty.lower(self, mod_key, generic_ctx)?,
                    None => AnyTypeKey::Primitive(PrimitiveType::Void),
                };
                let obj = self.objects.components.get_mut_unchecked(&component_key);
                obj.data.ty = InitState::Done(ty);
            },
        )
    }

    fn lower_const_with_key(
        &mut self,
        generic_ctx: &mut GenericContext,
        const_key: Key<super::objects::ConstObjTag>,
    ) -> Result<&mut AnyObject<ConstObj>, Diagnostic<Errors>> {
        let this = self.objects.constants.get_unchecked(&const_key);
        let ast_key = this.ast_object;
        let mod_key = this.module;
        let module_path = &self.types.modules.get_unchecked(&mod_key).path;
        Ok(
            if let ast::Object::Const {
                docs: _,
                ident: _,
                ty,
                expression,
            } = self
                .ast
                .get(module_path)
                .unwrap()
                .objects
                .get_unchecked(&ast_key)
                .clone()
                .deref()
            {
                let this = &mut *self;
                let obj_key: &ConstObjKey = &const_key;
                let self_def: &Option<ConstValue> = &None;
                let type_key = ty.lower(this, mod_key, generic_ctx)?;
                let obj = this.objects.constants.get_mut_unchecked(obj_key);
                obj.data.ty = InitState::Done(type_key);
                let mut v = match expression.const_eval(this, mod_key, generic_ctx, self_def) {
                    Ok(v) => v,
                    Err(e) => Err(e)?,
                };
                if !this.type_check_const_value(&mut v, &type_key) {
                    return Err(Diagnostic {
                        span: expression.location,
                        module: mod_key,
                        inner: Errors::TypeMismatch {
                            expected: type_key,
                            got: AnyTypeKey::Primitive(v.type_of()),
                        },
                    });
                }
                let obj = this.objects.constants.get_mut_unchecked(obj_key);
                obj.data.value = InitState::Done(v);
                this.objects.constants.get_mut_unchecked(obj_key)
            } else {
                self.objects.constants.get_mut_unchecked(&const_key)
            },
        )
    }

    fn lower_type_alias_with_key(
        &mut self,
        generic_ctx: &mut GenericContext,
        type_key: Key<super::objects::TypeAliasObjTag>,
    ) -> Result<&mut AnyObject<TypeAliasObj>, Diagnostic<Errors>> {
        let this = self.objects.types.get_unchecked(&type_key);
        let ast_key = this.ast_object;
        let mod_key = this.module;
        let module_path = &self.types.modules.get_unchecked(&mod_key).path;
        Ok(
            if let ast::Object::Type {
                ident,
                generics,
                ty,
                docs: _,
            } = self
                .ast
                .get(module_path)
                .unwrap()
                .objects
                .get_unchecked(&ast_key)
                .clone()
                .deref()
            {
                {
                    if self
                        .objects
                        .types
                        .get_unchecked(&type_key)
                        .data
                        .ty
                        .is_done()
                    {
                        return Ok(self.objects.types.get_mut_unchecked(&type_key));
                    }
                    let this = &mut *self;
                    generic_ctx.push_scope(&generics, this, &mod_key)?;
                    let obj = this.objects.types.get_mut_unchecked(&type_key);
                    let named_key =
                        if let InitState::Done(ty) | InitState::Progress(ty) = &mut obj.data.ty {
                            *ty
                        } else {
                            unreachable!("type should already be prepared")
                        };
                    this.types.named.get_mut_unchecked(&named_key).name = obj.identifier.clone();
                    let key = match ty {
                        Some(t) => t.lower(this, mod_key, generic_ctx)?,
                        _ => AnyTypeKey::Primitive(PrimitiveType::Void),
                    };
                    match &key {
                        AnyTypeKey::Enum(key) => {
                            let ty = this.types.enums.get_unchecked(key);
                            for variant in &ty.variants {
                                let ident = variant.0.clone();
                                let const_obj = AnyObject {
                                    data: ConstObj {
                                        ty: InitState::Done(AnyTypeKey::Primitive(
                                            variant.1.type_of(),
                                        )),
                                        value: InitState::Done(variant.1.clone()),
                                    },
                                    ast_object: ast_key,
                                    identifier: ident.clone(),
                                    module: mod_key,
                                };
                                let obj_key = this.objects.constants.push(const_obj);
                                this.objects
                                    .types
                                    .get_mut_unchecked(&type_key)
                                    .data
                                    .constants
                                    .insert(ident, obj_key);
                            }
                        }
                        _ => (),
                    }
                    this.types.named.get_mut_unchecked(&named_key).repr = key;
                    let obj = this.objects.types.get_mut_unchecked(&type_key);
                    obj.data.ty.mark_done();
                    obj.data
                        .generics
                        .extend(generic_ctx.scopes.last().unwrap().iter().cloned());
                    generic_ctx.pop_scope();
                    self.objects.types.get_mut_unchecked(&type_key)
                }
            } else {
                unreachable!()
            },
        )
    }

    fn lower_const(
        &mut self,
        mod_key: Key<ModuleTag>,
        generic_ctx: &mut GenericContext,
        obj_key: &ConstObjKey,
        ty: &ast::Span<ast::Type>,
        expression: &ast::Span<ast::Expression>,
        self_def: &Option<ConstValue>,
    ) -> Result<(), Error> {
        let type_key = ty.lower(self, mod_key, generic_ctx)?;
        let obj = self.objects.constants.get_mut_unchecked(obj_key);
        obj.data.ty = InitState::Done(type_key);
        let mut v = match expression.const_eval(self, mod_key, generic_ctx, self_def) {
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
        let obj = self.objects.constants.get_mut_unchecked(obj_key);
        obj.data.value = InitState::Done(v);
        Ok(())
    }

    pub fn resolve_const_path(
        &mut self,
        path: &[SmolStr],
        mod_key: ModuleKey,
        generic_ctx: &mut GenericContext,
        span: SpanIndex,
    ) -> Result<AnyObjectKey, Error> {
        enum PathNode {
            Module(ModuleKey),
            Object(AnyObjectKey),
        }
        if path.len() == 1 {
            match generic_ctx.find_generic(&path[0]) {
                Some(_) => todo!("toznam"),
                None => (),
            }
        }
        let mut current_path_node = PathNode::Module(mod_key);
        for (i, next_stop) in path.iter().enumerate() {
            match &current_path_node {
                PathNode::Module(module) => {
                    let module = self.types.modules.get_unchecked(&module);
                    match module.symbol_map.get(next_stop) {
                        Some(AnyObjectKey::Import(obj_key)) => {
                            current_path_node = PathNode::Module(
                                self.objects.imports.get_unchecked(obj_key).data.module,
                            )
                        }
                        Some(any_key) => current_path_node = PathNode::Object(*any_key),
                        _ => Err(Diagnostic {
                            inner: Errors::ObjectNotFound(path[..i].to_vec()),
                            span,
                            module: mod_key,
                        })?,
                    }
                }
                PathNode::Object(obj_key) => match obj_key {
                    AnyObjectKey::Type(type_key) => {
                        println!("Add generics to const path");
                        let type_obj = self.lower_type_alias_with_key(generic_ctx, *type_key)?;
                        match type_obj.data.constants.get(next_stop) {
                            Some(v) => {
                                current_path_node = PathNode::Object(AnyObjectKey::Const(*v))
                            }
                            _ => Err(Diagnostic {
                                inner: Errors::ObjectNotFound(path[..i].to_vec()),
                                span,
                                module: mod_key,
                            })?,
                        }
                    }
                    _ => todo!(),
                },
            }
        }
        match current_path_node {
            PathNode::Module(_) => todo!("err out"),
            PathNode::Object(key) => Ok(key),
        }
    }

    fn type_check_const_value(&self, value: &mut ConstValue, ty: &AnyTypeKey) -> bool {
        match (ty, value) {
            (AnyTypeKey::Primitive(PrimitiveType::Char), ConstValue::Char(_)) => true,

            (AnyTypeKey::Primitive(prim), ConstValue::Number(Number { size, value })) => {
                match value {
                    NumberValue::Any(_) => match size {
                        None => prim.is_numeric(),
                        Some(s) => prim.number_size() == Some(*s),
                    },

                    NumberValue::Float(_) => match prim.float_size() {
                        Some(expected) => match size {
                            None => {
                                *size = Some(expected);
                                true
                            }
                            Some(actual) => *actual == expected,
                        },
                        None => false,
                    },

                    NumberValue::Int(_) => match prim.int_size() {
                        Some(expected) => match size {
                            None => {
                                *size = Some(expected);
                                true
                            }
                            Some(actual) => *actual == expected,
                        },
                        None => false,
                    },

                    NumberValue::Uint(_) => match prim.uint_size() {
                        Some(expected) => match size {
                            None => {
                                *size = Some(expected);
                                true
                            }
                            Some(actual) => *actual == expected,
                        },
                        None => false,
                    },
                }
            }

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
                    let trt_key = match obj_key {
                        AnyObjectKey::Trait(k) => k,
                        _ => todo!("no to jsem necekal"),
                    };
                    let ty = match ctx.objects.traits.get_unchecked(&trt_key).data {
                        TraitObj {
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
                    constraints.push(ty);
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
        generic_ctx: &mut GenericContext,
        self_def: &Option<ConstValue>,
    ) -> Result<ConstValue, Error> {
        match self.const_reduce().as_ref() {
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
                            generic_ctx,
                            identifier_path.location,
                        ) {
                            Ok(key) => {
                                let obj_key = if let AnyObjectKey::Const(key) = key {
                                    key
                                } else {
                                    panic!("neni constanta bum")
                                };
                                let obj = ctx.objects.constants.get_unchecked(&obj_key);
                                match &obj.data {
                                    ConstObj {
                                        value: InitState::Done(v),
                                        ty: _,
                                    } => return Ok(v.clone()),
                                    ConstObj {
                                        value: InitState::Progress(_),
                                        ty: _,
                                    } => {
                                        panic!("we do not like circles around here")
                                    }
                                    ConstObj { .. } => {
                                        let obj = ctx.lower_const_with_key(generic_ctx, obj_key)?;
                                        match &obj.data {
                                            ConstObj {
                                                value: InitState::Done(v),
                                                ..
                                            } => Ok(v.clone()),
                                            _ => unreachable!(),
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                if path == ["self"] {
                                    match self_def {
                                        Some(v) => Ok(v.clone()),
                                        None => todo!("self undefined"),
                                    }
                                } else {
                                    return Err(e);
                                }
                            }
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
                let l = l.const_eval(ctx, mod_key, generic_ctx, self_def)?;
                let r = r.const_eval(ctx, mod_key, generic_ctx, self_def)?;
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
        generic_ctx: &mut GenericContext,
    ) -> Result<AnyTypeKey, Error> {
        let Self { literal, refs } = &self;
        let type_val = match literal.inner.as_ref() {
            ast::TypeLiteral::Path(identifier_path, generic_arguments) => {
                if identifier_path.path.len() == 1
                    && let Some(ident) = identifier_path.path.first()
                {
                    if let Some(ty) = PrimitiveType::from_str(&ident.inner) {
                        AnyTypeKey::Primitive(ty)
                    } else if let Some(ty) = generic_ctx.find_generic(ident) {
                        AnyTypeKey::Constraint(*ty)
                    } else {
                        resolve_type_path(
                            ctx,
                            module,
                            generic_ctx,
                            identifier_path,
                            generic_arguments,
                        )?
                    }
                } else {
                    resolve_type_path(ctx, module, generic_ctx, identifier_path, generic_arguments)?
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
                        param.ty.lower(ctx, module, generic_ctx)?,
                    ));
                }
                let key = ctx.types.structures.push_unique(StructType { parameters });
                AnyTypeKey::Struct(key)
            }
            ast::TypeLiteral::Enum(repr, step, ast_variants) => {
                let repr = match repr {
                    Some(repr) => match repr.lower(ctx, module, generic_ctx)? {
                        AnyTypeKey::Primitive(prim) => prim,
                        _ => todo!("must be prim"),
                    },
                    None => PrimitiveType::I32,
                };
                let mut variants = Vec::new();
                let mut iter = ast_variants.iter();
                let mut last_value = match iter.next() {
                    Some((ident, expr)) => {
                        let value = match expr {
                            Some(e) => e.const_eval(ctx, module, generic_ctx, &None)?,
                            None => repr.default(),
                        };
                        variants.push((ident.inner.as_ref().clone(), value.clone()));
                        value
                    }
                    None => panic!("handle pls"),
                };
                for (ident, expr) in iter {
                    let value = match expr {
                        Some(e) => e.const_eval(ctx, module, generic_ctx, &Some(last_value))?,
                        None => match step {
                            Some(step) => {
                                step.const_eval(ctx, module, generic_ctx, &Some(last_value))?
                            }
                            None => last_value.autostep(),
                        },
                    };
                    variants.push((ident.inner.as_ref().clone(), value.clone()));
                    last_value = value;
                }
                let key = ctx.types.enums.push_unique(EnumType { repr, variants });
                AnyTypeKey::Enum(key)
            }
            ast::TypeLiteral::Array(span, size) => {
                let ty = span.lower(ctx, module, generic_ctx)?;
                let size = match size {
                    Some(const_expr) => {
                        match const_expr.const_eval(ctx, module, generic_ctx, &None) {
                            Ok(ConstValue::Number(n)) => match n.value {
                                NumberValue::Uint(n) | NumberValue::Any(n) => Some(n as _),
                                NumberValue::Int(n) => Some(n as _),
                                _ => todo!(),
                            },
                            Err(e) => Err(e)?,
                            _ => todo!(),
                        }
                    }
                    None => None,
                };
                let key = ctx.types.arrays.push_unique(ArrayType {
                    element_type: ty,
                    size: size,
                });
                AnyTypeKey::Array(key)
            }
            ast::TypeLiteral::Tuple(spans) => {
                let mut parameters = Vec::new();
                for ty in spans {
                    parameters.push(ty.lower(ctx, module, generic_ctx)?);
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
        let ref_type = (0..*refs.inner).fold(type_val, |a, _| {
            AnyTypeKey::Reference(ctx.types.references.push_unique(RefType { inner: a }))
        });
        Ok(ref_type)
    }
}

fn resolve_type_path(
    ctx: &mut Context,
    module: Key<ModuleTag>,
    generic_ctx: &mut GenericContext,
    identifier_path: &ast::Span<ast::IdentifierPath>,
    generic_arguments: &Option<ast::Span<Vec<ast::Span<ast::Type>>>>,
) -> Result<AnyTypeKey, Diagnostic<Errors>> {
    let path: Vec<SmolStr> = identifier_path
        .path
        .iter()
        .map(|p| p.inner.as_ref().clone())
        .collect();
    let resolved = ctx.resolve_const_path(&path, module, generic_ctx, identifier_path.location)?;
    Ok(match resolved {
        AnyObjectKey::Type(key) => {
            if let TypeAliasObj {
                ty: InitState::Done(ty) | InitState::Progress(ty),
                generics,
                constants: _,
            } = &mut ctx.objects.types.get_mut_unchecked(&key).data
            {
                let ty = *ty;
                let mut new = AnyTypeKey::Named(ty);
                let gen_args = match generic_arguments {
                    Some(g) => g.inner.as_ref().clone(),
                    None => Vec::new(),
                };
                for (idx, (_, constraint)) in generics.clone().iter().enumerate() {
                    let substitution = gen_args[idx].lower(ctx, module, generic_ctx)?;
                    new = AnyTypeKey::Named(ty).substitute_named(
                        substitution,
                        constraint,
                        &mut ctx.types,
                    )?;
                }
                new
            } else {
                unreachable!("lamo")
            }
        }
        _ => unreachable!("all paths are expected to end with a type alias"),
    })
}
