//! The bounded least-recently-used cache for derived shaping, measurement and
//! preview state. Lookup, promotion, insertion and eviction are O(1). Hits
//! clone the value out, so callers store cheap handles (`Arc`, scalars).

use std::{collections::HashMap, hash::Hash, sync::Mutex};

const NONE: usize = usize::MAX;

struct Slot<K, V> {
    key: K,
    value: V,
    weight: usize,
    newer: usize,
    older: usize,
}

/// Entries live densely in `slots`, threaded newest→oldest by index links;
/// `index` maps each key to its slot.
pub(crate) struct BoundedLru<K, V> {
    index: HashMap<K, usize>,
    slots: Vec<Slot<K, V>>,
    newest: usize,
    oldest: usize,
    capacity: usize,
    budget: usize,
    weight: usize,
}

impl<K: Clone + Eq + Hash, V: Clone> BoundedLru<K, V> {
    pub(crate) fn new(capacity: usize) -> Self {
        Self::with_budget(capacity, usize::MAX)
    }

    /// Bound both the entry count and the summed weight of retained values.
    pub(crate) fn with_budget(capacity: usize, budget: usize) -> Self {
        Self {
            index: HashMap::new(),
            slots: Vec::new(),
            newest: NONE,
            oldest: NONE,
            capacity: capacity.max(1),
            budget,
            weight: 0,
        }
    }

    pub(crate) fn get(&mut self, key: &K) -> Option<V> {
        let slot = *self.index.get(key)?;
        self.unlink(slot);
        self.link_newest(slot);
        Some(self.slots[slot].value.clone())
    }

    pub(crate) fn insert(&mut self, key: K, value: V) {
        self.insert_weighted(key, value, 0);
    }

    /// Insert as the newest entry, then evict oldest entries until both bounds
    /// hold. An entry heavier than the whole budget is not retained.
    pub(crate) fn insert_weighted(&mut self, key: K, value: V, weight: usize) {
        if let Some(&slot) = self.index.get(&key) {
            self.remove(slot);
        }
        let slot = self.slots.len();
        self.slots.push(Slot {
            key: key.clone(),
            value,
            weight,
            newer: NONE,
            older: NONE,
        });
        self.index.insert(key, slot);
        self.link_newest(slot);
        self.weight += weight;
        while self.slots.len() > self.capacity || self.weight > self.budget {
            self.remove(self.oldest);
        }
    }

    pub(crate) fn clear(&mut self) {
        self.index.clear();
        self.slots.clear();
        self.newest = NONE;
        self.oldest = NONE;
        self.weight = 0;
    }

    pub(crate) fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        // Descending order: a removal only moves an already-kept slot down.
        for slot in (0..self.slots.len()).rev() {
            if !keep(&self.slots[slot].key) {
                self.remove(slot);
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn keys(&self) -> impl Iterator<Item = &K> {
        self.slots.iter().map(|slot| &slot.key)
    }

    fn remove(&mut self, slot: usize) {
        self.unlink(slot);
        let removed = self.slots.swap_remove(slot);
        self.index.remove(&removed.key);
        self.weight -= removed.weight;
        if let Some(moved) = self.slots.get(slot) {
            let (newer, older) = (moved.newer, moved.older);
            *self
                .index
                .get_mut(&moved.key)
                .expect("every slot is indexed") = slot;
            self.set_older_of(newer, slot);
            self.set_newer_of(older, slot);
        }
    }

    fn unlink(&mut self, slot: usize) {
        let (newer, older) = (self.slots[slot].newer, self.slots[slot].older);
        self.set_older_of(newer, older);
        self.set_newer_of(older, newer);
    }

    fn link_newest(&mut self, slot: usize) {
        let previous = self.newest;
        self.slots[slot].newer = NONE;
        self.slots[slot].older = previous;
        self.set_newer_of(previous, slot);
        self.newest = slot;
    }

    /// Point `slot`'s older link (or the newest end when `slot` is NONE).
    fn set_older_of(&mut self, slot: usize, older: usize) {
        if slot == NONE {
            self.newest = older;
        } else {
            self.slots[slot].older = older;
        }
    }

    /// Point `slot`'s newer link (or the oldest end when `slot` is NONE).
    fn set_newer_of(&mut self, slot: usize, newer: usize) {
        if slot == NONE {
            self.oldest = newer;
        } else {
            self.slots[slot].newer = newer;
        }
    }
}

/// Return the cached value for `key`, else run `compute` without holding the
/// lock and retain a `Some` result. `None` keys bypass the cache entirely.
/// A panic elsewhere cannot corrupt a cache, so poisoning is recovered.
pub(crate) fn memoize<K: Clone + Eq + Hash, V: Clone>(
    cache: &Mutex<BoundedLru<K, V>>,
    key: Option<K>,
    hit: impl FnOnce(),
    compute: impl FnOnce() -> Option<V>,
) -> Option<V> {
    let Some(key) = key else {
        return compute();
    };
    let cached = cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .get(&key);
    if cached.is_some() {
        hit();
        return cached;
    }
    let value = compute()?;
    cache
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .insert(key, value.clone());
    Some(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evicts_least_recently_used_entry() {
        let mut cache = BoundedLru::new(2);
        cache.insert("old", 1);
        cache.insert("hot", 2);
        assert_eq!(cache.get(&"old"), Some(1));
        cache.insert("new", 3);
        assert_eq!(cache.get(&"hot"), None);
        assert_eq!(cache.get(&"old"), Some(1));
        assert_eq!(cache.get(&"new"), Some(3));
        cache.insert("new", 4);
        assert_eq!(cache.keys().count(), 2);
        assert_eq!(cache.get(&"new"), Some(4));
    }

    #[test]
    fn weight_budget_evicts_oldest_and_rejects_oversized_entries() {
        let mut cache = BoundedLru::with_budget(8, 10);
        cache.insert_weighted('a', (), 4);
        cache.insert_weighted('b', (), 4);
        cache.insert_weighted('c', (), 4);
        assert_eq!(cache.get(&'a'), None);
        assert!(cache.get(&'b').is_some() && cache.get(&'c').is_some());
        cache.insert_weighted('d', (), 11);
        assert_eq!(cache.keys().count(), 0);
        cache.insert_weighted('e', (), 10);
        assert_eq!(cache.keys().copied().collect::<Vec<_>>(), vec!['e']);
    }

    #[test]
    fn retain_and_reinsertion_keep_links_consistent() {
        let mut cache = BoundedLru::new(4);
        for key in 0..4 {
            cache.insert(key, key * 10);
        }
        cache.retain(|key| key % 2 == 0);
        assert_eq!(cache.get(&1), None);
        assert_eq!(cache.get(&0), Some(0));
        cache.insert(5, 50);
        cache.insert(6, 60);
        cache.insert(7, 70);
        // 2 is now the oldest of 2, 0, 5, 6 and is evicted by 7.
        assert_eq!(cache.get(&2), None);
        for (key, value) in [(0, 0), (5, 50), (6, 60), (7, 70)] {
            assert_eq!(cache.get(&key), Some(value));
        }
        cache.clear();
        assert_eq!(cache.keys().count(), 0);
        cache.insert(1, 1);
        assert_eq!(cache.get(&1), Some(1));
    }

    #[test]
    fn memoize_counts_hits_and_skips_failed_computation() {
        let cache = Mutex::new(BoundedLru::new(4));
        let mut hits = 0;
        assert_eq!(memoize(&cache, Some(1), || hits += 1, || None::<u8>), None);
        assert_eq!(memoize(&cache, Some(1), || hits += 1, || Some(7)), Some(7));
        assert_eq!(memoize(&cache, Some(1), || hits += 1, || Some(8)), Some(7));
        assert_eq!(memoize(&cache, None, || hits += 1, || Some(9)), Some(9));
        assert_eq!(hits, 1);
    }
}
