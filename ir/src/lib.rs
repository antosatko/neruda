use core::panic;
use std::{
    collections::HashMap,
    ops::{Deref, DerefMut},
};

use arena::{Arena, Key};
use smol_str::SmolStr;

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

/* ===================== MODULE ===================== */
#[derive(Debug, Copy, Clone)]
pub struct ObjectTag;

#[derive(Debug, Clone)]
pub struct Module {
    pub name: SmolStr,
    pub docs: Vec<Span<SmolStr>>,
    pub objects: Arena<Span<Object>, ObjectTag>,
    pub symbols: HashMap<SmolStr, Key<ObjectTag>>,
}

/* ===================== OBJECTS ===================== */

#[derive(Debug, Clone)]
pub enum Object {
    Scheduler {
        ident: Span<SmolStr>,
        resources: Option<Span<Vec<Span<Value>>>>,
        systems: Option<Span<Vec<Span<IdentifierPath>>>>,
        docs: Vec<Span<SmolStr>>,
    },

    Function(Function),
}

#[derive(Debug, Clone)]
pub struct Function {
    pub ident: Span<SmolStr>,
    pub parameters: Vec<Span<Parameter>>,
    pub return_type: Option<Span<Type>>,
    pub body: Span<Body>,
    pub docs: Vec<Span<SmolStr>>,
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
    pub path: Span<IdentifierPath>,
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

    Number(Number),

    String(SmolStr),
    Char(char),

    Array(Vec<Span<Expression>>),
    Tuple(Vec<Span<Expression>>),
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
    Number(u128),
}

/* ====================== LOWERING =====================*/

pub fn numeric_literal(s: &str) -> Option<Number> {
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
        let value: f64 = num_str.parse().ok()?;
        let size = suffix.and_then(|s| s.parse().ok());
        Some(Number {
            value: NumberValue::Float(value),
            size,
        })
    } else {
        let value = i128::from_str_radix(num_str, radix).ok()?;

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
            None => (NumberValue::Number(value as u128), None),
            Some(_) => return None,
        };

        Some(Number {
            value: number_value,
            size,
        })
    }
}

pub fn float_literal(s: &str) -> Option<NumberValue> {
    let s = s.replace('_', "");
    Some(NumberValue::Float(s.parse().ok()?))
}

pub fn char_literal(s: &str) -> Option<char> {
    if s.starts_with(r"'\u{") {
        let unicode = s.trim_start_matches(r"'\u{").trim_end_matches("}'");
        match numeric_literal(unicode)?.value {
            NumberValue::Uint(n) => char::from_u32(n as _),
            NumberValue::Number(n) => char::from_u32(n as _),
            num => panic!("invalid digit: {num:?}"),
        }
    } else if s.starts_with(r"\'") {
        Some(match &s[2..3] {
            "0" => '\0',
            "a" => '\x07',
            "b" => '\x08',
            "f" => '\x0C',
            "n" => '\n',
            "r" => '\r',
            "t" => '\t',
            "v" => '\x0B',
            other => other.chars().next()?,
        })
    } else {
        s.chars().nth(1)
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
        let mut n = 0;
        for txt in self.path.inner.path.iter().map(|a| &a.inner) {
            if n > 0 {
                write!(f, "::")?;
            }
            n += 1;
            write!(f, "{}", txt)?;
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
