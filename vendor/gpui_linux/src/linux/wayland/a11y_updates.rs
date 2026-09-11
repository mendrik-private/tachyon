//! Convert complete GPUI frames into standard AccessKit incremental updates.
//!
//! This cache belongs to one adapter. Call only inside `update_if_active` and
//! force a complete update following every activation, including reconnects.

use accesskit::{Node, NodeId, TreeUpdate};
use collections::{FxHashMap, FxHashSet};

#[derive(Default)]
pub(crate) struct TreePublication {
    published: FxHashMap<NodeId, Node>,
}

impl TreePublication {
    pub(crate) fn update(&mut self, mut update: TreeUpdate, force_full: bool) -> TreeUpdate {
        if force_full {
            self.published = update.nodes.iter().cloned().collect();
            return update;
        }

        // GPUI frames contain ordinary nodes plus deltas for retained
        // subtrees. Omitted retained nodes stay live; forgetting them here
        // would republish the whole subtree when it is next attached.
        // A child can move out of a subtree whose old parent is removed in the
        // same update. Keep every child referenced by the new parent data;
        // otherwise recursively evicting the old subtree would make that
        // unchanged child look newly inserted.
        let retained_children = update
            .nodes
            .iter()
            .flat_map(|(_, node)| node.children().iter().copied())
            .collect::<FxHashSet<_>>();
        let mut removed_roots = Vec::new();
        for (id, new_node) in &update.nodes {
            let Some(old_node) = self.published.get(id) else {
                continue;
            };
            if old_node.children() == new_node.children() {
                continue;
            }
            let new_children = new_node
                .children()
                .iter()
                .copied()
                .collect::<FxHashSet<_>>();
            removed_roots.extend(
                old_node
                    .children()
                    .iter()
                    .filter(|child| !new_children.contains(child))
                    .copied(),
            );
        }
        while let Some(id) = removed_roots.pop() {
            if retained_children.contains(&id) {
                continue;
            }
            if let Some(node) = self.published.remove(&id) {
                removed_roots.extend(node.children());
            }
        }

        let incoming = std::mem::take(&mut update.nodes);
        update.nodes = incoming
            .into_iter()
            .filter_map(|(id, node)| {
                if self.published.get(&id) == Some(&node) {
                    None
                } else {
                    self.published.insert(id, node.clone());
                    Some((id, node))
                }
            })
            .collect();
        update
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use accesskit::{Role, Tree, TreeId};

    fn update(nodes: Vec<(NodeId, Node)>) -> TreeUpdate {
        TreeUpdate {
            nodes,
            tree: Some(Tree::new(NodeId(0))),
            tree_id: TreeId::ROOT,
            focus: NodeId(0),
        }
    }

    #[test]
    fn forced_baseline_retains_the_incremental_comparison_tree() {
        let mut root = Node::new(Role::Window);
        root.set_children((1..3001).map(NodeId).collect::<Vec<_>>());
        let mut nodes = vec![(NodeId(0), root)];
        nodes.extend((1..3001).map(|id| (NodeId(id), Node::new(Role::Paragraph))));
        let mut publication = TreePublication::default();

        let baseline = publication.update(update(nodes), true);

        assert_eq!(baseline.nodes.len(), 3001);
        assert_eq!(publication.published.len(), 3001);
    }

    #[test]
    fn first_increment_after_forced_baseline_is_empty_when_unchanged() {
        let mut publication = TreePublication::default();
        let baseline = vec![(NodeId(0), Node::new(Role::Window))];
        assert_eq!(publication.update(update(baseline), true).nodes.len(), 1);
        let ordinary = vec![(NodeId(0), Node::new(Role::Window))];
        assert!(
            publication
                .update(update(ordinary.clone()), false)
                .nodes
                .is_empty()
        );
        assert_eq!(publication.published.len(), 1);
        assert!(
            publication
                .update(update(ordinary), false)
                .nodes
                .is_empty()
        );
    }

    #[test]
    fn incremental_omission_keeps_the_retained_baseline() {
        let mut root = Node::new(Role::Window);
        root.set_children(vec![NodeId(1)]);
        let child = Node::new(Role::Paragraph);
        let mut publication = TreePublication::default();
        publication.update(
            update(vec![(NodeId(0), root.clone()), (NodeId(1), child.clone())]),
            true,
        );
        assert!(publication
            .update(update(vec![(NodeId(0), root)]), false)
            .nodes
            .is_empty());
        assert!(publication
            .update(update(vec![(NodeId(1), child)]), false)
            .nodes
            .is_empty());
        assert_eq!(publication.published.len(), 2);
    }

    #[test]
    fn removed_subtree_is_republished_when_its_id_is_reused() {
        let mut attached_root = Node::new(Role::Window);
        attached_root.set_children(vec![NodeId(1)]);
        let detached_root = Node::new(Role::Window);
        let child = Node::new(Role::Paragraph);
        let mut publication = TreePublication::default();
        publication.update(
            update(vec![
                (NodeId(0), attached_root.clone()),
                (NodeId(1), child.clone()),
            ]),
            true,
        );
        assert_eq!(
            publication
                .update(update(vec![(NodeId(0), detached_root)]), false)
                .nodes
                .len(),
            1
        );
        let reattached = publication.update(
            update(vec![(NodeId(0), attached_root), (NodeId(1), child)]),
            false,
        );
        assert_eq!(reattached.nodes.len(), 2);
    }
}
