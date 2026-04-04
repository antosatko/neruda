pub mod lowering;
pub mod objects;
pub mod types;

use self::types::Types;

pub struct Context {
    pub types: Types,
    pub objects: Objects,
}

#[derive(Default)]
pub struct Objects {}

impl Context {
    pub fn new() -> Self {
        Self {
            types: Types::default(),
            objects: Objects::default(),
        }
    }
}
