//! Retained complete accessibility snapshot for developer inspection.
use accesskit::{NodeId, TreeUpdate};
use collections::{FxHashMap, FxHashSet};

#[derive(Default)]
pub(super) struct RetainedTreeSnapshot {
    update: Option<TreeUpdate>,
    indices: FxHashMap<NodeId, usize>,
    #[cfg(test)]
    pub(super) retained_nodes_scanned: usize,
}

impl RetainedTreeSnapshot {
    pub(super) fn update(&self) -> Option<&TreeUpdate> {
        self.update.as_ref()
    }

    pub(super) fn capture(&mut self, incoming: &TreeUpdate) {
        #[cfg(test)]
        {
            self.retained_nodes_scanned = 0;
        }
        if self.update.is_none() {
            self.update = Some(incoming.clone());
            self.rebuild_indices();
            return;
        }

        if is_complete(incoming) {
            let previous = self.update.as_mut().expect("checked above");
            if previous.nodes.len() == incoming.nodes.len()
                && previous
                    .nodes
                    .iter()
                    .zip(&incoming.nodes)
                    .all(|((saved, _), (current, _))| saved == current)
            {
                for (saved, current) in previous.nodes.iter_mut().zip(&incoming.nodes) {
                    if saved != current {
                        saved.1.clone_from(&current.1);
                    }
                }
                previous.tree.clone_from(&incoming.tree);
                previous.tree_id = incoming.tree_id;
                previous.focus = incoming.focus;
            } else {
                previous.clone_from(incoming);
                self.rebuild_indices();
            }
            return;
        }

        // TreeUpdate is an incremental protocol. Merge only changed nodes so a
        // retained document subtree stays complete in the inspector without
        // cloning or indexing every descendant on a scroll frame.
        let previous = self.update.as_mut().expect("checked above");
        let mut topology_changed = false;
        for (id, current) in &incoming.nodes {
            if let Some(index) = self.indices.get(id).copied() {
                let saved = &mut previous.nodes[index].1;
                topology_changed |= saved.children() != current.children();
                if saved != current {
                    saved.clone_from(current);
                }
            } else {
                topology_changed = true;
                self.indices.insert(*id, previous.nodes.len());
                previous.nodes.push((*id, current.clone()));
            }
        }
        if let Some(tree) = &incoming.tree {
            topology_changed |= previous.tree.as_ref().map(|old| old.root) != Some(tree.root);
            previous.tree = Some(tree.clone());
        }
        previous.tree_id = incoming.tree_id;
        previous.focus = incoming.focus;

        if topology_changed {
            self.retain_reachable_nodes();
        }
    }

    fn rebuild_indices(&mut self) {
        self.indices.clear();
        let Some(update) = &self.update else {
            return;
        };
        self.indices.extend(
            update
                .nodes
                .iter()
                .enumerate()
                .map(|(index, (id, _))| (*id, index)),
        );
        #[cfg(test)]
        {
            self.retained_nodes_scanned += update.nodes.len();
        }
    }

    fn retain_reachable_nodes(&mut self) {
        let Some(update) = &mut self.update else {
            return;
        };
        let Some(root) = update.tree.as_ref().map(|tree| tree.root) else {
            return;
        };
        let mut reachable = FxHashSet::default();
        let mut pending = vec![root];
        while let Some(id) = pending.pop() {
            if !reachable.insert(id) {
                continue;
            }
            if let Some(index) = self.indices.get(&id) {
                pending.extend(update.nodes[*index].1.children().iter().copied());
            }
        }
        update.nodes.retain(|(id, _)| reachable.contains(id));
        #[cfg(test)]
        {
            self.retained_nodes_scanned += self.indices.len();
        }
        self.rebuild_indices();
    }
}

fn is_complete(update: &TreeUpdate) -> bool {
    let Some(root) = update.tree.as_ref().map(|tree| tree.root) else {
        return false;
    };
    let nodes = update
        .nodes
        .iter()
        .map(|(id, node)| (*id, node))
        .collect::<FxHashMap<_, _>>();
    let mut reachable = FxHashSet::default();
    let mut pending = vec![root];
    while let Some(id) = pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let Some(node) = nodes.get(&id) else {
            return false;
        };
        pending.extend(node.children().iter().copied());
    }
    reachable.len() == nodes.len()
}
