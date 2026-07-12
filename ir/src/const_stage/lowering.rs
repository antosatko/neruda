use std::collections::HashMap;
use std::ops::Deref;
use std::sync::Arc;

use arena::Key;
use smol_str::SmolStr;

use crate::ast::{
    self, AccessModifiers, ConstValue, Number, NumberValue, PathSelectorEndOptions, Span,
    SpanIndex, Type,
};
use crate::const_stage::objects::{
    AnyObject, AnyObjectKey, ComponentObj, ComponentObjKey, ConstObj, ConstObjKey, FunctionObj,
    FunctionObjKey, ImportObj, InitState, IrCache, Module, ResourceObj, ResourceObjKey, TraitObj,
    TypeAliasObj, TypeAliasObjKey,
};
use crate::const_stage::types::{
    AnyTypeKey, ArrayType, EnumType, FunctionType, GenericKey, ModuleKey, ModuleTag, PolymorphType,
    PrimitiveType, RefType, StructType, TraitType, TupleType,
};
use crate::const_stage::{Context, Diagnostic, Error, Errors};
use crate::ir::FunctionIr;

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
            let module_type = self.types.modules.get_mut_unchecked(module_key);
            for (obj_key, obj) in module.objects.iter_pairs() {
                let (ident, key): (SmolStr, AnyObjectKey) = match obj.inner.as_ref() {
                    ast::Object::Using { .. } => continue,
                    ast::Object::Import {
                        ident,
                        alias,
                        access,
                    } => {
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
                            path.extend_from_slice(&raw_path);
                            path
                        };

                        let ty_key = match map.get(&target_path) {
                            Some(k) => *k,
                            None => {
                                return Err(Error {
                                    inner: Errors::ObjectNotFound(raw_path),
                                    module: *module_key,
                                    span: ident.location,
                                });
                            }
                        };

                        let ident = match &alias.0 {
                            Some(name) => name.inner.inner.as_ref().clone(),
                            None => ident.inner.path.last().unwrap().inner.as_ref().clone(),
                        };

                        let obj = ImportObj { module: ty_key };
                        let obj = AnyObject::new(
                            ident.clone(),
                            obj,
                            obj_key,
                            *module_key,
                            access.modifier,
                        );
                        let key = self.objects.imports.push(obj);
                        module_type
                            .symbol_map
                            .insert(ident, AnyObjectKey::Import(key));
                        continue;
                    }
                    ast::Object::Resource {
                        access,
                        ident,
                        docs: _,
                        ty: _,
                        default_expression: _,
                        is_optional,
                    } => (
                        ident.deref().clone(),
                        AnyObjectKey::Resource(self.objects.resources.push(AnyObject {
                            data: ResourceObj {
                                default: InitState::Uninitialized,
                                optional: is_optional.is_some(),
                                ty: InitState::Uninitialized,
                            },
                            access: access.modifier,
                            identifier: ident.deref().clone(),
                            ast_object: obj_key,
                            module: *module_key,
                        })),
                    ),
                    ast::Object::Function(ast::Function {
                        ident,
                        access,
                        generics,
                        ..
                    }) => {
                        let mut fun = AnyObject {
                            data: FunctionObj {
                                return_type: InitState::Uninitialized,
                                params: Vec::with_capacity(0),
                                generics: Vec::new(),
                                type_of: InitState::Uninitialized,
                                ir: IrCache::Single(InitState::Uninitialized),
                                generic_scope: InitState::Uninitialized,
                            },
                            access: access.modifier,
                            identifier: ident.inner.as_ref().clone(),
                            ast_object: obj_key,
                            module: *module_key,
                        };
                        let ir = match generics.as_ref().map(|g| g.len()).unwrap_or(0) {
                            0 => IrCache::Single(InitState::Progress(
                                self.ir_cache.push(FunctionIr::new(&fun)),
                            )),
                            _ => IrCache::Polymorphic(HashMap::new()),
                        };
                        fun.data.ir = ir;
                        (
                            ident.inner.as_ref().clone(),
                            self.objects.functions.push(fun).into(),
                        )
                    }
                    ast::Object::Component { ident, access, .. } => (
                        ident.inner.as_ref().clone(),
                        self.objects
                            .components
                            .push(AnyObject {
                                data: ComponentObj {
                                    ty: InitState::Uninitialized,
                                },
                                access: access.modifier,
                                identifier: ident.inner.as_ref().clone(),
                                ast_object: obj_key,
                                module: *module_key,
                            })
                            .into(),
                    ),
                    ast::Object::TypeImpl { .. } => continue,
                    ast::Object::TraitImpl { .. } => continue,
                    ast::Object::Trait { ident, access, .. } => (
                        ident.inner.as_ref().clone(),
                        self.objects
                            .traits
                            .push(AnyObject {
                                data: TraitObj {
                                    ty: InitState::Progress(self.types.traits.push(TraitType {
                                        ident: ident.inner.as_ref().clone(),
                                    })),
                                },
                                access: access.modifier,
                                identifier: ident.inner.as_ref().clone(),
                                ast_object: obj_key,
                                module: *module_key,
                            })
                            .into(),
                    ),
                    ast::Object::Type { ident, access, .. } => (
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
                                access: access.modifier,
                                identifier: ident.inner.as_ref().clone(),
                                ast_object: obj_key,
                                module: *module_key,
                            })
                            .into(),
                    ),
                    ast::Object::System { .. } => continue,
                    ast::Object::Const { ident, access, .. } => (
                        ident.inner.as_ref().clone(),
                        self.objects
                            .constants
                            .push(AnyObject {
                                data: ConstObj {
                                    value: InitState::Uninitialized,
                                    ty: InitState::Uninitialized,
                                },
                                access: access.modifier,
                                identifier: ident.inner.as_ref().clone(),
                                ast_object: obj_key,
                                module: *module_key,
                            })
                            .into(),
                    ),
                };
                module_type.symbol_map.insert(ident, key);
            }
        }

        self.module_map = map;
        Ok(())
    }

    pub(crate) fn lower_using_stage(&mut self) -> Result<(), Error> {
        for (current_module_path, module_key) in &self.module_map.clone() {
            let ast_module = self.ast.get(current_module_path).unwrap().clone();

            for (obj_key, obj) in ast_module.objects.iter_pairs() {
                match obj.inner.as_ref() {
                    ast::Object::Using { selector } => {
                        self.use_selector(module_key, &None, obj_key, selector)?;
                    }
                    _ => continue,
                };
            }
        }
        Ok(())
    }

    fn use_selector(
        &mut self,
        module_key: &Key<ModuleTag>,
        prepend: &Option<ModuleKey>,
        obj_key: Key<ast::ObjectTag>,
        selector: &Span<ast::PathSelector>,
    ) -> Result<(), Diagnostic<Errors>> {
        match self.resolve_selector_target(
            match prepend {
                Some(p) => *p,
                None => *module_key,
            },
            selector,
        )? {
            (PathNode::Module(key), ident) => match &selector.ends_on.as_ref().map(|o| o.deref()) {
                Some(PathSelectorEndOptions::All) => {
                    let symbols = self
                        .types
                        .modules
                        .get_unchecked(&key)
                        .symbol_map
                        .values()
                        .cloned()
                        .collect::<Vec<_>>();
                    for symbol in symbols {
                        if let Ok(_) = self.check_access(*module_key, selector, symbol) {
                            let ident = symbol.ident(self).clone();
                            self.types
                                .modules
                                .get_mut_unchecked(module_key)
                                .symbol_map
                                .insert(ident, symbol);
                        }
                    }
                }
                Some(PathSelectorEndOptions::Set(set)) => {
                    let set = set.clone();
                    for selector in set {
                        self.use_selector(module_key, &Some(key), obj_key, &selector)?;
                    }
                }
                Some(PathSelectorEndOptions::Alias(alias)) => {
                    let ident = alias.deref().deref().clone();
                    let import = self.objects.imports.push(AnyObject {
                        data: ImportObj { module: key },
                        access: ast::AccessModifiers::Private,
                        identifier: ident.clone(),
                        ast_object: obj_key,
                        module: *module_key,
                    });
                    self.types
                        .modules
                        .get_mut_unchecked(module_key)
                        .symbol_map
                        .insert(ident, AnyObjectKey::Import(import));
                }
                None => {
                    let import = self.objects.imports.push(AnyObject {
                        data: ImportObj { module: key },
                        access: ast::AccessModifiers::Private,
                        identifier: ident.deref().clone(),
                        ast_object: obj_key,
                        module: *module_key,
                    });
                    self.types
                        .modules
                        .get_mut_unchecked(module_key)
                        .symbol_map
                        .insert(ident.deref().clone(), AnyObjectKey::Import(import));
                }
            },
            (PathNode::Object(key), ident) => match &selector.ends_on.as_ref().map(|o| o.deref()) {
                Some(PathSelectorEndOptions::All) => {
                    todo!("symbol propagation things")
                }
                Some(PathSelectorEndOptions::Set(set)) => todo!(),
                Some(PathSelectorEndOptions::Alias(alias)) => {
                    self.check_access(*module_key, selector, key)?;
                    self.types
                        .modules
                        .get_mut_unchecked(module_key)
                        .symbol_map
                        .insert(alias.deref().deref().clone(), key);
                }
                None => {
                    self.check_access(*module_key, selector, key)?;
                    self.types
                        .modules
                        .get_mut_unchecked(module_key)
                        .symbol_map
                        .insert(ident.deref().clone(), key);
                }
            },
        };
        Ok(())
    }

    fn check_access(
        &self,
        module: Key<ModuleTag>,
        selector: &Span<ast::PathSelector>,
        key: AnyObjectKey,
    ) -> Result<(), Diagnostic<Errors>> {
        if module == key.module(self) {
            return Ok(());
        }
        let access = key.access(self);
        match access {
            AccessModifiers::Private => Err(Error {
                inner: Errors::ObjectInaccesible(key),
                module,
                span: selector.location,
            }),
            AccessModifiers::Public => Ok(()),
            AccessModifiers::PublicModule => Ok(()),
            AccessModifiers::PublicProject => Ok(()),
        }?;
        Ok(())
    }

    fn resolve_selector_target(
        &self,
        module_key: Key<ModuleTag>,
        selector: &Span<ast::PathSelector>,
    ) -> Result<(PathNode, Span<SmolStr>), Diagnostic<Errors>> {
        let mut stop = (
            PathNode::Module(module_key),
            Span::new(Default::default(), SpanIndex::default()),
        );
        for next in &selector.path.path {
            stop = (
                self.resolve_next_static_stop_of_path(next, &stop.0, &module_key)?,
                next.clone(),
            );
        }
        Ok(stop)
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

        let resource_keys: Vec<ResourceObjKey> = self.objects.resources.iter_keys().collect();
        for resource_key in resource_keys {
            self.lower_resource_with_key(resource_key)?;
        }
        Ok(())
    }

    #[track_caller]
    pub fn lower_object_const_stage(&mut self, key: AnyObjectKey) -> Result<(), Error> {
        match key {
            AnyObjectKey::Import(_) => {
                unreachable!("imports are not part of const stage, this is a bug in the compiler")
            }
            AnyObjectKey::Const(key) => self.lower_const_with_key(key).map(|_| ()),
            AnyObjectKey::Type(key) => self.lower_type_alias_with_key(key).map(|_| ()),
            AnyObjectKey::Trait(_) => todo!(),
            AnyObjectKey::Component(key) => self.lower_component_with_key(key).map(|_| ()),
            AnyObjectKey::Function(key) => self.lower_function_with_key(key).map(|_| ()),
            AnyObjectKey::Resource(key) => self.lower_resource_with_key(key).map(|_| ()),
        }
    }

    fn lower_resource_with_key(
        &mut self,
        resource_key: ResourceObjKey,
    ) -> Result<&mut AnyObject<ResourceObj>, Diagnostic<Errors>> {
        let obj = self.objects.resources.get_unchecked(&resource_key);
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
                ast::Object::Resource {
                    ident,
                    docs: _,
                    access: _,
                    ty,
                    default_expression,
                    is_optional,
                } => {
                    let identifier = ident.deref().clone();
                    let (ty, default) = match (ty, default_expression) {
                        (Some(ty), Some(expr)) => {
                            let ty = ty.lower(self, mod_key)?;
                            let default = match expr.const_eval(self, mod_key, &None, &Some(ty)) {
                                ConstEvalResult::Value(v) => v,
                                ConstEvalResult::Error(err) | ConstEvalResult::NotConst(err) => {
                                    return Err(err);
                                }
                            };
                            (ty, Some(default))
                        }
                        (Some(ty), None) => {
                            let type_lowered = ty.lower(self, mod_key)?;
                            let default = match is_optional {
                                Some(_) => None,
                                None => {
                                    Some(type_lowered.const_default(self).map_err(|e| Error {
                                        inner: e,
                                        module: mod_key,
                                        span: ty.location,
                                    })?)
                                }
                            };
                            (type_lowered, default)
                        }
                        (None, Some(default)) => {
                            let default_val = match default.const_eval(self, mod_key, &None, &None)
                            {
                                ConstEvalResult::Value(v) => v,
                                ConstEvalResult::Error(err) | ConstEvalResult::NotConst(err) => {
                                    return Err(err);
                                }
                            };
                            let ty = default_val.type_of().map_err(|e| Error {
                                inner: e,
                                module: mod_key,
                                span: default.location,
                            })?;
                            (ty, Some(default_val))
                        }
                        (None, None) => match is_optional {
                            Some(_) => (AnyTypeKey::Void, None),
                            None => Err(Error {
                                inner: Errors::ExpectedOptionalResource { ident: identifier },
                                module: mod_key,
                                span: ident.location,
                            })?,
                        },
                    };
                    let res = self.objects.resources.get_mut_unchecked(&resource_key);
                    res.data.default = InitState::Done(default);
                    res.data.ty = InitState::Done(ty);

                    res
                }
                _ => unreachable!(),
            },
        )
    }

    fn lower_function_with_key(
        &mut self,
        function_key: Key<super::objects::FunctionObjTag>,
    ) -> Result<&mut AnyObject<FunctionObj>, Diagnostic<Errors>> {
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
                    access: _,
                    parameters,
                    return_type,
                    body: _,
                    docs: _,
                    invoke: _,
                }) => {
                    self.push_generic_scope(generics, &mod_key)?;
                    let return_type = match return_type {
                        Some(ty) => ty.lower(self, mod_key)?,
                        None => AnyTypeKey::Void,
                    };
                    let fun = self.objects.functions.get_mut_unchecked(&function_key);
                    fun.data.return_type = InitState::Done(return_type);
                    fun.data.generic_scope = InitState::Done(self.generic_ctx.current());

                    let mut params = Vec::new();
                    for param in parameters {
                        let ty = param.ty.lower(self, mod_key)?;
                        params.push((param.ident.clone(), InitState::Done(ty)));
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
                    let type_of = self.types.functions.push_unique(type_of);
                    fun.data.type_of = InitState::Done(type_of);

                    match &fun.data.ir {
                        IrCache::Single(InitState::Progress(ir)) => {
                            self.ir_cache
                                .get_mut_unchecked(ir)
                                .const_stage_update(fun, function_key);
                        }
                        _ => (),
                    }

                    self.generic_ctx.pop();

                    fun
                }
                _ => unreachable!(),
            },
        )
    }

    fn lower_component_with_key(
        &mut self,
        component_key: Key<super::objects::ComponentObjTag>,
    ) -> Result<&mut AnyObject<ComponentObj>, Diagnostic<Errors>> {
        let this = self.objects.components.get_unchecked(&component_key);
        let ast_key = this.ast_object;
        let mod_key = this.module;
        let module_path = &self.types.modules.get_unchecked(&mod_key).path;
        Ok(
            if let ast::Object::Component {
                ty,
                docs: _,
                access: _,
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
                    None => AnyTypeKey::Void,
                };
                let obj = self.objects.components.get_mut_unchecked(&component_key);
                obj.data.ty = InitState::Done(ty);

                obj
            } else {
                unreachable!("It is a component you silly")
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
                access: _,
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
                    ConstEvalResult::Value(v) => v,
                    ConstEvalResult::Error(err) | ConstEvalResult::NotConst(err) => {
                        return Err(err);
                    }
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
        if this.data.generics.is_done() && this.data.ty.is_done() {
            return Ok(self.objects.types.get_mut_unchecked(&type_key));
        }
        let ast_key = this.ast_object;
        let mod_key = this.module;
        let module_path = &self.types.modules.get_unchecked(&mod_key).path;
        Ok(
            if let ast::Object::Type {
                ident: _,
                access: _,
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
                        _ => AnyTypeKey::Void,
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
                                    access: ast::AccessModifiers::Public,
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
                    let introduced_scope = self.generic_ctx.node(&self.generic_ctx.current());
                    let key = if !introduced_scope.values.is_empty() {
                        let parameters =
                            introduced_scope.values.iter().map(|(_, ty)| *ty).collect();
                        let this_type = PolymorphType {
                            inner: key,
                            parameters,
                        };
                        let this_key = self.types.polymorphs.push_unique(this_type);
                        AnyTypeKey::Polymorph(this_key)
                    } else {
                        key
                    };
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

    pub fn resolve_first_static_stop_of_path(
        &self,
        stop: &Span<SmolStr>,
        mod_key: ModuleKey,
    ) -> Result<PathNode, Error> {
        match self.generic_ctx.get(stop.deref()) {
            Some(_) => todo!("toznam"),
            None => (),
        }
        let current = PathNode::Module(mod_key);
        self.resolve_next_static_stop_of_path(stop, &current, &mod_key)
    }

    pub fn resolve_next_static_stop_of_path(
        &self,
        next: &Span<SmolStr>,
        current: &PathNode,
        module: &ModuleKey,
    ) -> Result<PathNode, Error> {
        match current {
            PathNode::Module(key) => {
                let module = self.types.modules.get_unchecked(key);
                match module.symbol_map.get(next.deref()) {
                    Some(AnyObjectKey::Import(obj_key)) => {
                        return Ok(PathNode::Module(
                            self.objects.imports.get_unchecked(obj_key).data.module,
                        ));
                    }
                    Some(any_key) => return Ok(PathNode::Object(*any_key)),
                    _ => Err(Error {
                        inner: Errors::ObjectNotFound(vec![next.deref().clone()]),
                        module: *key,
                        span: next.location,
                    })?,
                }
            }
            PathNode::Object(key) => match key {
                AnyObjectKey::Type(type_key) => {
                    let type_obj = self.objects.types.get_unchecked(type_key);
                    match type_obj.data.constants.get(next.deref()) {
                        Some(v) => return Ok(PathNode::Object(AnyObjectKey::Const(*v))),
                        _ => Err(Error {
                            inner: Errors::ObjectNotFound(vec![next.deref().clone()]),
                            module: *module,
                            span: next.location,
                        })?,
                    }
                }
                _ => todo!(),
            },
        }
    }

    pub fn resolve_const_path(
        &mut self,
        path: &[Span<SmolStr>],
        mod_key: ModuleKey,
        span: SpanIndex,
        generics: &Option<Span<Vec<Span<Type>>>>,
    ) -> Result<AnyObjectKey, Error> {
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
            PathNode::Object(key) => {
                self.lower_object_const_stage(key)?;
                Ok(key)
            }
        }
    }
}

pub enum PathNode {
    Module(ModuleKey),
    Object(AnyObjectKey),
}

pub enum ConstEvalResult {
    Value(ConstValue),
    NotConst(Error),
    Error(Error),
}

impl ast::Expression {
    pub fn const_eval(
        &self,
        ctx: &mut Context,
        mod_key: ModuleKey,
        self_def: &Option<ConstValue>,
        expect: &Option<AnyTypeKey>,
    ) -> ConstEvalResult {
        let location;
        let result = match self.const_reduce().as_ref() {
            ast::Expression::Value(value) => {
                location = value.location;
                if !value.postfix.is_empty() {
                    return ConstEvalResult::NotConst(Error {
                        inner: Errors::Todo("allow postfix for const evaluation"),
                        module: mod_key,
                        span: value.postfix.first().map(|op| op.location).unwrap(),
                    });
                }
                match value.literal.inner.as_ref() {
                    ast::Literal::Identifier(identifier_path) => {
                        match ctx.resolve_const_path(
                            &identifier_path.path.path,
                            mod_key,
                            identifier_path.path.location,
                            &identifier_path.generics,
                        ) {
                            Ok(key) => {
                                let obj_key = if let AnyObjectKey::Const(key) = key {
                                    key
                                } else {
                                    return ConstEvalResult::NotConst(Error {
                                        inner: Errors::NotConst,
                                        module: mod_key,
                                        span: identifier_path.path.location,
                                    });
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
                                        return ConstEvalResult::Error(Error {
                                            inner: Errors::SelfReferencial,
                                            module: mod_key,
                                            span: identifier_path.path.location,
                                        });
                                    }
                                    ConstObj { .. } => {
                                        let obj = match ctx.lower_const_with_key(obj_key) {
                                            Ok(ok) => ok,
                                            Err(err) => return ConstEvalResult::Error(err),
                                        };
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
                                        None => {
                                            return ConstEvalResult::NotConst(Diagnostic {
                                                span: value.location,
                                                module: mod_key,
                                                inner: Errors::UndefinedSelf,
                                            });
                                        }
                                    }
                                } else {
                                    return ConstEvalResult::NotConst(e);
                                }
                            }
                        }
                    }
                    ast::Literal::Structure { fields, kw: _, ty } => {
                        let mut const_fields = Vec::with_capacity(fields.len());
                        for field in fields {
                            let ident = field.0.clone();
                            let value = match field.1.const_eval(ctx, mod_key, self_def, &None) {
                                ConstEvalResult::Value(v) => v,
                                any => return any,
                            };
                            const_fields.push(Span::new(
                                (ident, Span::new(value, field.location)),
                                field.location,
                            ));
                        }
                        let ty = match ty {
                            Some(ty) => Some(
                                match resolve_type_path(ctx, mod_key, &ty.path, &ty.generics) {
                                    Ok(ok) => ok,
                                    Err(err) => return ConstEvalResult::NotConst(err),
                                },
                            ),
                            None => None,
                        };
                        let initial = ConstValue::Structure {
                            fields: const_fields,
                            ty,
                        };
                        match ty {
                            Some(ty) => match initial.implicit_cast(ctx, ty).map_err(|e| Error {
                                inner: e,
                                module: mod_key,
                                span: location,
                            }) {
                                Ok(ok) => ok,
                                Err(err) => return ConstEvalResult::Error(err),
                            },
                            None => initial,
                        }
                    }
                    ast::Literal::Number(number) => ConstValue::Number(number.clone()),
                    ast::Literal::String(smol_str) => ConstValue::String(smol_str.clone()),
                    ast::Literal::Char(c) => ConstValue::Char(*c),
                    ast::Literal::Array(exprs) => {
                        let mut values = Vec::new();
                        let mut inner_ty = None;
                        for expr in exprs {
                            let v = match expr.const_eval(ctx, mod_key, self_def, &inner_ty) {
                                ConstEvalResult::Value(v) => v,
                                any => return any,
                            };
                            let typeof_v = match v.type_of().map_err(|e| Error {
                                inner: e,
                                module: mod_key,
                                span: expr.location,
                            }) {
                                Ok(ok) => ok,
                                Err(err) => return ConstEvalResult::Error(err),
                            };
                            match &inner_ty {
                                Some(ty) => {}
                                None => inner_ty = Some(typeof_v),
                            }
                            values.push(expr.clone().map(|_| v));
                        }
                        let ty = ArrayType {
                            element_type: match inner_ty {
                                Some(ty) => ty,
                                None => {
                                    return ConstEvalResult::Error(Error {
                                        inner: Errors::FailedTypeInfer,
                                        module: mod_key,
                                        span: value.location,
                                    });
                                }
                            },
                            size: Some(values.len()),
                        };
                        ConstValue::Array {
                            elements: values,
                            ty: AnyTypeKey::Array(ctx.types.arrays.push_unique(ty)),
                        }
                    }
                    ast::Literal::Tuple(exprs) => {
                        let mut values = Vec::with_capacity(exprs.len());
                        for expr in exprs {
                            let v = match expr.const_eval(ctx, mod_key, self_def, &None) {
                                ConstEvalResult::Value(v) => v,
                                any => return any,
                            };
                            values.push(expr.clone().map(|_| v));
                        }
                        let mut types = Vec::with_capacity(values.len());
                        for v in &values {
                            types.push(
                                match v.type_of().map_err(|e| Error {
                                    inner: e,
                                    module: mod_key,
                                    span: v.location,
                                }) {
                                    Ok(ok) => ok,
                                    Err(err) => return ConstEvalResult::Error(err),
                                },
                            );
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
                let left_val = match l.const_eval(ctx, mod_key, self_def, &None) {
                    ConstEvalResult::Value(v) => v,
                    any => return any,
                };
                let typeof_l = match left_val.type_of().map_err(|e| Error {
                    inner: e,
                    module: mod_key,
                    span: l.location,
                }) {
                    Ok(ok) => ok,
                    Err(err) => return ConstEvalResult::Error(err),
                };
                let right_val = match r.const_eval(ctx, mod_key, self_def, &Some(typeof_l)) {
                    ConstEvalResult::Value(v) => v,
                    any => return any,
                };
                match op.const_apply(&left_val, &right_val, mod_key) {
                    Ok(v) => v,
                    Err(mut e) => {
                        e.span = span;
                        return ConstEvalResult::Error(e);
                    }
                }
            }
        };
        match expect {
            Some(e) => {
                let result = match result.implicit_cast(ctx, *e).map_err(|e| Error {
                    inner: e,
                    module: mod_key,
                    span: location,
                }) {
                    Ok(ok) => ok,
                    Err(err) => return ConstEvalResult::Error(err),
                };
                ConstEvalResult::Value(result)
            }
            None => ConstEvalResult::Value(result),
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
                        AnyTypeKey::Generic(*ty)
                    } else {
                        resolve_type_path(ctx, module, identifier_path, generic_arguments)?
                    }
                } else {
                    resolve_type_path(ctx, module, identifier_path, generic_arguments)?
                }
            }
            ast::TypeLiteral::Function(parameters, returns) => {
                let mut params = Vec::new();
                for param in parameters {
                    params.push(param.1.lower(ctx, module)?);
                }
                let returns = match returns {
                    Some(r) => r.lower(ctx, module)?,
                    None => AnyTypeKey::Void,
                };
                let ty = FunctionType {
                    parameters: params,
                    returns,
                };
                let key = ctx.types.functions.push_unique(ty);

                AnyTypeKey::Function(key)
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
                        Some(v) => Some(match v.const_eval(ctx, module, &None, &Some(ty)) {
                            ConstEvalResult::Value(v) => v,
                            ConstEvalResult::Error(err) | ConstEvalResult::NotConst(err) => {
                                return Err(err);
                            }
                        }),
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
                            Some(e) => match e.const_eval(ctx, module, &None, &Some(repr)) {
                                ConstEvalResult::Value(v) => v,
                                ConstEvalResult::Error(err) | ConstEvalResult::NotConst(err) => {
                                    return Err(err);
                                }
                            },
                            None => repr.const_default(ctx).map_err(|e| Error {
                                inner: e,
                                module,
                                span: ident.location,
                            })?,
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
                                let value = match expr.const_eval(
                                    ctx,
                                    module,
                                    &Some(last_value),
                                    &Some(repr),
                                ) {
                                    ConstEvalResult::Value(v) => v,
                                    ConstEvalResult::Error(err)
                                    | ConstEvalResult::NotConst(err) => {
                                        return Err(err);
                                    }
                                };

                                value
                            }
                            None => match step {
                                Some(step) => {
                                    match step.const_eval(
                                        ctx,
                                        module,
                                        &Some(last_value),
                                        &Some(repr),
                                    ) {
                                        ConstEvalResult::Value(v) => v,
                                        ConstEvalResult::Error(err)
                                        | ConstEvalResult::NotConst(err) => {
                                            return Err(err);
                                        }
                                    }
                                }
                                None => last_value.autostep().map_err(|e| Error {
                                    inner: e,
                                    module,
                                    span: ident.location,
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
                            ConstEvalResult::Value(ConstValue::Number(n)) => match n.value {
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
                            ConstEvalResult::Error(e) | ConstEvalResult::NotConst(e) => Err(e)?,
                            ConstEvalResult::Value(got) => Err(Diagnostic {
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
                    size,
                });
                AnyTypeKey::Array(key)
            }
            ast::TypeLiteral::Tuple(spans) => {
                let mut parameters = Vec::new();
                for ty in spans {
                    parameters.push(ty.lower(ctx, module)?);
                }
                match parameters.is_empty() {
                    true => AnyTypeKey::Void,
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
    let resolved = ctx.resolve_const_path(
        &identifier_path.path,
        module,
        identifier_path.location,
        generic_arguments,
    )?;

    Ok(match resolved {
        AnyObjectKey::Type(key) => {
            let TypeAliasObj { ty, generics, .. } =
                &mut ctx.objects.types.get_mut_unchecked(&key).data;

            let ty = match ty {
                InitState::Done(t) | InitState::Progress(t) => AnyTypeKey::Named(*t),
                _ => unreachable!("type alias not encountered during init"),
            };

            let generics = *generics.get_done();

            apply_generic_arguments(
                ctx,
                module,
                identifier_path.location,
                generic_arguments,
                generics,
                ty,
            )?
            .0
        }

        _ => unreachable!("all paths are expected to end with a type alias"),
    })
}

pub fn apply_generic_arguments(
    ctx: &mut Context,
    module: Key<ModuleTag>,
    identifier_location: SpanIndex,
    generic_arguments: &Option<Span<Vec<Span<Type>>>>,
    generics: Key<arena_scope::ArenaTag>,
    ty: AnyTypeKey,
) -> Result<(AnyTypeKey, Vec<(GenericKey, AnyTypeKey)>), Diagnostic<Errors>> {
    let gen_args: &Vec<_> = match generic_arguments {
        Some(generic_arguments) => generic_arguments.deref(),
        None => return Ok((ty, Vec::with_capacity(0))),
    };

    let generics = ctx
        .generic_ctx
        .arena()
        .get_unchecked(&generics)
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
                None => identifier_location,
            },
            module,
        });
    }

    let substitutions = generics
        .iter()
        .zip(gen_args)
        .map(|((_, constraint), arg)| {
            let substitution = arg.lower(ctx, module)?;
            Ok((*constraint, substitution))
        })
        .collect::<Result<Vec<_>, Error>>()?;

    let span = gen_args
        .first()
        .map(|arg| arg.location)
        .unwrap_or(identifier_location);

    let new = ty.substitute_many(&mut ctx.types).map_err(|e| Error {
        inner: e,
        module,
        span,
    })?;

    Ok((new, substitutions))
}
