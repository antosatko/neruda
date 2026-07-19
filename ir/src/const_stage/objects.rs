use std::{collections::HashMap, sync::Arc};

use arena::{Arena, Key};
use arena_scope::ScopeKey;
use smol_str::SmolStr;

use crate::{
    ast::{self, AccessModifiers, ConstValue, Span},
    const_stage::{
        Context, Errors,
        types::{AnyTypeKey, ConstraintKey, FunctionKey, ModuleKey, NamedTypeKey, TraitKey},
    },
    ir::{FunctionIr, FunctionIrKey},
};

#[derive(Debug)]
pub struct AnyObject<T> {
    pub data: T,
    pub access: AccessModifiers,
    pub identifier: SmolStr,
    pub ast_object: Option<ast::AstObjectKey>,
    pub module: ModuleKey,
}

#[derive(Debug, Default, Clone, PartialEq)]
pub enum InitState<T, U = T> {
    Done(T),
    Progress(U),
    #[default]
    Uninitialized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ImportObjTag;
pub type ImportObjKey = Key<ImportObjTag>;
pub type ImportObjArena = Arena<AnyObject<ImportObj>, ImportObjTag>;
#[derive(Debug)]
pub struct ImportObj {
    pub module: ModuleKey,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConstObjTag;
pub type ConstObjKey = Key<ConstObjTag>;
pub type ConstObjArena = Arena<AnyObject<ConstObj>, ConstObjTag>;
#[derive(Debug)]
pub struct ConstObj {
    pub value: InitState<ConstValue, ()>,
    pub ty: InitState<AnyTypeKey, ()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TypeAliasObjTag;
pub type TypeAliasObjKey = Key<TypeAliasObjTag>;
pub type TypeAliasObjArena = Arena<AnyObject<TypeAliasObj>, TypeAliasObjTag>;
#[derive(Debug)]
pub struct TypeAliasObj {
    pub ty: InitState<NamedTypeKey>,
    pub generics: InitState<ScopeKey, ()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct TraitObjTag;
pub type TraitObjKey = Key<TraitObjTag>;
pub type TraitObjArena = Arena<AnyObject<TraitObj>, TraitObjTag>;
#[derive(Debug)]
pub struct TraitObj {
    pub ty: InitState<TraitKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ComponentObjTag;
pub type ComponentObjKey = Key<ComponentObjTag>;
pub type ComponentObjArena = Arena<AnyObject<ComponentObj>, ComponentObjTag>;
#[derive(Debug)]
pub struct ComponentObj {
    pub ty: InitState<AnyTypeKey, ()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ResourceObjTag;
pub type ResourceObjKey = Key<ResourceObjTag>;
pub type ResourceObjArena = Arena<AnyObject<ResourceObj>, ResourceObjTag>;
#[derive(Debug)]
pub struct ResourceObj {
    pub ty: InitState<AnyTypeKey, ()>,
    pub optional: bool,
    pub default: InitState<Option<ConstValue>, ()>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct FunctionObjTag;
pub type FunctionObjKey = Key<FunctionObjTag>;
pub type FunctionObjArena = Arena<AnyObject<FunctionObj>, FunctionObjTag>;
#[derive(Debug)]
pub struct FunctionObj {
    pub return_type: InitState<AnyTypeKey, ()>,
    pub params: Vec<(Span<SmolStr>, InitState<AnyTypeKey, ()>)>,
    pub generics: Vec<ConstraintKey>,
    pub type_of: InitState<FunctionKey, ()>,
    pub ir: IrCache,
    pub generic_scope: InitState<ScopeKey, ()>,
}

#[derive(Debug)]
pub enum IrCache {
    Single(InitState<FunctionIrKey>),
    Polymorphic(HashMap<AnyTypeKey, InitState<FunctionIrKey>>),
}

#[derive(Default)]
pub struct Objects {
    pub imports: ImportObjArena,
    pub constants: ConstObjArena,
    pub types: TypeAliasObjArena,
    pub traits: TraitObjArena,
    pub components: ComponentObjArena,
    pub functions: FunctionObjArena,
    pub resources: ResourceObjArena,
}

#[derive(Debug, Clone, Copy)]
pub enum AnyObjectKey {
    Import(ImportObjKey),
    Const(ConstObjKey),
    Type(TypeAliasObjKey),
    Trait(TraitObjKey),
    Component(ComponentObjKey),
    Function(FunctionObjKey),
    Resource(ResourceObjKey),
}

#[derive(Debug)]
pub struct FunctionData {
    pub return_type: InitState<AnyTypeKey, ()>,
    pub params: HashMap<SmolStr, InitState<AnyTypeKey, ()>>,
    pub generics: Vec<ConstraintKey>,
    pub ir: InitState<FunctionIr, ()>,
}

impl<T> AnyObject<T> {
    pub fn new(
        identifier: SmolStr,
        data: T,
        ast_object: Option<ast::AstObjectKey>,
        module: ModuleKey,
        access: AccessModifiers,
    ) -> Self {
        Self {
            identifier,
            access,
            data,
            ast_object,
            module,
        }
    }
}

impl<T> InitState<T, T> {
    pub fn mark_done(&mut self) {
        let new = match std::mem::replace(self, InitState::Uninitialized) {
            InitState::Done(v) => InitState::Done(v),
            InitState::Progress(v) => InitState::Done(v),
            InitState::Uninitialized => InitState::Uninitialized,
        };
        *self = new;
    }

    #[track_caller]
    pub fn get(&self) -> &T {
        match self {
            InitState::Done(v) => v,
            InitState::Progress(v) => v,
            InitState::Uninitialized => panic!("uninitialized"),
        }
    }
}

impl<T, U> InitState<T, U> {
    #[track_caller]
    pub fn get_done(&self) -> &T {
        match self {
            InitState::Done(v) => v,
            InitState::Progress(_) => panic!("in progress"),
            InitState::Uninitialized => panic!("uninitialized"),
        }
    }

    /// Returns `true` if the init state is [`Done`].
    ///
    /// [`Done`]: InitState::Done
    #[must_use]
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Done(..))
    }
}

pub struct Module {
    pub path: Vec<SmolStr>,
    pub symbol_map: HashMap<SmolStr, AnyObjectKey>,
    pub ast: Arc<ast::Module>,
    pub src: Arc<String>,
}

impl Module {
    pub fn new(ast: Arc<ast::Module>, src: Arc<String>) -> Self {
        Self {
            path: Default::default(),
            symbol_map: Default::default(),
            ast,
            src,
        }
    }
}

impl AnyObjectKey {
    pub fn ident<'a>(&self, ctx: &'a Context) -> &'a SmolStr {
        match self {
            AnyObjectKey::Import(key) => &ctx.objects.imports.get_unchecked(key).identifier,
            AnyObjectKey::Const(key) => &ctx.objects.constants.get_unchecked(key).identifier,
            AnyObjectKey::Type(key) => &ctx.objects.types.get_unchecked(key).identifier,
            AnyObjectKey::Trait(key) => &ctx.objects.traits.get_unchecked(key).identifier,
            AnyObjectKey::Component(key) => &ctx.objects.components.get_unchecked(key).identifier,
            AnyObjectKey::Function(key) => &ctx.objects.functions.get_unchecked(key).identifier,
            AnyObjectKey::Resource(key) => &ctx.objects.resources.get_unchecked(key).identifier,
        }
    }

    pub fn access(&self, ctx: &Context) -> AccessModifiers {
        match self {
            AnyObjectKey::Import(key) => ctx.objects.imports.get_unchecked(key).access,
            AnyObjectKey::Const(key) => ctx.objects.constants.get_unchecked(key).access,
            AnyObjectKey::Type(key) => ctx.objects.types.get_unchecked(key).access,
            AnyObjectKey::Trait(key) => ctx.objects.traits.get_unchecked(key).access,
            AnyObjectKey::Component(key) => ctx.objects.components.get_unchecked(key).access,
            AnyObjectKey::Function(key) => ctx.objects.functions.get_unchecked(key).access,
            AnyObjectKey::Resource(key) => ctx.objects.resources.get_unchecked(key).access,
        }
    }

    pub fn module(&self, ctx: &Context) -> ModuleKey {
        match self {
            AnyObjectKey::Import(key) => ctx.objects.imports.get_unchecked(key).module,
            AnyObjectKey::Const(key) => ctx.objects.constants.get_unchecked(key).module,
            AnyObjectKey::Type(key) => ctx.objects.types.get_unchecked(key).module,
            AnyObjectKey::Trait(key) => ctx.objects.traits.get_unchecked(key).module,
            AnyObjectKey::Component(key) => ctx.objects.components.get_unchecked(key).module,
            AnyObjectKey::Function(key) => ctx.objects.functions.get_unchecked(key).module,
            AnyObjectKey::Resource(key) => ctx.objects.resources.get_unchecked(key).module,
        }
    }

    pub fn type_of(&self, ctx: &Context) -> Result<AnyTypeKey, Errors> {
        Ok(match self {
            AnyObjectKey::Const(key) => {
                *ctx.objects.constants.get_unchecked(key).data.ty.get_done()
            }
            AnyObjectKey::Trait(key) => {
                AnyTypeKey::Trait(*ctx.objects.traits.get_unchecked(key).data.ty.get_done())
            }
            AnyObjectKey::Component(key) => {
                *ctx.objects.components.get_unchecked(key).data.ty.get_done()
            }
            AnyObjectKey::Function(key) => AnyTypeKey::Function(
                *ctx.objects
                    .functions
                    .get_unchecked(key)
                    .data
                    .type_of
                    .get_done(),
            ),
            AnyObjectKey::Resource(key) => {
                *ctx.objects.resources.get_unchecked(key).data.ty.get_done()
            }
            AnyObjectKey::Type(ty) => {
                AnyTypeKey::Named(*ctx.objects.types.get_unchecked(ty).data.ty.get_done())
            }
            AnyObjectKey::Import(_) => Err(Errors::FailedTypeInfer)?,
        })
    }
}

impl From<ImportObjKey> for AnyObjectKey {
    fn from(value: ImportObjKey) -> Self {
        AnyObjectKey::Import(value)
    }
}

impl From<ConstObjKey> for AnyObjectKey {
    fn from(value: ConstObjKey) -> Self {
        AnyObjectKey::Const(value)
    }
}

impl From<FunctionObjKey> for AnyObjectKey {
    fn from(value: FunctionObjKey) -> Self {
        AnyObjectKey::Function(value)
    }
}

impl From<ComponentObjKey> for AnyObjectKey {
    fn from(value: ComponentObjKey) -> Self {
        AnyObjectKey::Component(value)
    }
}

impl From<TraitObjKey> for AnyObjectKey {
    fn from(value: TraitObjKey) -> Self {
        AnyObjectKey::Trait(value)
    }
}

impl From<TypeAliasObjKey> for AnyObjectKey {
    fn from(value: TypeAliasObjKey) -> Self {
        AnyObjectKey::Type(value)
    }
}
