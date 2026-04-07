use std::collections::HashMap;

use smol_str::SmolStr;

use crate::ir::types::ModuleKey;

pub enum AnyObject {
    Import { module: ModuleKey },
}

#[derive(Default)]
pub struct Module {
    pub path: Vec<SmolStr>,
    pub symbols: HashMap<SmolStr, AnyObject>,
}
