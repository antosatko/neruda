use std::{borrow::Cow, collections::HashMap, sync::Arc};

use arena::{Arena, Key};
use smol_str::SmolStr;

use crate::{
    ast::{self, ConstValue},
    ir::{
        Context,
        types::{AnyTypeKey, ConstraintKey, ModuleKey, TraitKey},
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
        ty: InitState<AnyTypeKey, ()>,
        generics: Vec<ConstraintKey>,
    },
    Trait {
        ty: InitState<TraitKey>,
    },
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
            AnyObjectData::Const { ty, .. } => ty,
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
            } => *ty,
            _ => return None,
        })
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
                        InitState::Done(ty) => ty.stringify(&ctx.types),
                        _ => Cow::Borrowed("<type uninit>"),
                    }
                )
            }
            Self::Trait { .. } => "<trait>".to_string(),
        }
    }
}
