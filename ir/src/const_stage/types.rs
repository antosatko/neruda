use std::borrow::Cow;

use arena::{Arena, Key};
use smol_str::{SmolStr, ToSmolStr};

use crate::{
    ast::{ConstValue, Number, NumberValue},
    const_stage::{Errors, objects::Module},
};

pub type FunctionArena = Arena<FunctionType, FunctionTag>;
pub type FunctionKey = Key<FunctionTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct FunctionTag;
#[derive(PartialEq, Debug, Clone)]
pub struct FunctionType {
    pub returns: AnyTypeKey,
    pub parameters: Vec<AnyTypeKey>,
}

pub type ArrayArena = Arena<ArrayType, ArrayTag>;
pub type ArrayKey = Key<ArrayTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct ArrayTag;
#[derive(PartialEq, Debug, Clone)]
pub struct ArrayType {
    pub element_type: AnyTypeKey,
    pub size: Option<usize>,
}

pub type TupleArena = Arena<TupleType, TupleTag>;
pub type TupleKey = Key<TupleTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct TupleTag;
#[derive(PartialEq, Debug, Clone)]
pub struct TupleType {
    pub parameters: Vec<AnyTypeKey>,
}

pub type TraitArena = Arena<TraitType, TraitTag>;
pub type TraitKey = Key<TraitTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct TraitTag;
#[derive(PartialEq, Debug, Clone)]
pub struct TraitType {
    pub ident: SmolStr,
}

pub type StructArena = Arena<StructType, StructTag>;
pub type StructKey = Key<StructTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct StructTag;
#[derive(PartialEq, Debug, Clone)]
pub struct StructType {
    pub parameters: Vec<(SmolStr, AnyTypeKey, Option<ConstValue>)>,
}

pub type EnumArena = Arena<EnumType, EnumTag>;
pub type EnumKey = Key<EnumTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct EnumTag;
#[derive(PartialEq, Debug, Clone)]
pub struct EnumType {
    pub repr: AnyTypeKey,
    pub variants: Vec<(SmolStr, ConstValue)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct ConstraintTag;
pub type ConstraintArena = Arena<ConstraintType, ConstraintTag>;
pub type ConstraintKey = Key<ConstraintTag>;
#[derive(PartialEq, Debug, Clone)]
pub struct ConstraintType {
    pub constraints: Vec<TraitKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct MorphedTag;
pub type MorphedArena = Arena<MorphedType, MorphedTag>;
pub type MorphedKey = Key<MorphedTag>;
#[derive(PartialEq, Debug, Clone)]
pub struct MorphedType {
    pub parent: PolymorphKey,
    pub this: AnyTypeKey,
    pub arguments: Vec<AnyTypeKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct NamedTypeTag;
pub type NamedTypeArena = Arena<NamedTypeType, NamedTypeTag>;
pub type NamedTypeKey = Key<NamedTypeTag>;
#[derive(PartialEq, Debug, Clone)]
pub struct NamedTypeType {
    pub name: SmolStr,
    pub repr: AnyTypeKey,
}

pub type GenericArena = Arena<GenericType, GenericTag>;
pub type GenericKey = Key<GenericTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash, Eq)]
pub struct GenericTag;
#[derive(PartialEq, Debug, Clone)]
pub struct GenericType {
    pub constraint: ConstraintKey,
    pub ident: SmolStr,
}

pub type PolymorphArena = Arena<PolymorphType, PolymorphTag>;
pub type PolymorphKey = Key<PolymorphTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct PolymorphTag;
#[derive(PartialEq, Debug, Clone)]
pub struct PolymorphType {
    pub parameters: Vec<GenericKey>,
    pub inner: AnyTypeKey,
}

pub type RefArena = Arena<RefType, RefTag>;
pub type RefKey = Key<RefTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct RefTag;
#[derive(PartialEq, Debug, Clone)]
pub struct RefType {
    pub inner: AnyTypeKey,
}

pub type ModuleArena = Arena<Module, ModuleTag>;
pub type ModuleKey = Key<ModuleTag>;
#[derive(PartialEq, Debug, Clone, Copy, Hash)]
pub struct ModuleTag;

#[derive(PartialEq, Debug, Copy, Clone, Hash)]
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
    Bool,
    Void,
    EntityRef,
}

#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub enum AnyTypeKey {
    Primitive(PrimitiveType),
    Function(FunctionKey),
    Array(ArrayKey),
    Tuple(TupleKey),
    Struct(StructKey),
    Enum(EnumKey),
    Trait(TraitKey),
    Reference(RefKey),
    Named(NamedTypeKey),
    ModuleRef(ModuleKey),
    Polymorph(PolymorphKey),
    Generic(GenericKey),
    Morphed(MorphedKey),
}

#[derive(Default)]
pub struct Types {
    pub functions: FunctionArena,
    pub constraints: ConstraintArena,
    pub generics: GenericArena,
    pub polymorphs: PolymorphArena,
    pub morphs: MorphedArena,
    pub structures: StructArena,
    pub enums: EnumArena,
    pub arrays: ArrayArena,
    pub tuples: TupleArena,
    pub traits: TraitArena,
    pub references: RefArena,
    pub modules: ModuleArena,
    pub named: NamedTypeArena,
}

pub struct AutoTypes {
    pub any_trt: TraitKey,
    pub any_conr: ConstraintKey,
}

impl AutoTypes {
    pub fn new(types: &mut Types) -> Self {
        let any_trt = types.traits.push(TraitType {
            ident: "Any".to_smolstr(),
        });
        Self {
            any_conr: types.constraints.push(ConstraintType {
                constraints: [any_trt].into(),
            }),
            any_trt,
        }
    }
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
            "bool" => Some(Self::Bool),
            "entity" => Some(Self::EntityRef),
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
            PrimitiveType::Bool => "bool",
            PrimitiveType::Void => "()",
            PrimitiveType::EntityRef => "entity",
        }
    }

    pub fn default(&self) -> ConstValue {
        match self {
            PrimitiveType::I8 => ConstValue::Number(Number {
                value: NumberValue::Int(0),
                size: Some(8),
            }),
            PrimitiveType::I16 => ConstValue::Number(Number {
                value: NumberValue::Int(0),
                size: Some(16),
            }),
            PrimitiveType::I32 => ConstValue::Number(Number {
                value: NumberValue::Int(0),
                size: Some(32),
            }),
            PrimitiveType::I64 => ConstValue::Number(Number {
                value: NumberValue::Int(0),
                size: Some(64),
            }),
            PrimitiveType::I128 => ConstValue::Number(Number {
                value: NumberValue::Int(0),
                size: Some(128),
            }),

            PrimitiveType::U8 => ConstValue::Number(Number {
                value: NumberValue::Uint(0),
                size: Some(8),
            }),
            PrimitiveType::U16 => ConstValue::Number(Number {
                value: NumberValue::Uint(0),
                size: Some(16),
            }),
            PrimitiveType::U32 => ConstValue::Number(Number {
                value: NumberValue::Uint(0),
                size: Some(32),
            }),
            PrimitiveType::U64 => ConstValue::Number(Number {
                value: NumberValue::Uint(0),
                size: Some(64),
            }),
            PrimitiveType::U128 => ConstValue::Number(Number {
                value: NumberValue::Uint(0),
                size: Some(128),
            }),
            PrimitiveType::F32 => ConstValue::Number(Number {
                value: NumberValue::Float(0.0),
                size: Some(32),
            }),
            PrimitiveType::F64 => ConstValue::Number(Number {
                value: NumberValue::Float(0.0),
                size: Some(64),
            }),
            PrimitiveType::Char => ConstValue::Char(0 as char),
            PrimitiveType::Bool => ConstValue::Bool(false),
            PrimitiveType::Void => ConstValue::Tuple {
                elements: Vec::with_capacity(0),
                ty: AnyTypeKey::Primitive(PrimitiveType::Void),
            },
            PrimitiveType::F32x2 => todo!(),
            PrimitiveType::F64x2 => todo!(),
            PrimitiveType::F32x4 => todo!(),
            PrimitiveType::F64x4 => todo!(),
            PrimitiveType::EntityRef => todo!(),
        }
    }
}

impl PrimitiveType {
    pub fn int_size(&self) -> Option<u32> {
        match self {
            PrimitiveType::I8 => Some(8),
            PrimitiveType::I16 => Some(16),
            PrimitiveType::I32 => Some(32),
            PrimitiveType::I64 => Some(64),
            PrimitiveType::I128 => Some(128),
            _ => None,
        }
    }

    pub fn uint_size(&self) -> Option<u32> {
        match self {
            PrimitiveType::U8 => Some(8),
            PrimitiveType::U16 => Some(16),
            PrimitiveType::U32 => Some(32),
            PrimitiveType::U64 => Some(64),
            PrimitiveType::U128 => Some(128),
            _ => None,
        }
    }

    pub fn float_size(&self) -> Option<u32> {
        match self {
            PrimitiveType::F32 => Some(32),
            PrimitiveType::F64 => Some(64),
            _ => None,
        }
    }

    pub fn number_size(&self) -> Option<u32> {
        match self {
            PrimitiveType::I8 => Some(8),
            PrimitiveType::I16 => Some(16),
            PrimitiveType::I32 => Some(32),
            PrimitiveType::I64 => Some(64),
            PrimitiveType::I128 => Some(128),
            PrimitiveType::U8 => Some(8),
            PrimitiveType::U16 => Some(16),
            PrimitiveType::U32 => Some(32),
            PrimitiveType::U64 => Some(64),
            PrimitiveType::U128 => Some(128),
            PrimitiveType::F32 => Some(32),
            PrimitiveType::F64 => Some(64),
            _ => None,
        }
    }

    pub fn is_numeric(&self) -> bool {
        match self {
            PrimitiveType::I8 => true,
            PrimitiveType::I16 => true,
            PrimitiveType::I32 => true,
            PrimitiveType::I64 => true,
            PrimitiveType::I128 => true,
            PrimitiveType::U8 => true,
            PrimitiveType::U16 => true,
            PrimitiveType::U32 => true,
            PrimitiveType::U64 => true,
            PrimitiveType::U128 => true,
            PrimitiveType::F32 => true,
            PrimitiveType::F64 => true,
            _ => false,
        }
    }
}

impl FunctionType {
    pub fn stringify(&self, types: &Types) -> String {
        let mut out = String::from("function(");
        let mut iter = self.parameters.iter();
        if let Some(ty) = iter.next() {
            out.push_str(&format!("{}", ty.stringify(types)));
        }
        for ty in iter {
            out.push_str(&format!(", {}", ty.stringify(types)));
        }
        out.push(')');
        out.push_str(&format!(": {}", self.returns.stringify(types)));
        out
    }
}

impl StructType {
    pub fn stringify(&self, types: &Types) -> String {
        let mut out = String::from("struct { ");
        for (ident, ty, default) in &self.parameters {
            out.push_str(&format!("{ident}: {}", ty.stringify(types)));
            if let Some(v) = default {
                out.push_str(&format!(" = {}", v.stringify()));
            }
            out.push_str("; ");
        }
        out.push('}');
        out
    }
}

impl EnumType {
    pub fn stringify(&self, types: &Types) -> String {
        let mut out = format!("enum: {} {} ", self.repr.stringify(types), "{");
        for (ident, value) in &self.variants {
            out.push_str(&format!("{ident}: {}; ", value.stringify()));
        }
        out.push('}');
        out
    }
}

impl Module {
    pub fn stringify(&self) -> String {
        self.path.join("::")
    }
}

impl NamedTypeType {
    pub fn stringify(&self, _: &Types) -> String {
        self.name.to_string()
    }
}

impl RefType {
    pub fn stringify(&self, types: &Types) -> String {
        format!("&{}", self.inner.stringify(types))
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

impl TraitType {
    pub fn stringify(&self) -> String {
        self.ident.to_string()
    }
}

impl ConstraintType {
    pub fn stringify(&self, types: &Types) -> String {
        let mut result = String::new();
        let mut it = self.constraints.iter();
        if let Some(constraint) = it.next() {
            let trt = types.traits.get_unchecked(constraint);
            result.push_str(trt.ident.as_str());
        }
        for constraint in it {
            let trt = types.traits.get_unchecked(constraint);
            result.push_str(&format!(" + {}", trt.ident));
        }
        result
    }
}

impl MorphedType {
    pub fn stringify(&self, types: &Types) -> String {
        format!(
            "{}<{}>",
            self.this.stringify(types),
            self.arguments
                .iter()
                .map(|ty| ty.stringify(types).to_smolstr())
                .collect::<Vec<SmolStr>>()
                .join(", ")
        )
    }
}

impl PolymorphType {
    pub fn stringify(&self, types: &Types) -> String {
        format!(
            "<{}>{}",
            self.parameters
                .iter()
                .map(|ty| types
                    .generics
                    .get_unchecked(ty)
                    .stringify(types)
                    .to_smolstr())
                .collect::<Vec<SmolStr>>()
                .join(", "),
            self.inner.stringify(types),
        )
    }
}

impl GenericType {
    pub fn stringify(&self, types: &Types) -> String {
        match types
            .constraints
            .get_unchecked(&self.constraint)
            .stringify(types)
            .as_str()
        {
            "" => format!("{}", self.ident),
            constraints => format!("{}: {}", self.ident, constraints),
        }
    }
}

impl AnyTypeKey {
    #[must_use]
    pub fn check(&self, types: &Types, expect: &Self) -> Result<(), Errors> {
        let equals = match (expect, self) {
            (a, b) if a == b => return Ok(()),
            (AnyTypeKey::Array(exp), AnyTypeKey::Array(got)) => {
                let exp_unwrap = types.arrays.get_unchecked(exp);
                let got_unwrap = types.arrays.get_unchecked(got);
                got_unwrap
                    .element_type
                    .check(types, &exp_unwrap.element_type)?;
                match (exp_unwrap.size, got_unwrap.size) {
                    (None, _) => true,
                    (Some(exp), Some(got)) => (exp == got)
                        .ok_or(Errors::ArrayElementCountMismatch {
                            expected: (exp_unwrap.element_type, exp),
                            got: (got_unwrap.element_type, Some(got)),
                        })
                        .map(|_| true)?,
                    (Some(exp), got) => Err(Errors::ArrayElementCountMismatch {
                        expected: (exp_unwrap.element_type, exp),
                        got: (got_unwrap.element_type, got),
                    })?,
                }
            }
            (AnyTypeKey::Reference(exp), AnyTypeKey::Reference(got)) => {
                let exp_unwrap = types.references.get_unchecked(exp).inner;
                let got_unwrap = types.references.get_unchecked(got).inner;
                got_unwrap.check(types, &exp_unwrap).map(|_| true)?
            }
            _ => false,
        };
        match equals {
            true => Ok(()),
            false => match expect.unwrap(types) {
                Some(exp) => self.check(types, &exp),
                None => Err(Errors::TypeMismatch {
                    expected: *expect,
                    got: *self,
                }),
            },
        }
    }

    pub fn unwrap(self, types: &Types) -> Option<AnyTypeKey> {
        match self {
            AnyTypeKey::Named(key) => Some(types.named.get_unchecked(&key).repr),
            AnyTypeKey::Morphed(key) => Some(types.morphs.get_unchecked(&key).this),
            _ => None,
        }
    }

    pub fn unwrap_full(self, types: &Types) -> AnyTypeKey {
        let mut this = self;
        while let Some(new) = this.unwrap(types) {
            this = new;
        }
        this
    }

    pub fn substitute_many(
        &self,
        types: &mut Types,
        substitutions: &Vec<(GenericKey, AnyTypeKey)>,
    ) -> Result<AnyTypeKey, Errors> {
        Ok(match *self {
            AnyTypeKey::Primitive(_) | AnyTypeKey::Enum(_) => *self,
            AnyTypeKey::Generic(key) => match substitutions.iter().find(|(k, _)| k.eq(&key)) {
                Some((_, s)) => dbg!(*s), // TODO: add constraint checks etc
                None => *self,
            },
            AnyTypeKey::Function(key) => {
                let mut this = types.functions.get_unchecked(&key).clone();
                for param in &mut this.parameters {
                    *param = param.substitute_many(types, substitutions)?;
                }
                this.returns = this.returns.substitute_many(types, substitutions)?;
                let ty_key = types.functions.push_unique(this);
                AnyTypeKey::Function(ty_key)
            }
            AnyTypeKey::Array(key) => {
                let mut this = types.arrays.get_unchecked(&key).clone();
                this.element_type = this.element_type.substitute_many(types, substitutions)?;
                let ty_key = types.arrays.push_unique(this);
                AnyTypeKey::Array(ty_key)
            }
            AnyTypeKey::Tuple(key) => {
                let mut this = types.tuples.get_unchecked(&key).clone();
                for param in &mut this.parameters {
                    *param = param.substitute_many(types, substitutions)?;
                }
                let ty_key = types.tuples.push_unique(this);
                AnyTypeKey::Tuple(ty_key)
            }
            AnyTypeKey::Struct(key) => {
                let mut this = types.structures.get_unchecked(&key).clone();
                for (_, ty, _) in &mut this.parameters {
                    *ty = ty.substitute_many(types, substitutions)?;
                }
                let this_key = types.structures.push_unique(this);
                AnyTypeKey::Struct(this_key)
            }

            AnyTypeKey::Named(key) => {
                let mut this = types.named.get_unchecked(&key).clone();
                this.repr = this.repr.substitute_many(types, substitutions)?;
                let this_key = types.named.push_unique(this);
                AnyTypeKey::Named(this_key)
            }

            AnyTypeKey::Morphed(key) => {
                let this = types.morphs.get_unchecked(&key).this;
                this.substitute_many(types, substitutions)?
            }
            AnyTypeKey::Reference(key) => {
                let mut this = types.references.get_unchecked(&key).clone();
                this.inner = this.inner.substitute_many(types, substitutions)?;
                let this_key = types.references.push_unique(this);
                AnyTypeKey::Reference(this_key)
            }
            AnyTypeKey::Polymorph(key) => {
                let this = types.polymorphs.get_unchecked(&key);

                let inner = this.inner.clone().substitute_many(types, substitutions)?;

                let morphed = MorphedType {
                    arguments: substitutions.iter().map(|(_, ty)| *ty).collect(),
                    parent: key,
                    this: inner,
                };

                let morphed_key = types.morphs.push_unique(morphed);
                AnyTypeKey::Morphed(morphed_key)
            }
            /* AnyTypeKey::AnonymousStruct */
            AnyTypeKey::ModuleRef(_) | AnyTypeKey::Trait(_) => {
                Err(Errors::CouldNotSubstituteType(*self))?
            }
        })
    }

    pub fn stringify(&self, types: &Types) -> Cow<'static, str> {
        match self {
            AnyTypeKey::Primitive(primitive_type) => Cow::Borrowed(primitive_type.stringify()),
            AnyTypeKey::Function(key) => {
                Cow::Owned(types.functions.get_unchecked(key).stringify(types))
            }
            AnyTypeKey::Array(key) => Cow::Owned(types.arrays.get_unchecked(key).stringify(types)),
            AnyTypeKey::Reference(key) => {
                Cow::Owned(types.references.get_unchecked(key).stringify(types))
            }
            AnyTypeKey::Tuple(key) => Cow::Owned(types.tuples.get_unchecked(key).stringify(types)),
            AnyTypeKey::Struct(key) => {
                Cow::Owned(types.structures.get_unchecked(key).stringify(types))
            }
            AnyTypeKey::Morphed(key) => {
                Cow::Owned(types.morphs.get_unchecked(key).stringify(types))
            }
            AnyTypeKey::Polymorph(key) => {
                Cow::Owned(types.polymorphs.get_unchecked(key).stringify(types))
            }
            AnyTypeKey::Generic(key) => {
                Cow::Owned(types.generics.get_unchecked(key).stringify(types))
            }
            //AnyTypeKey::AnonymousStruct => Cow::Borrowed("{ ... }"),
            AnyTypeKey::Enum(key) => Cow::Owned(types.enums.get_unchecked(key).stringify(types)),
            AnyTypeKey::Trait(key) => Cow::Owned(types.traits.get_unchecked(key).stringify()),
            AnyTypeKey::ModuleRef(key) => Cow::Owned(types.modules.get_unchecked(key).stringify()),
            AnyTypeKey::Named(key) => Cow::Owned(types.named.get_unchecked(key).stringify(types)),
        }
    }
}
