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
        objects::{AnyObjectKey, FunctionObjKey},
        types::AnyTypeKey,
    },
};

pub type BlockKey = StackKey;

pub type VariableKey = Key<VariableTag>;
pub type VariableArena = Arena<Variable, VariableTag>;
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct VariableTag;
#[derive(Debug)]
pub struct Variable {
    pub identifier: Span<SmolStr>,
    pub ty: AnyTypeKey,
    pub value: ValueKey,
    pub used: bool,
    pub constant: bool,
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
    pub values: Arena<Value, ValueTag>,
    pub blocks: Stack<BasicBlock>,
    pub blocks_entry: StackKey,
}

#[derive(Debug, Clone, Default)]
pub struct BasicBlock {
    pub instructions: Vec<Instruction>,
    pub terminator: Option<Terminator>,
}

#[derive(Debug, Clone)]
pub enum Addr {
    Var(VariableKey),
    Value(ValueKey),
    Object(AnyObjectKey),
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Copy)]
pub struct ValueTag;
pub type ValueKey = Key<ValueTag>;
#[derive(Debug, Clone, Copy)]
pub struct Value {
    ty: AnyTypeKey,
}

#[derive(Debug, Clone)]
pub enum Instruction {
    LoadConst {
        src: ConstValue,
        dst: ValueKey,
    },
    BinOp {
        op: Operator,
        l: ValueKey,
        r: ValueKey,
        dst: ValueKey,
    },
    UnaryOp {
        op: UnaryOp,
        src: ValueKey,
        dst: ValueKey,
    },
    StoreVar {
        dst: VariableKey,
        src: ValueKey,
    },
    LoadVar {
        src: VariableKey,
        dst: ValueKey,
    },
    Call {
        fun: FunctionObjKey,
        result: ValueKey,
    },
}

#[derive(Debug, Clone)]
/// May require a boolean value on stack
enum Terminator {
    Return(Option<ValueKey>),
    Jump(BlockKey, Option<ValueKey>),
    Branch {
        then_block: BlockKey,
        else_block: BlockKey,
    },
    Unreachable,
}
