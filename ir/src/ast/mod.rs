pub mod const_expr;
pub mod format;

use core::panic;
use std::{
    fmt::Display,
    ops::{Add, Deref, DerefMut},
};

use arena::Arena;
use smol_str::SmolStr;

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
    pub inner: T,
    pub location: SpanIndex,
}

#[derive(Debug, Clone, Copy)]
pub struct SpanIndex {
    pub index: usize,
    pub len: usize,
}

impl<T> Span<T> {
    pub fn new(inner: T, location: SpanIndex) -> Self {
        Self { inner, location }
    }

    pub fn map<U, F>(self, f: F) -> Span<U>
    where
        F: FnOnce(T) -> U,
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

impl<T> DerefMut for Span<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
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

/* ===================== MODULE ===================== */
#[derive(Debug, Copy, Clone)]
pub struct ObjectTag;

#[derive(Debug, Clone)]
pub struct Module {
    pub name: SmolStr,
    pub docs: Vec<Span<SmolStr>>,
    pub objects: Arena<Span<Object>, ObjectTag>,
}

/* ===================== OBJECTS ===================== */

#[derive(Debug, Clone)]
pub enum Object {
    Scheduler {
        ident: Span<SmolStr>,
        resources: Option<Span<Vec<Span<Value>>>>,
        systems: Option<Span<Vec<Span<SystemInclusion>>>>,
        init: Option<(Span<Body>, Keyword)>,
        docs: Vec<Span<SmolStr>>,
    },

    Function {
        ident: Span<SmolStr>,
        generics: Option<Span<Vec<Span<GenericParameter>>>>,
        parameters: Vec<Span<Parameter>>,
        return_type: Option<Span<Type>>,
        body: Span<Body>,
        docs: Vec<Span<SmolStr>>,
    },

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

#[derive(Debug, Clone)]
pub struct Function {}

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
        expression: Span<Expression>,
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
}

/* ===================== EXPRESSIONS ===================== */

#[derive(Debug, Clone)]
pub enum Expression {
    Value(Value),

    Binary {
        l: Span<Box<Expression>>,
        r: Span<Box<Expression>>,
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

/* ===================== VALUES ===================== */

#[derive(Debug, Clone)]
pub struct Value {
    pub literal: Span<Literal>,
    pub postfix: Vec<Span<Postfix>>,
}

#[derive(Debug, Clone)]
pub enum Postfix {
    Field(Span<SmolStr>),
    Call(Vec<Span<Expression>>),
    Index(Span<Expression>),
    Refs(usize),
    Derefs(usize),
}

/* ===================== TYPES ===================== */

#[derive(Debug, Clone)]
pub struct Type {
    pub literal: Span<TypeLiteral>,
}

#[derive(Debug, Clone)]
pub enum TypeLiteral {
    Path(IdentifierPath, Option<Span<Vec<Span<Type>>>>),
    Struct(Vec<Span<Parameter>>),
    Array(Box<Span<Type>>, Option<usize>),
    Tuple(Vec<Span<Type>>),
    Enum(Vec<(Span<SmolStr>, Option<Span<Expression>>)>),
}

/* ===================== IDENTIFIERS ===================== */

#[derive(Debug, Clone)]
pub struct IdentifierPath {
    pub path: Vec<Span<SmolStr>>,
}

/* ===================== LITERALS ===================== */
#[derive(Debug, Clone)]
pub enum Literal {
    Identifier(IdentifierPath),

    Structure(
        Result<Span<IdentifierPath>, Keyword>,
        Vec<Span<(Span<SmolStr>, Span<Expression>)>>,
    ),

    Number(Number),

    String(SmolStr),
    Char(char),

    Array(Vec<Span<Expression>>),
    Tuple(Vec<Span<Expression>>),
}

#[derive(Debug, Clone)]
pub enum ConstValue {
    Structure(Vec<Span<(Span<SmolStr>, Span<ConstValue>)>>),

    Number(Number),

    String(SmolStr),
    Char(char),

    Bool(bool),

    Array(Vec<Span<ConstValue>>),
    Tuple(Vec<Span<ConstValue>>),
}

/* ===================== NUMBERS ===================== */

#[derive(Debug, Clone)]
pub struct Number {
    pub value: NumberValue,
    pub size: Option<u32>,
}

#[derive(Debug, Clone)]
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
    let (s, radix) = if s.starts_with("0x") {
        (&s[2..], 16)
    } else if s.starts_with("0b") {
        (&s[2..], 2)
    } else if s.starts_with("0o") {
        (&s[2..], 8)
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

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.literal.inner {
            TypeLiteral::Path(identifier_path, generics) => {
                let mut n = 0;
                for txt in identifier_path.path.iter().map(|a| &a.inner) {
                    if n > 0 {
                        write!(f, "::")?;
                    }
                    n += 1;
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
                write!(f, "struct {}", "{ ")?;
                for Parameter { ident, ty, docs: _ } in parameters.iter().map(|p| &p.inner) {
                    write!(f, "{}: {} ", ident.inner, ty.inner)?;
                }
                write!(f, "{}", "}")?;
            }
            TypeLiteral::Array(ty, len) => {
                write!(f, "[{}", ty.inner)?;
                if let Some(len) = len {
                    write!(f, ", {len}")?;
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
            TypeLiteral::Enum(variants) => {
                write!(f, "enum {}", "{")?;
                for (ident, expr) in variants {
                    write!(f, " {}", ident.inner)?;
                    if expr.is_some() {
                        write!(f, " = [...]")?;
                    }
                }
                write!(f, " {}", '}')?;
            }
        }
        Ok(())
    }
}

impl Body {
    pub fn len(&self) -> usize {
        match self {
            Body::Block(spans) => spans.len(),
            Body::Statement(_) => 1,
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.literal.inner {
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
