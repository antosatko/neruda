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
    fixed_columns: Vec<(Vec<UniqueComponentKey>, u32)>,
}

pub type UniqueComponentKey = Key<UniqueComponentTag>;
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq)]
pub struct UniqueComponentTag;
pub struct UniqueComponent {
    data: ErasedBox,
    entity: Option<EntityRefKey>,
}

struct RuleSet<T> {
    include: T,
    exclude: T,
    optional: T,
}

pub struct Query {
    components: RuleSet<Bitset>,
    unique: RuleSet<Vec<UniqueComponentKey>>,
    flags: RuleSet<u32>,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct QueryTag;
pub type QueryKey = Key<QueryTag>;

pub struct QueryCache {
    query: Query,
    process_unique: bool,
    archetypes: Vec<ArcheTypeQueryCache>,
}

pub struct ArcheTypeQueryCache {
    pub key: ArcheTypeKey,
    pub binds: Vec<Column>,
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
    query_cache: Arena<QueryCache, QueryTag>,
    bitset_size: usize,
    flag_components: u8,
}
