use std::{collections::HashMap, ops::Deref};

use arena::{Arena, Key};
use smol_str::SmolStr;

pub mod lowerng;

use crate::{
    ast::Span,
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
    pub used: bool,
}

pub struct BlockCtx {
    pub scopes: Vec<HashMap<SmolStr, VariableKey>>,
}

#[derive(Debug)]
pub struct FunctionIr {
    pub variables: VariableArena,
    pub instructions: Vec<Instruction>,
}

#[derive(Debug)]
pub enum Instruction {}

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
