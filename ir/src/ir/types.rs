use std::collections::HashMap;

use arena::{Arena, Key};
use smol_str::SmolStr;

use crate::ir::objects::AnyObjectKey;

pub type FunctionArena = Arena<FunctionType>;
pub type FunctionKey = Key<FunctionType>;
#[derive(PartialEq, Eq)]
pub struct FunctionType {
    pub returns: AnyTypeKey,
    pub parameters: Vec<(SmolStr, AnyTypeKey)>,
}

pub type TypeAliasArena = Arena<TypeAliasType>;
pub type TypeAliasKey = Key<TypeAliasType>;
#[derive(PartialEq, Eq)]
pub struct TypeAliasType {
    pub aliases: AnyTypeKey,
}

pub type ArrayArena = Arena<ArrayType>;
pub type ArrayKey = Key<ArrayType>;
#[derive(PartialEq, Eq)]
pub struct ArrayType {
    pub element_type: AnyTypeKey,
    pub size: Option<usize>,
}

pub type TupleArena = Arena<TupleType>;
pub type TupleKey = Key<TupleType>;
#[derive(PartialEq, Eq)]
pub struct TupleType {
    pub parameters: Vec<AnyTypeKey>,
}
pub type StructArena = Arena<StructType>;
pub type StructKey = Key<StructType>;
#[derive(PartialEq, Eq)]
pub struct StructType {
    pub parameters: Vec<(SmolStr, AnyTypeKey)>,
}

pub type ConstraintArena = Arena<ConstraintType>;
pub type ConstraintKey = Key<ConstraintType>;
#[derive(PartialEq, Eq)]
pub struct ConstraintType {
    pub constraints: (),
}

pub type GenericArena = Arena<GenericType>;
pub type GenericKey = Key<GenericType>;
#[derive(PartialEq, Eq)]
pub struct GenericType {
    pub generic_parameters: Vec<(SmolStr, ConstraintKey)>,
    pub inner: AnyTypeKey,
}

#[derive(PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveType {
    I8,
    I16,
    I32,
    I64,
    I128,
    U8,
    U16,
    U32,
    U64,
    U128,
    F32,
    F64,
    F32x2,
    F64x2,
    F32x4,
    F64x4,
    Char,
    Void,
}

#[derive(PartialEq, Eq)]
pub enum AnyTypeKey {
    Primitive(PrimitiveType),
    Constraint(ConstraintKey),
    Generic(GenericKey),
    Function(FunctionKey),
    TypeAlias(TypeAliasKey),
    Array(ArrayKey),
    Tuple(TupleKey),
    Struct(StructKey),
}

pub struct Module {
    pub symbols: HashMap<SmolStr, AnyObjectKey>,
}

#[derive(Default)]
pub struct Types {
    pub functions: FunctionArena,
    pub constraints: ConstraintArena,
    pub generics: GenericArena,
    pub type_aliases: TypeAliasArena,
    pub structures: StructArena,
    pub arrays: ArrayArena,
    pub tuples: TupleArena,
}

impl PrimitiveType {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "i8" => Some(Self::I8),
            "i16" => Some(Self::I16),
            "i32" => Some(Self::I32),
            "i64" => Some(Self::I64),
            "i128" => Some(Self::I128),
            "u8" => Some(Self::U8),
            "u16" => Some(Self::U16),
            "u32" => Some(Self::U32),
            "u64" => Some(Self::U64),
            "u128" => Some(Self::U128),
            "f32" => Some(Self::F32),
            "f64" => Some(Self::F64),
            "f32x2" => Some(Self::F32x2),
            "f64x2" => Some(Self::F64x2),
            "f32x4" => Some(Self::F32x4),
            "f64x4" => Some(Self::F64x4),
            "char" => Some(Self::Char),
            _ => None,
        }
    }
}
