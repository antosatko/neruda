use std::{collections::HashMap, ops::Deref};

use arena::{Arena, Key};
use smol_str::SmolStr;

pub mod lowerng;

use crate::{
    ast::{ConstValue, Operator, Span, UnaryOp},
    const_stage::{
        Error, Errors,
        types::{AnyTypeKey, ModuleKey},
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

/*pub type ValueKey = Key<ValueTag>;
pub type ValueArena = Arena<Value, ValueTag>;
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub struct ValueTag;
#[derive(Debug)]
pub struct Value {
    pub ty: AnyTypeKey,
}*/

pub struct IrScopeCtx {
    variables: HashMap<SmolStr, VariableKey>,
}

pub struct BlockCtx {
    pub scopes: Vec<HashMap<SmolStr, VariableKey>>,
}

#[derive(Debug)]
pub struct FunctionIr {
    pub variables: VariableArena,
    //pub values: ValueArena,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug, Clone)]
pub enum Addr {
    Var(VariableKey),
}

#[derive(Debug, Clone)]
pub enum Value {
    Const(ConstValue),
    Runtime(AnyTypeKey),
}

#[derive(Debug, Clone)]
pub enum Instruction {
    /// Pushes a constant value on stack
    PushConst { src: ConstValue },
    /// Pops two values on stack, applies operator and pushes result
    BinOp {
        op: Operator,
        lsrc: Value,
        rsrc: Value,
    },
    /// Pops value on stack, applies unary operator and pushes result
    UnaryOp { op: UnaryOp, src: Value },
    /// Pops value from stack and stores it in address
    Store { dst: Addr },
    /// Loads value from address and pushes it on stack
    Load { src: Addr },
}

impl BlockCtx {
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    pub fn find_var(&self, ident: &str) -> Option<VariableKey> {
        self.scopes
            .iter()
            .rev()
            .find_map(|s| s.get(ident).map(|v| *v))
    }

    pub fn declare_var(
        &mut self,
        ident: &Span<SmolStr>,
        module: &ModuleKey,
        key: VariableKey,
    ) -> Result<(), Error> {
        if ERR_VAR_SHADOWING && self.find_var(&ident).is_some() {
            return Err(Error {
                inner: Errors::DuplicateIdentifier(ident.deref().clone()),
                module: *module,
                span: ident.location,
            });
        }
        match self.scopes.last_mut() {
            Some(scope) => {
                scope.insert(ident.deref().clone(), key);
                Ok(())
            }
            None => unreachable!("Scope expected to exist for variable declaration"),
        }
    }
}
