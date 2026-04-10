use std::{borrow::Cow, collections::HashMap, sync::Arc};

use arena::{Arena, Key};
use smol_str::SmolStr;

use crate::{
    ast::{self, ConstValue},
    ir::{
        Context,
        types::{AnyTypeKey, ConstraintKey, ModuleKey},
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
}

impl AnyObject {
    pub fn new(identifier: SmolStr, data: AnyObjectData, ast_object: ast::AstObjectKey) -> Self {
        Self {
            identifier,
            data,
            ast_object,
        }
    }

    pub fn type_mut(&mut self) -> &mut InitState<AnyTypeKey, ()> {
        match &mut self.data {
            AnyObjectData::Import { .. } => panic!("Object import has no type"),
            AnyObjectData::Const { ty, .. } => ty,
            AnyObjectData::TypeAlias { ty, .. } => ty,
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
                    _ => format!("<{}>", 5),
                };
                format!(
                    "type{generics} = {}",
                    match ty {
                        InitState::Done(ty) => ty.stringify(&ctx.types),
                        _ => Cow::Borrowed("<type uninit>"),
                    }
                )
            }
        }
    }
}
