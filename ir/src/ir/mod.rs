use arena::{Arena, Key};
use arena_scope::{
    ScopeKey, ScopeTree,
    stack::{Stack, StackKey},
};
use smol_str::SmolStr;

pub mod lowerng;

use crate::{
    ast::{Operator, Span, SpanIndex, UnaryOp},
    const_stage::{
        ConstValueKey, Errors,
        objects::{AnyObjectKey, FunctionObjKey},
        types::{AnyTypeKey, PrimitiveType},
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
    pub needs_address: bool,
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

#[derive(Debug, Clone, Default, Copy, Hash, PartialEq, Eq)]
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
    pub substitutions: Option<ScopeKey>,
    pub void: ValueKey,
    pub returns: Option<AnyTypeKey>,
    pub parameters: Vec<(SmolStr, VariableKey)>,
}

#[derive(Debug, Clone, Default)]
pub struct BasicBlock {
    instructions: Vec<Span<Instruction>>,
    terminator: Option<Terminator>,
    instr_lock: bool,
    pub parameter: Option<ValueKey>,
}

#[derive(Debug, Clone)]
pub enum Addr {
    Var(VariableKey),
    Value(ValueKey),
    Object(AnyObjectKey),
    Function(FunctionIrKey),
    UnresolvedFunction(FunctionObjKey),
    MemoryRef { src: ValueKey, inner_ty: AnyTypeKey },
    Field { src: Box<Addr>, idx: usize },
    Never,
}

#[derive(Debug, Clone, Hash, PartialEq, Eq, Copy)]
pub struct ValueTag;
pub type ValueKey = Key<ValueTag>;
#[derive(Debug, Clone, Copy)]
pub struct Value {
    pub ty: AnyTypeKey,
    pub needs_address: bool,
}

impl Value {
    pub fn new(ty: AnyTypeKey) -> Self {
        Self {
            ty,
            needs_address: false,
        }
    }
}

#[derive(Debug, Clone)]
pub enum Instruction {
    LoadConst {
        src: ConstValueKey,
        dst: ValueKey,
    },
    BinOp {
        op: Operator,
        l: ValueKey,
        r: ValueKey,
        dst: ValueKey,
        ty: PrimitiveType,
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
    Unreachable,
    Exit(u8),
}

impl BasicBlock {
    pub fn instructions(&self) -> &[Span<Instruction>] {
        &self.instructions
    }

    pub fn terminator(&self) -> &Option<Terminator> {
        &self.terminator
    }

    pub fn lock_instructions(&mut self, lock: bool) {
        self.instr_lock = lock;
    }

    pub fn extend(&mut self, instrs: impl IntoIterator<Item = Instruction>, location: SpanIndex) {
        if self.instr_lock {
            return;
        }
        self.instructions
            .extend(instrs.into_iter().map(|i| Span::new(i, location)));
    }

    pub fn terminate(&mut self, term: Terminator, overwrite: bool) {
        match (&self.terminator, overwrite) {
            (None, _) | (Some(_), true) => self.terminator = Some(term),
            _ => (),
        }
    }
}

impl BlockCtx {
    pub fn get_break_block(&self, label: &Option<SmolStr>) -> Result<BlockKey, Errors> {
        for cf in self.control_stack.iter().rev() {
            if label.is_some() && label != &cf.label {
                continue;
            }
            match cf.kind {
                ControlFrameKind::Loop { break_block, .. } => return Ok(break_block),
            }
        }
        match label {
            Some(l) => todo!(),
            None => todo!(),
        }
    }

    pub fn get_continue_block(&self, label: &Option<SmolStr>) -> Result<BlockKey, Errors> {
        for cf in self.control_stack.iter().rev() {
            if label.is_some() && label != &cf.label {
                continue;
            }
            match cf.kind {
                ControlFrameKind::Loop { continue_block, .. } => return Ok(continue_block),
            }
        }
        match label {
            Some(l) => todo!(),
            None => todo!(),
        }
    }
}
