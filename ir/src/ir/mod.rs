pub mod lowering;
pub mod objects;
pub mod types;

use std::collections::HashMap;

use smol_str::SmolStr;

use crate::{ast, ir::objects::Module};

use self::types::Types;

#[derive(Default)]
pub struct Diagnostics {}

pub struct Context {
    pub types: Types,
    pub ast: HashMap<Vec<SmolStr>, ast::Module>,
    pub diagnostics: Diagnostics,
}

#[derive(Default)]
pub struct Objects {}

impl Context {
    pub fn from_ast(ast: HashMap<Vec<SmolStr>, ast::Module>) -> Self {
        let mut this = Self {
            types: Types::default(),
            diagnostics: Diagnostics::default(),
            ast,
        };

        this.lower_import_stage();

        this
    }
}
