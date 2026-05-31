use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use arena::Key;
use smol_str::SmolStr;

use crate::ast::{self, ConstValue, Number, NumberValue, Span, SpanIndex};
use crate::const_stage::objects::{
    AnyObject, AnyObjectKey, ComponentObj, ComponentObjKey, ConstObj, ConstObjKey, FunctionObj,
    FunctionObjKey, ImportObj, InitState, Module, TraitObj, TypeAliasObj, TypeAliasObjKey,
};
use crate::const_stage::types::{
    AnyTypeKey, ArrayType, EnumType, FunctionType, ModuleKey, ModuleTag, PrimitiveType, RefType,
    StructType, TraitType, TupleType,
};
use crate::const_stage::{Context, Diagnostic, Error, Errors};

impl Context {
    pub(crate) fn lower_import_stage(&mut self) -> Result<(), Error> {
        let map: HashMap<Vec<SmolStr>, Key<ModuleTag>> =
            HashMap::from_iter(self.ast.iter().map(|(k, ast)| {
                let mut module = Module::new(Arc::clone(ast));
                module.path = k.clone();
                let type_key = self.types.modules.push(module);
                (k.clone(), type_key)
            }));

        for (current_module_path, module) in &self.ast {
            let module_key = map.get(current_module_path).unwrap();
            let ir_module = self.types.modules.get_mut_unchecked(module_key);
            for (obj_key, obj) in module.objects.iter_pairs() {
                let (ident, key): (SmolStr, AnyObjectKey) = match obj.inner.as_ref() {
                    ast::Object::Import { ident, alias } => {
                        let raw_path: Vec<SmolStr> = ident
                            .inner
                            .path
                            .iter()
                            .map(|a| a.inner.as_ref().clone())
                            .collect();

                        let target_path = if raw_path.first().map(|s| s.as_str()) == Some("mod") {
                            raw_path[1..].to_vec()
                        } else {
                            let mut path = current_module_path.clone();
                            if !path.is_empty() {
                                path.pop();
                            }
                            path.extend(raw_path);
                            path
                        };

                        let ident = match &alias.0 {
                            Some(name) => name.inner.inner.as_ref().clone(),
                            None => ident.inner.path.last().unwrap().inner.as_ref().clone(),
                        };

                        let ty_key = match map.get(&target_path) {
                            Some(k) => *k,
                            None => {
                                todo!(
                                    "lamo: {target_path:?}\n{:?}",
                                    map.keys().cloned().collect::<Vec<Vec<SmolStr>>>()
                                )
                            }
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
                                    type_of: InitState::Uninitialized,
                                    ir: InitState::Uninitialized,
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
                                    generics: InitState::Uninitialized,
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
        let type_keys: Vec<TypeAliasObjKey> = self.objects.types.iter_keys().collect();
        for type_key in type_keys {
            self.lower_type_alias_with_key(type_key)?;
        }

        let const_keys: Vec<ConstObjKey> = self.objects.constants.iter_keys().collect();
        for const_key in const_keys {
            self.lower_const_with_key(const_key)?;
        }

        let component_keys: Vec<ComponentObjKey> = self.objects.components.iter_keys().collect();
        for component_key in component_keys {
            self.lower_component_with_key(component_key)?;
        }

        let function_keys: Vec<FunctionObjKey> = self.objects.functions.iter_keys().collect();
        for function_key in function_keys {
            self.lower_function_with_key(function_key)?;
        }
        Ok(())
    }

    fn lower_function_with_key(
        &mut self,
        function_key: Key<super::objects::FunctionObjTag>,
    ) -> Result<(), Diagnostic<Errors>> {
        let obj = self.objects.functions.get_unchecked(&function_key);
        let mod_key = obj.module;
        let module = self.types.modules.get_unchecked(&mod_key);
        Ok(
            match self
                .ast
                .get(&module.path)
                .unwrap()
                .objects
                .get_unchecked(&obj.ast_object)
                .clone()
                .deref()
            {
                ast::Object::Function(ast::Function {
                    ident: _,
                    generics,
                    parameters,
                    return_type,
                    body: _,
                    docs: _,
                }) => {
                    self.push_generic_scope(generics, &mod_key)?;
                    let return_type = match return_type {
                        Some(ty) => ty.lower(self, mod_key)?,
                        None => AnyTypeKey::Primitive(PrimitiveType::Void),
                    };
                    let fun = self.objects.functions.get_mut_unchecked(&function_key);
                    fun.data.return_type = InitState::Done(return_type);

                    let mut params = HashMap::new();
                    for param in parameters {
                        let ident = param.ident.inner.deref().clone();
                        let ty = param.ty.lower(self, mod_key)?;
                        params.insert(ident, InitState::Done(ty));
                    }
                    let fun = self.objects.functions.get_mut_unchecked(&function_key);
                    fun.data.params = params;

                    let type_of = FunctionType {
                        parameters: fun
                            .data
                            .params
                            .iter()
                            .map(|(_, ty)| *ty.get_done())
                            .collect(),
                        returns: *fun.data.return_type.get_done(),
                    };
                    let type_of = AnyTypeKey::Function(self.types.functions.push_unique(type_of));
                    fun.data.type_of = InitState::Done(type_of);

                    let g_scope = self.generic_ctx.pop();
                }
                _ => unreachable!(),
            },
        )
    }

    fn lower_component_with_key(
        &mut self,
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
                    Some(ty) => ty.lower(self, mod_key)?,
                    None => AnyTypeKey::Primitive(PrimitiveType::Void),
                };
                let obj = self.objects.components.get_mut_unchecked(&component_key);
                obj.data.ty = InitState::Done(ty);
            },
        )
    }

    fn lower_const_with_key(
        &mut self,
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
                let obj_key: &ConstObjKey = &const_key;
                let self_def: &Option<ConstValue> = &None;
                let type_key = ty.lower(self, mod_key)?;
                let obj = self.objects.constants.get_mut_unchecked(obj_key);
                obj.data.ty = InitState::Done(type_key);
                let v = match expression.const_eval(self, mod_key, self_def, &Some(type_key)) {
                    Ok(v) => v,
                    Err(e) => Err(e)?,
                };
                let obj = self.objects.constants.get_mut_unchecked(obj_key);
                obj.data.value = InitState::Done(v);
                self.objects.constants.get_mut_unchecked(obj_key)
            } else {
                self.objects.constants.get_mut_unchecked(&const_key)
            },
        )
    }

    fn lower_type_alias_with_key(
        &mut self,
        type_key: Key<super::objects::TypeAliasObjTag>,
    ) -> Result<&mut AnyObject<TypeAliasObj>, Diagnostic<Errors>> {
        let this = self.objects.types.get_unchecked(&type_key);
        let ast_key = this.ast_object;
        let mod_key = this.module;
        let module_path = &self.types.modules.get_unchecked(&mod_key).path;
        Ok(
            if let ast::Object::Type {
                ident: _,
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
                    self.generic_ctx.init();
                    self.push_generic_scope(&generics, &mod_key)?;
                    let obj = self.objects.types.get_mut_unchecked(&type_key);
                    let named_key =
                        if let InitState::Done(ty) | InitState::Progress(ty) = &mut obj.data.ty {
                            *ty
                        } else {
                            unreachable!("type should already be prepared")
                        };
                    self.types.named.get_mut_unchecked(&named_key).name = obj.identifier.clone();
                    let key = match ty {
                        Some(t) => t.lower(self, mod_key)?,
                        _ => AnyTypeKey::Primitive(PrimitiveType::Void),
                    };
                    match &key {
                        AnyTypeKey::Enum(enum_key) => {
                            let ty = self.types.enums.get_unchecked(enum_key);
                            for ident in ty
                                .variants
                                .iter()
                                .map(|(ident, _)| ident.clone())
                                .collect::<Vec<SmolStr>>()
                            {
                                let const_obj = AnyObject {
                                    data: ConstObj {
                                        ty: InitState::Done(AnyTypeKey::Enum(*enum_key)),
                                        value: InitState::Done(ConstValue::EnumVariant {
                                            parent: AnyTypeKey::Named(named_key),
                                            variant: ident.clone(),
                                        }),
                                    },
                                    ast_object: ast_key,
                                    identifier: ident.clone(),
                                    module: mod_key,
                                };
                                let obj_key = self.objects.constants.push(const_obj);
                                self.objects
                                    .types
                                    .get_mut_unchecked(&type_key)
                                    .data
                                    .constants
                                    .insert(ident, obj_key);
                            }
                        }
                        _ => (),
                    }
                    self.types.named.get_mut_unchecked(&named_key).repr = key;
                    let obj = self.objects.types.get_mut_unchecked(&type_key);
                    obj.data.ty.mark_done();
                    obj.data.generics = InitState::Done(self.generic_ctx.pop());
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
        obj_key: &ConstObjKey,
        ty: &ast::Span<ast::Type>,
        expression: &ast::Span<ast::Expression>,
        self_def: &Option<ConstValue>,
    ) -> Result<(), Error> {
        let type_key = ty.lower(self, mod_key)?;
        let obj = self.objects.constants.get_mut_unchecked(obj_key);
        obj.data.ty = InitState::Done(type_key);
        let mut v = match expression.const_eval(self, mod_key, self_def, &Some(type_key)) {
            Ok(v) => v,
            Err(e) => Err(e)?,
        };
        if !self.type_check_const_value(&mut v, &type_key) {
            return Err(Diagnostic {
                span: expression.location,
                module: mod_key,
                inner: Errors::TypeMismatch {
                    expected: type_key,
                    got: v.type_of(),
                },
            });
        }
        let obj = self.objects.constants.get_mut_unchecked(obj_key);
        obj.data.value = InitState::Done(v);
        Ok(())
    }

    pub fn resolve_const_path(
        &mut self,
        path: &[Span<SmolStr>],
        mod_key: ModuleKey,
        span: SpanIndex,
    ) -> Result<AnyObjectKey, Error> {
        enum PathNode {
            Module(ModuleKey),
            Object(AnyObjectKey),
        }
        if path.len() == 1 {
            match self.generic_ctx.get(&path[0]) {
                Some(_) => todo!("toznam"),
                None => (),
            }
        }
        let mut current_path_node = PathNode::Module(mod_key);
        for (i, next_stop) in path.iter().enumerate() {
            match &current_path_node {
                PathNode::Module(module) => {
                    let module = self.types.modules.get_unchecked(&module);
                    match module.symbol_map.get(next_stop.deref()) {
                        Some(AnyObjectKey::Import(obj_key)) => {
                            current_path_node = PathNode::Module(
                                self.objects.imports.get_unchecked(obj_key).data.module,
                            )
                        }
                        Some(any_key) => current_path_node = PathNode::Object(*any_key),
                        _ => Err(Diagnostic {
                            inner: Errors::ObjectNotFound(
                                path[..=i].iter().map(|v| v.deref().clone()).collect(),
                            ),
                            span: next_stop.location,
                            module: mod_key,
                        })?,
                    }
                }
                PathNode::Object(obj_key) => match obj_key {
                    AnyObjectKey::Type(type_key) => {
                        let type_obj = self.lower_type_alias_with_key(*type_key)?;
                        match type_obj.data.constants.get(next_stop.deref()) {
                            Some(v) => {
                                current_path_node = PathNode::Object(AnyObjectKey::Const(*v))
                            }
                            _ => Err(Diagnostic {
                                inner: Errors::ObjectNotFound(
                                    path[..=i].iter().map(|v| v.deref().clone()).collect(),
                                ),
                                span: next_stop.location,
                                module: mod_key,
                            })?,
                        }
                    }
                    _ => todo!(),
                },
            }
        }
        match current_path_node {
            PathNode::Module(module) => Err(Diagnostic {
                span,
                module: mod_key,
                inner: Errors::EvalModule(path.iter().map(|v| v.deref().clone()).collect(), module),
            }),
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

impl ast::Expression {
    pub fn const_eval(
        &self,
        ctx: &mut Context,
        mod_key: ModuleKey,
        self_def: &Option<ConstValue>,
        expect: &Option<AnyTypeKey>,
    ) -> Result<ConstValue, Error> {
        let location;
        let result = match self.const_reduce().as_ref() {
            ast::Expression::Value(value) => {
                location = value.location;
                if !value.postfix.is_empty() {
                    return Err(Diagnostic {
                        span: value.location,
                        inner: Errors::NotConst,
                        module: mod_key,
                    });
                }
                match value.literal.inner.as_ref() {
                    ast::Literal::Identifier(identifier_path) => {
                        match ctx.resolve_const_path(
                            &identifier_path.path.path,
                            mod_key,
                            identifier_path.path.location,
                        ) {
                            Ok(key) => {
                                let obj_key = if let AnyObjectKey::Const(key) = key {
                                    key
                                } else {
                                    Err(Error {
                                        inner: Errors::NotConst,
                                        module: mod_key,
                                        span: identifier_path.path.location,
                                    })?
                                };
                                let obj = ctx.objects.constants.get_unchecked(&obj_key);
                                match &obj.data {
                                    ConstObj {
                                        value: InitState::Done(v),
                                        ty: _,
                                    } => v.clone(),
                                    ConstObj {
                                        value: InitState::Progress(_),
                                        ty: _,
                                    } => {
                                        panic!("we do not like circles around here")
                                    }
                                    ConstObj { .. } => {
                                        let obj = ctx.lower_const_with_key(obj_key)?;
                                        match &obj.data {
                                            ConstObj {
                                                value: InitState::Done(v),
                                                ..
                                            } => v.clone(),
                                            _ => unreachable!(),
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                if identifier_path.path.path.len() == 1
                                    && identifier_path
                                        .path
                                        .path
                                        .first()
                                        .map(|v| v.deref().as_str())
                                        == Some("self")
                                {
                                    match self_def {
                                        Some(v) => v.clone(),
                                        None => Err(Diagnostic {
                                            span: value.location,
                                            module: mod_key,
                                            inner: Errors::UndefinedSelf,
                                        })?,
                                    }
                                } else {
                                    return Err(e);
                                }
                            }
                        }
                    }
                    ast::Literal::Structure { fields, kw: _, ty } => {
                        let mut const_fields = Vec::with_capacity(fields.len());
                        for field in fields {
                            let ident = field.0.clone();
                            let value = field.1.const_eval(ctx, mod_key, self_def, &None)?;
                            const_fields.push(Span::new(
                                (ident, Span::new(value, field.location)),
                                field.location,
                            ));
                        }
                        let ty = match ty {
                            Some(ty) => resolve_type_path(ctx, mod_key, &ty.path, &ty.generics)?,
                            None => AnyTypeKey::AnonymousStruct,
                        };
                        ConstValue::Structure {
                            fields: const_fields,
                            ty,
                        }
                    }
                    ast::Literal::Number(number) => ConstValue::Number(number.clone()),
                    ast::Literal::String(smol_str) => ConstValue::String(smol_str.clone()),
                    ast::Literal::Char(c) => ConstValue::Char(*c),
                    ast::Literal::Array(exprs) => {
                        let mut values = Vec::new();
                        let mut inner_ty = None;
                        for expr in exprs {
                            let v = expr.const_eval(ctx, mod_key, self_def, &inner_ty)?;
                            let typeof_v = v.type_of();
                            match &inner_ty {
                                Some(ty) => {
                                    typeof_v.check(&ctx.types, ty).map_err(|e| Error {
                                        inner: e,
                                        module: mod_key,
                                        span: expr.location,
                                    })?;
                                    inner_ty = Some(typeof_v);
                                }
                                None => inner_ty = Some(typeof_v),
                            }
                            values.push(expr.clone().map(|_| v));
                        }
                        let ty = ArrayType {
                            element_type: match inner_ty {
                                Some(ty) => ty,
                                None => Err(Error {
                                    inner: Errors::FailedTypeInfer,
                                    module: mod_key,
                                    span: value.location,
                                })?,
                            },
                            size: Some(values.len()),
                        };
                        let ty = AnyTypeKey::Array(ctx.types.arrays.push_unique(ty));
                        ConstValue::Array {
                            elements: values,
                            ty,
                        }
                    }
                    ast::Literal::Tuple(exprs) => {
                        let mut values = Vec::with_capacity(exprs.len());
                        for expr in exprs {
                            let v = expr.const_eval(ctx, mod_key, self_def, &None)?;
                            values.push(expr.clone().map(|_| v));
                        }
                        let mut types = Vec::with_capacity(values.len());
                        for v in &values {
                            types.push(v.type_of());
                        }
                        let ty = TupleType { parameters: types };
                        let ty = AnyTypeKey::Tuple(ctx.types.tuples.push_unique(ty));
                        ConstValue::Tuple {
                            elements: values,
                            ty,
                        }
                    }
                }
            }
            ast::Expression::Binary { l, r, op } => {
                let span = SpanIndex {
                    index: l.location.index,
                    len: r.location.len + r.location.index - l.location.index,
                };
                location = span;
                let left_val = l.const_eval(ctx, mod_key, self_def, &None)?;
                let typeof_l = left_val.type_of();
                let right_val = r.const_eval(ctx, mod_key, self_def, &Some(typeof_l))?;
                match op.const_apply(&left_val, &right_val, mod_key) {
                    Ok(v) => v,
                    Err(mut e) => {
                        e.span = span;
                        Err(e)?
                    }
                }
            }
        };
        match expect {
            Some(e) => {
                let result = result.implicit_cast(ctx, *e).map_err(|e| Error {
                    inner: e,
                    module: mod_key,
                    span: location,
                })?;
                Ok(result)
            }
            None => Ok(result),
        }
    }
}

impl ast::Type {
    pub fn lower(&self, ctx: &mut Context, module: ModuleKey) -> Result<AnyTypeKey, Error> {
        let Self { literal, refs } = &self;
        let type_val = match literal.inner.as_ref() {
            ast::TypeLiteral::Path(identifier_path, generic_arguments) => {
                if identifier_path.path.len() == 1
                    && let Some(ident) = identifier_path.path.first()
                {
                    if let Some(ty) = PrimitiveType::from_str(&ident.inner) {
                        AnyTypeKey::Primitive(ty)
                    } else if let Some(ty) = ctx.generic_ctx.get(ident) {
                        AnyTypeKey::Constraint(*ty)
                    } else {
                        resolve_type_path(ctx, module, identifier_path, generic_arguments)?
                    }
                } else {
                    resolve_type_path(ctx, module, identifier_path, generic_arguments)?
                }
            }
            ast::TypeLiteral::Struct(spans) => {
                let mut parameters: Vec<(SmolStr, AnyTypeKey, Option<ConstValue>)> = Vec::new();
                for param in spans {
                    let ident = &param.ident.inner;
                    if let Some((duplicate, _, _)) = parameters
                        .iter()
                        .find(|(p_ident, _, _)| p_ident == ident.as_ref())
                    {
                        Err(Error {
                            inner: Errors::DuplicateIdentifier(duplicate.clone()),
                            module,
                            span: param.ident.location,
                        })?
                    }

                    let ty = param.ty.lower(ctx, module)?;

                    let default = match &param.default_value {
                        Some(v) => Some(v.const_eval(ctx, module, &None, &Some(ty))?),
                        None => None,
                    };
                    parameters.push((ident.as_ref().clone(), ty, default));
                }
                let key = ctx.types.structures.push_unique(StructType { parameters });
                AnyTypeKey::Struct(key)
            }
            ast::TypeLiteral::Enum(repr, step, ast_variants) => {
                let repr = match repr {
                    Some(repr) => repr.lower(ctx, module)?,
                    None => AnyTypeKey::Primitive(PrimitiveType::I32),
                };
                let mut variants = Vec::new();
                let mut iter = ast_variants.iter();
                let first_value = match iter.next() {
                    Some((ident, expr)) => {
                        let value = match expr {
                            Some(e) => e.const_eval(ctx, module, &None, &Some(repr))?,
                            None => todo!("forgot to fix autostep for anytype"),
                        };
                        variants.push((ident.inner.as_ref().clone(), value.clone()));
                        Some(value)
                    }
                    None => None,
                };
                if let Some(mut last_value) = first_value {
                    for (ident, expr) in iter {
                        let value = match expr {
                            Some(expr) => {
                                let value =
                                    expr.const_eval(ctx, module, &Some(last_value), &Some(repr))?;

                                let ty = value.type_of();

                                ty.check(&ctx.types, &repr).map_err(|e| Error {
                                    inner: e,
                                    module: module,
                                    span: expr.location,
                                })?;

                                value
                            }
                            None => match step {
                                Some(step) => {
                                    step.const_eval(ctx, module, &Some(last_value), &Some(repr))?
                                }
                                None => last_value.autostep().map_err(|e| Error {
                                    inner: e,
                                    module: module,
                                    span: self.literal.location,
                                })?,
                            },
                        };
                        variants.push((ident.inner.as_ref().clone(), value.clone()));
                        last_value = value;
                    }
                }
                let key = ctx.types.enums.push_unique(EnumType { repr, variants });
                AnyTypeKey::Enum(key)
            }
            ast::TypeLiteral::Array(span, size) => {
                let ty = span.lower(ctx, module)?;
                let size = match size {
                    Some(const_expr) => {
                        match const_expr.const_eval(
                            ctx,
                            module,
                            &None,
                            &Some(AnyTypeKey::Primitive(PrimitiveType::U32)),
                        ) {
                            Ok(ConstValue::Number(n)) => match n.value {
                                NumberValue::Uint(n) | NumberValue::Any(n) => Some(n as _),
                                NumberValue::Int(n) => Some(n as _),
                                value => Err(Diagnostic {
                                    span: const_expr.location,
                                    module,
                                    inner: Errors::ExpectedNumericConst {
                                        got: ConstValue::Number(Number { value, size: None }),
                                    },
                                })?,
                            },
                            Err(e) => Err(e)?,
                            Ok(got) => Err(Diagnostic {
                                span: const_expr.location,
                                module,
                                inner: Errors::ExpectedNumericConst { got },
                            })?,
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
                    parameters.push(ty.lower(ctx, module)?);
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
    identifier_path: &ast::Span<ast::IdentifierPath>,
    generic_arguments: &Option<ast::Span<Vec<ast::Span<ast::Type>>>>,
) -> Result<AnyTypeKey, Error> {
    let resolved =
        ctx.resolve_const_path(&identifier_path.path, module, identifier_path.location)?;

    Ok(match resolved {
        AnyObjectKey::Type(key) => {
            let TypeAliasObj { ty, generics, .. } =
                &mut ctx.objects.types.get_mut_unchecked(&key).data;

            let ty = match ty {
                InitState::Done(t) | InitState::Progress(t) => *t,
                _ => unreachable!("type alias not initialized"),
            };

            let gen_args: Vec<_> = generic_arguments
                .as_ref()
                .map(|g| g.inner.as_ref().clone())
                .unwrap_or_default();

            let generics = ctx
                .generic_ctx
                .arena()
                .get_unchecked(generics.get_done())
                .values
                .clone();

            if gen_args.len() != generics.len() {
                return Err(Diagnostic {
                    inner: Errors::GenericArityMismatch {
                        expected: generics.len(),
                        found: gen_args.len(),
                    },
                    span: match generic_arguments {
                        Some(args) => args.location,
                        None => identifier_path.location,
                    },
                    module,
                });
            }
            let substitutions = generics
                .iter()
                .zip(&gen_args)
                .map(|((_, constraint), arg)| {
                    let substitution = arg.lower(ctx, module)?;
                    Ok((*constraint, substitution))
                })
                .collect::<Result<Vec<_>, Error>>()?;

            let span = gen_args
                .first()
                .map(|arg| arg.location)
                .unwrap_or(identifier_path.location);

            let new = AnyTypeKey::Named(ty).substitute_named_iter(
                substitutions,
                &mut ctx.types,
                module,
                span,
            )?;

            new
        }

        _ => unreachable!("all paths are expected to end with a type alias"),
    })
}
