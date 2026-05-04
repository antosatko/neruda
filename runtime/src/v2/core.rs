use arena::Key;

use crate::{
    bitset::Bitset,
    v2::{
        ArcheType, ArcheTypeKey, ArcheTypeQueryCache, ComponentRef, QueryKey, RuleSet, World,
        WorldRef,
        erased::{ErasedVec, TypeOps},
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

    pub fn iter_query(&mut self, query: QueryKey, mut cb: impl FnMut(WorldRef<'_>)) {
        let query_cache = self.query_cache.get_unchecked(&query);
        let RuleSet {
            include, exclude, ..
        } = query_cache.query.flags;
        let process_flags = include + exclude > 0;
        let process_unique = query_cache.process_unique;

        for ArcheTypeQueryCache { key, binds } in &query_cache.archetypes {
            let archetype = self.archetypes.get_unchecked(key);
            let e_range = 0..archetype.entities.len();
            for e in e_range {
                let archetype = self.archetypes.get_mut_unchecked(key);

                let (uniques, flags) = &archetype.fixed_columns[e];

                if process_flags && (*flags & include != include || *flags & exclude != 0) {
                    continue;
                }

                if process_unique
                    && (uniques
                        .iter()
                        .any(|unique| query_cache.query.unique.exclude.contains(unique)))
                {
                    continue;
                }

                let world_ref = WorldRef {
                    entity: e,
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

        let static_columns: Vec<ErasedVec> = self
            .static_components
            .iter()
            .map(|ops| ErasedVec::new(ops))
            .collect();
        let dyn_columns = signature
            .iter_inserted()
            .skip_while(|n| *n < static_components_cap)
            .map(|idx| ErasedVec::new(self.dynamic_components[idx - static_components_cap]))
            .collect();

        let key = unsafe { self.archetypes.empty_alloc() };

        for cache in self.query_cache.iter_mut().filter(|c| {
            c.query.components.include.is_subset(&signature)
                && c.query.components.exclude.is_disjoint(&signature)
        }) {
            let binds = cache
                .query
                .components
                .include
                .iter_intersection(&cache.query.components.optional)
                .skip_while(|n| *n < static_components_cap)
                .map(|n| signature.count_predecesors(n).unwrap())
                .collect();
            cache.archetypes.push(ArcheTypeQueryCache { key, binds });
        }

        let arch_ref = self.archetypes.get_mut_unchecked(&key) as *mut ArcheType;
        // safety: archetype must be written using raw pointer to avoid drop on unitialized data
        unsafe {
            arch_ref.write(ArcheType {
                signature,
                entities: Vec::new(),
                dyn_columns,
                static_columns,
                fixed_columns: Vec::new(),
            })
        };
        key
    }
}
