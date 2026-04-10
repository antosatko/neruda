pub mod lowering;
pub mod objects;
pub mod types;

use std::{collections::HashMap, sync::Arc};

use arena::Arena;
use smol_str::SmolStr;

use crate::{
    ast,
    ir::{objects::Module, types::ModuleArena},
};

use self::types::Types;

#[derive(Default)]
pub struct Diagnostics {}

pub struct Context {
    pub types: Types,
    pub ast: HashMap<Vec<SmolStr>, Arc<ast::Module>>,
    pub diagnostics: Diagnostics,
}

impl Context {
    pub fn from_ast(ast: HashMap<Vec<SmolStr>, Arc<ast::Module>>) -> Self {
        let mut this = Self {
            types: Types::default(),
            diagnostics: Diagnostics::default(),
            ast,
        };

        this.lower_import_stage();
        this.lower_const_stage();

        this
    }
}
