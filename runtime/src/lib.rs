use std::{cell::UnsafeCell, collections::HashMap, marker::PhantomData};

use any_vec::{AnyVec, any_value::AnyValueWrapper};
use arena::{Arena, DynArena, DynKey, Key};

use crate::bitset::Bitset;

mod bitset;

pub type Unit = u8;
pub type Mutability = bool;
pub type Required = bool;
pub type Array = AnyVec;
pub type ComponentId = usize;
pub type ComponentRef = usize;
pub type ComponentSize = usize;
pub type Signature = Bitset;
pub type ArchetypeId = usize;
pub type ComponentQuerry = (ComponentRef, Required, Mutability);
pub type EntityRef = DynKey<EntityTag>;
pub type ArrayParents = Vec<EntityRef>;
pub type TypedComponentRef<T> = (usize, PhantomData<T>);
pub type TypedOptComponentRef<T> = (usize, PhantomData<T>, Optional);

#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq)]
pub struct ArchetypeTag;
#[derive(Debug, Copy, Clone, PartialEq, PartialOrd, Ord, Eq)]
pub struct EntityTag;

#[derive(Debug)]
pub struct World {
    pub entities: DynArena<Entity, EntityTag>,
    pub archetypes: Arena<Archetype, ArchetypeTag>,
    components: Vec<ComponentData>,
}

#[derive(Debug)]
pub struct Archetype {
    signature: Signature,
    components: Vec<UnsafeCell<ComponentData>>,
    entities: ArrayParents,
    edges: HashMap<ComponentRef, ArcheTypeEdge>,
}

#[derive(Debug)]
pub struct ArcheTypeEdge {
    next: Option<Key<ArchetypeTag>>,
    prev: Option<Key<ArchetypeTag>>,
}

#[derive(Debug)]
pub struct Entity {
    pub archetype_key: Key<ArchetypeTag>,
    pub archetype_id: ArchetypeId,
}

#[derive(Debug)]
pub struct EntitySpawner<'world> {
    sig: Signature,
    components: Vec<(AnyVec, usize)>,
    world: &'world mut World,
    spawned: bool,
}

#[derive(Debug)]
struct ComponentData {
    pub container: AnyVec,
}

#[derive(Debug)]
pub struct WorldRefSig<'world, 'querry> {
    arch_entity_idx: usize,
    components: &'world [UnsafeCell<ComponentData>],
    signature: &'world Signature,
    binds: &'querry [Option<ComponentRef>],
}

#[derive(Debug, Clone, Hash, Default)]
struct QuerrySignature {
    include: Signature,
    exclude: Signature,
}

impl QuerrySignature {
    pub fn new(comps: usize) -> Self {
        Self {
            include: Bitset::with_capacity(comps),
            exclude: Bitset::with_capacity(comps),
        }
    }
}

pub struct Querry {
    signature: QuerrySignature,
    /// <(ComponentRef, is_bound)> - this information is later
    /// used for generating the bind table
    binds: Vec<(ComponentRef, bool)>,
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

impl World {
    pub fn new() -> Self {
        Self {
            entities: DynArena::new(),
            archetypes: Arena::new(),
            components: Vec::new(),
        }
    }

    pub fn define_component<T>(&mut self) -> TypedComponentRef<T>
    where
        T: Sized + Copy + 'static,
    {
        let id = self.components.len();
        self.components.push(ComponentData {
            container: AnyVec::new::<T>(),
        });
        (id, Default::default())
    }

    fn add_entity(&mut self, arch_key: &Key<ArchetypeTag>, arch_id: ArchetypeId) -> EntityRef {
        let id = self.entities.push(Entity {
            archetype_key: *arch_key,
            archetype_id: arch_id,
        });
        self.archetypes
            .get_mut_unchecked(arch_key)
            .entities
            .push(id);
        id
    }

    pub fn entity_count(&self) -> usize {
        self.entities.len()
    }

    pub fn archetype_count(&self) -> usize {
        self.archetypes.len()
    }

    pub fn component_count(&self) -> usize {
        self.components.len()
    }

    fn add_archetype_with_init(&mut self, mut archetype: Archetype) -> Key<ArchetypeTag> {
        for comp_ref in archetype.signature.iter_inserted() {
            let c_desc = &self.components[comp_ref];
            archetype.components.push(UnsafeCell::new(ComponentData {
                container: c_desc.container.clone_empty(),
            }));
        }
        self.archetypes.push(archetype)
    }

    pub fn append_component<T: 'static>(
        &mut self,
        e: &EntityRef,
        value: T,
        c_type: TypedComponentRef<T>,
    ) {
        let next = self.arch_next_edge(e, c_type);
        self.relocate_components(e, &next);
        let arche = self.find_archetype_mut(e);
        let any_val = AnyValueWrapper::new(value);
        let local_idx = arche
            .signature
            .count_predecesors(c_type.0)
            .expect("Component was just added to signature, it must exist");

        arche.components[local_idx]
            .get_mut()
            .container
            .push(any_val);
    }

    pub fn remove_component<T>(&mut self, e: &EntityRef, c_type: TypedComponentRef<T>) {
        let prev = self.arch_prev_edge(e, c_type);
        self.relocate_components(e, &prev);
    }

    fn arch_next_edge<T>(
        &mut self,
        e: &DynKey<EntityTag>,
        c_type: (usize, PhantomData<T>),
    ) -> Key<ArchetypeTag> {
        let arche = self.find_archetype_mut(e);
        let next = if let Some(ArcheTypeEdge {
            next: Some(next),
            prev: _,
        }) = arche.edges.get(&c_type.0)
        {
            *next
        } else {
            let mut signature = arche.signature.clone();
            signature.insert(c_type.0);
            let key = self.find_or_crate_archetype(signature);
            let arche = self.find_archetype_mut(e);
            match arche.edges.get_mut(&c_type.0) {
                Some(ArcheTypeEdge { next, prev: _ }) => *next = Some(key),
                None => drop(arche.edges.insert(
                    c_type.0,
                    ArcheTypeEdge {
                        next: Some(key),
                        prev: None,
                    },
                )),
            }
            key
        };
        next
    }

    fn arch_prev_edge<T>(
        &mut self,
        e: &DynKey<EntityTag>,
        c_type: (usize, PhantomData<T>),
    ) -> Key<ArchetypeTag> {
        let arche = self.find_archetype_mut(e);
        let prev = if let Some(ArcheTypeEdge {
            next: _,
            prev: Some(prev),
        }) = arche.edges.get(&c_type.0)
        {
            *prev
        } else {
            let mut signature = arche.signature.clone();
            signature.remove(c_type.0);
            let key = self.find_or_crate_archetype(signature);
            let arche = self.find_archetype_mut(e);
            match arche.edges.get_mut(&c_type.0) {
                Some(ArcheTypeEdge { next: _, prev }) => *prev = Some(key),
                None => drop(arche.edges.insert(
                    c_type.0,
                    ArcheTypeEdge {
                        next: None,
                        prev: Some(key),
                    },
                )),
            }
            key
        };
        prev
    }

    fn find_or_crate_archetype(&mut self, signature: Signature) -> Key<ArchetypeTag> {
        if let Some((key, _)) = &self
            .archetypes
            .iter_pairs()
            .find(|(_, a)| a.signature == signature)
        {
            *key
        } else {
            let archetype = Archetype::new_uninit(signature);
            self.add_archetype_with_init(archetype)
        }
    }

    fn find_archetype_mut(&mut self, e: &EntityRef) -> &mut Archetype {
        let entity = &self.entities.get_unchecked(e);
        self.archetypes.get_mut_unchecked(&entity.archetype_key)
    }

    pub fn delete_entity(&mut self, e: &EntityRef) {
        let entity = self.entities.get_unchecked(e);
        let arch = self.archetypes.get_mut_unchecked(&entity.archetype_key);
        if let Some(moved) = arch.remove_entity(entity.archetype_id) {
            self.entities.get_mut_unchecked(&moved).archetype_id = entity.archetype_id;
        }
        self.entities.delete(e);
    }

    fn relocate_components(&mut self, src: &EntityRef, dst: &Key<ArchetypeTag>) {
        let src_arch_key = self.entities.get_unchecked(src).archetype_key;

        let [src_arch, dst_arch] =
            unsafe { self.archetypes.get_disj_unchecked_mut([&src_arch_key, dst]) };

        let src_entity = self.entities.get_unchecked(src).archetype_id;

        for (src_idx, component) in src_arch.signature.iter_inserted().enumerate() {
            let src_container = &mut src_arch.components[src_idx].get_mut().container;
            let value = src_container.swap_remove(src_entity);

            if let Some(dst_idx) = dst_arch.signature.count_predecesors(component) {
                let dst_container = &mut dst_arch.components[dst_idx].get_mut().container;

                dst_container.push(value);
            }
        }
        let swapped = src_arch
            .entities
            .pop()
            .expect("There must be at least two at this point");
        if src_arch.entities.len() != src_entity {
            let src_entity_record = &mut self.entities.get_mut_unchecked(&swapped);
            src_entity_record.archetype_id = src_entity;
            src_arch.entities[src_entity] = swapped;
        }
        let entity_record = &mut self.entities.get_mut_unchecked(src);
        entity_record.archetype_key = *dst;
        entity_record.archetype_id = dst_arch.entities.len();

        dst_arch.entities.push(*src);
    }

    fn archetypes_matching<'a>(
        &'a self,
        q: &QuerrySignature,
    ) -> impl Iterator<Item = &'a Archetype> {
        self.archetypes.iter().filter(|arch| {
            arch.signature.is_superset(&q.include)
                && (q.exclude.empty() || !q.exclude.is_subset(&arch.signature))
        })
    }

    pub fn run_querry<'world>(
        &'world mut self,
        querry: &Querry,
        mut cb: impl FnMut(&WorldRefSig<'world, '_>),
    ) {
        let mut binds = Vec::new();
        for arch in self.archetypes_matching(&querry.signature) {
            if !querry.binds.is_empty() {
                binds.clear();
                binds.extend(querry.binds.iter().filter(|(_, bound)| *bound).map(
                    |(comp_ref, _)| {
                        arch.signature
                            .iter_inserted()
                            .position(|found_ref| &found_ref == comp_ref)
                    },
                ));
            }
            let mut w_ref = WorldRefSig {
                arch_entity_idx: 0,
                components: &arch.components,
                signature: &arch.signature,
                binds: &binds,
            };
            for (idx, _) in arch.entities.iter().enumerate() {
                w_ref.arch_entity_idx = idx;
                cb(&w_ref);
            }
        }
    }
}

impl Archetype {
    pub fn new_uninit(signature: Signature) -> Self {
        Self {
            signature,
            components: Vec::new(),
            entities: Vec::new(),
            edges: HashMap::new(),
        }
    }

    /// Swaps the entity with the last and removes record of it.
    /// This function does not delete any component data
    ///
    /// Returns: ref to the entity that is now in its place
    ///
    /// Panics: If invalid index is passed
    pub(crate) fn remove_entity(&mut self, e: usize) -> Option<EntityRef> {
        let len = self.entities.len();
        assert!(e < len, "Invalid entity index");

        if len == 1 {
            self.entities.clear();
            for component in &mut self.components {
                component.get_mut().container.clear();
            }
            return None;
        }
        if e == len - 1 {
            self.entities.pop();
            for component in &mut self.components {
                component.get_mut().container.pop();
            }
            return None;
        }

        for component in &mut self.components {
            let com = component.get_mut();
            com.container.swap_remove(e);
        }

        let e_moved = self
            .entities
            .pop()
            .expect("There must be at least 2 entities at this point");
        self.entities[e] = e_moved;
        Some(e_moved)
    }
}

impl<'world> WorldRefSig<'world, '_> {
    #[track_caller]
    pub fn get_unchecked<T: 'static>(&self, component: TypedComponentRef<T>) -> &'world T {
        let idx = self
            .signature
            .iter_inserted()
            .enumerate()
            .find(|(_, cref)| *cref == component.0)
            .unwrap()
            .0;

        unsafe {
            let column = &*self.components[idx].get();
            column
                .container
                .get_unchecked(self.arch_entity_idx)
                .downcast_ref_unchecked()
        }
    }

    #[track_caller]
    pub fn get_mut_unchecked<T: 'static>(&self, component: TypedComponentRef<T>) -> &'world mut T {
        let idx = self
            .signature
            .iter_inserted()
            .enumerate()
            .find(|(_, cref)| *cref == component.0)
            .unwrap()
            .0;

        unsafe {
            let column = &mut *self.components[idx].get();
            column
                .container
                .get_unchecked_mut(self.arch_entity_idx)
                .downcast_mut_unchecked()
        }
    }

    #[track_caller]
    pub fn get_bound_unchecked<T: 'static>(&self, typed_bind: TypedComponentRef<T>) -> &'world T {
        let idx = unsafe { self.binds[typed_bind.0].unwrap_unchecked() };

        unsafe {
            let column = &*self.components[idx].get();
            column
                .container
                .get_unchecked(self.arch_entity_idx)
                .downcast_ref_unchecked()
        }
    }

    #[track_caller]
    pub fn get_bound_opt<T: 'static>(
        &self,
        typed_bind: TypedOptComponentRef<T>,
    ) -> Option<&'world T> {
        let idx = self.binds[typed_bind.0]?;

        unsafe {
            let column = &*self.components[idx].get();
            Some(
                column
                    .container
                    .get_unchecked(self.arch_entity_idx)
                    .downcast_ref_unchecked(),
            )
        }
    }

    #[track_caller]
    pub fn get_mut_bound<T: 'static>(&self, typed_bind: TypedComponentRef<T>) -> &'world mut T {
        let idx = unsafe { self.binds[typed_bind.0].unwrap_unchecked() };

        unsafe {
            let column = &mut *self.components[idx].get();
            column
                .container
                .get_unchecked_mut(self.arch_entity_idx)
                .downcast_mut_unchecked()
        }
    }
}

impl<'world> EntitySpawner<'world> {
    pub fn new(world: &'world mut World) -> Self {
        Self {
            sig: Bitset::with_capacity(world.components.len()),
            components: Vec::with_capacity(10),
            world,
            spawned: false,
        }
    }

    pub fn clear(&mut self) {
        self.components.clear();
        self.sig.zero_all();
        self.spawned = false;
    }

    pub fn component<T: 'static>(mut self, data: T, kind: TypedComponentRef<T>) -> Self {
        if self.spawned {
            self.clear();
        }
        self.sig.insert(kind.0);
        let mut vec = AnyVec::with_capacity::<T>(1);
        vec.push(AnyValueWrapper::new(data));
        self.components.push((vec, kind.0));

        self
    }

    pub fn spawn(&mut self) -> EntityRef {
        self.spawned = true;
        self.components.sort_by(|l, r| l.1.cmp(&r.1));

        let key = self
            .world
            .archetypes
            .iter_pairs()
            .find(|(_, arch)| arch.signature.eq(&self.sig))
            .map(|(key, _)| key);

        match key {
            Some(key) => {
                let arch = self.world.archetypes.get_mut_unchecked(&key);
                for (idx, _) in arch.signature.iter_inserted().enumerate() {
                    let com = arch.components[idx].get_mut();
                    let src = &mut self.components[idx].0;

                    com.container
                        .push(src.pop().expect("Must contain exactly 1 element"))
                }
                let len = arch.entities.len();
                self.world.add_entity(&key, len)
            }
            None => {
                let arch = Archetype::new_uninit(self.sig.clone());
                let arch_key = self.world.add_archetype_with_init(arch);
                let arch = self.world.archetypes.get_mut_unchecked(&arch_key);
                for (idx, flag) in arch.signature.iter_inserted().enumerate() {
                    let mut container = self.world.components[flag].container.clone_empty();
                    container.push(
                        self.components[idx]
                            .0
                            .pop()
                            .expect("Must contain exactly 1 element"),
                    );
                    arch.components[idx] = UnsafeCell::new(ComponentData { container })
                }
                self.world.add_entity(&arch_key, 0)
            }
        }
    }
}

#[derive(Debug, Copy, Clone, Default)]
pub struct Optional;
impl Querry {
    pub fn new(world: &World) -> Self {
        Self {
            signature: QuerrySignature::new(world.components.len()),
            binds: Vec::with_capacity(10),
        }
    }

    pub fn include<T>(mut self, component: TypedComponentRef<T>) -> Self {
        self.signature.include.insert(component.0);
        self
    }

    pub fn include_bind<T>(
        mut self,
        component: TypedComponentRef<T>,
        bind: &mut (usize, PhantomData<T>),
    ) -> Self {
        bind.0 = self.binds.len();
        self.signature.include.insert(component.0);
        self.binds.push((component.0, true));
        self
    }

    pub fn optional_bind<T>(
        mut self,
        component: TypedComponentRef<T>,
        bind: &mut (usize, PhantomData<T>, Optional),
    ) -> Self {
        bind.0 = self.binds.len();
        self.binds.push((component.0, true));
        self
    }

    pub fn exclude<T>(mut self, component: TypedComponentRef<T>) -> Self {
        self.signature.exclude.insert(component.0);
        self
    }
}

#[cfg(test)]
mod tests {

    use crate::{EntitySpawner, Querry, World};

    #[test]
    fn arch() {
        let mut world = World::new();

        let c_position = world.define_component();
        let c_velocity = world.define_component();
        let c_player_tag = world.define_component();

        EntitySpawner::new(&mut world)
            .component((0.0, 0.0), c_position)
            .component((1.1, 5.5), c_velocity)
            .component((), c_player_tag)
            .spawn();

        EntitySpawner::new(&mut world)
            .component((100.0, 5.0), c_position)
            .component((-10.0, 0.0), c_velocity)
            .spawn();

        world.run_querry(
            &Querry::new(&world).include(c_position).include(c_velocity),
            |world| {
                let pos = world.get_mut_unchecked(c_position);
                let vel = world.get_unchecked(c_velocity);
                pos.0 += vel.0;
                pos.1 += vel.1;
            },
        );
        world.run_querry(
            &Querry::new(&world)
                .include(c_position)
                .include(c_velocity)
                .include(c_player_tag),
            |world| {
                let pos = *world.get_unchecked(c_position);
                assert_eq!(pos, (1.1, 5.5));
            },
        );
    }

    #[test]
    fn append_remove_component() {
        let mut world = World::new();

        let c_position = world.define_component();
        let c_velocity = world.define_component();
        let c_player_tag = world.define_component();

        EntitySpawner::new(&mut world)
            .component((0.0, 0.0), c_position)
            .component((1.1, 5.5), c_velocity)
            .component((), c_player_tag)
            .spawn();

        let e2 = EntitySpawner::new(&mut world)
            .component((100.0, 5.0), c_position)
            .component((-10.0, 0.0), c_velocity)
            .spawn();

        world.append_component(&e2, (), c_player_tag);

        let mut count = 0;
        world.run_querry(
            &Querry::new(&world)
                .include(c_position)
                .include(c_velocity)
                .include(c_player_tag),
            |_| {
                count += 1;
            },
        );
        assert_eq!(count, 2);
        let mut count = 0;
        world.run_querry(
            &Querry::new(&world)
                .include(c_position)
                .include(c_velocity)
                .exclude(c_player_tag),
            |_| {
                count += 1;
            },
        );
        assert_eq!(count, 0);

        world.remove_component(&e2, c_position);

        let mut count = 0;
        world.run_querry(&Querry::new(&world).exclude(c_position), |_| {
            count += 1;
        });
        assert_eq!(count, 1);
    }

    #[test]
    fn arch_multi_archetype_delete_every_5th() {
        let mut world = World::new();

        let c_position = world.define_component();
        let c_velocity = world.define_component();
        let c_player_tag = world.define_component();
        let c_health = world.define_component();

        const N: usize = 100;
        let mut removed = Vec::new();

        for i in 0..N {
            let mut spawner = EntitySpawner::new(&mut world).component((i, 0), c_position);

            if i % 2 == 0 {
                spawner = spawner.component((1.0f32, 0.0f32), c_velocity);
            }
            if i % 3 == 0 {
                spawner = spawner.component((), c_player_tag);
            }
            if i % 7 == 0 {
                spawner = spawner.component(100i32, c_health);
            }

            let e = spawner.spawn();
            if i % 5 == 0 {
                removed.push(e);
            }
        }

        for e in &removed {
            world.delete_entity(e);
        }

        let expected_remaining = N - removed.len();
        let mut count = 0;
        world.run_querry(&Querry::new(&world), |_| {
            count += 1;
        });

        assert_eq!(world.entity_count(), expected_remaining);
        assert_eq!(count, expected_remaining);

        let mut bind = Default::default();
        // world.run_querry(
        //     &Querry::new(&world).include_bind(c_position, &mut bind),
        //     |world| print!("{:?}, ", world.get_bound_unchecked(bind)),
        // );

        world.run_querry(
            &Querry::new(&world).include_bind(c_position, &mut bind),
            |world| assert_ne!(world.get_bound_unchecked(bind).0 % 5, 0),
        );
    }

    #[test]
    fn querry() {
        let mut world = World::new();

        let c_position = world.define_component();
        let c_velocity = world.define_component();
        let c_player_tag = world.define_component();
        let c_health = world.define_component();

        const N: usize = 100;

        for i in 0..N {
            let mut spawner = EntitySpawner::new(&mut world).component((i, 0), c_position);

            if i % 2 == 0 {
                spawner = spawner.component((1.0f32, 0.0f32), c_velocity);
            }
            if i % 3 == 0 {
                spawner = spawner.component((), c_player_tag);
            }
            if i % 7 == 0 {
                spawner = spawner.component(100i32, c_health);
            }

            spawner.spawn();
        }

        let mut b_position = Default::default();
        let mut b_velocity = Default::default();

        world.run_querry(
            &Querry::new(&world)
                .include_bind(c_position, &mut b_position)
                .exclude(c_velocity),
            |world| assert_ne!(world.get_bound_unchecked(b_position).0 % 2, 0),
        );

        world.run_querry(
            &Querry::new(&world)
                .include_bind(c_position, &mut b_position)
                .optional_bind(c_velocity, &mut b_velocity),
            |world| {
                assert_eq!(
                    world.get_bound_unchecked(b_position).0 % 2 == 0,
                    world.get_bound_opt(b_velocity).is_some()
                );
            },
        );
    }
}
