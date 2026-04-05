use std::{borrow::Cow, collections::HashMap};

use arena::{Arena, Key};
use smol_str::SmolStr;

use crate::ir::objects::AnyObjectKey;

pub type FunctionArena = Arena<FunctionType>;
pub type FunctionKey = Key<FunctionType>;
#[derive(PartialEq, Debug)]
pub struct FunctionType {
    pub returns: AnyTypeKey,
    pub parameters: Vec<(SmolStr, AnyTypeKey)>,
}

pub type ArrayArena = Arena<ArrayType>;
pub type ArrayKey = Key<ArrayType>;
#[derive(PartialEq, Debug)]
pub struct ArrayType {
    pub element_type: AnyTypeKey,
    pub size: Option<usize>,
}

pub type TupleArena = Arena<TupleType>;
pub type TupleKey = Key<TupleType>;
#[derive(PartialEq, Debug)]
pub struct TupleType {
    pub parameters: Vec<AnyTypeKey>,
}
pub type StructArena = Arena<StructType>;
pub type StructKey = Key<StructType>;
#[derive(PartialEq, Debug)]
pub struct StructType {
    pub parameters: Vec<(SmolStr, AnyTypeKey)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ConstraintTag;
pub type ConstraintArena = Arena<ConstraintType, ConstraintTag>;
pub type ConstraintKey = Key<ConstraintTag>;
#[derive(PartialEq, Debug)]
pub struct ConstraintType {
    pub constraints: (),
}

pub type GenericArena = Arena<GenericType>;
pub type GenericKey = Key<GenericType>;
#[derive(PartialEq, Debug)]
pub struct GenericType {
    pub generic_parameters: Vec<(SmolStr, ConstraintKey)>,
    pub inner: AnyTypeKey,
}

#[derive(PartialEq, Debug)]
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

#[derive(PartialEq, Debug)]
pub enum AnyTypeKey {
    Primitive(PrimitiveType),
    Constraint(ConstraintKey),
    Generic(GenericKey),
    Function(FunctionKey),
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

    pub fn stringify(&self) -> &'static str {
        match self {
            PrimitiveType::I8 => "i8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::I128 => "i128",
            PrimitiveType::U8 => "u8",
            PrimitiveType::U16 => "u16",
            PrimitiveType::U32 => "u32",
            PrimitiveType::U64 => "u64",
            PrimitiveType::U128 => "u128",
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
            PrimitiveType::F32x2 => "f32x2",
            PrimitiveType::F64x2 => "f64x2",
            PrimitiveType::F32x4 => "f32x4",
            PrimitiveType::F64x4 => "f64x4",
            PrimitiveType::Char => "char",
            PrimitiveType::Void => "()",
        }
    }
}

impl FunctionType {
    pub fn stringify(&self, types: &Types) -> String {
        let mut out = String::from("function(");
        let mut iter = self.parameters.iter();
        if let Some((ident, ty)) = iter.next() {
            out.push_str(&format!("{ident}: {}", ty.stringify(types)));
        }
        for (ident, ty) in iter {
            out.push_str(&format!(", {ident}: {}", ty.stringify(types)));
        }
        out.push(')');
        out.push_str(&format!(": {}", self.returns.stringify(types)));
        out
    }
}

impl StructType {
    pub fn stringify(&self, types: &Types) -> String {
        let mut out = String::from("struct { ");
        for (ident, ty) in &self.parameters {
            out.push_str(&format!("{ident}: {} ", ty.stringify(types)));
        }
        out.push('}');
        out
    }
}

impl ArrayType {
    pub fn stringify(&self, types: &Types) -> String {
        let mut out = format!("[{}", self.element_type.stringify(types));
        match self.size {
            Some(size) => out.push_str(&format!("; {size}]")),
            None => out.push(']'),
        }
        out
    }
}

impl TupleType {
    pub fn stringify(&self, types: &Types) -> String {
        let mut out = String::from("(");
        let mut iter = self.parameters.iter();
        if let Some(ty) = iter.next() {
            out.push_str(&ty.stringify(types));
        }
        for ty in iter {
            out.push_str(&format!(", {}", ty.stringify(types)));
        }
        out.push(')');
        out
    }
}

impl GenericType {
    pub fn stringify(&self, types: &Types) -> String {
        let mut out = String::from("<");
        let mut iter = self.generic_parameters.iter();
        if let Some((ident, constraints)) = iter.next() {
            out.push_str(&format!(
                "{ident}{}",
                types
                    .constraints
                    .get_unchecked(constraints)
                    .stringify(types)
            ));
        }
        for (ident, constraints) in iter {
            out.push_str(&format!(
                ", {ident}{}",
                types
                    .constraints
                    .get_unchecked(constraints)
                    .stringify(types)
            ));
        }
        out.push('>');
        out.push_str(&self.inner.stringify(types));
        out
    }
}

impl ConstraintType {
    pub fn stringify(&self, _: &Types) -> String {
        "".to_string()
    }
}

impl AnyTypeKey {
    pub fn stringify(&self, types: &Types) -> Cow<'static, str> {
        match self {
            AnyTypeKey::Primitive(primitive_type) => Cow::Borrowed(primitive_type.stringify()),
            AnyTypeKey::Constraint(key) => {
                Cow::Owned(types.constraints.get_unchecked(key).stringify(types))
            }
            AnyTypeKey::Generic(key) => {
                Cow::Owned(types.generics.get_unchecked(key).stringify(types))
            }
            AnyTypeKey::Function(key) => {
                Cow::Owned(types.functions.get_unchecked(key).stringify(types))
            }
            AnyTypeKey::Array(key) => Cow::Owned(types.arrays.get_unchecked(key).stringify(types)),
            AnyTypeKey::Tuple(key) => Cow::Owned(types.tuples.get_unchecked(key).stringify(types)),
            AnyTypeKey::Struct(key) => {
                Cow::Owned(types.structures.get_unchecked(key).stringify(types))
            }
        }
    }
}
