use std::{alloc::Layout, marker::PhantomData};

use arena::{Arena, Key};

use crate::v2::{ComponentRef, Query, QueryCache, World};

pub struct DynamicComponent<T>(ComponentRef, PhantomData<T>);
pub struct StaticComponent<T>(ComponentRef, PhantomData<T>);
pub struct FlagComponent(u8);

pub struct WorldBuilder {
    static_components: Vec<Layout>,
    dynamic_components: Vec<Layout>,
    flag_components: u8,
    queries: Arena<QueryCache>,
}

impl WorldBuilder {
    pub fn new() -> Self {
        Self {
            static_components: Default::default(),
            dynamic_components: Default::default(),
            queries: Default::default(),
            flag_components: 0,
        }
    }

    pub fn add_static_component<T: Sized>(&mut self) -> StaticComponent<T> {
        assert!(
            self.dynamic_components.is_empty(),
            "Static component initialization must predate dynamic"
        );
        let layout = Layout::new::<T>();
        let idx = self.static_components.len();
        self.static_components.push(layout);
        StaticComponent(idx, PhantomData)
    }

    pub fn add_dynamic_component<T: Sized>(&mut self) -> DynamicComponent<T> {
        let layout = Layout::new::<T>();
        let idx = self.dynamic_components.len();
        self.dynamic_components.push(layout);
        DynamicComponent(idx, PhantomData)
    }

    pub fn add_flag_component(&mut self) -> FlagComponent {
        let idx = self.flag_components;
        self.flag_components += 1;
        FlagComponent(idx)
    }

    pub fn add_query(&mut self, query: Query) -> Key<QueryCache> {
        let cache = QueryCache {
            query,
            archetypes: Vec::new(),
        };
        self.queries.push(cache)
    }

    pub fn build(mut self) -> World {
        assert!(
            self.flag_components <= 32,
            "Flags are restricted to only 32 at a time\nConsider using dynamic components without data"
        );

        self.dynamic_components.shrink_to_fit();
        self.static_components.shrink_to_fit();
        self.queries.shrink();

        let bitset_size = self.dynamic_components.len() + self.static_components.len();

        World {
            archetypes: Default::default(),
            entities: Default::default(),
            dynamic_components: self.dynamic_components,
            static_components: self.static_components,
            flag_components: self.flag_components,
            query_cache: self.queries,
            bitset_size,
        }
    }
}
