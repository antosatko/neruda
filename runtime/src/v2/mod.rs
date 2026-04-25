use std::alloc::Layout;

use arena::{Arena, DynArena, DynKey, Key};

pub mod core;
mod erased;
pub mod ext;
mod tests;

use crate::{
    bitset::Bitset,
    v2::erased::{ErasedBox, ErasedVec, TypeOps},
};

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
    entities: Vec<EntityRefKey>,
    dyn_columns: Vec<ErasedVec>,
    static_columns: Vec<ErasedVec>,
    flag_columns: Vec<u32>,
}

pub type UniqueComponentKey = Key<UniqueComponentTag>;
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq)]
pub struct UniqueComponentTag;
pub struct UniqueComponent {
    data: ErasedBox,
    entity: Option<EntityRefKey>,
}

pub struct Query {
    include: Bitset,
    exclude: Bitset,
    optional: Bitset,
    include_unique: Vec<UniqueComponentKey>,
    exclude_unique: Vec<UniqueComponentKey>,
}

pub struct QueryCache {
    query: Query,
    process_unique: bool,
    archetypes: Vec<(ArcheTypeKey, Vec<Column>)>,
}

pub struct WorldRef<'w> {
    archetype: &'w mut ArcheType,
    entity: Row,
    unique_components: &'w mut Arena<UniqueComponent, UniqueComponentTag>,
    binds: &'w Vec<usize>,
}

pub struct World {
    entities: DynArena<EntityRef>,
    archetypes: Arena<ArcheType, ArcheTypeTag>,
    static_components: Vec<&'static TypeOps>,
    dynamic_components: Vec<&'static TypeOps>,
    unique_components: Arena<UniqueComponent, UniqueComponentTag>,
    query_cache: Arena<QueryCache>,
    bitset_size: usize,
    flag_components: u8,
}
