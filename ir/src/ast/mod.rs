pub mod const_expr;

use core::panic;
use std::{
    fmt::Display,
    ops::{Add, Deref},
    path::PathBuf,
    sync::Arc,
};

use arena::{Arena, Key};
use smol_str::{SmolStr, ToSmolStr};

use crate::const_stage::{
    Errors,
    types::{AnyTypeKey, PrimitiveType},
};

#[derive(Debug, Clone)]
pub enum LoweringWarning {
    IdentifierTooLong(String),
    DivisionByZero,
}

#[derive(Debug, Clone)]
pub enum LoweringDiagnostic {
    ReducedConstExpr(Value),
}

#[derive(Debug, Clone, Default)]
pub struct Diagnostics {
    pub warns: Vec<Span<LoweringWarning>>,
    pub diagnostics: Vec<Span<LoweringDiagnostic>>,
}

impl Display for LoweringWarning {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoweringWarning::IdentifierTooLong(ident) => {
                write!(f, "Identifier '{:10}' is too long", ident)
            }
            LoweringWarning::DivisionByZero => {
                write!(f, "Division by zero")
            }
        }
    }
}

impl Display for LoweringDiagnostic {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoweringDiagnostic::ReducedConstExpr(expr) => {
                write!(f, "Expression reduced to {}", expr)
            }
        }
    }
}

/* ===================== SOURCE ===================== */

#[derive(Debug, Clone)]
pub struct Span<T> {
    pub inner: Arc<T>,
    pub location: SpanIndex,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct SpanIndex {
    pub index: usize,
    pub len: usize,
}

impl<T> Span<T> {
    pub fn new(inner: T, location: SpanIndex) -> Self {
        let inner = Arc::new(inner);
        Self { inner, location }
    }

    pub fn map<U, F>(self, f: F) -> Span<U>
    where
        F: FnOnce(Arc<T>) -> U,
    {
        Span::new(f(self.inner), self.location)
    }
}

impl<T> Deref for Span<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl Add for SpanIndex {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self {
            index: self.index,
            len: rhs.index - self.index + self.len,
        }
    }
}

impl<T> PartialEq for Span<T>
where
    T: PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        (*self.inner).eq(other)
    }
}

/* ===================== MODULE ===================== */

#[derive(Debug, Copy, Clone)]
pub struct ObjectTag;
pub type AstObjectKey = Key<ObjectTag>;

#[derive(Debug, Clone)]
pub struct Module {
    pub name: SmolStr,
    pub src: Arc<String>,
    pub path: Option<PathBuf>,
    pub docs: Vec<Span<SmolStr>>,
    pub objects: Arena<Span<Object>, ObjectTag>,
}

/* ===================== OBJECTS ===================== */

#[derive(Debug, Clone)]
pub enum Object {
    Function(Function),

    Component {
        ident: Span<SmolStr>,
        ty: Option<Span<Type>>,
        docs: Vec<Span<SmolStr>>,
    },

    Type {
        ident: Span<SmolStr>,
        generics: Option<Span<Vec<Span<GenericParameter>>>>,
        ty: Option<Span<Type>>,
        docs: Vec<Span<SmolStr>>,
    },

    System {
        ident: Span<SmolStr>,
        generics: Option<Span<Vec<Span<GenericParameter>>>>,
        docs: Vec<Span<SmolStr>>,
        query: Vec<Span<Clauses>>,
        before: Option<Span<Span<Body>>>,
        body: Span<Body>,
        after: Option<Span<Span<Body>>>,
    },

    Import {
        ident: Span<IdentifierPath>,
        alias: Alias,
    },

    Const {
        docs: Vec<Span<SmolStr>>,
        ident: Span<SmolStr>,
        ty: Span<Type>,
        expression: Span<Expression>,
    },

    Trait {
        docs: Vec<Span<SmolStr>>,
        ident: Span<SmolStr>,
        methods: Vec<Span<Function>>,
    },

    TypeImpl {
        ty: Span<Type>,
        generic_parameters: Option<Span<Vec<Span<GenericParameter>>>>,
        methods: Vec<Span<Function>>,
    },

    TraitImpl {
        ty: Span<Type>,
        trt: Span<IdentifierPath>,
        for_kw: Keyword,
        generic_parameters: Option<Span<Vec<Span<GenericParameter>>>>,
        methods: Vec<Span<Function>>,
    },

    Resource {
        ident: Span<SmolStr>,
        docs: Vec<Span<SmolStr>>,
        ty: Option<Span<Type>>,
        default_expression: Option<Span<Expression>>,
        is_optional: Option<Keyword>,
    },
}

#[derive(Debug, Clone)]
pub struct Function {
    pub ident: Span<SmolStr>,
    pub generics: Option<Span<Vec<Span<GenericParameter>>>>,
    pub parameters: Vec<Span<Parameter>>,
    pub return_type: Option<Span<Type>>,
    pub body: Span<Body>,
    pub docs: Vec<Span<SmolStr>>,
    pub invoke: Option<Keyword>,
}

#[derive(Debug, Clone)]
pub struct GenericParameter {
    pub identifier: Span<SmolStr>,
    pub constraints: Vec<Span<IdentifierPath>>,
}

#[derive(Debug, Clone)]
pub struct Mutability(pub Option<Span<()>>);
#[derive(Debug, Clone)]
pub struct Keyword(pub Span<()>);
#[derive(Debug, Clone)]
pub struct Alias(pub Option<Span<Span<SmolStr>>>);

#[derive(Debug, Clone)]
pub struct SystemInclusion {
    pub path: Span<IdentifierPath>,
    pub generics: Option<Span<Vec<Span<Type>>>>,
}

#[derive(Debug, Clone)]
pub enum Clauses {
    Select(SelectClause),
    Action((ActionClause, Keyword)),
    Restriction(RestrictionClause),
}

#[derive(Debug, Clone)]
pub struct SelectClause {
    pub foreign: Option<Keyword>,
    pub ident: Span<SmolStr>,
    pub docs: Vec<Span<SmolStr>>,
    pub include: Vec<(Span<IdentifierPath>, Mutability, Alias)>,
    pub exclude: Vec<(Span<IdentifierPath>, Alias)>,
    pub optional: Vec<(Span<IdentifierPath>, Mutability, Alias)>,
}

#[derive(Debug, Clone)]
pub struct ActionClause {
    pub ident: Span<SmolStr>,
    pub docs: Vec<Span<SmolStr>>,
    pub event: Vec<(Span<IdentifierPath>, Alias)>,
}

#[derive(Debug, Clone)]
pub struct RestrictionClause {
    pub expression: Span<Expression>,
}

/* ===================== BLOCK / STATEMENTS ===================== */

#[derive(Debug, Clone)]
pub enum Body {
    Block(Vec<Span<Statement>>),
    Statement(Span<Expression>),
}

#[derive(Debug, Clone)]
pub struct Block {
    pub statements: Vec<Span<Statement>>,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Var {
        ident: Span<SmolStr>,
        ty: Option<Span<Type>>,
        expression: Option<Span<Expression>>,
    },

    Return {
        expression: Option<Span<Expression>>,
    },

    Break {
        label: Option<Span<SmolStr>>,
    },
    Continue {
        label: Option<Span<SmolStr>>,
    },

    Loop {
        label: Option<Span<SmolStr>>,
        body: Span<Body>,
    },

    Expr {
        expression: Span<Expression>,
    },

    If {
        condition: Span<Expression>,
        then_block: Span<Body>,
        else_if: Vec<Span<ElseIf>>,
        else_block: Option<Span<Else>>,
    },

    While {
        label: Option<Span<SmolStr>>,
        condition: Span<Expression>,
        body: Span<Body>,
    },

    Invoke {
        invocations: Vec<Span<(Span<IdentifierLiteral>, Vec<Span<Expression>>)>>,
    },
}

#[derive(Debug, Clone)]
pub struct ElseIf {
    pub condition: Span<Expression>,
    pub block: Span<Body>,
}

#[derive(Debug, Clone)]
pub struct Else {
    pub block: Span<Body>,
}

/* ===================== PARAMETERS ===================== */

#[derive(Debug, Clone)]
pub struct Parameter {
    pub ident: Span<SmolStr>,
    pub ty: Span<Type>,
    pub docs: Vec<Span<SmolStr>>,
    pub default_value: Option<Span<Expression>>,
}

/* ===================== EXPRESSIONS ===================== */

#[derive(Debug, Clone)]
pub enum Expression {
    Value(Span<Value>),

    Binary {
        l: Span<Expression>,
        r: Span<Expression>,
        op: Span<Operator>,
    },
}

/* ===================== OPERATORS ===================== */

#[derive(Debug, Clone, Copy)]
pub enum Operator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,

    Eq,
    NEq,
    Gr,
    Le,
    GrEq,
    LeEq,

    And,
    Or,

    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
    ModAssign,
}

#[derive(Debug, Clone, Copy)]
pub enum UnaryOp {
    Sub,
    Neg,
    Ref,
    Deref,
}

/* ===================== VALUES ===================== */

#[derive(Debug, Clone)]
pub struct Value {
    pub literal: Span<Literal>,
    pub postfix: Vec<Span<Postfix>>,
    pub unary: Vec<Span<UnaryOp>>,
}

#[derive(Debug, Clone)]
pub enum Postfix {
    Field(Span<SmolStr>),
    Call(Vec<Span<Expression>>),
    Index(Span<Expression>),
    Ref,
    Deref,
}

/* ===================== TYPES ===================== */

#[derive(Debug, Clone)]
pub struct Type {
    pub refs: Span<usize>,
    pub literal: Span<TypeLiteral>,
}

#[derive(Debug, Clone)]
pub enum TypeLiteral {
    Path(Span<IdentifierPath>, Option<Span<Vec<Span<Type>>>>),
    Struct(Vec<Span<Parameter>>),
    Array(Box<Span<Type>>, Option<Span<Expression>>),
    Tuple(Vec<Span<Type>>),
    Enum(
        Option<Box<Span<Type>>>,
        Option<Span<Expression>>,
        Vec<(Span<SmolStr>, Option<Span<Expression>>)>,
    ),
}

/* ===================== IDENTIFIERS ===================== */

#[derive(Debug, Clone)]
pub struct IdentifierPath {
    pub path: Vec<Span<SmolStr>>,
}

/* ===================== LITERALS ===================== */
#[derive(Debug, Clone)]
pub struct IdentifierLiteral {
    pub path: Span<IdentifierPath>,
    pub generics: Option<Span<Vec<Span<Type>>>>,
}
#[derive(Debug, Clone)]
pub enum Literal {
    Identifier(IdentifierLiteral),

    Structure {
        kw: Keyword,
        ty: Option<Span<IdentifierLiteral>>,
        fields: Vec<Span<(Span<SmolStr>, Span<Expression>)>>,
    },

    Number(Number),

    String(SmolStr),
    Char(char),

    Array(Vec<Span<Expression>>),
    Tuple(Vec<Span<Expression>>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
    Structure {
        fields: Vec<Span<(Span<SmolStr>, Span<ConstValue>)>>,
        ty: Option<AnyTypeKey>,
    },

    Number(Number),

    String(SmolStr),
    Char(char),

    Bool(bool),
    EnumVariant {
        parent: AnyTypeKey,
        variant: SmolStr,
    },

    Array {
        elements: Vec<Span<ConstValue>>,
        ty: AnyTypeKey,
    },
    Tuple {
        elements: Vec<Span<ConstValue>>,
        ty: AnyTypeKey,
    },
}

/* ===================== NUMBERS ===================== */

#[derive(Debug, Clone, PartialEq)]
pub struct Number {
    pub value: NumberValue,
    pub size: Option<u32>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NumberValue {
    Float(f64),
    Uint(u128),
    Int(i128),
    Any(u128),
}

/* ====================== LOWERING =====================*/

#[derive(Debug, Clone)]
pub enum LoweringError {
    ParseIntError(std::num::ParseIntError),
    ParseFloatError(std::num::ParseFloatError),
    UnknownNumericSuffix(SmolStr),
    InvalidUtf8Char(u128),
    UnknownEscapeChar(char),
    UnclosedEscapeChar,
    EmptyCharLiteral,
    MutableExclusion,
}

pub fn numeric_literal(s: &str) -> Result<Number, LoweringError> {
    let (s, radix) = if let Some(s) = s.strip_prefix("0x") {
        (s, 16)
    } else if let Some(s) = s.strip_prefix("0b") {
        (s, 2)
    } else if let Some(s) = s.strip_prefix("0o") {
        (s, 8)
    } else {
        (s, 10)
    };

    let s = s.replace('_', "");

    let is_float = radix == 10 && (s.contains('.') || s.contains('f') || s.contains('F'));

    let (num_str, suffix) = if let Some(pos) =
        s.find(|c: char| c.is_ascii_alphabetic() && !(radix == 16 && c.is_ascii_hexdigit()))
    {
        (&s[..pos], Some(&s[pos..]))
    } else {
        (&s[..], None)
    };

    if is_float {
        let value = num_str.parse().map_err(LoweringError::ParseFloatError)?;
        let size = suffix.and_then(|s| s.parse().ok());
        Ok(Number {
            value: NumberValue::Float(value),
            size,
        })
    } else {
        let value = i128::from_str_radix(num_str, radix).map_err(LoweringError::ParseIntError)?;

        let (number_value, size) = match suffix.map(|s| s.to_lowercase()) {
            Some(ref s) if s.starts_with('u') => {
                let size = s[1..].parse().ok();
                (NumberValue::Uint(value as u128), size)
            }
            Some(ref s) if s.starts_with('i') => {
                let size = s[1..].parse().ok();
                (NumberValue::Int(value), size)
            }
            Some(ref s) if s.starts_with('c') => (NumberValue::Int(value), None),
            None => (NumberValue::Any(value as u128), None),
            Some(suffix) => return Err(LoweringError::UnknownNumericSuffix(suffix.into())),
        };

        Ok(Number {
            value: number_value,
            size,
        })
    }
}

pub fn float_literal(s: &str) -> Option<NumberValue> {
    let s = s.replace('_', "");
    Some(NumberValue::Float(s.parse().ok()?))
}

pub fn char_literal(s: &str) -> Result<char, LoweringError> {
    if s.starts_with(r"'\u{") {
        let unicode = s.trim_start_matches(r"'\u{").trim_end_matches("}'");
        match numeric_literal(unicode)?.value {
            NumberValue::Uint(n) => char::from_u32(n as _).ok_or(LoweringError::InvalidUtf8Char(n)),
            NumberValue::Any(n) => char::from_u32(n as _).ok_or(LoweringError::InvalidUtf8Char(n)),
            num => panic!("invalid digit: {num:?}"),
        }
    } else if s.starts_with(r"\'") {
        match &s[2..3] {
            "0" => Ok('\0'),
            "a" => Ok('\x07'),
            "b" => Ok('\x08'),
            "f" => Ok('\x0C'),
            "n" => Ok('\n'),
            "r" => Ok('\r'),
            "t" => Ok('\t'),
            "v" => Ok('\x0B'),
            other => other
                .chars()
                .next()
                .ok_or(LoweringError::UnclosedEscapeChar),
        }
    } else {
        s.chars().nth(1).ok_or(LoweringError::EmptyCharLiteral)
    }
}
pub fn string_literal(s: &str) -> SmolStr {
    let start_hashes = s.chars().take_while(|&c| c == '#').count();
    let content = &s[start_hashes + 1..s.len() - (start_hashes + 1)];
    content.into()
}

/* ================ PRECEDENCE ============ */

#[derive(Copy, Clone, PartialEq)]
pub enum Associativity {
    Left,
    Right,
}

impl Operator {
    pub fn precedence(self) -> u8 {
        match self {
            Operator::Mul | Operator::Div | Operator::Mod => 70,
            Operator::Add | Operator::Sub => 60,

            Operator::Gr | Operator::Le | Operator::GrEq | Operator::LeEq => 50,

            Operator::Eq | Operator::NEq => 45,
            Operator::And => 30,
            Operator::Or => 20,

            Operator::Assign
            | Operator::AddAssign
            | Operator::SubAssign
            | Operator::MulAssign
            | Operator::DivAssign
            | Operator::ModAssign => 10,
        }
    }

    pub fn associativity(self) -> Associativity {
        match self {
            Operator::Assign
            | Operator::AddAssign
            | Operator::SubAssign
            | Operator::MulAssign
            | Operator::DivAssign
            | Operator::ModAssign => Associativity::Right,
            _ => Associativity::Left,
        }
    }
}

#[derive(Debug, Clone)]
pub enum ExprItem {
    Value(Expression),
    Operator(Operator),
}

/* ================ DEBUG ============ */

impl std::fmt::Display for Operator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Operator::Add => write!(f, "+"),
            Operator::Sub => write!(f, "-"),
            Operator::Mul => write!(f, "*"),
            Operator::Div => write!(f, "/"),
            Operator::Mod => write!(f, "%"),
            Operator::Eq => write!(f, "=="),
            Operator::NEq => write!(f, "!="),
            Operator::Gr => write!(f, ">"),
            Operator::Le => write!(f, "<"),
            Operator::GrEq => write!(f, ">="),
            Operator::LeEq => write!(f, "<="),
            Operator::And => write!(f, "&&"),
            Operator::Or => write!(f, "%%"),
            Operator::Assign => write!(f, "="),
            Operator::AddAssign => write!(f, "+="),
            Operator::SubAssign => write!(f, "-="),
            Operator::MulAssign => write!(f, "*="),
            Operator::DivAssign => write!(f, "/="),
            Operator::ModAssign => write!(f, "%="),
        }
    }
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.literal.inner.deref() {
            TypeLiteral::Path(identifier_path, generics) => {
                for (n, txt) in identifier_path.path.iter().map(|a| &a.inner).enumerate() {
                    if n > 0 {
                        write!(f, "::")?;
                    }
                    write!(f, "{}", txt)?;
                }
                if let Some(generics) = generics {
                    write!(f, "<")?;
                    let mut iter = generics.inner.iter();
                    if let Some(first) = iter.next() {
                        write!(f, "{}", first.inner)?;
                    }
                    for generic in iter {
                        write!(f, ", {}", generic.inner)?;
                    }
                    write!(f, ">")?;
                }
            }
            TypeLiteral::Struct(parameters) => {
                write!(f, "struct {{")?;
                for Parameter {
                    ident,
                    ty,
                    docs: _,
                    default_value,
                } in parameters.iter().map(|p| p.inner.deref())
                {
                    write!(f, "{}: {} ", ident.inner, ty.inner)?;
                    if let Some(_) = default_value {
                        write!(f, "= ...; ")?;
                    }
                }
                write!(f, "}}")?;
            }
            TypeLiteral::Array(ty, len) => {
                write!(f, "[{}", ty.inner)?;
                if len.is_some() {
                    write!(f, ", expr")?;
                }
                write!(f, "]")?;
            }
            TypeLiteral::Tuple(params) => {
                write!(f, "(")?;
                for ty in params {
                    write!(f, " {}", ty.inner)?;
                }
                write!(f, " )")?;
            }
            TypeLiteral::Enum(repr, step, variants) => {
                write!(f, "enum")?;
                if step.is_some() {
                    write!(f, "(...)")?;
                }
                if let Some(repr) = repr {
                    write!(f, ": {}", repr.inner)?;
                }
                write!(f, " {{")?;
                for (ident, expr) in variants {
                    write!(f, " {}", ident.inner)?;
                    if expr.is_some() {
                        write!(f, " = [...]")?;
                    }
                }
                write!(f, " }}")?;
            }
        }
        Ok(())
    }
}

impl ConstValue {
    pub fn stringify(&self) -> SmolStr {
        match self {
            ConstValue::Number(number) => match number.value {
                NumberValue::Float(v) => format!("{v:.1}").to_smolstr(),
                NumberValue::Int(v) => v.to_smolstr(),
                NumberValue::Any(v) => v.to_smolstr(),
                NumberValue::Uint(v) => v.to_smolstr(),
            },
            ConstValue::EnumVariant { parent: _, variant } => variant.clone(),
            ConstValue::String(smol_str) => smol_str.clone(),
            ConstValue::Char(v) => format!("'{v}'").to_smolstr(),
            ConstValue::Bool(true) => "true".to_smolstr(),
            ConstValue::Bool(false) => "false".to_smolstr(),
            ConstValue::Array { elements, .. } => format!(
                "[{}]",
                elements
                    .iter()
                    .map(|v| v.stringify())
                    .collect::<Vec<SmolStr>>()
                    .join(", ")
            )
            .to_smolstr(),
            ConstValue::Tuple { elements, .. } => format!(
                "({})",
                elements
                    .iter()
                    .map(|v| v.stringify())
                    .collect::<Vec<SmolStr>>()
                    .join(", ")
            )
            .to_smolstr(),
            ConstValue::Structure { fields, .. } => format!(
                "struct {} {} {}",
                '{',
                fields
                    .iter()
                    .map(|f| format!("{}: {}", f.0.deref(), f.1.stringify()))
                    .collect::<Vec<String>>()
                    .join("; "),
                '}'
            )
            .to_smolstr(),
        }
    }

    pub fn type_of(&self) -> Result<AnyTypeKey, Errors> {
        Ok(match self {
            Self::Bool(_) => AnyTypeKey::Primitive(PrimitiveType::Bool),
            Self::Char(_) => AnyTypeKey::Primitive(PrimitiveType::Char),
            Self::Number(Number {
                value: NumberValue::Any(_),
                size: _,
            }) => AnyTypeKey::Primitive(PrimitiveType::I32),
            Self::Number(Number {
                value: NumberValue::Int(_),
                size: _,
            }) => AnyTypeKey::Primitive(PrimitiveType::I32),
            Self::Number(Number {
                value: NumberValue::Float(_),
                size: _,
            }) => AnyTypeKey::Primitive(PrimitiveType::F32),
            Self::Number(Number {
                value: NumberValue::Uint(_),
                size: _,
            }) => AnyTypeKey::Primitive(PrimitiveType::U32),
            Self::Structure { ty: Some(ty), .. } => *ty,
            Self::Array { ty, .. } => *ty,
            Self::Tuple { ty, .. } => *ty,
            Self::EnumVariant { parent, variant: _ } => *parent,
            _ => Err(Errors::FailedTypeInfer)?,
        })
    }

    pub fn autostep(&self) -> Result<ConstValue, Errors> {
        Ok(match self {
            ConstValue::Number(number) => match number.value {
                NumberValue::Float(n) => ConstValue::Number(Number {
                    value: NumberValue::Float(n + 1.0),
                    size: number.size,
                }),
                NumberValue::Int(n) => ConstValue::Number(Number {
                    value: NumberValue::Int(n + 1),
                    size: number.size,
                }),
                NumberValue::Uint(n) => ConstValue::Number(Number {
                    value: NumberValue::Uint(n + 1),
                    size: number.size,
                }),
                NumberValue::Any(n) => ConstValue::Number(Number {
                    value: NumberValue::Any(n + 1),
                    size: number.size,
                }),
            },
            ConstValue::Char(c) => {
                ConstValue::Char((*c as u8).checked_add(1).expect("handle pls") as _)
            }
            ConstValue::EnumVariant { .. }
            | ConstValue::Array { .. }
            | ConstValue::Tuple { .. }
            | ConstValue::Structure { .. }
            | ConstValue::String(_)
            | ConstValue::Bool(_) => Err(Errors::UndefinedAutostep(self.type_of()?))?,
        })
    }
}

impl Body {
    pub fn len(&self) -> usize {
        match self {
            Body::Block(spans) => spans.len(),
            Body::Statement(_) => 1,
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.literal.inner.deref() {
            Literal::Number(n) => write!(f, "{n}"),
            _ => write!(f, ":::not implemented:::"),
        }
    }
}

impl Display for Number {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.value {
            NumberValue::Float(n) => write!(f, "{n}"),
            NumberValue::Any(n) => write!(f, "{n}"),
            NumberValue::Int(n) => write!(f, "{n}"),
            NumberValue::Uint(n) => write!(f, "{n}"),
        }
    }
}

impl LoweringError {
    pub fn info(&self) -> (&'static str, &'static str, String) {
        match self {
            LoweringError::ParseIntError(e) => ("Invalid integer literal", "300", e.to_string()),
            LoweringError::ParseFloatError(e) => ("Invalid float literal", "301", e.to_string()),
            LoweringError::UnknownNumericSuffix(suffix) => (
                "Invalid numeric suffix",
                "302",
                format!("Unknown numeric suffix `{suffix}`"),
            ),
            LoweringError::MutableExclusion => (
                "conflicting modifiers",
                "303",
                "Component cannot be both mutable and excluded".to_string(),
            ),
            LoweringError::InvalidUtf8Char(c) => (
                "Invalid character",
                "304",
                format!("Invalid UTF-8 character `{c}`"),
            ),
            LoweringError::UnknownEscapeChar(c) => (
                "Invalid escape sequence",
                "305",
                format!("Unknown escape sequence `\\{c}`"),
            ),
            LoweringError::UnclosedEscapeChar => (
                "Unterminated escape sequence",
                "306",
                "Escape sequence is not terminated".to_string(),
            ),
            LoweringError::EmptyCharLiteral => (
                "Empty character literal",
                "307",
                "Character literal must contain exactly one character".to_string(),
            ),
        }
    }
}
