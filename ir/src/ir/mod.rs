use arena::{Arena, Key};
use arena_scope::{
    ScopeTree,
    stack::{Stack, StackKey},
};
use smol_str::SmolStr;

pub mod lowerng;

use crate::{
    ast::{ConstValue, Operator, Span, UnaryOp},
    const_stage::{
        objects::{ConstObjKey, FunctionObjKey},
        types::AnyTypeKey,
    },
};

const ERR_VAR_SHADOWING: bool = true;

pub type VariableKey = Key<VariableTag>;
pub type VariableArena = Arena<Variable, VariableTag>;
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct VariableTag;
#[derive(Debug)]
pub struct Variable {
    pub identifier: Span<SmolStr>,
    pub ty: AnyTypeKey,
    pub value: Value,
    pub used: bool,
}

#[derive(Debug)]
pub struct BlockCtx {
    pub variables: VariableCtx,
    pub source: FunctionObjKey,
}

pub type VariableCtx = ScopeTree<SmolStr, VariableKey>;

#[derive(Debug)]
pub struct FunctionIr {
    pub variables: VariableArena,
    pub instructions: Stack<Vec<Instruction>>,
    pub instructions_entry: StackKey,
}

#[derive(Debug, Clone)]
pub enum Addr {
    Var(VariableKey),
    Function(FunctionObjKey),
    Const(ConstObjKey),
}

#[derive(Debug, Clone)]
pub enum Value {
    Const(ConstValue),
    Runtime(AnyTypeKey),
    Addr(Addr),
}

#[derive(Debug, Clone)]
pub enum Instruction {
    /// Pushes a constant value on stack
    PushConst { src: ConstValue },
    /// Pops two values on stack, applies operator and pushes result
    BinOp { op: Operator },
    /// Pops value on stack, applies unary operator and pushes result
    UnaryOp { op: UnaryOp },
    /// Pops value from stack and stores it in variable
    StoreVar { dst: VariableKey },
    /// Loads value from variable and pushes it on stack
    PushVar { src: VariableKey },
    /// Calls a function, expects arguments to be pushed to the stack in the same order as defined in signature
    /// Pushes result to the stack
    Call { fun: FunctionObjKey },
    /// Returns from a function
    Return,
}
