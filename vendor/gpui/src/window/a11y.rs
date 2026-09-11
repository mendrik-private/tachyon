//! Accessibility support, provided by [AccessKit][accesskit].
//!
//! There are user-facing guide-level docs [here](crate::_accessibility).
//!
//! ## Architecture
//!
//! ```text
//!                              ┌────────────────────────────────┐   ┌─────────────────────┐
//!                           ┌─▶│ AccessKit Adapter (MacOS)      │◀─▶│ MacOS System APIs   │
//!                           │  └────────────────────────────────┘   └─────────────────────┘
//!                           │
//! ┌──────┐   ┌───────────┐  │  ┌────────────────────────────────┐   ┌─────────────────────┐
//! │ GPUI │◀─▶│ AccessKit │◀─┼─▶│ AccessKit Adapter (Windows)    │◀─▶│ Windows System APIs │
//! └──────┘   └───────────┘  │  └────────────────────────────────┘   └─────────────────────┘
//!                           │
//!                           │  ┌────────────────────────────────┐   ┌─────────────────────┐
//!                           └─▶│ AccessKit Adapter (Linux)      │◀─▶│ dbus                │
//!                              └────────────────────────────────┘   └─────────────────────┘
//! ```
//!
//! In order for GPUI apps to be usable for people using assistive technology,
//! we must do a few things:
//! - Inform the system when the UI changes meaningfully. This includes:
//!   - Reporting new/removed/changed UI elements
//!   - *Not* reporting irrelevant UI changes, e.g. an invisible `div()` being
//!     added.
//!   - Reporting the appearance and capabilities of each UI element. For example:
//!     - What does this piece of text say?
//!     - How far along is this progress bar?
//!     - Can this node be focused?
//!     - Can this node have a value directly assigned? (e.g. a slider)
//! - Allowing the system to interact with the UI by dispatching actions to
//!   nodes. Note that AccessKit has its own [`Action`] type, which is not the
//!   [`crate::Action`] trait.
//! - Activate and deactivate accessibility features when requested by the
//!   system.
//!
//! Activating and deactivating at the right time is trivial, so I won't go into
//! detail here. The other two are almost orthogonal in implementation.
//!
//! The state for both lives in the [`A11y`] struct in this module.
//!
//! ### Reporting UI changes
//!
//! Every frame, we build a [`TreeUpdate`] and send it to the platform-specific
//! adapter. A [`TreeUpdate`] is a representation of a subset of the UI tree.
//! When the adapter receives the update, it diffs it against the previous
//! update, and calls platform-specific APIs to inform screen readers about the
//! changes. Nodes may have been created, destroyed, or updated.
//!
//! Each node has an ID, and this ID *should* be stable across frames. If a
//! node's ID changes, then, from AccessKit's point of view, it is a different
//! node.
//!
//! We derive the node ID from the [`GlobalElementId`] in
//! [`GlobalElementId::accesskit_node_id`]. Nodes without [`GlobalElementId`]s
//! cannot produce an AccessKit [`NodeId`], and so are not included in the
//! accessibility tree. We try to warn when using accessibility APIs on
//! [`div()`] without setting an ID.
//!
//! This all happens in [`Drawable::prepaint`]. The [`A11y`] struct maintains a
//! stack of nodes during prepainting, which we can use to calculate the
//! [`NodeId`]s, and record parent-child relationships. Once all [`Element`]s in
//! a frame have been prepainted, we send the resulting [`TreeUpdate`] object to
//! the adapter and the screen reader can announce the changes.
//!
//! #### Synthetic children
//!
//! Additionally, some nodes can register "synthetic children" using
//! [`Element::a11y_synthetic_children`]. Normally, one accesskit node is pushed
//! for every [`Element`] with a role and id. However, sometimes a single
//! element may want to produce many accesskit nodes. These extra nodes are
//! referred to as "synthetic children" of the element providing a non-default
//! [`Element::a11y_synthetic_children`] implementation.
//!
//! The user is provided a builder-style API using [`A11ySubtreeBuilder`], which
//! allows them to create push nodes that are children of the current node, as
//! well as modify the current node itself.
//!
//! GPUI calls this callback *after* prepainting (and just before popping the
//! corresponding element), since this step may need prepaint information to be
//! available. In the future, we may want to add prepaint information more
//! generally to [`Element::write_a11y_info`], but for now that's not necessary.
//!
//! ### Responding to actions
//!
//! On adapter creation, we provide a callback to the adapter, which can be used
//! to dispatch actions. This callback forwards to [`A11y::action_listeners`], a
//! mapping from [`NodeId`]s to action handlers (basically just `Box<dyn
//! Fn()>`).
//!
//! This is populated in:
//! - [`Window::on_a11y_action`], which is called by:
//! - [`Interactivity::paint`], which is called by:
//! - [`StatefulInteractiveElement::on_a11y_action`], which is a public-facing API
//!
//! These are cleared at the start of a frame, and re-populated during painting.
//!
//! [`NodeId`]: accesskit::NodeId

use crate::*;

pub(crate) mod debug;
mod snapshot;

use crate::{App, Bounds, FocusId, Pixels, SharedString, Window};
use accesskit::{Action, NodeId, TreeUpdate};
use collections::{FxHashMap, FxHashSet};
use smallvec::SmallVec;
use std::hash::{Hash, Hasher};
use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicU64, Ordering},
};

/// The fixed AccessKit node ID used for the root of every window's a11y tree.
pub(crate) const ROOT_NODE_ID: NodeId = NodeId(0);

/// A listener for an accessibility action on a specific node.
pub(crate) type A11yActionListener =
    Box<dyn FnMut(Option<&accesskit::ActionData>, &mut Window, &mut App) + 'static>;

pub(crate) type A11yActionRequestListener =
    Box<dyn FnMut(&accesskit::ActionRequest, &mut Window, &mut App) -> bool + 'static>;

/// Per-window accessibility state.
///
/// Manages the AccessKit tree that is built each frame and the mappings
/// needed to dispatch incoming action requests back to the right elements.
pub(crate) struct A11y {
    /// Whether accessibility has been [forcibly disabled] for this window.
    ///
    /// [forcibly disabled]: crate::Application::new_inaccessible
    force_disabled: bool,
    /// Whether a11y features have been requested by the system.
    ///
    /// Updated by AccessKit using callbacks provided to the adapter. Can change
    /// halfway through a frame.
    active_flag: Arc<AtomicBool>,
    /// Whether a11y features are active for *this specific frame*.
    ///
    /// At the start of each frame, we load [`Self::active_flag`] (using
    /// [`Self::sync_active_flag`]) and use this to determine whether we
    /// should construct a [`TreeUpdate`] for this frame. It's important that
    /// this value is stable within a frame, because the builder API exposed by
    /// this type maintains a stack of nodes and each must be pushed and popped
    /// exactly once.
    ///
    /// At the end of the frame, we re-call [`Self::sync_active_flag`] to
    /// determine whether we should actually send the finished [`TreeUpdate`].
    active_this_frame: bool,
    /// The next active frame must contain a complete baseline. This is set on
    /// every activation because an adapter that just connected cannot consume
    /// deltas against GPUI's previous publication.
    needs_full_update: bool,
    pub(crate) nodes: A11yNodeBuilder,
    pub(crate) focus_ids: FxHashMap<NodeId, FocusId>,
    pub(crate) node_bounds: FxHashMap<NodeId, Bounds<Pixels>>,
    pub(crate) action_request_listeners: Vec<A11yActionRequestListener>,
    pub(crate) action_listeners: FxHashMap<NodeId, Vec<(Action, A11yActionListener)>>,
    /// The window's title, used to label the root node so assistive
    /// technology can tell windows apart.
    window_title: Option<SharedString>,
    /// The focus id we most recently reported as having no accessibility node,
    /// used to log at most once per focus change rather than every frame.
    last_focus_without_node: Option<FocusId>,
    /// Retains the last tree update (and, in debug builds, per-node provenance)
    /// so it can be dumped via [`crate::Window::debug_a11y_tree_json`].
    debug: debug::A11yDebug,
    /// Maps a view's [`EntityId`] to its `Render` type name
    #[cfg(debug_assertions)]
    pub(crate) view_type_names: FxHashMap<EntityId, &'static str>,
}

impl A11y {
    pub(crate) fn new(
        active_flag: Arc<AtomicBool>,
        force_disabled: bool,
        window_title: Option<SharedString>,
    ) -> Self {
        Self {
            force_disabled,
            active_flag,
            active_this_frame: false,
            needs_full_update: true,
            nodes: A11yNodeBuilder::new(),
            focus_ids: FxHashMap::default(),
            node_bounds: FxHashMap::default(),
            action_listeners: FxHashMap::default(),
            action_request_listeners: Vec::new(),
            window_title,
            last_focus_without_node: None,
            debug: debug::A11yDebug::default(),
            #[cfg(debug_assertions)]
            view_type_names: FxHashMap::default(),
        }
    }

    /// Logs (once per focus change) that the focused element is not exposed to
    /// assistive technology because it has no accessibility node. When this
    /// happens, screen readers fall back to announcing the whole window instead
    /// of the focused element. The fix is to give the element both an
    /// `.id(...)` and a `.role(...)`.
    pub(crate) fn note_focus_without_node(&mut self, focus_id: FocusId, reason: &str) {
        if self.last_focus_without_node != Some(focus_id) {
            self.last_focus_without_node = Some(focus_id);
            log::info!(
                "a11y: focused element ({focus_id:?}) has no accessibility node \
                 ({reason}); assistive technology will announce the whole window \
                 instead. Give it both an `.id(...)` and a `.role(...)` to expose it."
            );
        }
    }

    pub(crate) fn set_window_title(&mut self, title: impl Into<SharedString>) {
        self.window_title = Some(title.into());
    }

    /// Ensures that [`Self::is_active`] returns up to date information.
    ///
    /// See the docs for [`Self::active_flag`] and [`Self::active_this_frame`]
    /// for more commentary.
    pub(crate) fn sync_active_flag(&mut self) {
        let active = !self.force_disabled && self.active_flag.load(Ordering::SeqCst);
        if active && !self.active_this_frame {
            self.needs_full_update = true;
        }
        self.active_this_frame = active;
    }

    pub(crate) fn is_active(&self) -> bool {
        self.active_this_frame
    }

    pub(crate) fn set_focusable(&mut self, node_id: NodeId, focus_id: FocusId) {
        self.focus_ids.insert(node_id, focus_id);
    }

    /// Report `node_id` as the currently-focused node, if it is present in the
    /// tree.
    ///
    /// Must only be called once per frame.
    pub(crate) fn set_focus(&mut self, node_id: NodeId) {
        // A focused node must have been registered as focusable this frame.
        if !self.focus_ids.contains_key(&node_id) {
            if cfg!(debug_assertions) {
                panic!("set_focus called for a node that was not registered with set_focusable");
            } else {
                log::warn!(
                    "a11y: set_focus called for a node that was not registered with \
                     set_focusable ({node_id:?})"
                );
            }
        }
        if self.nodes.has_node(node_id) {
            // The focused element is properly exposed; reset the dedup so a
            // later focus on a node-less element logs again.
            self.last_focus_without_node = None;
            self.nodes.set_focus(node_id);
        } else {
            // The element registered a focus handle and an id, but never got a
            // node because it has no role.
            if let Some(focus_id) = self.focus_ids.get(&node_id).copied() {
                self.note_focus_without_node(focus_id, "it has an id but no role");
            }
        }
    }

    pub(crate) fn set_active_descendant(&mut self, node_id: NodeId) {
        // The active descendant must be a descendant of the focused container,
        // not the focused node itself.
        if self.nodes.node_is_focused(node_id) {
            if cfg!(debug_assertions) {
                panic!("set_active_descendant called on the focused node");
            } else {
                log::warn!("a11y: set_active_descendant called on the focused node ({node_id:?})");
            }
            return;
        }
        if self.nodes.has_node(node_id) && self.nodes.focus_is_ancestor_of_current() {
            self.nodes.set_active_descendant(node_id);
        }
    }

    /// Clear per-frame state and push the root node to start a new frame.
    pub(crate) fn begin_frame(&mut self) {
        self.focus_ids.clear();
        self.node_bounds.clear();
        self.action_listeners.clear();
        self.action_request_listeners.clear();
        self.nodes.begin_frame(self.window_title.as_ref());
    }

    /// Finalize the tree and produce a [`TreeUpdate`] for the platform adapter.
    pub(crate) fn end_frame(&mut self, frame: debug::FrameDebugInfo) -> TreeUpdate {
        let update = self.nodes.finalize(self.needs_full_update);
        self.needs_full_update = false;
        self.debug.capture(
            &update,
            self.nodes.focus,
            self.nodes.active_descendant,
            self.window_title.as_ref(),
            frame,
        );
        #[cfg(debug_assertions)]
        self.debug.capture_node_info(&self.nodes.node_info);
        update
    }

    pub(crate) fn debug_tree_json(&self) -> Option<String> {
        self.debug.to_json()
    }
}

/// Builder API for synthetic children. See the docs for
/// [`Element::a11y_synthetic_children`].
pub struct A11ySubtreeBuilder<'a> {
    parent_id: NodeId,
    nodes: &'a mut A11yNodeBuilder,
    /// Provenance of the real element whose `a11y_synthetic_children` is
    /// running.
    #[cfg(debug_assertions)]
    creator: debug::NodeCreator,
}

impl<'a> A11ySubtreeBuilder<'a> {
    pub(crate) fn new(parent_id: NodeId, nodes: &'a mut A11yNodeBuilder) -> Self {
        Self {
            parent_id,
            nodes,
            #[cfg(debug_assertions)]
            creator: debug::NodeCreator::default(),
        }
    }

    #[cfg(debug_assertions)]
    pub(crate) fn with_creator(mut self, creator: debug::NodeCreator) -> Self {
        self.creator = creator;
        self
    }

    /// Derive a [`NodeId`] for a synthetic child.
    ///
    /// The generated ID is based on the hash of `key`, as well as the parent's
    /// ID. This means that `key`s must be unique within the same
    /// [`Element::a11y_synthetic_children`] call, but may be duplicated across
    /// different calls.
    pub fn synthetic_node_id(&self, key: impl Hash) -> NodeId {
        let mut hasher = std::hash::DefaultHasher::default();
        self.parent_id.0.hash(&mut hasher);
        key.hash(&mut hasher);
        NodeId(hasher.finish())
    }

    /// Append a synthetic leaf node as a child of this element's node.
    ///
    /// Returns `false` if a node with this id is already present in the tree,
    /// in which case the node is discarded.
    pub fn push_child(&mut self, id: NodeId, node: accesskit::Node) -> bool {
        let pushed = self.nodes.push_leaf(id, node);
        #[cfg(debug_assertions)]
        if pushed {
            self.nodes.record_node_info(
                id,
                debug::NodeDebugInfo {
                    synthetic: true,
                    view: self.creator.view,
                    element_id: self.creator.element_id.clone(),
                    source_location: self.creator.source_location,
                },
            );
        }
        pushed
    }

    /// Attach an immutable, previously validated synthetic subtree to the
    /// current node. Descendants are published only when the subtree is new,
    /// replaced, or a newly activated adapter needs a complete baseline.
    pub fn push_retained_subtree(&mut self, subtree: &RetainedA11ySubtree) -> bool {
        self.nodes.push_retained(subtree.clone())
    }

    /// Publish a property-only update for one node in an attached retained
    /// subtree. The node's children must match the immutable baseline.
    pub fn update_retained_node(
        &mut self,
        subtree: &RetainedA11ySubtree,
        id: NodeId,
        node: accesskit::Node,
    ) -> bool {
        self.nodes.update_retained(subtree, id, node)
    }

    /// A mutable reference to the parent node.
    pub fn parent_node(&mut self) -> &mut accesskit::Node {
        self.nodes
            .current_node_mut()
            .expect("A11ySubtreeBuilder exists only while its element's node is on the stack")
    }
}

static NEXT_RETAINED_SUBTREE_ID: AtomicU64 = AtomicU64::new(1);

/// An immutable synthetic accessibility subtree that GPUI can attach without
/// reconstructing all of its descendants on every frame.
#[derive(Clone, Debug)]
pub struct RetainedA11ySubtree {
    inner: Arc<RetainedA11ySubtreeInner>,
}

#[derive(Debug)]
struct RetainedA11ySubtreeInner {
    identity: u64,
    roots: Vec<NodeId>,
    nodes: Vec<(NodeId, accesskit::Node)>,
    node_indices: FxHashMap<NodeId, usize>,
}

/// Validation failure returned while constructing a retained subtree.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetainedA11ySubtreeError {
    /// The subtree has no roots or nodes.
    Empty,
    /// Two nodes use the same ID.
    DuplicateNode(NodeId),
    /// The root list repeats an ID.
    DuplicateRoot(NodeId),
    /// A root ID is absent from the node list.
    MissingRoot(NodeId),
    /// A child reference points outside the retained subtree.
    MissingChild {
        /// The node containing the invalid child reference.
        parent: NodeId,
        /// The missing child ID.
        child: NodeId,
    },
    /// A node is owned by more than one parent.
    MultipleParents(NodeId),
    /// A declared root is also owned by another node.
    RootHasParent(NodeId),
    /// A node has no path from any declared root.
    UnreachableNode(NodeId),
}

impl std::fmt::Display for RetainedA11ySubtreeError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "invalid retained accessibility subtree: {self:?}"
        )
    }
}

impl std::error::Error for RetainedA11ySubtreeError {}

impl RetainedA11ySubtree {
    /// Validate and retain a complete synthetic subtree. All child references
    /// must remain inside the subtree, and every node must have one canonical
    /// path from exactly one root.
    pub fn new(
        roots: Vec<NodeId>,
        nodes: Vec<(NodeId, accesskit::Node)>,
    ) -> Result<Self, RetainedA11ySubtreeError> {
        if roots.is_empty() || nodes.is_empty() {
            return Err(RetainedA11ySubtreeError::Empty);
        }
        let mut node_indices = FxHashMap::default();
        for (index, (id, _)) in nodes.iter().enumerate() {
            if node_indices.insert(*id, index).is_some() {
                return Err(RetainedA11ySubtreeError::DuplicateNode(*id));
            }
        }
        let mut root_ids = FxHashSet::default();
        for root in &roots {
            if !root_ids.insert(*root) {
                return Err(RetainedA11ySubtreeError::DuplicateRoot(*root));
            }
            if !node_indices.contains_key(root) {
                return Err(RetainedA11ySubtreeError::MissingRoot(*root));
            }
        }
        let mut parent_counts = FxHashMap::default();
        for (parent, node) in &nodes {
            for child in node.children() {
                if !node_indices.contains_key(child) {
                    return Err(RetainedA11ySubtreeError::MissingChild {
                        parent: *parent,
                        child: *child,
                    });
                }
                let count = parent_counts.entry(*child).or_insert(0usize);
                *count += 1;
                if *count > 1 {
                    return Err(RetainedA11ySubtreeError::MultipleParents(*child));
                }
            }
        }
        for root in &roots {
            if parent_counts.contains_key(root) {
                return Err(RetainedA11ySubtreeError::RootHasParent(*root));
            }
        }
        let mut reachable = FxHashSet::default();
        let mut pending = roots.clone();
        while let Some(id) = pending.pop() {
            if !reachable.insert(id) {
                continue;
            }
            let node = &nodes[node_indices[&id]].1;
            pending.extend(node.children().iter().copied());
        }
        if reachable.len() != nodes.len() {
            let id = nodes
                .iter()
                .map(|(id, _)| *id)
                .find(|id| !reachable.contains(id))
                .expect("different retained node and reachability counts");
            return Err(RetainedA11ySubtreeError::UnreachableNode(id));
        }
        Ok(Self {
            inner: Arc::new(RetainedA11ySubtreeInner {
                identity: NEXT_RETAINED_SUBTREE_ID.fetch_add(1, Ordering::Relaxed),
                roots,
                nodes,
                node_indices,
            }),
        })
    }

    /// Return a retained baseline node by ID.
    pub fn node(&self, id: NodeId) -> Option<&accesskit::Node> {
        self.inner
            .node_indices
            .get(&id)
            .map(|index| &self.inner.nodes[*index].1)
    }

    /// Replace node properties while preserving this subtree's publication
    /// identity. Every node ID and child list must remain unchanged. Callers
    /// still publish the changed nodes with [`A11ySubtreeBuilder::update_retained_node`];
    /// the revised baseline makes later full publications and reattachments
    /// use the latest properties without replacing the whole native tree.
    pub fn revise_properties(
        &self,
        nodes: Vec<(NodeId, accesskit::Node)>,
    ) -> Result<Self, Vec<(NodeId, accesskit::Node)>> {
        if nodes.len() != self.inner.nodes.len()
            || nodes.iter().any(|(id, node)| {
                self.node(*id)
                    .is_none_or(|baseline| baseline.children() != node.children())
            })
        {
            return Err(nodes);
        }
        let node_indices = nodes
            .iter()
            .enumerate()
            .map(|(index, (id, _))| (*id, index))
            .collect::<FxHashMap<_, _>>();
        if node_indices.len() != nodes.len() {
            return Err(nodes);
        }
        Ok(Self {
            inner: Arc::new(RetainedA11ySubtreeInner {
                identity: self.inner.identity,
                roots: self.inner.roots.clone(),
                nodes,
                node_indices,
            }),
        })
    }
}

struct RetainedAttachment {
    subtree: RetainedA11ySubtree,
    updates: FxHashMap<NodeId, accesskit::Node>,
}

pub(crate) struct A11yNodeBuilder {
    ids_stack: SmallVec<[NodeId; 16]>,
    nodes_stack: SmallVec<[accesskit::Node; 16]>,
    /// This is the exact type required by accesskit, so we can't just make it a
    /// `HashMap<NodeId, Node>` to remove the need for `seen_ids`
    all_nodes: Vec<(NodeId, accesskit::Node)>,
    seen_ids: FxHashSet<NodeId>,
    retained: Vec<RetainedAttachment>,
    previously_retained: FxHashMap<u64, RetainedA11ySubtree>,
    #[cfg(test)]
    retained_nodes_cloned: usize,
    #[cfg(test)]
    retained_nodes_scanned: usize,
    /// The node that GPUI considers focused. Note that this may be different to
    /// what is reported to accesskit - see [`Self::active_descendant`]
    focus: Option<NodeId>,
    /// If a node calls `.aria_active_descendant()`, AND an ancestor is focused,
    /// override it as the focused node. This supports the "active descendant"
    /// pattern, which allows a focused container to act as if a descendant is
    /// focused.
    active_descendant: Option<NodeId>,
    #[cfg(debug_assertions)]
    node_info: FxHashMap<NodeId, debug::NodeDebugInfo>,
}

impl A11yNodeBuilder {
    fn new() -> Self {
        Self {
            ids_stack: SmallVec::new(),
            nodes_stack: SmallVec::new(),
            all_nodes: Vec::new(),
            seen_ids: FxHashSet::default(),
            retained: Vec::new(),
            previously_retained: FxHashMap::default(),
            #[cfg(test)]
            retained_nodes_cloned: 0,
            #[cfg(test)]
            retained_nodes_scanned: 0,
            focus: None,
            active_descendant: None,
            #[cfg(debug_assertions)]
            node_info: FxHashMap::default(),
        }
    }

    /// Records provenance for a node already pushed this frame. Debug builds only.
    #[cfg(debug_assertions)]
    pub(crate) fn record_node_info(&mut self, id: NodeId, info: debug::NodeDebugInfo) {
        self.node_info.insert(id, info);
    }

    #[must_use]
    fn can_push(&mut self, id: NodeId) -> bool {
        debug_assert!(!self.ids_stack.is_empty(), "node pushed before push_root");

        if self.seen_ids.contains(&id)
            || self
                .retained
                .iter()
                .any(|attachment| attachment.subtree.inner.node_indices.contains_key(&id))
        {
            debug_assert!(
                false,
                "Duplicate a11y node id: {id:?}. In a release build, this node would be silently discarded from the a11y tree."
            );
            return false;
        }

        self.seen_ids.insert(id);
        true
    }

    fn push_retained(&mut self, subtree: RetainedA11ySubtree) -> bool {
        if self.ids_stack.last().is_none() {
            return false;
        }
        let duplicate = self
            .seen_ids
            .iter()
            .any(|id| subtree.inner.node_indices.contains_key(id))
            || self.retained.iter().any(|attachment| {
                attachment.subtree.inner.identity == subtree.inner.identity
                    || attachment
                        .subtree
                        .inner
                        .node_indices
                        .keys()
                        .any(|id| subtree.inner.node_indices.contains_key(id))
            });
        if duplicate {
            debug_assert!(false, "duplicate node in retained accessibility subtree");
            return false;
        }
        if let Some(parent_node) = self.nodes_stack.last_mut() {
            for root in &subtree.inner.roots {
                parent_node.push_child(*root);
            }
        }
        self.retained.push(RetainedAttachment {
            subtree,
            updates: FxHashMap::default(),
        });
        true
    }

    fn update_retained(
        &mut self,
        subtree: &RetainedA11ySubtree,
        id: NodeId,
        node: accesskit::Node,
    ) -> bool {
        let Some(attachment) = self
            .retained
            .iter_mut()
            .find(|attachment| attachment.subtree.inner.identity == subtree.inner.identity)
        else {
            debug_assert!(
                false,
                "retained node updated before its subtree was attached"
            );
            return false;
        };
        let Some(baseline) = subtree.node(id) else {
            debug_assert!(false, "updated node is not part of the retained subtree");
            return false;
        };
        if node.children() != baseline.children() {
            debug_assert!(false, "retained node updates cannot change child ownership");
            return false;
        }
        attachment.updates.insert(id, node);
        true
    }

    /// Push a new node onto the stack. It becomes a child of the current
    /// top-of-stack node.
    ///
    /// Returns `true` if the node was successfully pushed.
    pub(crate) fn push(&mut self, id: NodeId, node: accesskit::Node) -> bool {
        if !self.can_push(id) {
            return false;
        }

        if let Some(parent) = self.nodes_stack.last_mut() {
            parent.push_child(id);
        }
        self.ids_stack.push(id);
        self.nodes_stack.push(node);
        true
    }

    /// Add a leaf node as a child of the current top-of-stack node, without
    /// pushing it onto the stack. Semantically equivalent to a [`Self::push`]
    /// followed by a [`Self::pop`].
    ///
    /// Returns `true` if the node was successfully pushed.
    pub(crate) fn push_leaf(&mut self, id: NodeId, node: accesskit::Node) -> bool {
        if !self.can_push(id) {
            return false;
        }

        if let Some(parent) = self.nodes_stack.last_mut() {
            parent.push_child(id);
        }
        self.all_nodes.push((id, node));
        true
    }

    pub(crate) fn current_node_mut(&mut self) -> Option<&mut accesskit::Node> {
        self.nodes_stack.last_mut()
    }

    /// Pop the current node off the stack and finalize it into the all_nodes
    /// list.
    pub(crate) fn pop(&mut self) {
        debug_assert!(self.ids_stack.len() > 1, "pop would remove the root node");

        if let (Some(id), Some(node)) = (self.ids_stack.pop(), self.nodes_stack.pop()) {
            self.all_nodes.push((id, node));
        }
    }

    /// Push the root node to start a new frame.
    fn begin_frame(&mut self, window_title: Option<&SharedString>) {
        self.all_nodes.clear();
        self.ids_stack.clear();
        self.nodes_stack.clear();
        self.seen_ids.clear();
        self.retained.clear();
        #[cfg(test)]
        {
            self.retained_nodes_cloned = 0;
            self.retained_nodes_scanned = 0;
        }
        #[cfg(debug_assertions)]
        self.node_info.clear();
        let mut root_node = accesskit::Node::new(accesskit::Role::Window);
        if let Some(title) = window_title {
            root_node.set_label(title.to_string());
        }

        self.ids_stack.push(ROOT_NODE_ID);
        self.nodes_stack.push(root_node);
        self.focus = None;
        self.active_descendant = None;
    }

    /// Returns whether a node with the given ID has been pushed in this frame.
    pub(crate) fn has_node(&self, id: NodeId) -> bool {
        id == ROOT_NODE_ID
            || self.seen_ids.contains(&id)
            || self
                .retained
                .iter()
                .any(|attachment| attachment.subtree.inner.node_indices.contains_key(&id))
    }

    /// Returns whether `id` is the node currently reported as focused.
    pub(crate) fn node_is_focused(&self, id: NodeId) -> bool {
        self.focus == Some(id)
    }

    pub(crate) fn focus_is_ancestor_of_current(&self) -> bool {
        let Some(focus) = self.focus else {
            return false;
        };

        // The current node is on top of the stack; everything below it is an
        // ancestor.
        let ancestor_count = self.ids_stack.len().saturating_sub(1);
        self.ids_stack[..ancestor_count].contains(&focus)
    }

    pub(crate) fn set_active_descendant(&mut self, id: NodeId) {
        if self
            .active_descendant
            .is_some_and(|existing| existing != id)
        {
            if cfg!(debug_assertions) {
                panic!("active descendant claimed by multiple nodes in one frame");
            } else {
                log::warn!(
                    "a11y: multiple nodes claimed the active descendant this frame; \
                     using last-wins ({id:?})"
                );
            }
        }
        self.active_descendant = Some(id);
    }

    pub(crate) fn set_focus(&mut self, id: NodeId) {
        if self.focus.is_some() {
            if cfg!(debug_assertions) {
                panic!("set_focus called more than once in a single frame");
            } else {
                log::warn!(
                    "a11y: set_focus called more than once in a single frame; \
                     using last-wins ({id:?})"
                );
            }
        }
        self.focus = Some(id);
    }

    fn finalize(&mut self, force_full: bool) -> TreeUpdate {
        // Stack should contain only the root node
        debug_assert_eq!(self.ids_stack.len(), 1);
        debug_assert_eq!(self.ids_stack[0], ROOT_NODE_ID);

        if self.ids_stack.len() != 1 {
            log::error!(
                "a11y: Stack imbalance at end of frame: expected 1 (root), got {}. \
                 Some elements may have pushed without popping.",
                self.ids_stack.len()
            );
        }

        // Pop remaining nodes (should just be the root).
        while !self.ids_stack.is_empty() {
            if let (Some(id), Some(node)) = (self.ids_stack.pop(), self.nodes_stack.pop()) {
                self.all_nodes.push((id, node));
            }
        }

        let focus = match self.active_descendant {
            Some(id) if self.has_node(id) => id,
            Some(id) => {
                if cfg!(debug_assertions) {
                    panic!("active_descendant set to {id:?}, which is not in the tree");
                } else {
                    log::warn!("active_descendant set to {id:?}, which is not in the tree");
                    self.focus.unwrap_or(ROOT_NODE_ID)
                }
            }

            _ => self.focus.unwrap_or(ROOT_NODE_ID),
        };

        let ordinary_node_count = self.all_nodes.len();
        let mut nodes = std::mem::take(&mut self.all_nodes);
        for attachment in &mut self.retained {
            let publish_all = force_full
                || !self
                    .previously_retained
                    .contains_key(&attachment.subtree.inner.identity);
            if publish_all {
                for (id, baseline) in &attachment.subtree.inner.nodes {
                    let node = attachment
                        .updates
                        .remove(id)
                        .unwrap_or_else(|| baseline.clone());
                    nodes.push((*id, node));
                    #[cfg(test)]
                    {
                        self.retained_nodes_cloned += 1;
                        self.retained_nodes_scanned += 1;
                    }
                }
            } else {
                nodes.extend(attachment.updates.drain());
            }
        }
        let update = TreeUpdate {
            nodes,
            tree: Some(accesskit::Tree::new(ROOT_NODE_ID)),
            tree_id: accesskit::TreeId::ROOT,
            focus,
        };

        let update = self.repair_tree_update(update, ordinary_node_count);
        self.previously_retained = self
            .retained
            .iter()
            .map(|attachment| {
                (
                    attachment.subtree.inner.identity,
                    attachment.subtree.clone(),
                )
            })
            .collect();
        update
    }

    /// Accesskit panics on invalid [`TreeUpdate`]s. This function defensively
    /// checks invariants that accesskit panics on, and tries to fix them.
    fn repair_tree_update(&self, mut update: TreeUpdate, ordinary_node_count: usize) -> TreeUpdate {
        // Focus must point to a node in the tree.
        if !self.has_node(update.focus) {
            log::error!(
                "a11y: Focused node {:?} is not in the tree ({} nodes). \
                 Falling back to root. This is a bug in the a11y tree builder.",
                update.focus,
                update.nodes.len()
            );
            update.focus = ROOT_NODE_ID;
        }

        // Every ordinary child reference must point to a node in the live
        // frame. Retained descendants may be omitted from this incremental
        // update because their baseline is already present in the adapter.
        for (id, node) in update.nodes.iter_mut().take(ordinary_node_count) {
            let has_invalid_child = node
                .children()
                .iter()
                .any(|child_id| !self.has_node(*child_id));
            if has_invalid_child {
                let children = node.children();
                let invalid_count = children
                    .iter()
                    .filter(|child_id| !self.has_node(**child_id))
                    .count();
                log::error!(
                    "a11y: Node {:?} references {} children not present in the tree. \
                     Stripping invalid child references.",
                    id,
                    invalid_count
                );
                let valid: Vec<NodeId> = children
                    .iter()
                    .copied()
                    .filter(|child_id| self.has_node(*child_id))
                    .collect();
                node.set_children(valid);
            }
        }

        update
    }
}

#[cfg(test)]
mod tests {
    // Import specific items rather than glob-importing `super`, which would pull
    // in gpui's own `test` attribute macro and shadow the standard one.
    use super::{
        A11y, A11yNodeBuilder, ROOT_NODE_ID, RetainedA11ySubtree, RetainedA11ySubtreeError,
    };
    use crate::FocusId;
    use accesskit::{NodeId, Role};
    use std::sync::{Arc, atomic::AtomicBool};

    fn test_node() -> accesskit::Node {
        accesskit::Node::new(Role::GenericContainer)
    }

    fn new_builder() -> A11yNodeBuilder {
        let mut builder = A11yNodeBuilder::new();
        builder.begin_frame(None);
        builder
    }

    fn new_a11y() -> A11y {
        let mut a11y = A11y::new(Arc::new(AtomicBool::new(true)), false, None);
        a11y.begin_frame();
        a11y
    }

    fn retained_tree(label: &str) -> RetainedA11ySubtree {
        let mut section = test_node();
        section.set_children(vec![NodeId(2), NodeId(3)]);
        let mut first = accesskit::Node::new(Role::Paragraph);
        first.set_label(format!("{label} first"));
        let mut second = accesskit::Node::new(Role::Paragraph);
        second.set_label(format!("{label} second"));
        RetainedA11ySubtree::new(
            vec![NodeId(1)],
            vec![
                (NodeId(1), section),
                (NodeId(2), first),
                (NodeId(3), second),
            ],
        )
        .unwrap()
    }

    #[test]
    fn retained_subtree_skips_unchanged_descendant_work() {
        let subtree = retained_tree("original");
        let mut builder = new_builder();
        assert!(builder.push_retained(subtree.clone()));
        builder.set_focus(NodeId(2));
        let first = builder.finalize(false);
        assert_eq!(
            first.nodes.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
            [0, 1, 2, 3]
        );
        assert_eq!(builder.retained_nodes_cloned, 3);
        assert_eq!(builder.retained_nodes_scanned, 3);

        builder.begin_frame(None);
        assert!(builder.push_retained(subtree.clone()));
        builder.set_focus(NodeId(2));
        let unchanged = builder.finalize(false);
        assert_eq!(
            unchanged
                .nodes
                .iter()
                .map(|(id, _)| id.0)
                .collect::<Vec<_>>(),
            [0]
        );
        assert_eq!(builder.retained_nodes_cloned, 0);
        assert_eq!(builder.retained_nodes_scanned, 0);

        builder.begin_frame(None);
        assert!(builder.push_retained(subtree.clone()));
        let mut changed = subtree.node(NodeId(2)).unwrap().clone();
        changed.set_label("changed");
        assert!(builder.update_retained(&subtree, NodeId(2), changed));
        let delta = builder.finalize(false);
        assert_eq!(
            delta.nodes.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
            [0, 2]
        );
        assert_eq!(builder.retained_nodes_cloned, 0);
        assert_eq!(builder.retained_nodes_scanned, 0);
    }

    #[test]
    fn retained_property_revision_keeps_identity_and_latest_full_baseline() {
        let original = retained_tree("original");
        let mut builder = new_builder();
        assert!(builder.push_retained(original.clone()));
        builder.finalize(false);

        let mut nodes = original.inner.nodes.clone();
        let changed = nodes.iter_mut().find(|(id, _)| *id == NodeId(2)).unwrap();
        changed.1.set_label("changed");
        let changed_node = changed.1.clone();
        let revised = original.revise_properties(nodes).unwrap();

        builder.begin_frame(None);
        assert!(builder.push_retained(revised.clone()));
        assert!(builder.update_retained(&revised, NodeId(2), changed_node));
        let delta = builder.finalize(false);
        assert_eq!(
            delta.nodes.iter().map(|(id, _)| id.0).collect::<Vec<_>>(),
            [0, 2]
        );

        builder.begin_frame(None);
        assert!(builder.push_retained(revised.clone()));
        assert_eq!(builder.finalize(false).nodes.len(), 1);

        builder.begin_frame(None);
        builder.finalize(false);
        builder.begin_frame(None);
        assert!(builder.push_retained(revised));
        let reattached = builder.finalize(false);
        assert_eq!(reattached.nodes.len(), 4);
        assert_eq!(
            reattached
                .nodes
                .iter()
                .find(|(id, _)| *id == NodeId(2))
                .and_then(|(_, node)| node.label()),
            Some("changed")
        );
    }

    #[test]
    fn retained_subtree_republishes_after_detach_replacement_and_activation() {
        let original = retained_tree("original");
        let replacement = retained_tree("replacement");
        let mut builder = new_builder();
        assert!(builder.push_retained(original.clone()));
        builder.finalize(false);

        builder.begin_frame(None);
        let detached = builder.finalize(false);
        assert_eq!(detached.nodes[0].1.children(), &[]);

        builder.begin_frame(None);
        assert!(builder.push_retained(original.clone()));
        assert_eq!(builder.finalize(false).nodes.len(), 4);

        builder.begin_frame(None);
        assert!(builder.push_retained(replacement));
        assert_eq!(builder.finalize(false).nodes.len(), 4);

        builder.begin_frame(None);
        assert!(builder.push_retained(original));
        assert_eq!(builder.finalize(true).nodes.len(), 4);
    }

    #[test]
    fn retained_subtree_rejects_invalid_hierarchy() {
        let mut missing_child = test_node();
        missing_child.push_child(NodeId(2));
        assert_eq!(
            RetainedA11ySubtree::new(vec![NodeId(1)], vec![(NodeId(1), missing_child)])
                .unwrap_err(),
            RetainedA11ySubtreeError::MissingChild {
                parent: NodeId(1),
                child: NodeId(2),
            }
        );

        let mut cycle_a = test_node();
        cycle_a.push_child(NodeId(2));
        let mut cycle_b = test_node();
        cycle_b.push_child(NodeId(1));
        assert_eq!(
            RetainedA11ySubtree::new(
                vec![NodeId(1)],
                vec![(NodeId(1), cycle_a), (NodeId(2), cycle_b)],
            )
            .unwrap_err(),
            RetainedA11ySubtreeError::RootHasParent(NodeId(1))
        );
    }

    #[test]
    fn active_descendant_honored_when_container_focused() {
        let mut builder = new_builder();
        let container = NodeId(1);
        let item = NodeId(2);

        assert!(builder.push(container, test_node()));
        builder.set_focus(container);
        assert!(builder.push(item, test_node()));

        // The item is on top of the stack; the focused container is its
        // ancestor, so the claim is honored.
        assert!(builder.focus_is_ancestor_of_current());
        builder.set_active_descendant(item);

        builder.pop(); // item
        builder.pop(); // container
        let update = builder.finalize(false);
        assert_eq!(update.focus, item);
    }

    #[test]
    fn active_descendant_honored_for_deep_descendant() {
        let mut builder = new_builder();
        let container = NodeId(1);
        let group = NodeId(2);
        let item = NodeId(3);

        assert!(builder.push(container, test_node()));
        builder.set_focus(container);
        assert!(builder.push(group, test_node()));
        assert!(builder.push(item, test_node()));

        // The item is a grandchild of the focused container; depth doesn't
        // matter, the focused ancestor is still on the stack.
        assert!(builder.focus_is_ancestor_of_current());
        builder.set_active_descendant(item);

        builder.pop(); // item
        builder.pop(); // group
        builder.pop(); // container
        let update = builder.finalize(false);
        assert_eq!(update.focus, item);
    }

    #[test]
    fn active_descendant_ignored_when_focus_in_other_subtree() {
        let mut builder = new_builder();
        let focused_container = NodeId(1);
        let focused_leaf = NodeId(2);
        let other_container = NodeId(3);
        let other_item = NodeId(4);

        // First subtree holds real focus.
        assert!(builder.push(focused_container, test_node()));
        assert!(builder.push(focused_leaf, test_node()));
        builder.set_focus(focused_leaf);
        builder.pop(); // focused_leaf
        builder.pop(); // focused_container

        // Second subtree: its item would claim the active descendant, but the
        // focus is not on any of its ancestors, so the gate rejects it.
        assert!(builder.push(other_container, test_node()));
        assert!(builder.push(other_item, test_node()));
        assert!(!builder.focus_is_ancestor_of_current());
        builder.pop(); // other_item
        builder.pop(); // other_container

        let update = builder.finalize(false);
        assert_eq!(update.focus, focused_leaf);
    }

    #[test]
    fn active_descendant_ignored_when_nothing_focused() {
        let mut builder = new_builder();
        let container = NodeId(1);
        let item = NodeId(2);

        assert!(builder.push(container, test_node()));
        assert!(builder.push(item, test_node()));

        // Nothing is focused (focus defaults to the root window node), so the
        // gate rejects the claim.
        assert!(!builder.focus_is_ancestor_of_current());
        builder.pop();
        builder.pop();

        let update = builder.finalize(false);
        assert_eq!(update.focus, ROOT_NODE_ID);
    }

    #[test]
    fn regular_focus_used_when_no_active_descendant() {
        let mut builder = new_builder();
        let focused = NodeId(1);

        assert!(builder.push(focused, test_node()));
        builder.set_focus(focused);
        builder.pop();

        let update = builder.finalize(false);
        assert_eq!(update.focus, focused);
    }

    #[test]
    fn focus_is_ancestor_excludes_self_and_non_ancestors() {
        let mut builder = new_builder();
        let container = NodeId(1);
        let item = NodeId(2);

        assert!(builder.push(container, test_node()));
        builder.set_focus(container);

        // With the focused container itself on top, it is not its own (strict)
        // ancestor, so the gate is false.
        assert!(!builder.focus_is_ancestor_of_current());

        assert!(builder.push(item, test_node()));
        // Now the focused container is a strict ancestor of the item on top.
        assert!(builder.focus_is_ancestor_of_current());

        builder.pop();
        builder.pop();
    }

    // The double-claim guard panics only in debug builds; in release it falls
    // back to last-wins with a warning.
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "active descendant claimed by multiple nodes")
    )]
    fn multiple_active_descendant_claims_panic_in_debug() {
        let mut builder = new_builder();
        builder.set_active_descendant(NodeId(1));
        builder.set_active_descendant(NodeId(2));
    }

    // Setting focus twice in one frame means two elements both claimed window
    // focus; that panics in debug and falls back to last-wins in release.
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "set_focus called more than once")
    )]
    fn setting_focus_twice_panics_in_debug() {
        let mut builder = new_builder();
        builder.set_focus(NodeId(1));
        builder.set_focus(NodeId(2));
    }

    // Focusing a node that was never registered as focusable is a bug: panic in
    // debug, warn in release.
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "was not registered with set_focusable")
    )]
    fn set_focus_without_set_focusable() {
        let mut a11y = new_a11y();
        let node = NodeId(1);
        assert!(a11y.nodes.push(node, test_node()));
        // set_focusable was never called for `node`.
        a11y.set_focus(node);
    }

    // The focused node cannot also be its own active descendant: panic in
    // debug, warn in release.
    #[test]
    #[cfg_attr(debug_assertions, should_panic(expected = "on the focused node"))]
    fn set_active_descendant_on_focused_node() {
        let mut a11y = new_a11y();
        let node = NodeId(1);
        assert!(a11y.nodes.push(node, test_node()));
        a11y.set_focusable(node, FocusId::default());
        a11y.set_focus(node);
        a11y.set_active_descendant(node);
    }

    // Two sibling children of a focused container both claim the active
    // descendant (both pass the focus gate). The second claim is a bug: panic
    // in debug, last-wins + warn in release.
    #[test]
    #[cfg_attr(
        debug_assertions,
        should_panic(expected = "active descendant claimed by multiple nodes")
    )]
    fn two_siblings_claiming_active_descendant() {
        let mut a11y = new_a11y();
        let container = NodeId(1);
        let first = NodeId(2);
        let second = NodeId(3);

        assert!(a11y.nodes.push(container, test_node()));
        a11y.set_focusable(container, FocusId::default());
        a11y.set_focus(container);

        assert!(a11y.nodes.push(first, test_node()));
        a11y.set_active_descendant(first);
        a11y.nodes.pop(); // first

        assert!(a11y.nodes.push(second, test_node()));
        a11y.set_active_descendant(second);
        a11y.nodes.pop(); // second

        a11y.nodes.pop(); // container
    }

    // Node A is focused; node C (a child of the unfocused node B) claims the
    // active descendant. The final tree must still report A as focused.
    #[test]
    fn active_descendant_in_unfocused_subtree_keeps_real_focus() {
        let mut a11y = new_a11y();
        let a = NodeId(1);
        let b = NodeId(2);
        let c = NodeId(3);

        assert!(a11y.nodes.push(a, test_node()));
        a11y.set_focusable(a, FocusId::default());
        a11y.set_focus(a);
        a11y.nodes.pop(); // a

        assert!(a11y.nodes.push(b, test_node()));
        assert!(a11y.nodes.push(c, test_node()));
        a11y.set_active_descendant(c);
        a11y.nodes.pop(); // c
        a11y.nodes.pop(); // b

        let update = a11y.end_frame(Default::default());
        assert_eq!(update.focus, a);
    }
}
