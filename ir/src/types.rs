use std::collections::HashMap;

use arena::{Arena, Key};
use smol_str::SmolStr;

pub type FunctionArena = Arena<FunctionType>;
pub type FunctionKey = Key<FunctionType>;
pub struct FunctionType {
    pub returns: AnyTypeKey,
    pub parameters: Vec<(SmolStr, AnyTypeKey)>,
}

pub type TypeAliasArena = Arena<TypeAliasType>;
pub type TypeAliasKey = Key<TypeAliasType>;
pub struct TypeAliasType {
    pub aliases: AnyTypeKey,
}

pub enum AnyTypeKey {
    Function(FunctionKey),
    TypeAlias(TypeAliasKey),
}

pub struct Context {
    pub types: Types,
}

pub struct Module {
    pub symbols: HashMap<SmolStr, AnyTypeKey>,
}

pub struct Types {
    pub functions: FunctionArena,
    pub type_aliases: TypeAliasArena,
}
