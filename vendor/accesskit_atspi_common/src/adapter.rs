// Copyright 2022 The AccessKit Authors. All rights reserved.
// Licensed under the Apache License, Version 2.0 (found in
// the LICENSE-APACHE file) or the MIT license (found in
// the LICENSE-MIT file), at your option.

// Derived from Chromium's accessibility abstraction.
// Copyright 2017 The Chromium Authors. All rights reserved.
// Use of this source code is governed by a BSD-style license that can be
// found in the LICENSE.chromium file.

use crate::{
    context::{ActionHandlerNoMut, ActionHandlerWrapper, AppContext, Context},
    filters::filter,
    node::{NodeIdOrRoot, NodeWrapper, PlatformNode, PlatformRoot},
    util::WindowBounds,
    AdapterCallback, Event, ObjectEvent, WindowEvent,
};
use accesskit::{ActionHandler, Role, TreeUpdate};
use accesskit_consumer::{FilterResult, Node, NodeId, Tree, TreeChangeHandler, TreeState};
use atspi_common::{InterfaceSet, Politeness, State};
use std::fmt::{Debug, Formatter};
use std::{
    collections::HashSet,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc, RwLock,
    },
};

struct AdapterChangeHandler<'a> {
    adapter: &'a Adapter,
    coalesce_document_geometry: bool,
    geometry_documents: HashSet<NodeId>,
    added_nodes: HashSet<NodeId>,
    removed_nodes: HashSet<NodeId>,
    checked_text_change: HashSet<NodeId>,
    selection_changed: HashSet<NodeId>,
    text_documents_compared: usize,
}

impl<'a> AdapterChangeHandler<'a> {
    const MAX_TEXT_EVENT_REPLACEMENT_BYTES: usize = 64 * 1024;
    const RETAINED_DOCUMENT_TEXT_MARKER: &'static str = "Tachyon retained document text";

    fn first_text_run<'tree>(node: Node<'tree>) -> Option<Node<'tree>> {
        if node.role() == Role::TextRun {
            return Some(node);
        }
        for child in node.children() {
            if let Some(run) = Self::first_text_run(child) {
                return Some(run);
            }
        }
        None
    }

    fn is_geometry_only_update(old_node: &Node, new_node: &Node) -> bool {
        if old_node.role() != new_node.role() || old_node.data() == new_node.data() {
            return false;
        }

        let mut old_data = old_node.data().clone();
        let mut new_data = new_node.data().clone();
        old_data.clear_bounds();
        old_data.clear_transform();
        new_data.clear_bounds();
        new_data.clear_transform();
        old_data == new_data
    }

    fn document_ancestor(mut node: Node<'_>) -> Option<NodeId> {
        loop {
            if node.role() == Role::Document {
                return Some(node.id());
            }
            node = node.parent()?;
        }
    }

    fn emit_text_difference(&self, id: NodeId, old_text: &str, new_text: &str) {
        let mut prefix_byte_count = old_text
            .as_bytes()
            .iter()
            .zip(new_text.as_bytes())
            .take_while(|(old, new)| old == new)
            .count();
        while prefix_byte_count > 0
            && (!old_text.is_char_boundary(prefix_byte_count)
                || !new_text.is_char_boundary(prefix_byte_count))
        {
            prefix_byte_count -= 1;
        }
        if prefix_byte_count == old_text.len() && prefix_byte_count == new_text.len() {
            return;
        }

        let max_suffix = (old_text.len() - prefix_byte_count)
            .min(new_text.len() - prefix_byte_count);
        let mut suffix_byte_count = old_text
            .as_bytes()
            .iter()
            .rev()
            .zip(new_text.as_bytes().iter().rev())
            .take(max_suffix)
            .take_while(|(old, new)| old == new)
            .count();
        while suffix_byte_count > 0
            && (!old_text.is_char_boundary(old_text.len() - suffix_byte_count)
                || !new_text.is_char_boundary(new_text.len() - suffix_byte_count))
        {
            suffix_byte_count -= 1;
        }
        let Ok(prefix_usv_count) = new_text[..prefix_byte_count].chars().count().try_into() else {
            return;
        };

        let old_content = &old_text[prefix_byte_count..old_text.len() - suffix_byte_count];
        if let Ok(length) = old_content.chars().count().try_into()
            && length > 0
        {
            self.adapter.emit_object_event(
                id,
                ObjectEvent::TextRemoved {
                    start_index: prefix_usv_count,
                    length,
                    content: old_content.to_string(),
                },
            );
        }

        let new_content = &new_text[prefix_byte_count..new_text.len() - suffix_byte_count];
        if let Ok(length) = new_content.chars().count().try_into()
            && length > 0
        {
            self.adapter.emit_object_event(
                id,
                ObjectEvent::TextInserted {
                    start_index: prefix_usv_count,
                    length,
                    content: new_content.to_string(),
                },
            );
        }
    }

    fn new(adapter: &'a Adapter, coalesce_document_geometry: bool) -> Self {
        Self {
            adapter,
            coalesce_document_geometry,
            geometry_documents: HashSet::new(),
            added_nodes: HashSet::new(),
            removed_nodes: HashSet::new(),
            checked_text_change: HashSet::new(),
            selection_changed: HashSet::new(),
            text_documents_compared: 0,
        }
    }

    fn emit_coalesced_geometry_changes(&self) {
        if self.geometry_documents.is_empty() {
            return;
        }
        let bounds = *self.adapter.context.read_root_window_bounds();
        let tree = self.adapter.context.read_tree();
        for id in &self.geometry_documents {
            if let Some(node) = tree.state().node_by_id(*id) {
                NodeWrapper(&node).notify_ancestor_bounds_change(&bounds, self.adapter);
            }
        }
    }

    fn add_node(&mut self, node: &Node) {
        let id = node.id();
        if self.added_nodes.contains(&id) {
            return;
        }
        self.added_nodes.insert(id);

        let role = node.role();
        let is_root = node.is_root();
        let wrapper = NodeWrapper(node);
        let interfaces = wrapper.interfaces();
        self.adapter.register_interfaces(node.id(), interfaces);
        if is_root && role == Role::Window {
            let adapter_index = self
                .adapter
                .context
                .read_app_context()
                .adapter_index(self.adapter.id)
                .unwrap();
            self.adapter.window_created(adapter_index, node.id());
        }

        let live = wrapper.live();
        if live != Politeness::None {
            if let Some(name) = wrapper.name() {
                self.adapter
                    .emit_object_event(node.id(), ObjectEvent::Announcement(name, live));
            }
        }
        if let Some(true) = node.is_selected() {
            self.enqueue_selection_changed_if_needed(node);
        }
    }

    fn add_subtree(&mut self, node: &Node) {
        self.add_node(node);
        for child in node.filtered_children(&filter) {
            self.add_subtree(&child);
        }
    }

    fn remove_node(&mut self, node: &Node) {
        let id = node.id();
        if self.removed_nodes.contains(&id) {
            return;
        }
        self.removed_nodes.insert(id);

        let role = node.role();
        let is_root = node.is_root();
        let wrapper = NodeWrapper(node);
        if is_root && role == Role::Window {
            self.adapter.window_destroyed(node.id());
        }
        self.adapter
            .emit_object_event(node.id(), ObjectEvent::StateChanged(State::Defunct, true));
        self.adapter
            .unregister_interfaces(node.id(), wrapper.interfaces());
        if let Some(true) = node.is_selected() {
            self.enqueue_selection_changed_if_needed(node);
        }
    }

    fn remove_subtree(&mut self, node: &Node) {
        for child in node.filtered_children(&filter) {
            self.remove_subtree(&child);
        }
        self.remove_node(node);
    }

    fn emit_text_change_if_needed_parent(&mut self, old_node: &Node, new_node: &Node) {
        if !NodeWrapper(new_node).supports_text() || !NodeWrapper(old_node).supports_text() {
            return;
        }
        let id = new_node.id();
        if self.checked_text_change.contains(&id) {
            return;
        }
        self.checked_text_change.insert(id);
        if let (Some(old_run), Some(new_run)) = (
            Self::first_text_run(*old_node),
            Self::first_text_run(*new_node),
        ) {
            let old_text = old_run.data().value().unwrap_or_default();
            let new_text = new_run.data().value().unwrap_or_default();
            if old_run.data().label() == Some(Self::RETAINED_DOCUMENT_TEXT_MARKER)
                && new_run.data().label() == Some(Self::RETAINED_DOCUMENT_TEXT_MARKER)
            {
                if old_text.len().abs_diff(new_text.len())
                    <= Self::MAX_TEXT_EVENT_REPLACEMENT_BYTES
                {
                    self.emit_text_difference(id, old_text, new_text);
                }
                return;
            }
            if old_text.len().abs_diff(new_text.len()) > Self::MAX_TEXT_EVENT_REPLACEMENT_BYTES {
                return;
            }
        }
        self.text_documents_compared += 1;
        let old_text = old_node.document_range().text();
        let new_text = new_node.document_range().text();

        self.emit_text_difference(id, &old_text, &new_text);
    }

    fn emit_text_change_if_needed(&mut self, old_node: &Node, new_node: &Node) {
        if let Role::TextRun | Role::GenericContainer = new_node.role() {
            if let (Some(old_parent), Some(new_parent)) = (
                old_node.filtered_parent(&filter),
                new_node.filtered_parent(&filter),
            ) {
                self.emit_text_change_if_needed_parent(&old_parent, &new_parent);
            }
        } else {
            self.emit_text_change_if_needed_parent(old_node, new_node);
        }
    }

    fn emit_text_selection_change(&self, old_node: Option<&Node>, new_node: &Node) {
        if !NodeWrapper(new_node).supports_text() {
            return;
        }
        let Some(old_node) = old_node else {
            if let Some(selection) = new_node.text_selection() {
                if !selection.is_degenerate() {
                    self.adapter
                        .emit_object_event(new_node.id(), ObjectEvent::TextSelectionChanged);
                }
            }
            if let Some(selection_focus) = new_node.text_selection_focus() {
                if let Ok(offset) = selection_focus.to_global_usv_index().try_into() {
                    self.adapter
                        .emit_object_event(new_node.id(), ObjectEvent::CaretMoved(offset));
                }
            }
            return;
        };
        if !old_node.is_focused() || new_node.raw_text_selection() == old_node.raw_text_selection()
        {
            return;
        }

        if let Some(selection) = new_node.text_selection() {
            if !selection.is_degenerate()
                || old_node
                    .text_selection()
                    .map(|selection| !selection.is_degenerate())
                    .unwrap_or(false)
            {
                self.adapter
                    .emit_object_event(new_node.id(), ObjectEvent::TextSelectionChanged);
            }
        }

        let old_caret_position = old_node
            .raw_text_selection()
            .map(|selection| selection.focus);
        let new_caret_position = new_node
            .raw_text_selection()
            .map(|selection| selection.focus);
        if old_caret_position != new_caret_position {
            if let Some(selection_focus) = new_node.text_selection_focus() {
                if let Ok(offset) = selection_focus.to_global_usv_index().try_into() {
                    self.adapter
                        .emit_object_event(new_node.id(), ObjectEvent::CaretMoved(offset));
                }
            }
        }
    }

    fn enqueue_selection_changed_if_needed_parent(&mut self, node: Node) {
        if !node.is_container_with_selectable_children() {
            return;
        }
        let id = node.id();
        if self.selection_changed.contains(&id) {
            return;
        }
        self.selection_changed.insert(id);
    }

    fn enqueue_selection_changed_if_needed(&mut self, node: &Node) {
        if !node.is_item_like() {
            return;
        }
        if let Some(node) = node.selection_container(&filter) {
            self.enqueue_selection_changed_if_needed_parent(node);
        }
    }

    fn emit_selection_changed(&mut self) {
        for id in self.selection_changed.iter() {
            if self.removed_nodes.contains(id) {
                continue;
            }
            self.adapter
                .emit_object_event(*id, ObjectEvent::SelectionChanged);
        }
    }
}

impl TreeChangeHandler for AdapterChangeHandler<'_> {
    fn node_added(&mut self, node: &Node) {
        if filter(node) == FilterResult::Include {
            self.add_node(node);
        }
    }

    fn node_updated(&mut self, old_node: &Node, new_node: &Node) {
        let geometry_only = Self::is_geometry_only_update(old_node, new_node);
        if self.coalesce_document_geometry
            && geometry_only
            && let Some(document) = Self::document_ancestor(*new_node)
        {
            self.geometry_documents.insert(document);
            return;
        }
        let transparent_geometry_only = geometry_only
            && old_node.role() == Role::GenericContainer
            && new_node.role() == Role::GenericContainer;
        if !transparent_geometry_only {
            self.emit_text_change_if_needed(old_node, new_node);
        }
        let filter_old = filter(old_node);
        let filter_new = filter(new_node);
        if filter_new != filter_old {
            if filter_new == FilterResult::Include {
                if filter_old == FilterResult::ExcludeSubtree {
                    self.add_subtree(new_node);
                } else {
                    self.add_node(new_node);
                }
            } else if filter_old == FilterResult::Include {
                if filter_new == FilterResult::ExcludeSubtree {
                    self.remove_subtree(old_node);
                } else {
                    self.remove_node(old_node);
                }
            }
        } else if filter_new == FilterResult::Include {
            let old_wrapper = NodeWrapper(old_node);
            let new_wrapper = NodeWrapper(new_node);
            let old_interfaces = old_wrapper.interfaces();
            let new_interfaces = new_wrapper.interfaces();
            let kept_interfaces = old_interfaces & new_interfaces;
            self.adapter
                .unregister_interfaces(new_wrapper.id(), old_interfaces ^ kept_interfaces);
            self.adapter
                .register_interfaces(new_node.id(), new_interfaces ^ kept_interfaces);
            let bounds = *self.adapter.context.read_root_window_bounds();
            new_wrapper.notify_changes(&bounds, self.adapter, &old_wrapper);
            self.emit_text_selection_change(Some(old_node), new_node);
            if new_node.is_selected() != old_node.is_selected() {
                self.enqueue_selection_changed_if_needed(new_node);
            }
        } else if filter_new == FilterResult::ExcludeNode
            && new_node.role() == Role::GenericContainer
            && transparent_geometry_only
        {
            // A transparent geometry owner can move an immutable semantic
            // subtree without becoming its text-range owner. Notify the first
            // exposed descendants so clients invalidate cached coordinates;
            // do not rebuild an unrelated ancestor's document text.
            let bounds = *self.adapter.context.read_root_window_bounds();
            for child in new_node.filtered_children(&filter) {
                NodeWrapper(&child).notify_ancestor_bounds_change(&bounds, self.adapter);
            }
        }
    }

    fn focus_moved(&mut self, old_node: Option<&Node>, new_node: Option<&Node>) {
        if let (None, Some(new_node)) = (old_node, new_node) {
            if let Some(root_window) = root_window(new_node.tree_state) {
                self.adapter.window_activated(&NodeWrapper(&root_window));
            }
        } else if let (Some(old_node), None) = (old_node, new_node) {
            if let Some(root_window) = root_window(old_node.tree_state) {
                self.adapter.window_deactivated(&NodeWrapper(&root_window));
            }
        }
        if let Some(node) = new_node {
            self.adapter
                .emit_object_event(node.id(), ObjectEvent::StateChanged(State::Focused, true));
            self.emit_text_selection_change(None, node);
        }
        if let Some(node) = old_node {
            self.adapter
                .emit_object_event(node.id(), ObjectEvent::StateChanged(State::Focused, false));
        }
    }

    fn node_removed(&mut self, node: &Node) {
        if filter(node) == FilterResult::Include {
            self.remove_node(node);
        }
    }
}

static NEXT_ADAPTER_ID: AtomicUsize = AtomicUsize::new(0);

/// If you use this function, you must ensure that only one adapter at a time
/// has a given ID.
pub fn next_adapter_id() -> usize {
    NEXT_ADAPTER_ID.fetch_add(1, Ordering::Relaxed)
}

pub struct Adapter {
    id: usize,
    callback: Box<dyn AdapterCallback + Send + Sync>,
    context: Arc<Context>,
}

impl Debug for Adapter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Adapter")
            .field("id", &self.id)
            .field("callback", &"AdapterCallback")
            .field("context", &self.context)
            .finish()
    }
}

impl Adapter {
    pub fn new(
        app_context: &Arc<RwLock<AppContext>>,
        callback: impl 'static + AdapterCallback + Send + Sync,
        initial_state: TreeUpdate,
        is_window_focused: bool,
        root_window_bounds: WindowBounds,
        action_handler: impl 'static + ActionHandler + Send,
    ) -> Self {
        let id = next_adapter_id();
        Self::with_id(
            id,
            app_context,
            callback,
            initial_state,
            is_window_focused,
            root_window_bounds,
            action_handler,
        )
    }

    pub fn with_id(
        id: usize,
        app_context: &Arc<RwLock<AppContext>>,
        callback: impl 'static + AdapterCallback + Send + Sync,
        initial_state: TreeUpdate,
        is_window_focused: bool,
        root_window_bounds: WindowBounds,
        action_handler: impl 'static + ActionHandler + Send,
    ) -> Self {
        Self::with_wrapped_action_handler(
            id,
            app_context,
            callback,
            initial_state,
            is_window_focused,
            root_window_bounds,
            Arc::new(ActionHandlerWrapper::new(action_handler)),
        )
    }

    /// This is an implementation detail of `accesskit_unix`, required for
    /// robust state transitions with minimal overhead.
    pub fn with_wrapped_action_handler(
        id: usize,
        app_context: &Arc<RwLock<AppContext>>,
        callback: impl 'static + AdapterCallback + Send + Sync,
        initial_state: TreeUpdate,
        is_window_focused: bool,
        root_window_bounds: WindowBounds,
        action_handler: Arc<dyn ActionHandlerNoMut + Send + Sync>,
    ) -> Self {
        let tree = Tree::new(initial_state, is_window_focused);
        let focus_id = tree.state().focus().map(|node| node.id());
        let context = Context::new(app_context, tree, action_handler, root_window_bounds);
        context.write_app_context().push_adapter(id, &context);
        let adapter = Self {
            id,
            callback: Box::new(callback),
            context,
        };
        adapter.register_tree();
        if let Some(id) = focus_id {
            adapter.emit_object_event(id, ObjectEvent::StateChanged(State::Focused, true));
        }
        adapter
    }

    fn register_tree(&self) {
        fn add_children(node: Node<'_>, to_add: &mut Vec<(NodeId, InterfaceSet)>) {
            for child in node.filtered_children(&filter) {
                let child_id = child.id();
                let wrapper = NodeWrapper(&child);
                let interfaces = wrapper.interfaces();
                to_add.push((child_id, interfaces));
                add_children(child, to_add);
            }
        }

        let mut objects_to_add = Vec::new();

        let (adapter_index, root_id) = {
            let tree = self.context.read_tree();
            let tree_state = tree.state();
            let mut app_context = self.context.write_app_context();
            app_context.toolkit_name = tree_state.toolkit_name().map(|s| s.to_string());
            app_context.toolkit_version = tree_state.toolkit_version().map(|s| s.to_string());
            let adapter_index = app_context.adapter_index(self.id).unwrap();
            let root = tree_state.root();
            let root_id = root.id();
            let wrapper = NodeWrapper(&root);
            objects_to_add.push((root_id, wrapper.interfaces()));
            add_children(root, &mut objects_to_add);
            (adapter_index, root_id)
        };

        for (id, interfaces) in objects_to_add {
            self.register_interfaces(id, interfaces);
            if id == root_id {
                self.window_created(adapter_index, id);
            }
        }
    }

    pub fn platform_node(&self, id: NodeId) -> PlatformNode {
        PlatformNode::new(&self.context, self.id, id)
    }

    pub fn root_id(&self) -> NodeId {
        self.context.read_tree().state().root_id()
    }

    pub fn platform_root(&self) -> PlatformRoot {
        PlatformRoot::new(&self.context.app_context)
    }

    fn register_interfaces(&self, id: NodeId, new_interfaces: InterfaceSet) {
        self.callback.register_interfaces(self, id, new_interfaces);
    }

    fn unregister_interfaces(&self, id: NodeId, old_interfaces: InterfaceSet) {
        self.callback
            .unregister_interfaces(self, id, old_interfaces);
    }

    pub(crate) fn emit_object_event(&self, target: NodeId, event: ObjectEvent) {
        let target = NodeIdOrRoot::Node(target);
        self.callback
            .emit_event(self, Event::Object { target, event });
    }

    fn emit_root_object_event(&self, event: ObjectEvent) {
        let target = NodeIdOrRoot::Root;
        self.callback
            .emit_event(self, Event::Object { target, event });
    }

    pub fn set_root_window_bounds(&mut self, new_bounds: WindowBounds) {
        let mut bounds = self.context.root_window_bounds.write().unwrap();
        *bounds = new_bounds;
    }

    pub fn update(&mut self, update: TreeUpdate) {
        // Large measured reflows can adjust the bounds of thousands of nodes
        // in one document. Keep the consumer tree exact, but coalesce their
        // redundant notifications into one document-level invalidation.
        let coalesce_document_geometry = update.nodes.len() >= 256;
        let mut handler =
            AdapterChangeHandler::new(self, coalesce_document_geometry);
        let mut tree = self.context.tree.write().unwrap();
        tree.update_and_process_changes(update, &mut handler);
        drop(tree);
        handler.emit_selection_changed();
        handler.emit_coalesced_geometry_changes();
    }

    #[cfg(test)]
    fn update_and_count_text_documents_compared(&mut self, update: TreeUpdate) -> usize {
        let mut handler = AdapterChangeHandler::new(self, false);
        let mut tree = self.context.tree.write().unwrap();
        tree.update_and_process_changes(update, &mut handler);
        drop(tree);
        handler.emit_selection_changed();
        handler.text_documents_compared
    }

    pub fn update_window_focus_state(&mut self, is_focused: bool) {
        let mut handler = AdapterChangeHandler::new(self, false);
        let mut tree = self.context.tree.write().unwrap();
        tree.update_host_focus_state_and_process_changes(is_focused, &mut handler);
    }

    fn window_created(&self, adapter_index: usize, window: NodeId) {
        self.emit_root_object_event(ObjectEvent::ChildAdded(adapter_index, window));
    }

    fn window_activated(&self, window: &NodeWrapper<'_>) {
        self.callback.emit_event(
            self,
            Event::Window {
                target: window.id(),
                name: window.name().unwrap_or_default(),
                event: WindowEvent::Activated,
            },
        );
        self.emit_object_event(window.id(), ObjectEvent::StateChanged(State::Active, true));
        self.emit_root_object_event(ObjectEvent::ActiveDescendantChanged(window.id()));
    }

    fn window_deactivated(&self, window: &NodeWrapper<'_>) {
        self.callback.emit_event(
            self,
            Event::Window {
                target: window.id(),
                name: window.name().unwrap_or_default(),
                event: WindowEvent::Deactivated,
            },
        );
        self.emit_object_event(window.id(), ObjectEvent::StateChanged(State::Active, false));
    }

    fn window_destroyed(&self, window: NodeId) {
        self.emit_root_object_event(ObjectEvent::ChildRemoved(window));
    }

    pub fn id(&self) -> usize {
        self.id
    }

    pub fn is_window_focused(&self) -> bool {
        self.context.read_tree().state().is_host_focused()
    }

    pub fn root_window_bounds(&self) -> WindowBounds {
        *self.context.read_root_window_bounds()
    }

    /// This is an implementation detail of `accesskit_unix`, required for
    /// robust state transitions with minimal overhead.
    pub fn wrapped_action_handler(&self) -> Arc<dyn ActionHandlerNoMut + Send + Sync> {
        Arc::clone(&self.context.action_handler)
    }
}

fn root_window(current_state: &TreeState) -> Option<Node<'_>> {
    const WINDOW_ROLES: &[Role] = &[Role::AlertDialog, Role::Dialog, Role::Window];
    let root = current_state.root();
    if WINDOW_ROLES.contains(&root.role()) {
        Some(root)
    } else {
        None
    }
}

impl Drop for Adapter {
    fn drop(&mut self) {
        let root_id = self.context.read_tree().state().root_id();
        self.window_destroyed(root_id);
        // Note: We deliberately do the following here, not in a Drop
        // implementation on context, because AppContext owns a second
        // strong reference to Context, and we need that to be released.
        self.context.write_app_context().remove_adapter(self.id);
    }
}

#[cfg(test)]
mod tachyon_text_update_tests {
    use super::*;
    use accesskit::{ActionRequest, Affine, Node as NodeData, NodeId as LocalId, Rect, TreeId};
    use std::sync::Mutex;

    #[derive(Clone, Default)]
    struct Events(Arc<Mutex<Vec<Event>>>);

    impl AdapterCallback for Events {
        fn register_interfaces(&self, _: &Adapter, _: NodeId, _: InterfaceSet) {}
        fn unregister_interfaces(&self, _: &Adapter, _: NodeId, _: InterfaceSet) {}
        fn emit_event(&self, _: &Adapter, event: Event) {
            self.0.lock().unwrap().push(event);
        }
    }

    struct Actions;
    impl ActionHandler for Actions {
        fn do_action(&mut self, _: ActionRequest) {}
    }

    fn document(text: &str) -> TreeUpdate {
        let mut document = NodeData::new(Role::Document);
        document.set_children(vec![LocalId(2)]);
        let mut run = NodeData::new(Role::TextRun);
        run.set_value(text);
        run.set_character_lengths(text.chars().map(|c| c.len_utf8() as u8).collect::<Vec<_>>());
        TreeUpdate {
            nodes: vec![(LocalId(1), document), (LocalId(2), run)],
            tree: Some(accesskit::Tree::new(LocalId(1))),
            tree_id: TreeId::ROOT,
            focus: LocalId(1),
        }
    }

    fn adapter(update: TreeUpdate, events: Events) -> Adapter {
        Adapter::new(
            &AppContext::new(None),
            events,
            update,
            false,
            WindowBounds::default(),
            Actions,
        )
    }

    fn retained_text_editor(text: &str, extra_child: bool) -> TreeUpdate {
        let mut window = NodeData::new(Role::Window);
        window.set_children(vec![LocalId(2)]);
        let mut editor = NodeData::new(Role::MultilineTextInput);
        editor.set_children(if extra_child {
            vec![LocalId(3), LocalId(6)]
        } else {
            vec![LocalId(3)]
        });
        let mut geometry = NodeData::new(Role::GenericContainer);
        geometry.set_children(vec![LocalId(4)]);
        let mut document = NodeData::new(Role::Document);
        document.set_children(vec![LocalId(5)]);
        let mut run = NodeData::new(Role::TextRun);
        run.set_label(AdapterChangeHandler::RETAINED_DOCUMENT_TEXT_MARKER);
        run.set_value(text);
        run.set_character_lengths(text.chars().map(|c| c.len_utf8() as u8).collect::<Vec<_>>());
        let mut nodes = vec![
            (LocalId(1), window),
            (LocalId(2), editor),
            (LocalId(3), geometry),
            (LocalId(4), document),
            (LocalId(5), run),
        ];
        if extra_child {
            nodes.push((LocalId(6), NodeData::new(Role::Group)));
        }
        TreeUpdate {
            nodes,
            tree: Some(accesskit::Tree::new(LocalId(1))),
            tree_id: TreeId::ROOT,
            focus: LocalId(2),
        }
    }

    #[test]
    fn retained_text_makes_large_child_only_editor_updates_bounded() {
        let text = "x".repeat(128 * 1024);
        let events = Events::default();
        let mut adapter = adapter(retained_text_editor(&text, false), events);

        assert_eq!(
            adapter.update_and_count_text_documents_compared(retained_text_editor(&text, true)),
            0
        );
    }

    #[test]
    fn retained_text_edit_uses_direct_run_and_emits_exact_unicode_delta() {
        let prefix = "x".repeat(512 * 1024);
        let suffix = "y".repeat(512 * 1024);
        let old = format!("{prefix}é文{suffix}");
        let new = format!("{prefix}猫文{suffix}");
        let events = Events::default();
        let mut adapter = adapter(retained_text_editor(&old, false), events.clone());
        events.0.lock().unwrap().clear();

        assert_eq!(
            adapter.update_and_count_text_documents_compared(retained_text_editor(&new, false)),
            0,
            "the marked canonical run must bypass document-range concatenation"
        );
        let events = events.0.lock().unwrap();
        assert!(events.iter().any(|event| matches!(event, Event::Object {
            event: ObjectEvent::TextRemoved { start_index, length: 1, content }, ..
        } if *start_index == prefix.len() as i32 && content == "é")));
        assert!(events.iter().any(|event| matches!(event, Event::Object {
            event: ObjectEvent::TextInserted { start_index, length: 1, content }, ..
        } if *start_index == prefix.len() as i32 && content == "猫")));
    }

    #[test]
    fn wholesale_text_replacement_does_not_emit_an_unbounded_event() {
        let events = Events::default();
        let mut adapter = adapter(document(""), events);
        let replacement = "x".repeat(
            AdapterChangeHandler::MAX_TEXT_EVENT_REPLACEMENT_BYTES + 1,
        );

        assert_eq!(
            adapter.update_and_count_text_documents_compared(document(&replacement)),
            0
        );
    }

    #[test]
    fn disabled_buttons_are_not_enabled_or_sensitive() {
        for disabled in [false, true] {
            let mut root = NodeData::new(Role::Document);
            root.set_children(vec![LocalId(2)]);
            let mut button = NodeData::new(Role::Button);
            button.set_label("Zoom in");
            if disabled {
                button.set_disabled();
            }
            let tree = Tree::new(
                TreeUpdate {
                    nodes: vec![(LocalId(1), root), (LocalId(2), button)],
                    tree: Some(accesskit::Tree::new(LocalId(1))),
                    tree_id: TreeId::ROOT,
                    focus: LocalId(1),
                },
                false,
            );
            let child = tree.state().root().children().next().unwrap();
            let state = NodeWrapper(&child).state(true);
            assert_eq!(state.contains(atspi_common::State::Enabled), !disabled);
            assert_eq!(state.contains(atspi_common::State::Sensitive), !disabled);
        }
    }

    #[test]
    fn filtered_child_notifications_preserve_order_indexes_and_visibility_changes() {
        fn children(ids: &[u64], hidden: Option<u64>) -> TreeUpdate {
            let mut parent = NodeData::new(Role::Document);
            parent.set_children(ids.iter().copied().map(LocalId).collect::<Vec<_>>());
            let mut nodes = vec![(LocalId(1), parent)];
            for &id in ids {
                let mut child = NodeData::new(Role::Button);
                child.set_label(format!("Child {id}"));
                if hidden == Some(id) {
                    child.set_hidden();
                }
                nodes.push((LocalId(id), child));
            }
            TreeUpdate {
                nodes,
                tree: Some(accesskit::Tree::new(LocalId(1))),
                tree_id: TreeId::ROOT,
                focus: LocalId(1),
            }
        }
        for (old_ids, new_ids, old_hidden, new_hidden, expected) in [
            (
                vec![2, 3, 4],
                vec![4, 5, 2, 6],
                None,
                None,
                vec![(true, 1, 5), (true, 3, 6), (false, 0, 3)],
            ),
            // Raw child IDs are unchanged, but the filtered membership changes.
            (
                vec![2, 3, 4],
                vec![2, 3, 4],
                Some(3),
                Some(4),
                vec![(true, 1, 3), (false, 0, 4)],
            ),
            // The adapter's existing contract emits no add/remove for reordering.
            (vec![2, 3, 4], vec![4, 2, 3], None, None, vec![]),
            ((2..3002).collect(), (2..3002).collect(), None, None, vec![]),
        ] {
            let initial = children(&old_ids, old_hidden);
            let events = Events::default();
            let adapter = adapter(initial.clone(), events.clone());
            events.0.lock().unwrap().clear();
            let old_tree = Tree::new(initial, false);
            let mut changed = children(&new_ids, new_hidden);
            changed.nodes[0]
                .1
                .set_transform(Affine::translate((0., -120.)));
            let new_tree = Tree::new(changed, false);
            let old_root = old_tree.state().root();
            let new_root = new_tree.state().root();
            NodeWrapper(&new_root).notify_changes(
                &WindowBounds::default(),
                &adapter,
                &NodeWrapper(&old_root),
            );
            let actual = events
                .0
                .lock()
                .unwrap()
                .iter()
                .filter_map(|event| match event {
                    Event::Object {
                        event: ObjectEvent::ChildAdded(index, child),
                        ..
                    } => Some((
                        true,
                        *index,
                        new_tree.state().locate_node(*child).unwrap().0 .0,
                    )),
                    Event::Object {
                        event: ObjectEvent::ChildRemoved(child),
                        ..
                    } => Some((false, 0, old_tree.state().locate_node(*child).unwrap().0 .0)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(actual, expected);
        }
    }

    #[test]
    fn geometry_only_document_update_does_not_announce_text_changes() {
        let initial = document("aé文");
        let events = Events::default();
        let adapter = adapter(initial.clone(), events.clone());
        events.0.lock().unwrap().clear();
        let old_tree = Tree::new(initial.clone(), false);
        let mut scrolled = initial;
        scrolled.nodes[0]
            .1
            .set_transform(Affine::translate((0., -120.)));
        let new_tree = Tree::new(scrolled, false);
        let mut handler = AdapterChangeHandler::new(&adapter, false);
        handler.node_updated(&old_tree.state().root(), &new_tree.state().root());
        // Notification correctness, not a performance/scan-count oracle.
        assert!(!events.0.lock().unwrap().iter().any(|event| matches!(
            event,
            Event::Object {
                event: ObjectEvent::TextInserted { .. } | ObjectEvent::TextRemoved { .. },
                ..
            }
        )));
    }

    #[test]
    fn transparent_geometry_scroll_is_bounded_and_invalidates_document_extents() {
        fn update(scroll: f64) -> TreeUpdate {
            let mut root = NodeData::new(Role::Window);
            root.set_children([LocalId(1)]);
            let mut editor = NodeData::new(Role::MultilineTextInput);
            editor.set_children([LocalId(2)]);
            let mut geometry = NodeData::new(Role::GenericContainer);
            geometry.set_children([LocalId(3)]);
            geometry.set_transform(Affine::translate((0., -scroll)));
            let mut document = NodeData::new(Role::Document);
            document.set_bounds(Rect::new(0., 0., 800., 90_000.));
            document.set_children((4..3004).map(LocalId).collect::<Vec<_>>());
            let mut nodes = vec![
                (LocalId(0), root),
                (LocalId(1), editor),
                (LocalId(2), geometry),
                (LocalId(3), document),
            ];
            for id in 4..3004 {
                let mut run = NodeData::new(Role::TextRun);
                run.set_value(format!("Paragraph {id}"));
                nodes.push((LocalId(id), run));
            }
            TreeUpdate {
                nodes,
                tree: Some(accesskit::Tree::new(LocalId(0))),
                tree_id: TreeId::ROOT,
                focus: LocalId(1),
            }
        }

        let events = Events::default();
        let mut adapter = adapter(update(0.), events.clone());
        events.0.lock().unwrap().clear();
        let mut geometry = NodeData::new(Role::GenericContainer);
        geometry.set_children([LocalId(3)]);
        geometry.set_transform(Affine::translate((0., -120.)));
        let compared = adapter.update_and_count_text_documents_compared(TreeUpdate {
            nodes: vec![(LocalId(2), geometry)],
            tree: Some(accesskit::Tree::new(LocalId(0))),
            tree_id: TreeId::ROOT,
            focus: LocalId(1),
        });

        assert_eq!(
            compared, 0,
            "scrolling the transparent owner must not scan document text"
        );
        assert_eq!(
            events
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Object {
                        event: ObjectEvent::BoundsChanged(_),
                        ..
                    }
                ))
                .count(),
            1,
            "the exposed document must receive one coordinate invalidation"
        );
    }

    #[test]
    fn large_document_reflow_coalesces_geometry_notifications() {
        const CHILDREN: u64 = 300;
        fn initial() -> TreeUpdate {
            let mut document = NodeData::new(Role::Document);
            document.set_children((2..2 + CHILDREN).map(LocalId).collect::<Vec<_>>());
            document.set_bounds(Rect::new(0., 0., 800., 10_000.));
            let mut nodes = vec![(LocalId(1), document)];
            nodes.extend((2..2 + CHILDREN).map(|id| {
                let mut paragraph = NodeData::new(Role::Paragraph);
                paragraph.set_bounds(Rect::new(0., id as f64 * 20., 800., id as f64 * 20. + 18.));
                (LocalId(id), paragraph)
            }));
            TreeUpdate {
                nodes,
                tree: Some(accesskit::Tree::new(LocalId(1))),
                tree_id: TreeId::ROOT,
                focus: LocalId(1),
            }
        }

        let events = Events::default();
        let mut adapter = adapter(initial(), events.clone());
        events.0.lock().unwrap().clear();
        let nodes = (2..2 + CHILDREN)
            .map(|id| {
                let mut paragraph = NodeData::new(Role::Paragraph);
                paragraph.set_bounds(Rect::new(
                    0.,
                    id as f64 * 20. + 10.,
                    800.,
                    id as f64 * 20. + 28.,
                ));
                (LocalId(id), paragraph)
            })
            .collect();
        adapter.update(TreeUpdate {
            nodes,
            tree: Some(accesskit::Tree::new(LocalId(1))),
            tree_id: TreeId::ROOT,
            focus: LocalId(1),
        });

        let bounds_events = events
                .0
                .lock()
                .unwrap()
                .iter()
                .filter(|event| matches!(
                    event,
                    Event::Object {
                        event: ObjectEvent::BoundsChanged(_),
                        ..
                    }
                ))
                .count();
        assert_eq!(bounds_events, 1);
        let tree = adapter.context.read_tree();
        assert_eq!(
            tree.state()
                .root()
                .children()
                .next()
                .unwrap()
                .raw_bounds()
                .unwrap()
                .y0,
            50.
        );
    }

    #[test]
    fn descendant_text_change_is_announced_with_simultaneous_parent_scroll() {
        let events = Events::default();
        let mut adapter = adapter(document("aé文"), events.clone());
        events.0.lock().unwrap().clear();
        let mut changed = document("a猫文");
        changed.nodes[0]
            .1
            .set_transform(Affine::translate((0., -120.)));
        adapter.update(changed);
        let events = events.0.lock().unwrap();
        assert!(events.iter().any(|e| matches!(e, Event::Object {
            event: ObjectEvent::TextRemoved { start_index: 1, length: 1, content }, ..
        } if content == "é")));
        assert!(events.iter().any(|e| matches!(e, Event::Object {
            event: ObjectEvent::TextInserted { start_index: 1, length: 1, content }, ..
        } if content == "猫")));
    }

    #[test]
    fn inserted_and_removed_text_runs_are_announced() {
        let events = Events::default();
        let initial = document("aé文");
        let mut adapter = adapter(initial.clone(), events.clone());
        events.0.lock().unwrap().clear();
        let mut appended = initial.clone();
        appended.nodes[0]
            .1
            .set_children(vec![LocalId(2), LocalId(3)]);
        let mut run = NodeData::new(Role::TextRun);
        run.set_value("猫");
        run.set_character_lengths([3]);
        appended.nodes.push((LocalId(3), run));
        adapter.update(appended);
        assert!(events
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::Object {
            event: ObjectEvent::TextInserted { start_index: 3, length: 1, content }, ..
        } if content == "猫")));
        events.0.lock().unwrap().clear();
        adapter.update(initial);
        assert!(events
            .0
            .lock()
            .unwrap()
            .iter()
            .any(|e| matches!(e, Event::Object {
            event: ObjectEvent::TextRemoved { start_index: 3, length: 1, content }, ..
        } if content == "猫")));
    }
}
