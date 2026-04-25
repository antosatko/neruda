use std::{alloc::Layout, marker::PhantomData, mem::MaybeUninit, ptr::NonNull};

use arena::{Arena, Key};

use crate::{
    bitset::Bitset,
    v2::{
        ArcheTypeKey, Column, ComponentRef, EntityRef, EntityRefKey, Query, QueryCache, World,
        erased::{ErasedBox, TypeOps},
    },
};

pub struct DynamicComponent<T>(ComponentRef, PhantomData<T>);
pub struct StaticComponent<T>(ComponentRef, PhantomData<T>);
pub struct FlagComponent(u8);

pub struct WorldBuilder {
    static_components: Vec<&'static TypeOps>,
    dynamic_components: Vec<&'static TypeOps>,
    queries: Arena<QueryCache>,
    flag_components: u8,
}

pub struct EntitySpawner {
    archetype: ArcheTypeKey,
    data: Vec<(Column, ErasedBox)>,
    /// increments on each write, points to the current index to write data into
    ///
    /// while moving data to target arch, this must equal `self.data.len()` and
    /// resets to 0 to prepare for future writes
    data_ptr: usize,
}

pub struct EntitySpawnerBuilder {
    signature: Bitset,
    input_order: Vec<ComponentRef>,
}

impl EntitySpawner {
    pub fn insert<T>(&mut self, data: T) {
        unsafe {
            self.data[self.data_ptr]
                .1
                .write_move(&data as *const T as *mut u8)
        };
        self.data_ptr += 1;
    }

    #[track_caller]
    pub fn spawn(&mut self, world: &mut World) -> EntityRefKey {
        debug_assert_eq!(self.data_ptr, self.data.len(), "Empty component slots");
        self.data_ptr = 0;
        let archetype = world.archetypes.get_mut_unchecked(&self.archetype);
        for (column, ebox) in &mut self.data {
            let container = match archetype.static_columns.get_mut(*column) {
                Some(c) => c,
                None => &mut archetype.dyn_columns[*column - archetype.static_columns.len()],
            };
            container.push_box(ebox);
        }
        let entity = EntityRef {
            archetype: self.archetype,
            row: archetype.entities.len(),
        };
        let entity_key = world.entities.push(entity);
        archetype.entities.push(entity_key);
        entity_key
    }
}

impl EntitySpawnerBuilder {
    pub fn new(world: &World) -> Self {
        Self {
            signature: world.empty_bitset(),
            input_order: Vec::new(),
        }
    }

    pub fn with(mut self, component: impl Component) -> Self {
        let c_ref = component.get_ref();
        self.signature.insert(c_ref);
        self.input_order.push(c_ref);
        self
    }

    pub fn build(self, world: &mut World) -> EntitySpawner {
        let mut data: Vec<(usize, ErasedBox)> = self
            .input_order
            .iter()
            .map(|r| (*r, ErasedBox::new_uninit(world.component_typeops(*r))))
            .collect();
        data.sort_by(|(l, _), (r, _)| l.cmp(r));
        let archetype = world
            .archetypes
            .iter_pairs()
            .find(|(_, a)| a.signature.eq(&self.signature))
            .map(|(k, _)| k);
        let archetype = match archetype {
            Some(a) => a,
            None => world.create_archetype(self.signature),
        };
        EntitySpawner {
            data,
            archetype,
            data_ptr: 0,
        }
    }
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
        let layout = TypeOps::new::<T>();
        let idx = self.static_components.len();
        self.static_components.push(Box::leak(Box::new(layout)));
        StaticComponent(idx, PhantomData)
    }

    pub fn add_dynamic_component<T: Sized>(&mut self) -> DynamicComponent<T> {
        let layout = TypeOps::new::<T>();
        let idx = self.dynamic_components.len();
        self.dynamic_components.push(Box::leak(Box::new(layout)));
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

pub trait Component {
    fn get_ref(&self) -> ComponentRef;
}

impl<T> Component for StaticComponent<T> {
    fn get_ref(&self) -> ComponentRef {
        self.0
    }
}

impl<T> Component for DynamicComponent<T> {
    fn get_ref(&self) -> ComponentRef {
        self.0
    }
}
