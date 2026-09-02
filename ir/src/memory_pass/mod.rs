use std::{collections::HashMap, range::Range};

use arena::{Arena, Key};
use smol_str::SmolStr;

use crate::const_stage::{
    types::{AnyTypeKey, PrimitiveType, Vector},
    ConstValueKey,
};

pub struct ModuleIr {
    pub constants: Constants,
}

pub struct Constants {
    pub data: Vec<u8>,
    pub cache: HashMap<SmolStr, Range<usize>>,
}

pub struct FunctionMemoryIr {
    pub stack_slots: StackSlotArena,
    pub values: ValueArena,
    pub variables: VariableArena,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct StackSlotTag;
pub type StackSlotKey = Key<StackSlotTag>;
pub type StackSlotArena = Arena<StackSlot, StackSlotTag>;
pub struct StackSlot {
    pub ty: AnyTypeKey,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ValueTag;
pub type ValueKey = Key<ValueTag>;
pub type ValueArena = Arena<Value, ValueTag>;
pub struct Value {
    pub ty: ValueType,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VariableTag;
pub type VariableKey = Key<VariableTag>;
pub type VariableArena = Arena<Variable, VariableTag>;
pub struct Variable {
    pub ty: ValueType,
}

pub enum Instructions {
    StackStore {
        slot: StackSlotKey,
        src: ValueKey,
        offset: usize,
    },
    StackLoad {
        slot: StackSlotKey,
        dst: ValueKey,
        offset: usize,
    },
    VariableStore {
        src: ValueKey,
        dst: VariableKey,
    },
    VariableLoad {
        src: VariableKey,
        dst: ValueKey,
    },
    ConstLoad {
        dst: ValueKey,
        src: ConstValueKey,
    },
    ConstRef {
        src: usize,
        dst: ValueKey,
    },
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash)]
pub struct BlockTag;
pub type BlockKey = Key<BlockTag>;
pub type BlockArena = Arena<Block, BlockTag>;
pub struct Block {
    pub params: Vec<ValueKey>,
    pub instrs: Vec<Instructions>,
    pub termin: Terminator,
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
    Unreachable,
    Trap(u8),
}

pub enum ValueType {
    Primitive(PrimitiveType),
    Vector(Vector),
    Pointer(AnyTypeKey),
}
