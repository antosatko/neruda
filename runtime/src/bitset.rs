use fixedbitset::FixedBitSet;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Default)]
pub struct Bitset {
    pub data: FixedBitSet,
}

impl Bitset {
    pub fn new() -> Self {
        Self {
            data: FixedBitSet::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        let mut data = FixedBitSet::with_capacity(cap);
        data.grow(cap);
        Self { data }
    }

    pub fn resize_total(&mut self, size: usize) {
        self.data.grow_and_insert(size);
    }

    pub fn zero_all(&mut self) {
        self.data.set_range(.., false);
    }

    pub fn insert(&mut self, n: usize) {
        self.data.set(n, true);
    }

    pub fn remove(&mut self, n: usize) {
        self.data.set(n, false);
    }

    pub fn set(&mut self, n: usize, val: bool) {
        self.data.set(n, val);
    }

    pub fn get(&self, n: usize) -> bool {
        self.data.contains(n)
    }

    pub fn into_union(&mut self, other: &Self) {
        self.data.union_with(&other.data);
    }

    pub fn empty(&self) -> bool {
        self.data.is_clear()
    }

    pub fn iter_inserted(&self) -> impl Iterator<Item = usize> {
        self.data.ones()
    }

    pub fn is_subset(&self, other: &Self) -> bool {
        self.data.is_subset(&other.data)
    }

    pub fn is_disjoint(&self, other: &Self) -> bool {
        self.data.is_disjoint(&other.data)
    }

    pub fn is_superset(&self, other: &Self) -> bool {
        self.data.is_superset(&other.data)
    }

    pub fn iter_intersection<'a>(&'a self, other: &'a Self) -> impl Iterator<Item = usize> {
        self.data.intersection(&other.data)
    }

    pub fn count_predecesors(&self, n: usize) -> Option<usize> {
        self.get(n)
            .then(|| self.iter_inserted().take_while(|m| *m < n).count())
    }
}
#[cfg(test)]
pub mod tests {
    use crate::bitset::Bitset;

    #[test]
    pub fn capacity() {
        const CAPACITY: usize = 50;
        let mut set = Bitset::with_capacity(CAPACITY);

        set.zero_all();

        set.insert(30);
    }
}
