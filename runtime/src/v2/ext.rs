use std::{alloc::Layout, fmt::Debug, marker::PhantomData, mem::ManuallyDrop};

pub use arena::{Arena, Key};

use crate::{
    ComponentRef,
    bitset::Bitset,
    v2::{
        ArcheTypeKey, Column, EntityRef, EntityRefKey, Query, QueryCache, QueryKey, QueryTag,
        RuleSet, UniqueComponent, UniqueComponentKey, UniqueComponentTag, World, WorldRef,
        erased::{ErasedBox, TypeOps},
    },
};

#[derive(Debug, Copy, Clone)]
pub struct DynamicComponentRef<T>(ComponentRef, PhantomData<T>);
#[derive(Debug, Copy, Clone)]
pub struct StaticComponentRef<T>(ComponentRef, PhantomData<T>);
#[derive(Debug, Copy, Clone)]
pub struct UniqueComponentRef<T>(UniqueComponentKey, PhantomData<T>);
#[derive(Debug, Copy, Clone)]
pub struct FlagComponent(u8);

pub struct WorldBuilder {
    static_components: Vec<&'static TypeOps>,
    dynamic_components: Vec<&'static TypeOps>,
    unique_components: Arena<UniqueComponent, UniqueComponentTag>,
    queries: Arena<QueryCache, QueryTag>,
    flag_components: u8,
    bitset_size: usize,
}

pub struct EntitySpawner {
    archetype: ArcheTypeKey,
    data: Vec<(Column, ErasedBox)>,
    unique_data: Vec<(UniqueComponentKey, ErasedBox)>,
    flags: u32,
    /// increments on each write, points to the current index to write data into
    ///
    /// while moving data to target arch, this must equal `self.data.len()` and
    /// resets to 0 to prepare for future writes
    data_ptr: usize,
    unique_data_ptr: usize,
}

pub struct EntitySpawnerBuilder {
    signature: Bitset,
    input_order: Vec<ComponentRef>,
    unique_input_order: Vec<UniqueComponentKey>,
    flags: u32,
}

impl EntitySpawner {
    #[track_caller]
    pub fn insert<T>(&mut self, data: T) {
        debug_assert_eq!(self.data[self.data_ptr].1.ops().layout, Layout::new::<T>());
        let mut data = ManuallyDrop::new(data);
        unsafe {
            self.data[self.data_ptr]
                .1
                .write_move((&mut *data as *mut T).cast())
        };
        self.data_ptr += 1;
    }

    #[track_caller]
    pub fn insert_unique<T>(&mut self, data: T) {
        debug_assert_eq!(
            self.unique_data[self.unique_data_ptr].1.ops().layout,
            Layout::new::<T>()
        );
        let mut data = ManuallyDrop::new(data);
        unsafe {
            self.unique_data[self.unique_data_ptr]
                .1
                .write_move((&mut *data as *mut T).cast())
        };
        self.unique_data_ptr += 1;
    }

    #[track_caller]
    pub fn spawn(&mut self, world: &mut World) -> EntityRefKey {
        debug_assert_eq!(self.data_ptr, self.data.len(), "Empty component slots");
        debug_assert_eq!(
            self.unique_data_ptr,
            self.unique_data.len(),
            "Empty unique component slots"
        );
        self.data_ptr = 0;
        self.unique_data_ptr = 0;
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
        let mut unique_component_refs = Vec::with_capacity(self.unique_data.len());
        for (key, ebox) in &mut self.unique_data {
            match world.unique_components.get_mut_unchecked(key) {
                UniqueComponent {
                    entity: Some(_), ..
                } => panic!("Attempted to assign unique component multiple times"),
                UniqueComponent { data, entity } => {
                    *entity = Some(entity_key);
                    ebox.move_into_box(data);
                }
            }
            unique_component_refs.push(*key);
        }
        archetype
            .fixed_columns
            .push((unique_component_refs, self.flags));
        archetype.entities.push(entity_key);
        entity_key
    }
}

impl EntitySpawnerBuilder {
    pub fn new(world: &World) -> Self {
        Self {
            signature: world.empty_bitset(),
            input_order: Vec::new(),
            unique_input_order: Vec::with_capacity(0),
            flags: 0,
        }
    }

    pub fn with(mut self, component: impl Component) -> Self {
        let c_ref = component.get_ref();
        self.signature.insert(c_ref);
        self.input_order.push(c_ref);
        self
    }

    pub fn with_unique<T>(mut self, component: UniqueComponentRef<T>) -> Self {
        self.unique_input_order.push(component.0);
        self
    }

    pub fn with_flag(mut self, flag: FlagComponent) -> Self {
        self.flags |= 1 << flag.0;
        self
    }

    pub fn build(&self, world: &mut World) -> EntitySpawner {
        let mut data: Vec<(usize, ErasedBox)> = self
            .input_order
            .iter()
            .map(|r| (*r, ErasedBox::new_uninit(world.component_typeops(*r))))
            .collect();
        data.sort_by(|(l, _), (r, _)| l.cmp(r));
        let unique_data = self
            .unique_input_order
            .iter()
            .map(|key| {
                (
                    *key,
                    world.unique_components.get_unchecked(key).data.clear_copy(),
                )
            })
            .collect();
        let archetype = world
            .archetypes
            .iter_pairs()
            .find(|(_, a)| a.signature.eq(&self.signature))
            .map(|(k, _)| k);
        let archetype = match archetype {
            Some(a) => a,
            None => world.create_archetype(self.signature.clone()),
        };
        EntitySpawner {
            data,
            unique_data,
            archetype,
            data_ptr: 0,
            unique_data_ptr: 0,
            flags: self.flags,
        }
    }
}

impl Query {
    pub fn with_include(mut self, component: impl Component) -> Self {
        self.components.include.insert(component.get_ref());
        self
    }

    pub fn with_exclude(mut self, component: impl Component) -> Self {
        self.components.exclude.insert(component.get_ref());
        self
    }

    pub fn with_optional(mut self, component: impl Component) -> Self {
        self.components.optional.insert(component.get_ref());
        self
    }

    pub fn with_include_unique<T>(mut self, component: UniqueComponentRef<T>) -> Self {
        self.unique.include.push(component.0);
        self
    }

    pub fn with_exclude_unique<T>(mut self, component: UniqueComponentRef<T>) -> Self {
        self.unique.exclude.push(component.0);
        self
    }

    pub fn with_include_flag(mut self, flag: FlagComponent) -> Self {
        self.flags.include |= 1 << flag.0;
        self
    }

    pub fn with_exclude_flag(mut self, flag: FlagComponent) -> Self {
        self.flags.exclude |= 1 << flag.0;
        self
    }
}

impl WorldBuilder {
    pub fn new() -> Self {
        Self {
            static_components: Default::default(),
            dynamic_components: Default::default(),
            queries: Default::default(),
            unique_components: Default::default(),
            flag_components: 0,
            bitset_size: 0,
        }
    }

    pub fn add_static_component<T: Sized>(&mut self) -> StaticComponentRef<T> {
        assert!(
            self.dynamic_components.is_empty(),
            "Static component initialization must predate dynamic"
        );
        assert!(
            self.queries.len() == 0,
            "Component initialization must predate querries"
        );
        self.bitset_size += 1;
        let layout = TypeOps::new::<T>();
        let idx = self.static_components.len();
        self.static_components.push(Box::leak(Box::new(layout)));
        StaticComponentRef(idx, PhantomData)
    }

    pub fn add_dynamic_component<T: Sized>(&mut self) -> DynamicComponentRef<T> {
        assert!(
            self.queries.len() == 0,
            "Component initialization must predate querries"
        );
        self.bitset_size += 1;
        let layout = TypeOps::new::<T>();
        let idx = self.dynamic_components.len();
        self.dynamic_components.push(Box::leak(Box::new(layout)));
        DynamicComponentRef(idx, PhantomData)
    }

    pub fn add_flag_component(&mut self) -> FlagComponent {
        let idx = self.flag_components;
        self.flag_components += 1;
        FlagComponent(idx)
    }

    pub fn add_unique_component<T: Sized>(&mut self) -> UniqueComponentRef<T> {
        let key = self.unique_components.push(UniqueComponent {
            data: ErasedBox::new_uninit(Box::leak(Box::new(TypeOps::new::<T>()))),
            entity: None,
        });
        UniqueComponentRef(key, PhantomData)
    }

    pub fn add_query(&mut self, query: Query) -> QueryKey {
        debug_assert!(
            query.unique.optional.is_empty(),
            "Optional unique components are not allowed"
        );
        let cache = QueryCache {
            process_unique: query.unique.exclude.len() + query.unique.optional.len() > 0,
            query,
            archetypes: Vec::new(),
        };
        self.queries.push(cache)
    }

    pub fn new_query(&self) -> Query {
        Query {
            components: RuleSet {
                include: Bitset::with_capacity(self.bitset_size),
                exclude: Bitset::with_capacity(self.bitset_size),
                optional: Bitset::with_capacity(self.bitset_size),
            },
            flags: RuleSet {
                include: 0,
                exclude: 0,
                optional: 0,
            },
            unique: RuleSet {
                include: Vec::with_capacity(0),
                exclude: Vec::with_capacity(0),
                optional: Vec::with_capacity(0),
            },
        }
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
            unique_components: self.unique_components,
            query_cache: self.queries,
            bitset_size,
        }
    }
}

impl<'w> WorldRef<'w> {
    #[inline]
    pub fn entity(&self) -> EntityRefKey {
        self.archetype.entities[self.entity]
    }

    #[inline]
    pub fn get_flag(&self, flag: FlagComponent) -> bool {
        (self.archetype.fixed_columns[self.entity].1 & 1 << flag.0) > 0
    }

    #[inline]
    pub fn set_flag(&mut self, flag: FlagComponent) {
        self.archetype.fixed_columns[self.entity].1 |= 1 << flag.0
    }

    #[inline]
    pub fn remove_flag(&mut self, flag: FlagComponent) {
        self.archetype.fixed_columns[self.entity].1 &= !(1 << flag.0)
    }

    #[inline]
    pub fn get_static<T>(&self, component: StaticComponentRef<T>) -> &T {
        let ptr = unsafe {
            self.archetype.static_columns[component.0]
                .ptr
                .as_ptr()
                .add(self.entity * align_of::<T>())
                .cast::<T>()
        };

        unsafe { &*ptr }
    }

    #[inline]
    pub fn get_static_mut<T>(&mut self, component: StaticComponentRef<T>) -> &mut T {
        let ptr = unsafe {
            self.archetype.static_columns[component.0]
                .ptr
                .as_ptr()
                .add(self.entity * align_of::<T>())
                .cast::<T>()
        };

        unsafe { &mut *ptr }
    }
}

pub trait Component {
    fn get_ref(&self) -> ComponentRef;
}

impl<T> Component for StaticComponentRef<T> {
    fn get_ref(&self) -> ComponentRef {
        self.0
    }
}

impl<T> Component for DynamicComponentRef<T> {
    fn get_ref(&self) -> ComponentRef {
        self.0
    }
}
