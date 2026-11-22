/// A set of edges (or of any indices below a bound), one bit each: the test in the inner loop
/// of the search is an index and a mask, where a hashed set costs a hash and a probe.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EdgeSet {
    words: Vec<u64>,
    len: usize,
}

impl EdgeSet {
    /// An empty set that holds indices below `bound` without growing.
    pub fn with_bound(bound: usize) -> EdgeSet {
        EdgeSet { words: vec![0; bound.div_ceil(64)], len: 0 }
    }

    pub fn contains(&self, edge: u32) -> bool {
        self.words.get(edge as usize / 64).is_some_and(|w| w & (1 << (edge % 64)) != 0)
    }

    /// Adds `edge`; returns whether it was not in the set.
    pub fn insert(&mut self, edge: u32) -> bool {
        let word = edge as usize / 64;
        if word >= self.words.len() {
            self.words.resize(word + 1, 0);
        }
        let mask = 1 << (edge % 64);
        let added = self.words[word] & mask == 0;
        self.words[word] |= mask;
        self.len += usize::from(added);
        added
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The edges of the set, in increasing order.
    pub fn iter(&self) -> impl Iterator<Item = u32> + '_ {
        self.words.iter().enumerate().flat_map(|(i, &w)| (0..64u32).filter(move |b| w & (1 << b) != 0).map(move |b| i as u32 * 64 + b))
    }
}

impl FromIterator<u32> for EdgeSet {
    fn from_iter<I: IntoIterator<Item = u32>>(iter: I) -> EdgeSet {
        let mut set = EdgeSet::default();
        for edge in iter {
            set.insert(edge);
        }
        set
    }
}
