use std::{borrow::Cow, collections::HashMap};

use arena::{Arena, Key};
use smol_str::{SmolStr, ToSmolStr};

use crate::{
    ast::{ConstValue, Number, NumberValue, SpanIndex},
    const_stage::{Error, Errors, objects::Module},
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
#[derive(PartialEq, Debug)]
pub struct ArrayType {
    pub element_type: AnyTypeKey,
    pub size: Option<usize>,
}

pub type TupleArena = Arena<TupleType, TupleTag>;
pub type TupleKey = Key<TupleTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct TupleTag;
#[derive(PartialEq, Debug)]
pub struct TupleType {
    pub parameters: Vec<AnyTypeKey>,
}

pub type TraitArena = Arena<TraitType, TraitTag>;
pub type TraitKey = Key<TraitTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct TraitTag;
#[derive(PartialEq, Debug)]
pub struct TraitType {
    pub ident: SmolStr,
}

pub type StructArena = Arena<StructType, StructTag>;
pub type StructKey = Key<StructTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct StructTag;
#[derive(PartialEq, Debug)]
pub struct StructType {
    pub parameters: Vec<(SmolStr, AnyTypeKey, Option<ConstValue>)>,
}

pub type EnumArena = Arena<EnumType, EnumTag>;
pub type EnumKey = Key<EnumTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct EnumTag;
#[derive(PartialEq, Debug)]
pub struct EnumType {
    pub repr: AnyTypeKey,
    pub variants: Vec<(SmolStr, ConstValue)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct ConstraintTag;
pub type ConstraintArena = Arena<ConstraintType, ConstraintTag>;
pub type ConstraintKey = Key<ConstraintTag>;
#[derive(PartialEq, Debug)]
pub struct ConstraintType {
    pub constraints: Vec<TraitKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct MorphedTag;
pub type MorphedArena = Arena<MorphedType, MorphedTag>;
pub type MorphedKey = Key<MorphedTag>;
#[derive(PartialEq, Debug)]
pub struct MorphedType {
    pub parent: AnyTypeKey,
    pub this: AnyTypeKey,
    pub arguments: Vec<AnyTypeKey>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
pub struct NamedTypeTag;
pub type NamedTypeArena = Arena<NamedTypeType, NamedTypeTag>;
pub type NamedTypeKey = Key<NamedTypeTag>;
#[derive(PartialEq, Debug)]
pub struct NamedTypeType {
    pub name: SmolStr,
    pub repr: AnyTypeKey,
}

pub type GenericArena = Arena<GenericType, GenericTag>;
pub type GenericKey = Key<GenericTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct GenericTag;
#[derive(PartialEq, Debug)]
pub struct GenericType {
    pub generic_parameters: Vec<(SmolStr, ConstraintKey)>,
    pub inner: AnyTypeKey,
}

pub type RefArena = Arena<RefType, RefTag>;
pub type RefKey = Key<RefTag>;
#[derive(PartialEq, Debug, Copy, Clone, Hash)]
pub struct RefTag;
#[derive(PartialEq, Debug)]
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
    Constraint(ConstraintKey),
    Function(FunctionKey),
    Array(ArrayKey),
    Tuple(TupleKey),
    Struct(StructKey),
    Enum(EnumKey),
    Trait(TraitKey),
    Reference(RefKey),
    Named(NamedTypeKey),
    ModuleRef(ModuleKey),
    Morphed(MorphedKey),
    AnonymousStruct,
}

#[derive(Default)]
pub struct Types {
    pub functions: FunctionArena,
    pub constraints: ConstraintArena,
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
            self.parent.stringify(types),
            self.arguments
                .iter()
                .map(|ty| ty.stringify(types).to_smolstr())
                .collect::<Vec<SmolStr>>()
                .join(", ")
        )
    }
}

impl AnyTypeKey {
    #[must_use]
    pub fn check(&self, types: &Types, expect: &Self) -> Result<(), Errors> {
        // currently very redundant, may change in future idk
        let equals = match (expect, self) {
            (a, b) if a == b => return Ok(()),
            (AnyTypeKey::Array(exp), AnyTypeKey::Array(got)) => {
                dbg!("Array to array comparison ignores size atm");
                let exp_unwrap = types.arrays.get_unchecked(exp);
                let got_unwrap = types.arrays.get_unchecked(got);
                match (exp_unwrap, got_unwrap) {
                    (
                        ArrayType {
                            element_type: exp_unwrap,
                            size: None,
                        },
                        ArrayType {
                            element_type: got_unwrap,
                            size: _,
                        },
                    ) => return got_unwrap.check(types, &exp_unwrap),
                    (
                        ArrayType {
                            element_type: exp_unwrap,
                            size: Some(a),
                        },
                        ArrayType {
                            element_type: got_unwrap,
                            size: Some(b),
                        },
                    ) if a == b => return got_unwrap.check(types, &exp_unwrap),
                    (_, _) => dbg!(false), // might wanna make this more descriptive
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

    fn substitute_named_many(
        &self,
        substitutions: &HashMap<ConstraintKey, AnyTypeKey>,
        types: &mut Types,
        module: ModuleKey,
        span: SpanIndex,
    ) -> Result<AnyTypeKey, Error> {
        let this = match self {
            AnyTypeKey::Function(key) => {
                let FunctionType {
                    returns,
                    parameters,
                } = types.functions.get_unchecked(key).clone();

                let new_returns =
                    returns.substitute_named_many(substitutions, types, module, span)?;

                let mut new_parameters = Vec::with_capacity(parameters.len());
                for p in parameters {
                    new_parameters.push(p.substitute_named_many(
                        substitutions,
                        types,
                        module,
                        span,
                    )?);
                }

                AnyTypeKey::Function(types.functions.push_unique(FunctionType {
                    parameters: new_parameters,
                    returns: new_returns,
                }))
            }

            AnyTypeKey::Array(key) => {
                let ArrayType { element_type, size } = *types.arrays.get_unchecked(key);

                let new_inner =
                    element_type.substitute_named_many(substitutions, types, module, span)?;

                AnyTypeKey::Array(types.arrays.push_unique(ArrayType {
                    size,
                    element_type: new_inner,
                }))
            }

            AnyTypeKey::Tuple(key) => {
                let ty = types.tuples.get_unchecked(key);

                let parameters = ty
                    .parameters
                    .iter()
                    .map(|ty| match ty {
                        AnyTypeKey::Constraint(cons) => {
                            substitutions.get(cons).copied().unwrap_or(*ty)
                        }
                        _ => *ty,
                    })
                    .collect();

                AnyTypeKey::Tuple(types.tuples.push_unique(TupleType { parameters }))
            }

            AnyTypeKey::Reference(key) => {
                let resolved = types
                    .references
                    .get_unchecked(key)
                    .inner
                    .clone()
                    .substitute_named_many(substitutions, types, module, span)?;

                AnyTypeKey::Reference(types.references.push_unique(RefType { inner: resolved }))
            }

            AnyTypeKey::Struct(key) => {
                let original = types.structures.get_unchecked(key);

                let mut parameters = Vec::with_capacity(original.parameters.len());

                for (ident, ty, default) in original.parameters.clone().iter() {
                    let substituted = ty
                        .substitute_named_many(substitutions, types, module, span)
                        .unwrap_or(*ty);

                    parameters.push((ident.clone(), substituted, default.clone()));
                }

                AnyTypeKey::Struct(types.structures.push_unique(StructType { parameters }))
            }

            AnyTypeKey::Named(key) => types
                .named
                .get_unchecked(key)
                .repr
                .clone()
                .substitute_named_many(substitutions, types, module, span)?,

            AnyTypeKey::ModuleRef(_)
            | AnyTypeKey::Trait(_)
            | AnyTypeKey::Primitive(_)
            | AnyTypeKey::Constraint(_)
            | AnyTypeKey::Morphed(_)
            | AnyTypeKey::AnonymousStruct
            | AnyTypeKey::Enum(_) => Err(crate::const_stage::Diagnostic {
                span,
                module,
                inner: Errors::CouldNotSubstituteType(*self),
            })?,
        };
        Ok(this)
    }

    pub fn substitute_named_iter<I>(
        &self,
        substitutions: I,
        types: &mut Types,
        module: ModuleKey,
        span: SpanIndex,
    ) -> Result<AnyTypeKey, Error>
    where
        I: IntoIterator<Item = (ConstraintKey, AnyTypeKey)>,
    {
        let ordered: Vec<(_, _)> = substitutions.into_iter().collect();
        if ordered.is_empty() {
            return Ok(*self);
        }

        let map: HashMap<_, _> = ordered.iter().cloned().collect();

        let this = self.substitute_named_many(&map, types, module, span)?;

        let morph = MorphedType {
            parent: *self,
            this,
            arguments: ordered.iter().map(|(_, ty)| *ty).collect(),
        };
        let key = types.morphs.push_unique(morph);
        Ok(AnyTypeKey::Morphed(key))
    }

    pub fn substitute_named(
        &self,
        substitution: AnyTypeKey,
        cons: &ConstraintKey,
        types: &mut Types,
        module: ModuleKey,
        span: SpanIndex,
    ) -> Result<AnyTypeKey, Error> {
        match self {
            AnyTypeKey::Function(key) => {
                let FunctionType {
                    returns,
                    parameters,
                } = types.functions.get_unchecked(key).clone();
                let new_returns =
                    returns.substitute_named(substitution, cons, types, module, span)?;
                let mut new_parameters = Vec::with_capacity(parameters.len());
                for p in parameters {
                    new_parameters.push(p.substitute_named(
                        substitution,
                        cons,
                        types,
                        module,
                        span,
                    )?);
                }
                let new = FunctionType {
                    parameters: new_parameters,
                    returns: new_returns,
                };
                Ok(AnyTypeKey::Function(types.functions.push_unique(new)))
            }
            AnyTypeKey::Array(key) => {
                let ArrayType { element_type, size } = *types.arrays.get_unchecked(key);
                let new_inner =
                    element_type.substitute_named(substitution, cons, types, module, span)?;
                let new = ArrayType {
                    size,
                    element_type: new_inner,
                };
                Ok(AnyTypeKey::Array(types.arrays.push_unique(new)))
            }
            AnyTypeKey::Tuple(key) => {
                let ty = types.tuples.get_unchecked(key);
                let result = ty
                    .parameters
                    .iter()
                    .map(|ty| match ty {
                        AnyTypeKey::Constraint(key) => {
                            if key == cons {
                                substitution
                            } else {
                                *ty
                            }
                        }
                        _ => *ty,
                    })
                    .collect();

                let new = TupleType { parameters: result };

                Ok(AnyTypeKey::Tuple(types.tuples.push_unique(new)))
            }
            AnyTypeKey::Reference(key) => {
                let resolved = types
                    .references
                    .get_unchecked(key)
                    .inner
                    .clone()
                    .substitute_named(substitution, cons, types, module, span)?;
                Ok(AnyTypeKey::Reference(
                    types.references.push_unique(RefType { inner: resolved }),
                ))
            }
            AnyTypeKey::Struct(key) => {
                let original = types.structures.get_unchecked(key);
                let mut parameters = Vec::with_capacity(original.parameters.len());
                for i in 0..original.parameters.len() {
                    let (ident, ty, default) =
                        types.structures.get_unchecked(key).parameters[i].clone();
                    let s = ty
                        .substitute_named(substitution, cons, types, module, span)
                        .unwrap_or(ty);
                    parameters.push((ident, s, default));
                }
                Ok(AnyTypeKey::Struct(
                    types.structures.push_unique(StructType { parameters }),
                ))
            }
            AnyTypeKey::Named(key) => types
                .named
                .get_unchecked(key)
                .repr
                .clone()
                .substitute_named(substitution, cons, types, module, span),
            AnyTypeKey::ModuleRef(_)
            | AnyTypeKey::Trait(_)
            | AnyTypeKey::Primitive(_)
            | AnyTypeKey::Constraint(_)
            | AnyTypeKey::Morphed(_)
            | AnyTypeKey::AnonymousStruct
            | AnyTypeKey::Enum(_) => {
                return Err(crate::const_stage::Diagnostic {
                    span,
                    module,
                    inner: Errors::CouldNotSubstituteType(*self),
                });
            }
        }
    }

    pub fn stringify(&self, types: &Types) -> Cow<'static, str> {
        match self {
            AnyTypeKey::Primitive(primitive_type) => Cow::Borrowed(primitive_type.stringify()),
            AnyTypeKey::Constraint(key) => {
                Cow::Owned(types.constraints.get_unchecked(key).stringify(types))
            }
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
            AnyTypeKey::AnonymousStruct => Cow::Borrowed("{ ... }"),
            AnyTypeKey::Enum(key) => Cow::Owned(types.enums.get_unchecked(key).stringify(types)),
            AnyTypeKey::Trait(key) => Cow::Owned(types.traits.get_unchecked(key).stringify()),
            AnyTypeKey::ModuleRef(key) => Cow::Owned(types.modules.get_unchecked(key).stringify()),
            AnyTypeKey::Named(key) => Cow::Owned(types.named.get_unchecked(key).stringify(types)),
        }
    }
}
