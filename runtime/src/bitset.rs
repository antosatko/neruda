use smallbitvec::SmallBitVec;

#[derive(Debug, Clone, Hash, PartialEq, Eq, Default)]
pub struct Bitset {
    pub data: SmallBitVec,
}

impl Bitset {
    pub fn new() -> Self {
        Self {
            data: SmallBitVec::new(),
        }
    }

    pub fn with_capacity(cap: usize) -> Self {
        let mut data = SmallBitVec::with_capacity(cap);
        data.extend((0..cap).map(|_| false));
        Self { data }
    }

    pub fn resize_total(&mut self, size: usize) {
        self.data
            .extend((self.data.capacity()..size).map(|_| false));
    }

    pub fn zero_all(&mut self) {
        let cap = self.data.len();
        self.data.clear();
        self.data.extend((0..cap).map(|_| false));
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
        self.data.get(n).unwrap_or(false)
    }

    pub fn into_union(&mut self, other: &Self) {
        other.iter_inserted().for_each(|n| self.insert(n));
    }

    pub fn empty(&self) -> bool {
        self.data.all_false()
    }

    pub fn iter_inserted(&self) -> impl Iterator<Item = usize> {
        self.data
            .iter()
            .enumerate()
            .filter(|(_, v)| *v)
            .map(|(n, _)| n)
    }

    pub fn is_subset(&self, other: &Self) -> bool {
        self.iter_inserted()
            .all(|n| other.data.get(n).unwrap_or(false))
    }

    pub fn is_superset(&self, other: &Self) -> bool {
        other.is_subset(self)
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
