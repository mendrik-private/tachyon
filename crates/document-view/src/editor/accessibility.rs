//! Canonical accessibility publication, independent of the virtual paint window.
//!
//! Geometry and labels are retained until the immutable line allocation changes.
//! Scrolling changes one ancestor transform, not every descendant's geometry.
//! No offscreen GPUI controls, text shaping, or document transactions are created.

use std::{rc::Rc, sync::Weak};

use gpui::accesskit::{Action, Affine, Node, NodeId as AccessibleId, Rect};

use super::*;

#[derive(Default)]
pub(super) struct SemanticCache {
    geometry: Weak<Vec<VisualLineSpec>>,
    width: f32,
    tree: Option<Rc<SemanticTree>>,
    revision: u64,
    native: Rc<RefCell<Option<NativeTree>>>,
}

impl SemanticCache {
    pub(super) fn current(&self) -> Option<Rc<SemanticTree>> {
        self.tree.clone()
    }

    pub(super) fn get(
        &mut self,
        projection: &TextProjection,
        lines: &Arc<Vec<VisualLineSpec>>,
        width: f32,
    ) -> Rc<SemanticTree> {
        let identity = Arc::downgrade(lines);
        if self.geometry.ptr_eq(&identity)
            && self.width == width
            && let Some(tree) = &self.tree
        {
            return tree.clone();
        }
        let bounds = semantic_bounds_for_lines(projection, lines, 0..lines.len());
        let html_labels: HashMap<_, _> = lines.iter().filter_map(|line| {
            let preview = line.html_preview.as_ref()?;
            let node = segment_for_line(projection, &line.projected_range())?.node_id;
            matches!(projection.block(node), Some(BlockNode::PreservedSource { source, .. }) if source == &preview.source)
                .then_some((node, preview.accessible_text.as_str()))
        }).collect();
        let math_markup: HashMap<_, _> = lines
            .iter()
            .filter_map(|line| {
                let markup = line.display_math.as_ref()?.light.semantics.clone()?;
                let node = segment_for_line(projection, &line.projected_range())?.node_id;
                Some((node, markup))
            })
            .collect();
        let mut section_heading = None;
        let diagrams: HashMap<_, _> = lines
            .iter()
            .filter_map(|line| {
                let diagram = line.diagram.as_ref()?;
                let node = segment_for_line(projection, &line.projected_range())?.node_id;
                (!diagram.source_visible).then_some((node, diagram.light.description.as_str()))
            })
            .collect();
        let margin_notes = projection
            .segments()
            .iter()
            .filter(|segment| segment.context.margin_note_anchor.is_some())
            .map(|segment| segment.top_level_node_id)
            .collect::<HashSet<_>>();
        let specs = projection
            .roots()
            .filter_map(|block| semantic_block_in_section(block, &bounds, &mut section_heading))
            .map(|mut spec| {
                apply_html_labels(&mut spec, projection, &html_labels);
                apply_math_markup(&mut spec, &math_markup);
                apply_diagram_labels(&mut spec, &diagrams);
                apply_bibliography_roles(&mut spec, projection);
                if margin_notes.contains(&spec.node_id) {
                    spec.role = Role::Note;
                }
                if projection.segment_for_node(spec.node_id).is_some_and(|s| {
                    s.context
                        .figure_text
                        .is_some_and(|(_, role)| role.is_caption())
                }) {
                    spec.role = Role::FigureCaption;
                }
                spec
            })
            .collect();
        let mut authored_descriptions: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        let images = projection
            .roots()
            .filter(|root| matches!(root, BlockNode::Image(_)))
            .map(BlockNode::id)
            .collect::<Vec<_>>();
        let image_ordinals = images
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect::<HashMap<_, _>>();
        for segment in projection.segments() {
            if let Some(anchor) = segment.context.margin_note_anchor {
                authored_descriptions
                    .entry(anchor)
                    .or_default()
                    .push(segment.node_id);
            }
            if let Some((image, role)) = segment.context.figure_text {
                if let Some(first) = role.gallery_start() {
                    if let Some((start, end)) =
                        image_ordinals.get(&first).zip(image_ordinals.get(&image))
                    {
                        for member in &images[*start..=*end] {
                            authored_descriptions
                                .entry(*member)
                                .or_default()
                                .push(segment.node_id);
                        }
                    }
                    continue;
                }
                authored_descriptions
                    .entry(image)
                    .or_default()
                    .push(segment.node_id);
            }
        }
        self.revision = self.revision.wrapping_add(1).max(1);
        let tree = Rc::new(SemanticTree {
            specs,
            document_text: projection.text().to_owned(),
            width,
            authored_descriptions,
            footnotes: super::footnotes::controls(projection, lines, width),
            revision: self.revision,
            native: self.native.clone(),
        });
        self.geometry = identity;
        self.width = width;
        self.tree = Some(tree.clone());
        tree
    }
}

fn apply_diagram_labels(spec: &mut SemanticNodeSpec, descriptions: &HashMap<NodeId, &str>) {
    if let Some(description) = descriptions.get(&spec.node_id) {
        spec.role = Role::Figure;
        spec.label = (*description).to_owned();
    }
    for child in &mut spec.children {
        apply_diagram_labels(child, descriptions);
    }
}

fn apply_bibliography_roles(spec: &mut SemanticNodeSpec, projection: &TextProjection) {
    // Keep a numbered/bulleted list's parent and child structure intact. The
    // entry is its canonical list item, not an extra nested list-item wrapper.
    let citation = |id| {
        projection
            .segment_for_node(id)
            .is_some_and(|s| s.context.bibliography.is_some())
    };
    if let Some(segment) = projection.segment_for_node(spec.node_id)
        && segment.context.bibliography.is_some()
    {
        // A link embedded in a citation is not a whole-entry resource action.
        // Native pointer/keyboard link handling still addresses its source run.
        spec.resource_link = None;
        if segment.context.list_depth > 0 {
            spec.role = Role::Paragraph;
        }
    }
    if spec.role == Role::ListItem && spec.children.iter().any(|child| citation(child.node_id))
        || citation(spec.node_id)
            && projection
                .segment_for_node(spec.node_id)
                .is_some_and(|s| s.context.list_depth == 0)
    {
        spec.role = Role::DocBiblioEntry;
    }
    for child in &mut spec.children {
        apply_bibliography_roles(child, projection);
    }
}

fn apply_math_markup(
    spec: &mut SemanticNodeSpec,
    markup: &HashMap<NodeId, Arc<crate::math::semantics::MathMarkup>>,
) {
    if spec.role == Role::Math {
        spec.math_markup = markup.get(&spec.node_id).cloned();
    }
    for child in &mut spec.children {
        apply_math_markup(child, markup);
    }
}

/// Recompute aggregate names too: a list item/cell/quote must not leak the
/// parser's unfiltered HTML description through its parent accessible name.
fn apply_html_labels(
    spec: &mut SemanticNodeSpec,
    projection: &TextProjection,
    labels: &HashMap<NodeId, &str>,
) -> bool {
    if let Some(BlockNode::PreservedSource { source, .. }) = projection.block(spec.node_id) {
        spec.label = labels
            .get(&spec.node_id)
            .copied()
            .unwrap_or(source)
            .to_owned();
        return true;
    }
    let mut changed = false;
    if let Some(text) = projection.block(spec.node_id).and_then(BlockNode::text) {
        let mut label = text.as_string();
        for run in text.runs().iter().rev() {
            if let Some(number) = run.styles.iter().find_map(|style| match style {
                InlineStyle::FootnoteReference(label) => projection
                    .footnotes
                    .label(label)
                    .and_then(|note| note.number),
                _ => None,
            }) {
                label.replace_range(run.range.clone(), &format!(" [footnote {number}]"));
                changed = true;
            }
        }
        if changed {
            spec.label = label;
        }
    }
    for child in &mut spec.children {
        changed |= apply_html_labels(child, projection, labels);
    }
    if changed
        && matches!(
            spec.role,
            Role::ListItem
                | Role::CheckBox
                | Role::Cell
                | Role::ColumnHeader
                | Role::Blockquote
                | Role::Alert
                | Role::Note
        )
    {
        spec.label = spec
            .children
            .iter()
            .map(|child| child.label.as_str())
            .collect::<Vec<_>>()
            .join("\n");
    }
    if let Some(note) = projection.footnotes.definition(spec.node_id)
        && let Some(number) = note.number
    {
        spec.label = format!("Footnote {number}. {}", spec.label);
    }
    changed
}

pub(super) struct SemanticTree {
    specs: Vec<SemanticNodeSpec>,
    document_text: String,
    width: f32,
    authored_descriptions: HashMap<NodeId, Vec<NodeId>>,
    footnotes: HashMap<NodeId, Vec<super::footnotes::Control>>,
    revision: u64,
    native: Rc<RefCell<Option<NativeTree>>>,
}

struct NativeTree {
    scope: AccessibleId,
    revision: u64,
    subtree: gpui::RetainedA11ySubtree,
    pending_updates: Vec<(AccessibleId, Node)>,
    targets: ActionTargets,
    math_scroll_nodes: HashMap<AccessibleId, (NodeId, bool, Node)>,
    active_math_scroll_nodes: HashSet<AccessibleId>,
}

struct CompiledNativeRevision {
    document_id: AccessibleId,
    nodes: Vec<(AccessibleId, Node)>,
    targets: ActionTargets,
    math_scroll_nodes: HashMap<AccessibleId, (NodeId, bool, Node)>,
}

impl NativeTree {
    fn new(scope: AccessibleId, revision: u64, compiled: CompiledNativeRevision) -> Self {
        Self {
            scope,
            revision,
            subtree: gpui::RetainedA11ySubtree::new(vec![compiled.document_id], compiled.nodes)
                .expect("compiled semantic tree must have one valid canonical hierarchy"),
            pending_updates: Vec::new(),
            targets: compiled.targets,
            math_scroll_nodes: compiled.math_scroll_nodes,
            active_math_scroll_nodes: HashSet::new(),
        }
    }

    fn apply_revision(
        &mut self,
        revision: u64,
        compiled: CompiledNativeRevision,
    ) -> Result<(), CompiledNativeRevision> {
        let CompiledNativeRevision {
            document_id,
            nodes,
            targets,
            math_scroll_nodes,
        } = compiled;
        let pending_updates = nodes
            .iter()
            .filter(|(id, node)| self.subtree.node(*id).is_none_or(|old| old != node))
            .cloned()
            .collect();
        match self.subtree.revise_properties(nodes) {
            Ok(subtree) => {
                self.revision = revision;
                self.subtree = subtree;
                self.pending_updates = pending_updates;
                self.targets = targets;
                self.math_scroll_nodes = math_scroll_nodes;
                Ok(())
            }
            Err(nodes) => Err(CompiledNativeRevision {
                document_id,
                nodes,
                targets,
                math_scroll_nodes,
            }),
        }
    }
}

pub(super) type ActionTargets = Arc<HashMap<AccessibleId, ActionTarget>>;

#[derive(Clone, Copy)]
pub(super) struct ActionTarget {
    accessible_id: AccessibleId,
    node_id: NodeId,
    bounds: SemanticBounds,
    activation: Option<SemanticActivation>,
    math_scroll: bool,
    math_scroll_value: bool,
}

#[derive(Clone, Copy)]
enum SemanticActivation {
    ToggleTask,
    OpenImageLink,
    OpenResourceLink,
    Footnote(super::footnotes::Navigation),
}

impl SemanticTree {
    #[cfg(test)]
    pub(super) fn label_for_role(&self, role: Role) -> Option<&str> {
        self.specs
            .iter()
            .find(|spec| spec.role == role)
            .map(|spec| spec.label.as_str())
    }

    fn compile_native_revision(
        &self,
        builder: &gpui::A11ySubtreeBuilder,
        bounds: Bounds<Pixels>,
    ) -> CompiledNativeRevision {
        let mut nodes = Vec::new();
        let mut targets = Vec::new();
        let mut roots = Vec::new();
        for spec in &self.specs {
            roots.push(compile_node(
                spec,
                self.width,
                builder,
                &mut nodes,
                &mut targets,
                &self.footnotes,
            ));
        }

        // AT-SPI's Text interface is backed by AccessKit TextRun nodes. Keep
        // one canonical run ahead of the semantic object roots so text-range
        // detection remains constant-depth even for huge documents.
        let text_id = builder.synthetic_node_id("document-text-run");
        let mut text = Node::new(Role::TextRun);
        text.set_label("Tachyon retained document text");
        text.set_value(self.document_text.clone());
        text.set_character_lengths(
            self.document_text
                .chars()
                .map(|character| character.len_utf8() as u8)
                .collect::<Vec<_>>(),
        );
        text.set_bounds(Rect::new(
            0.,
            0.,
            f64::from(bounds.size.width),
            f64::from(bounds.size.height),
        ));
        nodes.push((text_id, text));
        roots.insert(0, text_id);

        // accesskit_consumer constructs both ends of its double-ended text
        // iterator up front. An empty final run bounds the reverse lookup.
        let text_end_id = builder.synthetic_node_id("document-text-end");
        let mut text_end = Node::new(Role::TextRun);
        text_end.set_value("");
        text_end.set_character_lengths(Vec::<u8>::new());
        text_end.set_bounds(Rect::new(
            0.,
            f64::from(bounds.size.height),
            f64::from(bounds.size.width),
            f64::from(bounds.size.height),
        ));
        nodes.push((text_end_id, text_end));
        roots.push(text_end_id);

        // The element above this subtree is the transparent geometry owner.
        // The Document stays beneath it with the complete canonical text and
        // all offscreen semantic objects.
        let document_id = builder.synthetic_node_id("document-semantic-root");
        let mut document = Node::new(Role::Document);
        document.set_bounds(Rect::new(
            0.,
            0.,
            f64::from(bounds.size.width),
            f64::from(bounds.size.height),
        ));
        document.set_children(roots);
        nodes.push((document_id, document));

        let descriptions = self
            .authored_descriptions
            .iter()
            .map(|(image, labels)| {
                (
                    builder.synthetic_node_id(image.get()),
                    labels
                        .iter()
                        .map(|id| builder.synthetic_node_id(id.get()))
                        .collect::<Vec<_>>(),
                )
            })
            .collect::<HashMap<_, _>>();
        let described_ids = descriptions
            .values()
            .flatten()
            .copied()
            .collect::<HashSet<_>>();
        let labels = nodes
            .iter()
            .filter(|(id, _)| described_ids.contains(id))
            .filter_map(|(id, node)| node.label().map(|text| (*id, text.to_owned())))
            .collect::<HashMap<_, _>>();
        for (id, node) in &mut nodes {
            if let Some(targets) = descriptions.get(id) {
                node.set_described_by(targets.clone());
                node.set_description(
                    targets
                        .iter()
                        .filter_map(|id| labels.get(id))
                        .map(String::as_str)
                        .collect::<Vec<_>>()
                        .join(" "),
                );
            }
        }

        let math_scroll_nodes = targets
            .iter()
            .filter(|target| target.math_scroll || target.math_scroll_value)
            .filter_map(|target| {
                let baseline = nodes
                    .iter()
                    .find(|(id, _)| *id == target.accessible_id)?
                    .1
                    .clone();
                Some((
                    target.accessible_id,
                    (target.node_id, target.math_scroll_value, baseline),
                ))
            })
            .collect();
        let targets = Arc::new(
            targets
                .into_iter()
                .map(|target| (target.accessible_id, target))
                .collect(),
        );
        CompiledNativeRevision {
            document_id,
            nodes,
            targets,
            math_scroll_nodes,
        }
    }

    pub(super) fn publish(
        &self,
        builder: &mut gpui::A11ySubtreeBuilder,
        bounds: Bounds<Pixels>,
        scale: f32,
        math_scroll_handles: &HashMap<NodeId, ScrollHandle>,
    ) -> ActionTargets {
        let scope = builder.synthetic_node_id("document-semantic-scope");
        let mut native = self.native.borrow_mut();
        if native
            .as_ref()
            .is_none_or(|tree| tree.scope != scope || tree.revision != self.revision)
        {
            let compiled = self.compile_native_revision(builder, bounds);
            match native.as_mut() {
                Some(tree) if tree.scope == scope => {
                    if let Err(compiled) = tree.apply_revision(self.revision, compiled) {
                        *tree = NativeTree::new(scope, self.revision, compiled);
                    }
                }
                _ => *native = Some(NativeTree::new(scope, self.revision, compiled)),
            }
        }

        let tree = native.as_mut().expect("compiled semantic tree");
        let subtree = &tree.subtree;
        let inserted = builder.push_retained_subtree(subtree);
        debug_assert!(inserted, "canonical semantic IDs must be unique");
        for (id, node) in tree.pending_updates.drain(..) {
            let updated = builder.update_retained_node(subtree, id, node);
            debug_assert!(updated, "semantic delta node must belong to retained tree");
        }

        let departed = tree
            .active_math_scroll_nodes
            .iter()
            .filter(|id| !tree.math_scroll_nodes.contains_key(id))
            .copied()
            .collect::<Vec<_>>();
        for id in departed {
            if let Some(baseline) = subtree.node(id) {
                let updated = builder.update_retained_node(subtree, id, baseline.clone());
                debug_assert!(updated, "departed math node must belong to retained tree");
            }
            tree.active_math_scroll_nodes.remove(&id);
        }
        for (id, (node_id, value_control, baseline)) in &tree.math_scroll_nodes {
            if let Some(handle) = math_scroll_handles.get(node_id) {
                let mut node = baseline.clone();
                let maximum = f64::from(handle.max_offset().x.max(px(0.)));
                let current = f64::from((-handle.offset().x).clamp(px(0.), px(maximum as f32)));
                node.set_scroll_x(current);
                node.set_scroll_x_min(0.);
                node.set_scroll_x_max(maximum);
                node.add_action(Action::ScrollLeft);
                node.add_action(Action::ScrollRight);
                node.add_action(Action::SetScrollOffset);
                if *value_control {
                    node.set_numeric_value(current);
                    node.set_min_numeric_value(0.);
                    node.set_max_numeric_value(maximum);
                    node.set_numeric_value_step(
                        f64::from(handle.bounds().size.width)
                            .mul_add(0.8, 0.)
                            .max(40.),
                    );
                    node.add_action(Action::Decrement);
                    node.add_action(Action::Increment);
                    node.add_action(Action::SetValue);
                }
                let updated = builder.update_retained_node(subtree, *id, node);
                debug_assert!(
                    updated,
                    "dynamic semantic node must belong to retained tree"
                );
                tree.active_math_scroll_nodes.insert(*id);
            } else if tree.active_math_scroll_nodes.remove(id) {
                let updated = builder.update_retained_node(subtree, *id, baseline.clone());
                debug_assert!(
                    updated,
                    "departed dynamic node must belong to retained tree"
                );
            }
        }

        let parent = builder.parent_node();
        parent.set_bounds(Rect::new(
            0.,
            0.,
            f64::from(bounds.size.width),
            f64::from(bounds.size.height),
        ));
        let scale = f64::from(scale);
        parent.set_transform(Affine::new([
            scale,
            0.,
            0.,
            scale,
            f64::from(bounds.origin.x) * scale,
            f64::from(bounds.origin.y) * scale,
        ]));
        tree.targets.clone()
    }
}

fn compile_node(
    spec: &SemanticNodeSpec,
    width: f32,
    builder: &gpui::A11ySubtreeBuilder,
    nodes: &mut Vec<(AccessibleId, Node)>,
    targets: &mut Vec<ActionTarget>,
    footnotes: &HashMap<NodeId, Vec<super::footnotes::Control>>,
) -> AccessibleId {
    let id = builder.synthetic_node_id(spec.node_id.get());
    let mut node = Node::new(spec.role);
    node.set_label(spec.label.clone());
    if let Some(level) = spec.level {
        node.set_level(level);
    }
    if let Some(row) = spec.row_index {
        node.set_row_index(row);
    }
    if let Some(column) = spec.column_index {
        node.set_column_index(column);
    }
    if matches!(spec.role, Role::Cell | Role::ColumnHeader) {
        // Markdown tables have no authored row/column spans. Publishing the
        // exact unit spans keeps the native table model explicit even on
        // adapters that currently expose only the semantic hierarchy.
        node.set_row_span(1);
        node.set_column_span(1);
    }
    if let Some(toggled) = spec.toggled {
        node.set_toggled(toggled);
    }
    if spec.role == Role::Table {
        node.set_row_count(spec.children.len());
        node.set_column_count(
            spec.children
                .iter()
                .map(|row| row.children.len())
                .max()
                .unwrap_or(0),
        );
    }
    let b = spec.bounds;
    node.set_bounds(Rect::new(
        f64::from(b.x_fraction * width),
        f64::from(b.y),
        f64::from((b.x_fraction + b.width_fraction) * width),
        f64::from(b.y + b.height.max(1.)),
    ));
    node.add_action(Action::ScrollIntoView);
    let checkbox = spec.role == Role::CheckBox;
    if let Some(target) = &spec.resource_link {
        node.set_url(target.clone());
    }
    if checkbox || spec.resource_link.is_some() {
        node.add_action(Action::Click);
    }
    targets.push(ActionTarget {
        accessible_id: id,
        node_id: spec.node_id,
        bounds: b,
        activation: if checkbox {
            Some(SemanticActivation::ToggleTask)
        } else {
            spec.resource_link
                .as_ref()
                .map(|_| SemanticActivation::OpenResourceLink)
        },
        math_scroll: spec.role == Role::Math,
        math_scroll_value: spec.role == Role::Math,
    });
    let mut children = spec
        .children
        .iter()
        .map(|child| compile_node(child, width, builder, nodes, targets, footnotes))
        .collect::<Vec<_>>();
    for (ordinal, control) in footnotes
        .get(&spec.node_id)
        .into_iter()
        .flatten()
        .enumerate()
    {
        let control_id =
            builder.synthetic_node_id(("footnote-control", spec.node_id.get(), ordinal));
        let mut link = Node::new(Role::Link);
        link.set_label(control.label.clone());
        let bounds = control.bounds;
        link.set_bounds(Rect::new(
            f64::from(bounds.x_fraction * width),
            f64::from(bounds.y),
            f64::from((bounds.x_fraction + bounds.width_fraction) * width),
            f64::from(bounds.y + bounds.height),
        ));
        link.add_action(Action::Click);
        link.add_action(Action::ScrollIntoView);
        targets.push(ActionTarget {
            accessible_id: control_id,
            node_id: spec.node_id,
            bounds,
            activation: Some(SemanticActivation::Footnote(control.action)),
            math_scroll: false,
            math_scroll_value: false,
        });
        nodes.push((control_id, link));
        children.push(control_id);
    }
    node.set_children(children);
    if let Some(markup) = &spec.math_markup {
        node.set_html_tag("math");
        node.set_inner_html(markup.xml());
        let mut ordinal = 0;
        node.set_children(vec![compile_math_markup(
            markup,
            spec.node_id,
            &mut ordinal,
            builder,
            nodes,
        )]);
    }
    if let Some(target) = &spec.image_link {
        // The canonical image keeps its identity and description. The authored
        // link is a semantic parent, not a second rendered figure or caption.
        let link_id = builder.synthetic_node_id(("image-link", spec.node_id.get()));
        let mut link = Node::new(Role::Link);
        link.set_url(target.clone());
        link.set_labelled_by(vec![id]);
        link.set_bounds(node.bounds().expect("image bounds were assigned"));
        link.set_children(vec![id]);
        link.add_action(Action::Click);
        link.add_action(Action::ScrollIntoView);
        targets.push(ActionTarget {
            accessible_id: link_id,
            node_id: spec.node_id,
            bounds: b,
            activation: Some(SemanticActivation::OpenImageLink),
            math_scroll: false,
            math_scroll_value: false,
        });
        nodes.push((id, node));
        nodes.push((link_id, link));
        link_id
    } else {
        nodes.push((id, node));
        id
    }
}

fn compile_math_markup(
    markup: &crate::math::semantics::MathMarkup,
    owner: NodeId,
    ordinal: &mut usize,
    builder: &gpui::A11ySubtreeBuilder,
    nodes: &mut Vec<(AccessibleId, Node)>,
) -> AccessibleId {
    let id = builder.synthetic_node_id(("math-markup", owner.get(), *ordinal));
    *ordinal += 1;
    let mut node = math_markup_node(markup);
    node.set_children(
        markup
            .children
            .iter()
            .map(|child| compile_math_markup(child, owner, ordinal, builder, nodes))
            .collect::<Vec<_>>(),
    );
    nodes.push((id, node));
    id
}

fn math_markup_node(markup: &crate::math::semantics::MathMarkup) -> Node {
    let mut node = Node::new(if markup.text.is_empty() {
        Role::Group
    } else {
        Role::Label
    });
    node.set_html_tag(markup.tag);
    if !markup.text.is_empty() {
        // AccessKit derives a Label's native name from its text value. A
        // control label here leaves MathML tokens empty in the AT-SPI tree.
        node.set_value(markup.text.clone());
    }
    node
}

/// Resolve against the immutable target map belonging to the published tree.
/// Unknown targets and unsupported actions must reach GPUI's other handlers.
impl ActionTarget {
    fn supports(self, action: Action) -> bool {
        match action {
            Action::ScrollIntoView => true,
            Action::Click => self.activation.is_some(),
            Action::ScrollLeft | Action::ScrollRight | Action::SetScrollOffset => self.math_scroll,
            Action::Decrement | Action::Increment | Action::SetValue => self.math_scroll_value,
            _ => false,
        }
    }
}

pub(super) fn register_actions(
    targets: &ActionTargets,
    editor: &Entity<RichDocumentEditor>,
    window: &mut Window,
) {
    let targets = Arc::clone(targets);
    let entity = editor.downgrade();
    window.on_a11y_action_request(move |request, window, cx| {
        let Some(&target) = targets.get(&request.target_node) else {
            return false;
        };
        if !target.supports(request.action) {
            return false;
        }
        let _ = entity.update(cx, |editor, cx| match request.action {
            Action::ScrollIntoView => editor.reveal_semantic_bounds(target.bounds, cx),
            Action::Click => match target.activation {
                Some(SemanticActivation::ToggleTask) => {
                    editor.toggle_task(target.node_id, window, cx)
                }
                Some(SemanticActivation::OpenImageLink) => {
                    editor.open_semantic_image_link(target.node_id, window, cx)
                }
                Some(SemanticActivation::OpenResourceLink) => {
                    editor.open_semantic_resource_link(target.node_id, window, cx)
                }
                Some(SemanticActivation::Footnote(action)) => {
                    editor.navigate_footnote(action, window, cx)
                }
                None => {}
            },
            Action::ScrollLeft | Action::Decrement => {
                editor.scroll_math_accessibly(target.node_id, -1., cx)
            }
            Action::ScrollRight | Action::Increment => {
                editor.scroll_math_accessibly(target.node_id, 1., cx)
            }
            Action::SetScrollOffset => {
                if let Some(gpui::accesskit::ActionData::SetScrollOffset(offset)) = &request.data {
                    editor.set_accessible_math_scroll(target.node_id, offset.x as f32, cx);
                }
            }
            Action::SetValue => {
                if let Some(gpui::accesskit::ActionData::NumericValue(value)) = &request.data {
                    editor.set_accessible_math_scroll(target.node_id, *value as f32, cx);
                }
            }
            _ => {}
        });
        true
    });
}

impl RichDocumentEditor {
    pub(super) fn scroll_math_accessibly(
        &mut self,
        node_id: NodeId,
        direction: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.math_scroll_handles.get(&node_id).cloned() else {
            return;
        };
        let current = f32::from(-handle.offset().x);
        let step = (f32::from(handle.bounds().size.width) * 0.8).max(40.);
        self.set_accessible_math_scroll(node_id, current + direction * step, cx);
    }

    pub(super) fn set_accessible_math_scroll(
        &mut self,
        node_id: NodeId,
        requested: f32,
        cx: &mut Context<Self>,
    ) {
        let Some(handle) = self.math_scroll_handles.get(&node_id).cloned() else {
            return;
        };
        let maximum = f32::from(handle.max_offset().x).max(0.);
        let next = requested.clamp(0., maximum);
        let offset = handle.offset();
        if (f32::from(-offset.x) - next).abs() < f32::EPSILON {
            return;
        }
        self.stop_momentum();
        handle.set_offset(point(px(-next), offset.y));
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
    }

    pub(super) fn open_semantic_image_link(
        &mut self,
        node: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Resolve the live canonical node at activation, never a target captured
        // by a retained geometry tree before editing/undo replaced its content.
        let target = match self.document.snapshot().node(node) {
            Some(BlockNode::Image(image)) => image.link.as_ref().map(|link| link.target.0.clone()),
            _ => None,
        };
        if let Some(target) = target {
            self.open_link_target(&target, window, cx);
        }
    }

    fn open_semantic_resource_link(
        &mut self,
        node: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target = match self.document.snapshot().node(node) {
            Some(BlockNode::Paragraph(p)) => {
                crate::adaptive::resource::target(p).map(str::to_owned)
            }
            _ => None,
        };
        if let Some(target) = target {
            self.open_link_target(&target, window, cx);
        }
    }

    fn reveal_semantic_bounds(&mut self, bounds: SemanticBounds, cx: &mut Context<Self>) {
        let (current, viewport) = self.scroll_metrics();
        let margin = 12.;
        if bounds.y >= current && bounds.y + bounds.height <= current + viewport {
            return;
        }
        self.stop_momentum();
        self.jump_generation = self.jump_generation.saturating_add(1);
        let maximum = f32::from(self.scroll_handle.max_offset().y).max(0.);
        let y = (bounds.y - margin).clamp(0., maximum);
        let x = self.scroll_handle.offset().x;
        self.scroll_handle.set_offset(point(x, px(-y)));
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
    }

    pub(super) fn toggle_task(
        &mut self,
        item_id: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.stop_momentum();
        match self.apply_command(EditCommand::ToggleTask { item_id }) {
            Ok(result) => {
                self.refresh_after_transaction(&result);
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Err(error) => self.record_error(error, window),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn margin_note_descriptions_attach_to_the_source_paragraph_without_duplicate_nodes() {
        let document = Document::from_markdown(include_str!(
            "../../../../performance/layout-fixtures/101-margin-notes.md"
        ))
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = Arc::new(build_visual_lines_with_images(
            &projection,
            &HashMap::new(),
            1280.,
        ));
        let tree = SemanticCache::default().get(&projection, &lines, 1280.);
        assert_eq!(tree.authored_descriptions.len(), 2);
        assert_eq!(
            tree.specs
                .iter()
                .filter(|spec| spec.role == Role::Note)
                .count(),
            2
        );
        let mut count = 0;
        for segment in projection
            .segments()
            .iter()
            .filter(|s| s.context.margin_note_anchor.is_some())
        {
            count += 1;
            assert!(
                tree.authored_descriptions[&segment.context.margin_note_anchor.unwrap()]
                    .contains(&segment.node_id)
            );
            fn occurrences(specs: &[SemanticNodeSpec], id: NodeId) -> usize {
                specs
                    .iter()
                    .map(|s| usize::from(s.node_id == id) + occurrences(&s.children, id))
                    .sum()
            }
            assert_eq!(occurrences(&tree.specs, segment.node_id), 1);
        }
        assert_eq!(count, 3);
    }

    #[test]
    fn a_margin_note_cluster_describes_one_anchor_in_source_order() {
        let document = Document::from_markdown(include_str!(
            "../../../../performance/layout-fixtures/110-margin-note-cluster.md"
        ))
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let notes = projection
            .segments()
            .iter()
            .filter(|s| s.context.margin_note_anchor.is_some())
            .collect::<Vec<_>>();
        assert_eq!(notes.len(), 3);
        for width in [400., 1280.] {
            let lines = Arc::new(build_visual_lines_with_images(
                &projection,
                &HashMap::new(),
                width,
            ));
            let tree = SemanticCache::default().get(&projection, &lines, width);
            assert_eq!(tree.authored_descriptions.len(), 1);
            assert_eq!(
                tree.authored_descriptions[&notes[0].context.margin_note_anchor.unwrap()],
                notes.iter().map(|s| s.node_id).collect::<Vec<_>>()
            );
            let note_specs = tree
                .specs
                .iter()
                .filter(|spec| spec.role == Role::Note)
                .collect::<Vec<_>>();
            assert_eq!(note_specs.len(), 3);
            for (spec, note) in note_specs.iter().zip(&notes) {
                assert_eq!(spec.children.len(), 1);
                assert_eq!(spec.children[0].node_id, note.node_id);
            }
        }
    }

    #[test]
    fn shared_gallery_semantics_describe_all_images_without_duplicate_text_nodes() {
        let document = Document::from_markdown(include_str!(
            "../../../../performance/layout-fixtures/98-shared-gallery-captions.md"
        ))
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = Arc::new(build_visual_lines_with_images(
            &projection,
            &HashMap::new(),
            1280.,
        ));
        let tree = SemanticCache::default().get(&projection, &lines, 1280.);
        let images = projection
            .roots()
            .filter(|root| matches!(root, BlockNode::Image(_)))
            .map(BlockNode::id)
            .collect::<Vec<_>>();
        assert_eq!(images.len(), 5);
        assert_eq!(
            tree.authored_descriptions[&images[0]],
            tree.authored_descriptions[&images[1]]
        );
        assert_eq!(tree.authored_descriptions[&images[0]].len(), 2);
        for image in &images[2..4] {
            assert_eq!(tree.authored_descriptions[image].len(), 3);
        }
        assert_eq!(
            &tree.authored_descriptions[&images[2]][1..],
            &tree.authored_descriptions[&images[3]][1..]
        );
        assert!(!tree.authored_descriptions.contains_key(&images[4]));
        for segment in projection.segments().iter().filter(|s| {
            s.context
                .figure_text
                .is_some_and(|(_, role)| role.gallery_start().is_some())
        }) {
            let specs = tree
                .specs
                .iter()
                .filter(|s| s.node_id == segment.node_id)
                .collect::<Vec<_>>();
            assert_eq!(specs.len(), 1);
            assert_eq!(
                specs[0].role == Role::FigureCaption,
                segment.context.figure_text.unwrap().1.is_caption()
            );
        }
    }

    #[test]
    fn bibliography_semantics_keep_entry_order_without_whole_citation_link_actions() {
        let document = Document::from_markdown(include_str!(
            "../../../../performance/layout-fixtures/71-bibliography.md"
        ))
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = Arc::new(build_visual_lines_with_images(
            &projection,
            &HashMap::new(),
            1280.,
        ));
        let tree = SemanticCache::default().get(&projection, &lines, 1280.);
        fn visit(spec: &SemanticNodeSpec, entries: &mut Vec<String>) {
            if spec.role == Role::DocBiblioEntry {
                assert!(spec.resource_link.is_none());
                entries.push(spec.label.clone());
                for child in &spec.children {
                    assert_eq!(child.role, Role::Paragraph);
                    assert!(child.resource_link.is_none());
                }
            }
            for child in &spec.children {
                visit(child, entries);
            }
        }
        let mut entries = Vec::new();
        for spec in &tree.specs {
            visit(spec, &mut entries);
        }
        assert_eq!(entries.len(), 8);
        for (entry, author) in entries.iter().zip([
            "Vale,", "Ibarra,", "Okafor,", "Rowan,", "Moreno,", "Ellis,", "Patel,", "Kim,",
        ]) {
            assert!(entry.starts_with(author));
        }
    }
    use crate::init_editor;

    #[gpui::test]
    fn footnote_semantics_publish_source_order_controls_and_live_edited_note_labels(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "# References\n\nA claim[^source] and the same source[^source].\n\n[^source]: Read this evidence.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.refresh_projection();
                let mut cache = SemanticCache::default();
                let before = cache.get(
                    &editor.projection,
                    &editor.visual_lines,
                    editor.layout_width,
                );
                assert_eq!(before.footnotes.values().map(Vec::len).sum::<usize>(), 3);
                assert!(
                    before
                        .specs
                        .iter()
                        .any(|spec| spec.label.contains("[footnote 1]"))
                );
                assert!(
                    !before
                        .specs
                        .iter()
                        .any(|spec| spec.label.contains("[^source]"))
                );
                let note = editor.projection.footnotes.label("source").unwrap().clone();
                assert!(
                    before
                        .specs
                        .iter()
                        .any(|spec| spec.node_id == note.definition
                            && spec.role == Role::Note
                            && spec.label == "Footnote 1. Read this evidence.")
                );
                editor.navigate_footnote(
                    super::super::footnotes::Navigation::Definition(note.definition),
                    window,
                    cx,
                );
                let start = editor.cursor_offset();
                editor.replace_range(start..start, "Verified. ", true, window, cx);
                let after = cache.get(
                    &editor.projection,
                    &editor.visual_lines,
                    editor.layout_width,
                );
                assert!(
                    after
                        .specs
                        .iter()
                        .any(|spec| spec.node_id == note.definition
                            && spec.label == "Footnote 1. Verified. Read this evidence.")
                );
                assert!(!Rc::ptr_eq(&before, &after));
                assert!(
                    Rc::ptr_eq(
                        &after,
                        &cache.get(
                            &editor.projection,
                            &editor.visual_lines,
                            editor.layout_width
                        )
                    ),
                    "scrolling must retain the semantic publication"
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            })
        });
    }

    #[test]
    fn resource_semantics_publish_one_destination_and_preserve_full_descriptions() {
        let source = "[Handbook](guide.md): The complete description.\n\n[Compact](other.md)\n\n[One](one.md) and [two](two.md).\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = Arc::new(build_visual_lines(&document, &projection));
        let tree = SemanticCache::default().get(&projection, &lines, 760.);
        assert_eq!(tree.specs[0].resource_link.as_deref(), Some("guide.md"));
        assert_eq!(tree.specs[0].label, "Handbook: The complete description.");
        assert_eq!(tree.specs[1].resource_link.as_deref(), Some("other.md"));
        assert!(
            tree.specs[2].resource_link.is_none(),
            "never choose an arbitrary destination from a paragraph"
        );
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn math_tokens_use_native_text_values_without_changing_formula_source() {
        let document = Document::from_markdown(include_str!(
            "../../../../performance/layout-fixtures/40-accessible-math.md"
        ))
        .unwrap();
        let original = document.snapshot().serialize().unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = Arc::new(build_visual_lines(&document, &projection));
        let tree = SemanticCache::default().get(&projection, &lines, 760.);
        let mut tokens = Vec::new();
        fn visit(markup: &crate::math::semantics::MathMarkup, tokens: &mut Vec<String>) {
            let node = math_markup_node(markup);
            assert_eq!(node.html_tag(), Some(markup.tag));
            if !markup.text.is_empty() {
                assert_eq!(node.role(), Role::Label);
                assert_eq!(node.value(), Some(markup.text.as_str()));
                tokens.push(markup.text.clone());
            }
            for child in &markup.children {
                visit(child, tokens);
            }
        }
        let formulas: Vec<_> = tree.specs.iter().filter(|s| s.role == Role::Math).collect();
        assert_eq!(formulas.len(), 6);
        for formula in formulas
            .iter()
            .filter(|formula| formula.math_markup.is_some())
        {
            visit(
                formula.math_markup.as_ref().expect("rendered MathML"),
                &mut tokens,
            );
        }
        assert!(tokens.windows(2).any(|pair| pair == ["37", "41"]));
        assert!(tokens.iter().any(|token| token == "∑"));
        assert!(formulas.last().unwrap().math_markup.is_none());
        assert_eq!(document.snapshot().serialize().unwrap(), original);
    }

    #[gpui::test]
    fn gallery_link_semantics_activate_live_canonical_target_without_content_edits(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let original = "# Start\n\n[![Linked description](first.png)](#destination)\n\n![Unlinked description](second.png)\n\n## Destination\n\nTarget paragraph.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(original).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let tree = editor.semantic_cache.get(
                    &editor.projection,
                    &editor.visual_lines,
                    editor.layout_width,
                );
                let images: Vec<_> = tree
                    .specs
                    .iter()
                    .filter(|spec| spec.role == Role::Image)
                    .collect();
                assert_eq!(images.len(), 2);
                assert_eq!(images[0].label, "Linked description");
                assert_eq!(images[0].image_link.as_deref(), Some("#destination"));
                assert_eq!(images[1].label, "Unlinked description");
                assert_eq!(images[1].image_link, None);
                let image = images[0].node_id;
                editor.open_semantic_image_link(image, window, cx);
                assert!(
                    editor.projection.text()[editor.cursor_offset()..].starts_with("Destination")
                );
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);

                let transaction = editor
                    .apply_command(EditCommand::DeleteBlock { node_id: image })
                    .unwrap();
                editor.refresh_after_transaction(&transaction);
                let after_delete = editor.document.snapshot().serialize().unwrap();
                let selection = editor.selection.clone();
                editor.open_semantic_image_link(image, window, cx);
                assert_eq!(
                    editor.selection, selection,
                    "stale native target must not navigate"
                );
                assert_eq!(
                    editor.document.snapshot().serialize().unwrap(),
                    after_delete
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
                editor.open_semantic_image_link(image, window, cx);
                assert!(
                    editor.projection.text()[editor.cursor_offset()..].starts_with("Destination")
                );
            });
        });
    }

    #[test]
    fn html_semantics_expose_original_readable_content_and_exclude_closed_bodies() {
        let document = Document::from_markdown(include_str!(
            "../../../../performance/layout-fixtures/28-accessible-html.md"
        ))
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = Arc::new(build_visual_lines(&document, &projection));
        assert!(
            lines.iter().any(|line| line.html_preview.is_some()),
            "HTML fixture must exercise the actual Blitz preview"
        );
        let mut cache = SemanticCache::default();
        let tree = cache.get(&projection, &lines, 760.);
        let labels = tree
            .specs
            .iter()
            .map(|node| node.label.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        assert!(
            labels
                .contains("Visible HTML body marker with strong emphasis and ordinary rich text."),
            "{labels}"
        );
        assert!(labels.contains("Unicode marker: 東京 café 🦀."));
        assert!(labels.contains("Open HTML body marker."));
        assert!(labels.contains("Closed HTML disclosure"));
        for hidden in [
            "Closed HTML body marker.",
            "Nested hidden body marker.",
            "Display-none body marker.",
            "Visibility-hidden body marker.",
        ] {
            assert!(
                !labels.contains(hidden),
                "inaccessible/closed content leaked: {hidden}; {labels}"
            );
        }
        assert!(
            labels.contains("<custom-widget>Unknown original HTML source marker.</custom-widget>")
        );
    }

    #[test]
    fn nested_html_names_refresh_on_disclosure_changes_without_source_edits() {
        let original = "> <details><summary>Nested summary</summary><p>Hidden body</p></details>\n\n<custom-widget>\nOriginal unsupported source\n</custom-widget>\n";
        let document = Document::from_markdown(original).unwrap();
        let mut projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = Arc::new(build_visual_lines(&document, &projection));
        let mut cache = SemanticCache::default();
        let closed = cache.get(&projection, &lines, 760.);
        assert_eq!(closed.specs[0].label, "Nested summary");
        assert_eq!(closed.specs[0].children[0].label, "Nested summary");
        assert!(
            closed.specs[1]
                .label
                .contains("<custom-widget>\nOriginal unsupported source")
        );
        assert!(Rc::ptr_eq(&closed, &cache.get(&projection, &lines, 760.)));
        let line = lines
            .iter()
            .find(|line| line.html_preview.is_some())
            .unwrap();
        let node = segment_for_line(&projection, &line.projected_range())
            .unwrap()
            .node_id;
        Arc::make_mut(&mut projection.html_disclosures).insert(
            node,
            crate::html::DisclosureState {
                source: line.html_preview.as_ref().unwrap().source.clone(),
                overrides: [(0, true)].into(),
            },
        );
        let opened_lines = Arc::new(build_visual_lines(&document, &projection));
        let opened = cache.get(&projection, &opened_lines, 760.);
        assert_eq!(opened.specs[0].label, "Nested summary\nHidden body");
        assert_eq!(
            opened.specs[0].children[0].label,
            "Nested summary\nHidden body"
        );
        assert!(!Rc::ptr_eq(&closed, &opened));
        assert_eq!(
            closed.specs[0].label, "Nested summary",
            "retained old worker tree is immutable"
        );
        assert_eq!(document.snapshot().serialize().unwrap(), original);
    }

    #[gpui::test]
    fn accessibility_reveal_preserves_selection_and_task_action_uses_content_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(include_str!(
                    "../../../../performance/layout-fixtures/27-accessible-document.md"
                ))
                .unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            let _ = window.draw(cx);
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let original = editor.document.snapshot().serialize().unwrap();
                let selection = editor.selection.clone();
                let tree = editor.semantic_cache.get(
                    &editor.projection,
                    &editor.visual_lines,
                    editor.layout_width,
                );
                let last = tree.specs.last().unwrap().bounds;
                editor.reveal_semantic_bounds(last, cx);
                assert!(editor.scroll_metrics().0 > 0.);
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
                let task = tree
                    .specs
                    .iter()
                    .find(|spec| spec.role == Role::List && spec.children[0].role == Role::CheckBox)
                    .unwrap()
                    .children[0]
                    .node_id;
                editor.toggle_task(task, window, cx);
                assert_eq!(
                    editor.document.snapshot().serialize().unwrap(),
                    original.replace("- [ ] Unchecked native task", "- [x] Unchecked native task")
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
            })
        });
    }

    #[test]
    fn canonical_semantics_include_offscreen_nodes_and_reuse_only_unchanged_geometry() {
        let document = Document::from_markdown(include_str!(
            "../../../../performance/layout-fixtures/27-accessible-document.md"
        ))
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let mut lines = Arc::new(build_visual_lines(&document, &projection));
        let mut cache = SemanticCache::default();
        assert!(cache.current().is_none());
        let tree = cache.get(&projection, &lines, 760.);
        assert_eq!(tree.document_text, projection.text());
        assert!(Rc::ptr_eq(&tree, &cache.current().unwrap()));
        let headings = tree
            .specs
            .iter()
            .filter(|spec| spec.role == Role::Heading)
            .map(|spec| spec.label.as_str())
            .collect::<Vec<_>>();
        assert_eq!(headings.last(), Some(&"7. Final offscreen heading"));
        assert_eq!(headings.len(), 10);
        assert!(tree.specs.last().unwrap().bounds.y > 1000.);
        assert!(Rc::ptr_eq(&tree, &cache.get(&projection, &lines, 760.)));
        assert!(!Rc::ptr_eq(&tree, &cache.get(&projection, &lines, 761.)));
        let before_edit = cache.get(&projection, &lines, 760.);
        // Arc::make_mut dissociates the cache's Weak even with no worker owner.
        Arc::make_mut(&mut lines).last_mut().unwrap().y += 10.;
        let after_edit = cache.get(&projection, &lines, 760.);
        assert!(!Rc::ptr_eq(&before_edit, &after_edit));
        assert!(
            Rc::ptr_eq(&before_edit.native, &after_edit.native),
            "semantic revisions must keep one retained native publication owner"
        );
        assert_ne!(before_edit.revision, after_edit.revision);
        assert!(
            after_edit.specs.last().unwrap().bounds.y > before_edit.specs.last().unwrap().bounds.y
        );
    }

    #[test]
    fn native_semantic_revision_sends_properties_and_rejects_topology_changes() {
        let root_id = AccessibleId(1);
        let text_id = AccessibleId(2);
        let mut root = Node::new(Role::Document);
        root.set_children([text_id]);
        let mut text = Node::new(Role::TextRun);
        text.set_value("before");
        let baseline = CompiledNativeRevision {
            document_id: root_id,
            nodes: vec![(root_id, root), (text_id, text)],
            targets: ActionTargets::default(),
            math_scroll_nodes: HashMap::new(),
        };
        let mut native = NativeTree::new(AccessibleId(9), 1, baseline);

        let mut revised_root = Node::new(Role::Document);
        revised_root.set_children([text_id]);
        let mut revised_text = Node::new(Role::TextRun);
        revised_text.set_value("after");
        assert!(
            native
                .apply_revision(
                    2,
                    CompiledNativeRevision {
                        document_id: root_id,
                        nodes: vec![(root_id, revised_root), (text_id, revised_text)],
                        targets: ActionTargets::default(),
                        math_scroll_nodes: HashMap::new(),
                    },
                )
                .is_ok()
        );
        assert_eq!(native.revision, 2);
        assert_eq!(native.pending_updates.len(), 1);
        assert_eq!(
            native.subtree.node(text_id).and_then(Node::value),
            Some("after")
        );

        let mut structural_root = Node::new(Role::Document);
        structural_root.set_children([text_id, AccessibleId(3)]);
        let mut structural_text = Node::new(Role::TextRun);
        structural_text.set_value("after");
        let structural = native.apply_revision(
            3,
            CompiledNativeRevision {
                document_id: root_id,
                nodes: vec![
                    (root_id, structural_root),
                    (text_id, structural_text),
                    (AccessibleId(3), Node::new(Role::Paragraph)),
                ],
                targets: ActionTargets::default(),
                math_scroll_nodes: HashMap::new(),
            },
        );
        assert!(structural.is_err());
        assert_eq!(native.revision, 2);
    }
}
