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
        types::{AnyTypeKey, ConstraintKey, GenericKey},
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
    pub mutated: bool,
}

#[derive(Debug)]
pub struct BlockCtx {
    pub variables: VariableCtx,
    pub source: FunctionObjKey,
    pub control_stack: Vec<ControlFrame>,
}

pub type VariableCtx = ScopeTree<SmolStr, VariableKey>;

#[derive(Debug)]
pub struct ControlFrame {
    pub label: Option<SmolStr>,
    pub kind: ControlFrameKind,
}

#[derive(Debug)]
pub enum ControlFrameKind {
    Loop {
        break_block: BlockKey,
        continue_block: BlockKey,
    },
}

#[derive(Debug, Clone, Default, Copy, Hash, PartialEq)]
pub struct FunctionIrTag;
pub type FunctionIrKey = Key<FunctionIrTag>;
pub type FunctionIrArena = Arena<FunctionIr, FunctionIrTag>;
#[derive(Debug)]
pub struct FunctionIr {
    pub source: Option<FunctionObjKey>,
    pub type_of: Option<AnyTypeKey>,
    pub variables: VariableArena,
    pub values: Arena<Value, ValueTag>,
    pub blocks: Stack<BasicBlock>,
    pub blocks_entry: StackKey,
    pub substitutions: Vec<(GenericKey, AnyTypeKey)>,
    pub void: ValueKey,
    pub returns: Option<AnyTypeKey>,
    pub parameters: Vec<(SmolStr, VariableKey)>,
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
    Function(FunctionIrKey),
    UnresolvedFunction(FunctionObjKey),
    MemoryRef { src: ValueKey, inner_ty: AnyTypeKey },
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
        fun: FunctionIrKey,
        arguments: Vec<ValueKey>,
        result: ValueKey,
    },
    AddressOfObj {
        obj: AnyObjectKey,
        dst: ValueKey,
    },
    AddressOfFun {
        fun: FunctionIrKey,
        dst: ValueKey,
    },
    AddressOfVar {
        var: VariableKey,
        dst: ValueKey,
    },
    AddressOfVal {
        val: ValueKey,
        dst: ValueKey,
    },
    Deref {
        src: ValueKey,
        dst: ValueKey,
    },
}

#[derive(Debug, Clone)]
pub enum Terminator {
    Return(Option<ValueKey>),
    Jump(BlockKey, Option<ValueKey>),
    Branch {
        condition: ValueKey,
        then_block: BlockKey,
        else_block: BlockKey,
    },
    Eval(ValueKey),
    Unreachable,
}
