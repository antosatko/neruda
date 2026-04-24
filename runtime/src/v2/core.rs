use crate::{
    bitset::Bitset,
    v2::{
        ArcheType, ArcheTypeKey, World,
        erased_vec::{ErasedVec, TypeOps},
    },
};

impl World {
    fn empty_bitset(&self) -> Bitset {
        Bitset::with_capacity(self.bitset_size)
    }

    fn create_archetype(&mut self, signature: Bitset) -> ArcheTypeKey {
        let static_components_cap = self.static_components.len();

        let static_columns: Vec<ErasedVec> = signature
            .iter_inserted()
            .take_while(|n| *n < static_components_cap)
            .map(|idx| {
                ErasedVec::new(unsafe {
                    Box::leak(Box::new(TypeOps::from_layout(self.static_components[idx])))
                })
            })
            .collect();
        let dyn_columns = signature
            .iter_inserted()
            .skip_while(|n| *n < static_components_cap)
            .map(|idx| unsafe {
                ErasedVec::new(Box::leak(Box::new(TypeOps::from_layout(
                    self.dynamic_components[idx - static_components_cap],
                ))))
            })
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

        *self.archetypes.get_mut_unchecked(&key) = ArcheType {
            entities: Vec::new(),
            flag_columns: Vec::new(),
            static_columns,
            dyn_columns,
            signature,
        };
        key
    }
}
