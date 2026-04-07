use std::collections::HashMap;

use smol_str::SmolStr;

use crate::{
    ast::{self, ConstValue},
    ir::types::{AnyTypeKey, ModuleKey},
};

pub enum AnyObject {
    Import { module: ModuleKey },
    Const { value: ConstValue, ty: AnyTypeKey },
}

#[derive(Default)]
pub struct Module {
    pub path: Vec<SmolStr>,
    pub symbols: HashMap<SmolStr, AnyObject>,
    pub hoisted_symbols: HashMap<SmolStr, ast::Object>,
}
