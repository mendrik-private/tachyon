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
}

impl SemanticCache {
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
            let node = segment_for_line(projection, &line.range)?.node_id;
            matches!(projection.block(node), Some(BlockNode::PreservedSource { source, .. }) if source == &preview.source)
                .then_some((node, preview.accessible_text.as_str()))
        }).collect();
        let math_markup: HashMap<_, _> = lines
            .iter()
            .filter_map(|line| {
                let markup = line.display_math.as_ref()?.light.semantics.clone()?;
                let node = segment_for_line(projection, &line.range)?.node_id;
                Some((node, markup))
            })
            .collect();
        let specs = projection
            .roots()
            .filter_map(|block| semantic_block(block, &bounds))
            .map(|mut spec| {
                apply_html_labels(&mut spec, projection, &html_labels);
                apply_math_markup(&mut spec, &math_markup);
                spec
            })
            .collect();
        let tree = Rc::new(SemanticTree {
            specs,
            width,
            native: RefCell::new(None),
        });
        self.geometry = identity;
        self.width = width;
        self.tree = Some(tree.clone());
        tree
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
    changed
}

pub(super) struct SemanticTree {
    specs: Vec<SemanticNodeSpec>,
    width: f32,
    native: RefCell<Option<NativeTree>>,
}

struct NativeTree {
    scope: AccessibleId,
    roots: Vec<AccessibleId>,
    nodes: Vec<(AccessibleId, Node)>,
    targets: Vec<ActionTarget>,
}

#[derive(Clone, Copy)]
pub(super) struct ActionTarget {
    accessible_id: AccessibleId,
    node_id: NodeId,
    bounds: SemanticBounds,
    activation: Option<SemanticActivation>,
}

#[derive(Clone, Copy)]
enum SemanticActivation {
    ToggleTask,
    OpenImageLink,
}

impl SemanticTree {
    pub(super) fn publish(
        &self,
        builder: &mut gpui::A11ySubtreeBuilder,
        bounds: Bounds<Pixels>,
        scale: f32,
    ) -> Vec<ActionTarget> {
        let scope = builder.synthetic_node_id("document-semantic-scope");
        let mut native = self.native.borrow_mut();
        if native.as_ref().is_none_or(|tree| tree.scope != scope) {
            let mut tree = NativeTree {
                scope,
                roots: Vec::new(),
                nodes: Vec::new(),
                targets: Vec::new(),
            };
            for spec in &self.specs {
                tree.roots.push(compile_node(
                    spec,
                    self.width,
                    builder,
                    &mut tree.nodes,
                    &mut tree.targets,
                ));
            }
            *native = Some(tree);
        }
        let tree = native.as_ref().expect("compiled semantic tree");
        for (id, node) in &tree.nodes {
            let inserted = builder.push_child(*id, node.clone());
            debug_assert!(inserted, "canonical semantic IDs must be unique");
        }
        // push_child initially attaches each node to the canvas. Restore only
        // the roots: descendants already have their one canonical parent.
        let parent = builder.parent_node();
        parent.set_children(tree.roots.clone());
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
    if checkbox {
        node.add_action(Action::Click);
    }
    targets.push(ActionTarget {
        accessible_id: id,
        node_id: spec.node_id,
        bounds: b,
        activation: checkbox.then_some(SemanticActivation::ToggleTask),
    });
    node.set_children(
        spec.children
            .iter()
            .map(|child| compile_node(child, width, builder, nodes, targets))
            .collect::<Vec<_>>(),
    );
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

pub(super) fn register_actions(
    targets: &[ActionTarget],
    editor: &Entity<RichDocumentEditor>,
    window: &mut Window,
) {
    for &target in targets {
        let entity = editor.downgrade();
        window.on_a11y_action(
            target.accessible_id,
            Action::ScrollIntoView,
            move |_, _, cx| {
                let _ = entity.update(cx, |editor, cx| {
                    editor.reveal_semantic_bounds(target.bounds, cx)
                });
            },
        );
        if let Some(activation) = target.activation {
            let entity = editor.downgrade();
            window.on_a11y_action(target.accessible_id, Action::Click, move |_, window, cx| {
                let _ = entity.update(cx, |editor, cx| match activation {
                    SemanticActivation::ToggleTask => {
                        editor.toggle_task(target.node_id, window, cx)
                    }
                    SemanticActivation::OpenImageLink => {
                        editor.open_semantic_image_link(target.node_id, window, cx)
                    }
                });
            });
        }
    }
}

impl RichDocumentEditor {
    fn open_semantic_image_link(
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
    use crate::init_editor;

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
        assert_eq!(formulas.len(), 5);
        for formula in &formulas[..4] {
            visit(
                formula.math_markup.as_ref().expect("rendered MathML"),
                &mut tokens,
            );
        }
        assert_eq!(&tokens[..2], ["37", "41"]);
        assert!(tokens.iter().any(|token| token == "∑"));
        assert!(formulas[4].math_markup.is_none());
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
        let node = segment_for_line(&projection, &line.range).unwrap().node_id;
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
        let tree = cache.get(&projection, &lines, 760.);
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
            after_edit.specs.last().unwrap().bounds.y > before_edit.specs.last().unwrap().bounds.y
        );
    }
}
