use std::alloc::Layout;

use arena::{Arena, DynArena, DynKey, Key};

pub mod core;
mod erased_vec;
pub mod ext;
mod tests;

use crate::{bitset::Bitset, v2::erased_vec::ErasedVec};

pub type Row = usize;
pub type ComponentRef = usize;
pub type Column = usize;

pub type EntityRefKey = DynKey<EntityRef>;
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq)]
pub struct EntityRef {
    pub archetype: ArcheTypeKey,
    pub row: Row,
}

pub type ArcheTypeKey = Key<ArcheTypeTag>;
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq)]
pub struct ArcheTypeTag;
pub struct ArcheType {
    signature: Bitset,
    entities: Vec<EntityRef>,
    dyn_columns: Vec<ErasedVec>,
    static_columns: Vec<ErasedVec>,
    flag_columns: Vec<u32>,
}

pub struct Query {
    include: Bitset,
    exclude: Bitset,
    optional: Bitset,
}

pub struct QueryCache {
    query: Query,
    archetypes: Vec<(ArcheTypeKey, Vec<Column>)>,
}

pub struct World {
    entities: DynArena<EntityRef>,
    archetypes: Arena<ArcheType, ArcheTypeTag>,
    static_components: Vec<Layout>,
    dynamic_components: Vec<Layout>,
    query_cache: Arena<QueryCache>,
    bitset_size: usize,
    flag_components: u8,
}
