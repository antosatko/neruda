use std::{collections::HashMap, sync::Arc};

use arena::{Arena, Key};
use smol_str::SmolStr;

use crate::{
    ast::{self, ConstValue},
    ir::{
        Context,
        types::{AnyTypeKey, ConstraintKey, ModuleKey, NamedTypeKey, TraitKey},
    },
};

#[derive(Debug)]
pub struct AnyObject<T> {
    pub data: T,
    pub identifier: SmolStr,
    pub ast_object: ast::AstObjectKey,
    pub module: ModuleKey,
}

#[derive(Debug, Default)]
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
    pub generics: Vec<(SmolStr, ConstraintKey)>,
    pub constants: HashMap<SmolStr, ConstObjKey>,
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
pub struct FunctionObjTag;
pub type FunctionObjKey = Key<FunctionObjTag>;
pub type FunctionObjArena = Arena<AnyObject<FunctionObj>, FunctionObjTag>;
#[derive(Debug)]
pub struct FunctionObj {
    pub return_type: InitState<AnyTypeKey, ()>,
    pub params: HashMap<SmolStr, InitState<AnyTypeKey, ()>>,
    pub generics: Vec<ConstraintKey>,
}

#[derive(Default)]
pub struct Objects {
    pub imports: ImportObjArena,
    pub constants: ConstObjArena,
    pub types: TypeAliasObjArena,
    pub traits: TraitObjArena,
    pub components: ComponentObjArena,
    pub functions: FunctionObjArena,
}

#[derive(Debug, Clone, Copy)]
pub enum AnyObjectKey {
    Import(ImportObjKey),
    Const(ConstObjKey),
    Type(TypeAliasObjKey),
    Trait(TraitObjKey),
    Component(ComponentObjKey),
    Function(FunctionObjKey),
}

#[derive(Debug)]
pub struct FunctionData {
    pub return_type: InitState<AnyTypeKey, ()>,
    pub params: HashMap<SmolStr, InitState<AnyTypeKey, ()>>,
    pub generics: Vec<ConstraintKey>,
}

impl<T> AnyObject<T> {
    pub fn new(
        identifier: SmolStr,
        data: T,
        ast_object: ast::AstObjectKey,
        module: ModuleKey,
    ) -> Self {
        Self {
            identifier,
            data,
            ast_object,
            module,
        }
    }

    #[track_caller]
    pub fn type_state_mut(&mut self) -> &mut InitState<AnyTypeKey, ()> {
        todo!()
    }

    #[track_caller]
    pub fn type_state_mut_eager(&mut self) -> &mut InitState<NamedTypeKey> {
        todo!()
    }

    #[track_caller]
    pub fn type_of(&self) -> Option<AnyTypeKey> {
        todo!()
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
}

impl Module {
    pub fn new(ast: Arc<ast::Module>) -> Self {
        Self {
            path: Default::default(),
            symbol_map: Default::default(),
            ast,
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
        }
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

/*impl AnyObjectData {
    pub fn stringify(&self, ctx: &Context) -> String {
        match self {
            Self::Import { module } => format!(
                "module {}",
                ctx.types.modules.get_unchecked(module).stringify()
            ),
            Self::Const { value, ty } => format!(
                "const: {} = {}",
                match ty {
                    InitState::Done(ty) => ty.stringify(&ctx.types),
                    _ => Cow::Borrowed("<type uninit>"),
                },
                match value {
                    InitState::Done(value) => value.stringify(),
                    _ => Cow::Borrowed("<value uninit>"),
                },
            ),
            Self::TypeAlias { ty, generics } => {
                let generics = match generics.is_empty() {
                    true => Cow::Borrowed(""),
                    _ => Cow::Owned(format!(
                        "<{}>",
                        generics
                            .iter()
                            .map(|(i, _)| i.as_str())
                            .collect::<Vec<&str>>()
                            .join(", ")
                            .as_str()
                    )),
                };
                format!(
                    "type{generics} = {}",
                    match ty {
                        InitState::Done(ty) => {
                            ctx.types.named.get_unchecked(ty).repr.stringify(&ctx.types)
                        }
                        _ => Cow::Borrowed("<type uninit>"),
                    }
                )
            }
            Self::Component { ty } => {
                format!(
                    "component = {}",
                    match ty {
                        InitState::Done(ty) => {
                            ty.stringify(&ctx.types)
                        }
                        _ => Cow::Borrowed("<type uninit>"),
                    }
                )
            }
            Self::Trait { .. } => "<trait>".to_string(),
            Self::Function(FunctionData {
                return_type,
                params,
                generics,
            }) => {
                let mut result = "function ".to_string();
                if !generics.is_empty() {
                    result.push('<');
                    let mut it = generics.iter();
                    if let Some(ty) = it.next() {
                        result.push_str(&format!(
                            "{}",
                            AnyTypeKey::Constraint(*ty).stringify(&ctx.types)
                        ));
                    }
                    for ty in it {
                        result.push_str(&format!(
                            ", {}",
                            AnyTypeKey::Constraint(*ty).stringify(&ctx.types)
                        ));
                    }
                    result.push('>');
                }
                if !params.is_empty() {
                    result.push('(');
                    let mut it = params.iter();
                    if let Some((ident, ty)) = it.next() {
                        result
                            .push_str(&format!("{ident}: {}", ty.get_done().stringify(&ctx.types)));
                    }
                    for (ident, ty) in it {
                        result.push_str(&format!(
                            ", {ident}: {}",
                            ty.get_done().stringify(&ctx.types)
                        ));
                    }
                    result.push(')');
                }
                result.push_str(&format!(
                    ": {}",
                    return_type.get_done().stringify(&ctx.types)
                ));
                result
            }
        }
    }
}*/
