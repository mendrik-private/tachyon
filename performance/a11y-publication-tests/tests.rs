#![cfg(test)]

#[path = "../../vendor/gpui_linux/src/linux/wayland/a11y_updates.rs"]
mod a11y_updates;

#[path = "../../vendor/gpui/src/window/a11y/snapshot.rs"]
mod snapshot;

use a11y_updates::TreePublication;
use accesskit::{Affine, Node, NodeId, Role, Tree, TreeId, TreeUpdate};
use accesskit_consumer::{Node as ConsumerNode, TreeChangeHandler};

#[test]
fn debug_snapshot_scroll_retains_unchanged_storage_and_complete_semantics() {
    let initial = frame(0., "Original paragraph", &[2, 3], 2);
    let mut saved = snapshot::RetainedTreeSnapshot::default();
    saved.capture(&initial);
    let original_label = saved.update().unwrap().nodes[2]
        .1
        .label()
        .unwrap()
        .as_ptr();
    let next = frame(120., "Original paragraph", &[2, 3], 3);
    saved.capture(&next);
    let captured = saved.update().unwrap();
    assert_eq!(captured.nodes, next.nodes);
    assert_eq!(captured.focus, next.focus);
    assert_eq!(
        captured.nodes[2].1.label().unwrap().as_ptr(),
        original_label,
        "scrolling must not allocate another copy of unchanged document text"
    );

    // Reordering, editing, inserting and removing nodes still produce the exact
    // complete snapshot consumed by the inspector, never a platform delta.
    for next in [
        frame(200., "Edited paragraph", &[3, 2], 3),
        frame(200., "Inserted paragraph", &[3, 2, 4], 4),
        frame(0., "Remaining paragraph", &[4], 4),
    ] {
        saved.capture(&next);
        let captured = saved.update().unwrap();
        assert_eq!(captured.nodes, next.nodes);
        assert_eq!(captured.tree, next.tree);
        assert_eq!(captured.tree_id, next.tree_id);
        assert_eq!(captured.focus, next.focus);
        let consumer = accesskit_consumer::Tree::new(captured.clone(), true);
        let reference = accesskit_consumer::Tree::new(next, true);
        assert_eq!(consumer.state().focus_id(), reference.state().focus_id());
    }
}

#[test]
fn debug_snapshot_merges_retained_subtree_deltas() {
    let initial = frame(0., "Retained paragraph", &[2, 3], 2);
    let mut saved = snapshot::RetainedTreeSnapshot::default();
    saved.capture(&initial);
    let original_label = saved.update().unwrap().nodes[2]
        .1
        .label()
        .unwrap()
        .as_ptr();

    let mut document = initial.nodes[1].1.clone();
    document.set_transform(Affine::translate((0., -120.)));
    let delta = TreeUpdate {
        nodes: vec![(NodeId(1), document)],
        tree: None,
        tree_id: TreeId::ROOT,
        focus: NodeId(3),
    };
    saved.capture(&delta);
    let captured = saved.update().unwrap();
    assert_eq!(captured.nodes.len(), initial.nodes.len());
    assert_eq!(captured.focus, NodeId(3));
    assert_eq!(
        saved.retained_nodes_scanned, 0,
        "a transform delta must not rebuild or traverse the retained index"
    );
    assert_eq!(
        captured.nodes[2].1.label().unwrap().as_ptr(),
        original_label,
        "a transform delta must retain descendant property storage"
    );
    let consumer = accesskit_consumer::Tree::new(captured.clone(), true);
    assert!(
        consumer
            .state()
            .node_by_tree_local_id(NodeId(3), TreeId::ROOT)
            .is_some()
    );
}

struct IgnoreChanges;
impl TreeChangeHandler for IgnoreChanges {
    fn node_added(&mut self, _: &ConsumerNode) {}
    fn node_updated(&mut self, _: &ConsumerNode, _: &ConsumerNode) {}
    fn focus_moved(&mut self, _: Option<&ConsumerNode>, _: Option<&ConsumerNode>) {}
    fn node_removed(&mut self, _: &ConsumerNode) {}
}

fn frame(scroll: f64, text: &str, children: &[u64], focus: u64) -> TreeUpdate {
    let mut root = Node::new(Role::Window);
    root.set_children(vec![NodeId(1)]);
    let mut document = Node::new(Role::Document);
    document.set_children(children.iter().map(|id| NodeId(*id)).collect::<Vec<_>>());
    document.set_transform(Affine::translate((0., -scroll)));
    let mut nodes = vec![(NodeId(0), root), (NodeId(1), document)];
    for id in children {
        let mut paragraph = Node::new(Role::Paragraph);
        paragraph.set_label(format!("{text} {id}"));
        paragraph.add_action(accesskit::Action::ScrollIntoView);
        nodes.push((NodeId(*id), paragraph));
    }
    TreeUpdate {
        nodes,
        tree: Some(Tree::new(NodeId(0))),
        tree_id: TreeId::ROOT,
        focus: NodeId(focus),
    }
}

#[test]
fn scroll_updates_only_transform_and_preserves_every_descendant() {
    let mut publisher = TreePublication::default();
    let children = (2..3002).collect::<Vec<_>>();
    let first = publisher.update(frame(0., "unchanged", &children, 1), false);
    assert_eq!(first.nodes.len(), 3002);
    let mut consumer = accesskit_consumer::Tree::new(first, true);
    for scroll in [10., 30., 0.] {
        let delta = publisher.update(frame(scroll, "unchanged", &children, 1), false);
        assert_eq!(
            delta.nodes.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
            [1]
        );
        consumer.update_and_process_changes(delta, &mut IgnoreChanges);
        for id in &children {
            let node = consumer
                .state()
                .node_by_tree_local_id(NodeId(*id), TreeId::ROOT)
                .unwrap();
            assert_eq!(
                node.label().as_deref(),
                Some(format!("unchanged {id}").as_str())
            );
            assert!(
                node.data()
                    .supports_action(accesskit::Action::ScrollIntoView)
            );
        }
    }
}

#[test]
fn edits_removal_reorder_focus_and_id_reuse_match_complete_tree() {
    let mut publisher = TreePublication::default();
    let initial = frame(0., "before", &[2, 3], 2);
    let mut reference = accesskit_consumer::Tree::new(initial.clone(), true);
    let mut consumer = accesskit_consumer::Tree::new(publisher.update(initial, false), true);
    for (text, ids, focus) in [
        ("é 日本語", vec![3, 2, 4], 4),
        ("é 日本語", vec![4], 4),
        ("restored", vec![2, 3], 2),
        ("restored", vec![2, 3], 3),
    ] {
        let complete = frame(20., text, &ids, focus);
        reference.update_and_process_changes(complete.clone(), &mut IgnoreChanges);
        consumer.update_and_process_changes(publisher.update(complete, false), &mut IgnoreChanges);
        for id in 0..5 {
            assert_eq!(
                consumer
                    .state()
                    .node_by_tree_local_id(NodeId(id), TreeId::ROOT)
                    .map(|n| n.data().clone()),
                reference
                    .state()
                    .node_by_tree_local_id(NodeId(id), TreeId::ROOT)
                    .map(|n| n.data().clone())
            );
        }
        assert_eq!(consumer.state().focus_id(), reference.state().focus_id());
    }
}

#[test]
fn reactivation_always_provides_a_complete_baseline() {
    let mut publisher = TreePublication::default();
    let complete = frame(30., "same", &[2, 3], 3);
    publisher.update(complete.clone(), false);
    assert!(publisher.update(complete.clone(), false).nodes.is_empty());
    let restarted = publisher.update(complete.clone(), true);
    assert_eq!(restarted.nodes, complete.nodes);
    assert_eq!(restarted.tree_id, complete.tree_id);
    assert_eq!(restarted.focus, complete.focus);
    let consumer = accesskit_consumer::Tree::new(restarted, true);
    assert!(
        consumer
            .state()
            .node_by_tree_local_id(NodeId(3), TreeId::ROOT)
            .is_some()
    );
}

#[test]
fn moving_an_unchanged_subtree_out_of_a_removed_parent_preserves_it() {
    let mut publisher = TreePublication::default();
    let mut initial = frame(0., "same", &[2, 3, 4], 4);
    initial.nodes[1].1.set_children(vec![NodeId(2), NodeId(3)]);
    initial.nodes[2].1.set_children(vec![NodeId(4)]);
    let mut consumer = accesskit_consumer::Tree::new(publisher.update(initial, false), true);
    let mut moved = frame(0., "same", &[3, 4], 4);
    moved.nodes[1].1.set_children(vec![NodeId(3)]);
    moved.nodes[2].1.set_children(vec![NodeId(4)]);
    let reference = accesskit_consumer::Tree::new(moved.clone(), true);
    let delta = publisher.update(moved, false);
    assert_eq!(
        delta.nodes.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
        [1, 3]
    );
    consumer.update_and_process_changes(delta, &mut IgnoreChanges);
    for id in 0..5 {
        let lookup = |tree: &accesskit_consumer::Tree| {
            tree.state()
                .node_by_tree_local_id(NodeId(id), TreeId::ROOT)
                .map(|n| (n.data().clone(), n.parent_id()))
        };
        assert_eq!(lookup(&consumer), lookup(&reference));
    }
}
