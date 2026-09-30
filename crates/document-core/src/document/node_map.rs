use std::{
    collections::{BTreeMap, BTreeSet},
    fmt,
    sync::Arc,
};

use rustc_hash::FxHashMap;

use crate::NodeId;

const FANOUT: usize = 64;

type Leaf<V> = Arc<FxHashMap<NodeId, V>>;
type Branch<V> = Arc<[Leaf<V>; FANOUT]>;

/// Persistent per-node snapshot metadata: a fixed two-level trie of hash-map
/// leaves. Clones share every level, and a write copies only the root and
/// branch pointer arrays plus the one leaf holding the key, so a transaction
/// costs what it touches rather than document size or undo history.
#[derive(Clone)]
pub(crate) struct NodeMap<V> {
    root: Arc<[Branch<V>; FANOUT]>,
    len: usize,
}

impl<V> Default for NodeMap<V> {
    fn default() -> Self {
        let leaf: Leaf<V> = Arc::default();
        let branch: Branch<V> = Arc::new(std::array::from_fn(|_| Arc::clone(&leaf)));
        Self {
            root: Arc::new(std::array::from_fn(|_| Arc::clone(&branch))),
            len: 0,
        }
    }
}

impl<V> NodeMap<V> {
    fn slot(id: NodeId) -> (usize, usize) {
        // Fibonacci hashing spreads sequentially allocated IDs over all leaves.
        let hash = id.get().wrapping_mul(0x9E37_79B9_7F4A_7C15);
        ((hash >> 58) as usize, (hash >> 52) as usize % FANOUT)
    }

    fn leaf(&self, id: NodeId) -> &FxHashMap<NodeId, V> {
        let (branch, leaf) = Self::slot(id);
        &self.root[branch][leaf]
    }

    fn leaves(&self) -> impl Iterator<Item = &Leaf<V>> {
        self.root.iter().flat_map(|branch| branch.iter())
    }

    pub(crate) fn len(&self) -> usize {
        self.len
    }

    pub(crate) fn get(&self, id: NodeId) -> Option<&V> {
        self.leaf(id).get(&id)
    }

    pub(crate) fn contains_key(&self, id: NodeId) -> bool {
        self.leaf(id).contains_key(&id)
    }

    pub(crate) fn iter(&self) -> impl Iterator<Item = (NodeId, &V)> {
        self.leaves()
            .flat_map(|leaf| leaf.iter().map(|(id, value)| (*id, value)))
    }

    /// Version identity: a write to a map shared with another snapshot always
    /// produces a new root, even when it stores equal values.
    pub(crate) fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.root, &other.root)
    }

    #[cfg(test)]
    pub(crate) fn unshared_leaves(&self, other: &Self) -> usize {
        self.leaves()
            .zip(other.leaves())
            .filter(|(left, right)| !Arc::ptr_eq(left, right))
            .count()
    }
}

impl<V: Clone> NodeMap<V> {
    fn leaf_mut(&mut self, id: NodeId) -> &mut FxHashMap<NodeId, V> {
        let (branch, leaf) = Self::slot(id);
        let branch = &mut Arc::make_mut(&mut self.root)[branch];
        Arc::make_mut(&mut Arc::make_mut(branch)[leaf])
    }

    pub(crate) fn insert(&mut self, id: NodeId, value: V) -> Option<V> {
        let previous = self.leaf_mut(id).insert(id, value);
        self.len += usize::from(previous.is_none());
        previous
    }

    pub(crate) fn remove(&mut self, id: NodeId) -> Option<V> {
        if !self.contains_key(id) {
            return None;
        }
        let previous = self.leaf_mut(id).remove(&id);
        self.len -= usize::from(previous.is_some());
        previous
    }
}

impl<V: PartialEq> PartialEq for NodeMap<V> {
    fn eq(&self, other: &Self) -> bool {
        // Equal keys share a slot, so versions compare leaf by leaf.
        self.len == other.len
            && self
                .leaves()
                .zip(other.leaves())
                .all(|(left, right)| Arc::ptr_eq(left, right) || left == right)
    }
}

impl<V: Eq> Eq for NodeMap<V> {}

impl<V: fmt::Debug> fmt::Debug for NodeMap<V> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_map()
            .entries(self.iter().collect::<BTreeMap<_, _>>())
            .finish()
    }
}

/// Node IDs recorded by a snapshot, sharing structure between snapshots like
/// the snapshot's per-node revisions. Iteration order is unspecified.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct NodeIdSet(NodeMap<()>);

impl NodeIdSet {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.len() == 0
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    #[must_use]
    pub fn contains(&self, id: &NodeId) -> bool {
        self.0.contains_key(*id)
    }

    pub fn iter(&self) -> impl Iterator<Item = NodeId> + '_ {
        self.0.iter().map(|(id, ())| id)
    }

    /// Returns whether the ID was newly added. Re-adding a present ID leaves
    /// storage shared with earlier snapshots.
    pub(crate) fn insert(&mut self, id: NodeId) -> bool {
        !self.contains(&id) && self.0.insert(id, ()).is_none()
    }

    pub(crate) fn extend(&mut self, ids: impl IntoIterator<Item = NodeId>) {
        for id in ids {
            self.insert(id);
        }
    }

    pub(crate) fn clear(&mut self) {
        if !self.is_empty() {
            *self = Self::default();
        }
    }

    #[cfg(test)]
    pub(crate) fn unshared_leaves(&self, other: &Self) -> usize {
        self.0.unshared_leaves(&other.0)
    }
}

impl fmt::Debug for NodeIdSet {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_set()
            .entries(self.iter().collect::<BTreeSet<_>>())
            .finish()
    }
}
