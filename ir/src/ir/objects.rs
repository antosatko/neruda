use std::{borrow::Cow, collections::HashMap};

use arena::{Arena, Key};
use smol_str::SmolStr;

use crate::{
    ast::{self, ConstValue},
    ir::{
        Context,
        types::{AnyTypeKey, ConstraintKey, ModuleKey, NamedTypeKey, TraitKey},
    },
};

#[derive(Debug, Clone)]
pub struct AnyObjectTag;
pub type AnyObjectkey = Key<AnyObjectTag>;

#[derive(Debug)]
pub struct AnyObject {
    pub data: AnyObjectData,
    pub identifier: SmolStr,
    pub ast_object: ast::AstObjectKey,
}

#[derive(Debug, Default)]
pub enum InitState<T, U = T> {
    Done(T),
    Progress(U),
    #[default]
    Uninitialized,
}

#[derive(Debug)]
pub enum AnyObjectData {
    Import {
        module: ModuleKey,
    },
    Const {
        value: InitState<ConstValue, ()>,
        ty: InitState<AnyTypeKey, ()>,
    },
    TypeAlias {
        ty: InitState<NamedTypeKey>,
        generics: Vec<ConstraintKey>,
    },
    Trait {
        ty: InitState<TraitKey>,
    },
    Function(FunctionData),
}

#[derive(Debug)]
pub struct FunctionData {
    pub return_type: InitState<AnyTypeKey, ()>,
    pub params: HashMap<SmolStr, InitState<AnyTypeKey, ()>>,
    pub generics: Vec<ConstraintKey>,
}

impl AnyObject {
    pub fn new(identifier: SmolStr, data: AnyObjectData, ast_object: ast::AstObjectKey) -> Self {
        Self {
            identifier,
            data,
            ast_object,
        }
    }

    #[track_caller]
    pub fn type_state_mut(&mut self) -> &mut InitState<AnyTypeKey, ()> {
        match &mut self.data {
            AnyObjectData::Import { .. } => panic!("Object import has no type state"),
            AnyObjectData::Trait { .. } => panic!("Object trait has no type state"),
            AnyObjectData::TypeAlias { .. } => panic!("Object TypeAlias is eager"),
            AnyObjectData::Function(_) => panic!("not applicable to function"),
            AnyObjectData::Const { ty, .. } => ty,
        }
    }

    #[track_caller]
    pub fn type_state_mut_eager(&mut self) -> &mut InitState<NamedTypeKey> {
        match &mut self.data {
            AnyObjectData::Import { .. } => panic!("Object import has no type state"),
            AnyObjectData::Trait { .. } => panic!("Object trait has no type state"),
            AnyObjectData::Const { .. } => panic!("Object const is not eager"),
            AnyObjectData::Function(_) => panic!("not applicable to function"),
            AnyObjectData::TypeAlias { ty, .. } => ty,
        }
    }

    #[track_caller]
    pub fn type_of(&self) -> Option<AnyTypeKey> {
        Some(match &self.data {
            AnyObjectData::Import { module } => AnyTypeKey::ModuleRef(*module),
            AnyObjectData::Trait {
                ty: InitState::Done(ty),
            } => AnyTypeKey::Trait(*ty),
            AnyObjectData::Const {
                ty: InitState::Done(ty),
                ..
            } => *ty,
            AnyObjectData::TypeAlias {
                ty: InitState::Done(ty),
                ..
            } => AnyTypeKey::Named(*ty),
            _ => return None,
        })
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
}

#[derive(Default)]
pub struct Module {
    pub path: Vec<SmolStr>,
    pub objects: Arena<AnyObject, AnyObjectTag>,
    pub symbol_map: HashMap<SmolStr, AnyObjectkey>,
}

impl AnyObjectData {
    pub fn stringify(&self, ctx: &Context) -> String {
        match self {
            Self::Import { module } => format!(
                "module {}",
                ctx.types
                    .modules
                    .get_unchecked(module)
                    .stringify(&ctx.types)
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
                let generics = match generics.len() {
                    0 => "".to_string(),
                    l => format!("<{}>", l),
                };
                format!(
                    "type{generics} = {}",
                    match ty {
                        InitState::Done(ty) => AnyTypeKey::Named(*ty).stringify(&ctx.types),
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
}
