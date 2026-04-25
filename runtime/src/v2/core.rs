use arena::Key;

use crate::{
    bitset::Bitset,
    v2::{
        ArcheType, ArcheTypeKey, Column, ComponentRef, Query, QueryCache, Row, World, WorldRef,
        erased::{self, ErasedVec, TypeOps},
    },
};

impl World {
    pub(crate) fn empty_bitset(&self) -> Bitset {
        Bitset::with_capacity(self.bitset_size)
    }

    pub(crate) fn component_typeops(&self, component: ComponentRef) -> &'static TypeOps {
        match self.static_components.get(component) {
            Some(ops) => ops,
            None => self.dynamic_components[component - self.static_components.len()],
        }
    }

    pub(crate) fn iter_query(&mut self, query: Key<QueryCache>, mut cb: impl FnMut(WorldRef<'_>)) {
        let query_cache = self.query_cache.get_unchecked(&query);
        let process_unique = query_cache.process_unique;

        for (arch_key, binds) in &query_cache.archetypes {
            let archetype = self.archetypes.get_unchecked(arch_key);
            let e_range = 0..archetype.entities.len();
            for i in e_range {
                let archetype = self.archetypes.get_mut_unchecked(arch_key);
                let world_ref = WorldRef {
                    entity: i,
                    binds,
                    archetype,
                    unique_components: &mut self.unique_components,
                };
                cb(world_ref);
            }
        }
    }

    pub(crate) fn create_archetype(&mut self, signature: Bitset) -> ArcheTypeKey {
        let static_components_cap = self.static_components.len();

        let static_columns: Vec<ErasedVec> = signature
            .iter_inserted()
            .take_while(|n| *n < static_components_cap)
            .map(|idx| ErasedVec::new(self.static_components[idx]))
            .collect();
        let dyn_columns = signature
            .iter_inserted()
            .skip_while(|n| *n < static_components_cap)
            .map(|idx| ErasedVec::new(self.dynamic_components[idx - static_components_cap]))
            .collect();

        let key = unsafe { self.archetypes.empty_alloc() };

        for cache in self.query_cache.iter_mut().filter(|c| {
            c.query.include.is_subset(&signature) && c.query.exclude.is_disjoint(&signature)
        }) {
            let columns = cache
                .query
                .include
                .iter_intersection(&cache.query.optional)
                .skip_while(|n| *n < static_components_cap)
                .map(|n| signature.count_predecesors(n).unwrap())
                .collect();
            cache.archetypes.push((key, columns));
        }

        let arch_ref = self.archetypes.get_mut_unchecked(&key) as *mut ArcheType;
        // safety: archetype must be written using raw pointer to avoid drop on unitialized data
        unsafe {
            arch_ref.write(ArcheType {
                signature,
                entities: Vec::new(),
                dyn_columns,
                static_columns,
                flag_columns: Vec::new(),
            })
        };
        key
    }
}
