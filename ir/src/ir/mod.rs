pub mod lowering;
pub mod objects;
pub mod types;

use std::{collections::HashMap, sync::Arc};

use arena::Arena;
use smol_str::SmolStr;

use crate::{
    ast,
    ir::{
        objects::Module,
        types::{AutoTypes, ModuleArena},
    },
};

use self::types::Types;

#[derive(Default)]
pub struct Diagnostics {}

pub struct Context {
    pub types: Types,
    pub auto_types: AutoTypes,
    pub ast: HashMap<Vec<SmolStr>, Arc<ast::Module>>,
    pub diagnostics: Diagnostics,
}

impl Context {
    pub fn from_ast(ast: HashMap<Vec<SmolStr>, Arc<ast::Module>>) -> Self {
        let mut types = Types::default();
        let auto_types = AutoTypes::new(&mut types);
        let mut this = Self {
            types,
            auto_types,
            diagnostics: Diagnostics::default(),
            ast,
        };

        this.lower_import_stage();
        this.lower_const_stage();

        this
    }
}
