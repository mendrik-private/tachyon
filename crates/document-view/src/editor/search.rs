//! Per-view find state over immutable canonical content. One worker at a time;
//! only the requested result is retained, even for millions of matches.
use super::*;
use gpui_component::Disableable as _;
use gpui_component::input::{Escape, InputEvent};

// The component's icon-only Button exposes its native interactivity but not
// the StatefulInteractiveElement naming methods. Forward those methods to the
// existing stateful button; no duplicate control or accessibility node.
#[derive(IntoElement)]
struct NamedButton(Button);
impl InteractiveElement for NamedButton {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.0.interactivity()
    }
}
impl StatefulInteractiveElement for NamedButton {}
impl gpui::RenderOnce for NamedButton {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        self.0
    }
}
fn named_button(button: Button, label: &'static str) -> NamedButton {
    NamedButton(button).aria_label(label)
}

struct Part {
    node: NodeId,
    text: String,
    html: Option<Arc<str>>,
    leaves: Vec<Range<usize>>,
}

struct Index {
    generation: u64,
    parts: Vec<Part>,
}

#[derive(Clone)]
struct Match {
    node: NodeId,
    range: Range<usize>,
    html: Option<Arc<str>>,
    position: Option<document_core::HtmlTextPosition>,
    disclosures: Vec<usize>,
    text: String,
}

pub(super) struct FindState {
    input: Entity<InputState>,
    pub visible: bool,
    query: String,
    serial: u64,
    requested: usize,
    count: usize,
    index: Option<Arc<Index>>,
    task: Option<Task<()>>,
    running: bool,
    pending: Option<Match>,
    reveal: Option<(Match, u64)>,
    navigate: bool,
    status: String,
    pub read_only_match: Option<String>,
}

impl FindState {
    pub fn new(window: &mut Window, cx: &mut Context<RichDocumentEditor>) -> Self {
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("Find in document…"));
        cx.subscribe_in(&input, window, |editor, input, event, _, cx| match event {
            InputEvent::Change => {
                editor.find.query = input.read(cx).value().to_string();
                editor.find.requested = 0;
                editor.request_find(cx);
            }
            InputEvent::PressEnter { shift, .. } => editor.find_next(*shift, cx),
            _ => {}
        })
        .detach();
        Self {
            input,
            visible: false,
            query: String::new(),
            serial: 0,
            requested: 0,
            count: 0,
            index: None,
            task: None,
            running: false,
            pending: None,
            reveal: None,
            navigate: false,
            status: String::new(),
            read_only_match: None,
        }
    }

    pub fn invalidate(&mut self) {
        self.serial = self.serial.wrapping_add(1);
        self.index = None;
        self.count = 0;
        self.read_only_match = None;
        self.cancel_navigation();
    }

    pub fn cancel_navigation(&mut self) {
        self.navigate = false;
        self.pending = None;
        self.reveal = None;
    }
}

impl Index {
    fn build(snapshot: &DocumentSnapshot, generation: u64) -> Self {
        let projection = TextProjection::from_snapshot(snapshot);
        let mut parts = Vec::new();
        for segment in projection.segments() {
            let mut part = Part {
                node: segment.node_id,
                text: projection.text()[segment.projection_range()].to_owned(),
                html: None,
                leaves: Vec::new(),
            };
            if let Some(BlockNode::PreservedSource { source, .. }) =
                projection.block(segment.node_id)
            {
                part.html = Some(source.clone());
                part.text.clear();
                if let Some(texts) = document_core::editable_html_text_nodes(source) {
                    for text in texts {
                        if !part.leaves.is_empty() {
                            part.text.push('\n');
                        }
                        let start = part.text.len();
                        part.text.push_str(&text);
                        part.leaves.push(start..part.text.len());
                    }
                } else {
                    part.text = document_core::inert_html_fragment(source)
                        .map_or_else(|| source.to_string(), |fragment| fragment.text().to_owned());
                }
            } else if segment.context.preserved_source {
                continue;
            }
            parts.push(part);
        }
        Self { generation, parts }
    }

    fn find(&self, query: &str, requested: usize) -> (usize, Option<Match>) {
        if query.trim().is_empty() {
            return (0, None);
        }
        let query = query.to_lowercase();
        let mut count = 0;
        let mut selected = None;
        let mut first = None;
        for part in &self.parts {
            for_matches(&part.text, &query, |range| {
                if count == requested || count == 0 {
                    let position = part
                        .leaves
                        .iter()
                        .enumerate()
                        .find_map(|(text_node, leaf)| {
                            leaf.contains(&range.start)
                                .then_some(document_core::HtmlTextPosition {
                                    text_node,
                                    byte_offset: range.start.saturating_sub(leaf.start),
                                })
                        });
                    let found = Match {
                        text: part.text[range.clone()].to_owned(),
                        node: part.node,
                        range,
                        html: part.html.clone(),
                        position,
                        disclosures: Vec::new(),
                    };
                    if count == 0 {
                        first = Some(found.clone());
                    }
                    if count == requested {
                        selected = Some(found);
                    }
                }
                count += 1;
            });
        }
        (
            count,
            selected.or(first).map(|mut found| {
                if let (Some(source), Some(position)) = (&found.html, found.position) {
                    found.disclosures =
                        document_core::html_text_disclosures(source, position).unwrap_or_default();
                }
                found
            }),
        )
    }
}

/// Unicode lowercase matching with original UTF-8 addresses. ASCII avoids a
/// per-character address map; expansions (İ → i + dot) keep whole source chars.
fn for_matches(text: &str, query: &str, mut visit: impl FnMut(Range<usize>)) {
    let folded = text.to_lowercase();
    if text.is_ascii() {
        for (start, _) in folded.match_indices(query) {
            visit(start..start + query.len());
        }
        return;
    }
    let mut offsets = Vec::new();
    let mut folded_byte = 0;
    for (byte, ch) in text.char_indices() {
        for lower in ch.to_lowercase() {
            offsets.push((folded_byte, byte, byte + ch.len_utf8()));
            folded_byte += lower.len_utf8();
        }
    }
    for (start, _) in folded.match_indices(query) {
        let first = offsets.partition_point(|entry| entry.0 < start);
        let last = offsets.partition_point(|entry| entry.0 < start + query.len()) - 1;
        visit(offsets[first].1..offsets[last].2);
    }
}

impl RichDocumentEditor {
    fn hide_find_toolbar(&mut self) {
        // Search keeps focus in its input. Cancel the selection-settle timer
        // too: merely hiding the current toolbar lets that timer reopen it.
        self.toolbar_animation_task.take();
        self.toolbar_animation_generation = self.toolbar_animation_generation.saturating_add(1);
        self.toolbar_visible = false;
        self.toolbar_opacity = 0.;
    }

    pub fn open_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.document.composition_active() {
            window.play_system_bell();
            return;
        }
        self.stop_momentum();
        self.find.visible = true;
        self.hide_find_toolbar();
        self.link_popover_visible = false;
        self.find.input.update(cx, |input, cx| {
            input.focus_handle(cx).focus(window, cx);
            input.select_all(window, cx);
        });
        self.request_find(cx);
    }

    pub fn find_next(&mut self, previous: bool, cx: &mut Context<Self>) {
        if self.find.query.is_empty() {
            return;
        }
        self.find.visible = true;
        let count = self.find.count.max(1);
        self.find.requested = if previous {
            (self.find.requested + count - 1) % count
        } else {
            (self.find.requested + 1) % count
        };
        self.request_find(cx);
    }

    fn request_find(&mut self, cx: &mut Context<Self>) {
        self.find.read_only_match = None;
        self.find.serial = self.find.serial.wrapping_add(1);
        self.find.navigate = true;
        self.find.pending = None;
        self.find.reveal = None;
        self.find.status = if self.find.query.trim().is_empty() {
            String::new()
        } else {
            "Searching…".into()
        };
        self.start_find(cx);
        cx.notify();
    }

    fn start_find(&mut self, cx: &mut Context<Self>) {
        if self.find.running || !self.find.visible {
            return;
        }
        if self.find.query.trim().is_empty() {
            self.find.count = 0;
            self.find.requested = 0;
            self.find.cancel_navigation();
            return;
        }
        let generation = self.document.generation();
        let cached = self
            .find
            .index
            .clone()
            .filter(|index| index.generation == generation);
        let snapshot = self.document.snapshot();
        let query = self.find.query.clone();
        let serial = self.find.serial;
        let requested = self.find.requested;
        self.find.running = true;
        self.find.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(75))
                .await;
            let (index, count, found) = cx
                .background_executor()
                .spawn(async move {
                    let index =
                        cached.unwrap_or_else(|| Arc::new(Index::build(&snapshot, generation)));
                    let (count, found) = index.find(&query, requested);
                    (index, count, found)
                })
                .await;
            let _ = this.update(cx, |editor, cx| {
                editor.find.running = false;
                if !editor.find.visible {
                    return;
                }
                if generation != editor.document.generation() || serial != editor.find.serial {
                    editor.start_find(cx);
                    return;
                }
                editor.find.index = Some(index);
                editor.find.count = count;
                if requested >= count {
                    editor.find.requested = 0;
                }
                editor.find.status = if editor.find.query.trim().is_empty() {
                    String::new()
                } else if count == 0 {
                    "No results".into()
                } else {
                    format!("{} / {count}", editor.find.requested + 1)
                };
                if editor.find.navigate {
                    editor.find.pending = found;
                }
                cx.notify();
            });
        }));
    }

    fn close_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find.visible = false;
        self.find.serial = self.find.serial.wrapping_add(1);
        self.find.cancel_navigation();
        self.focus_handle.focus(window, cx);
        cx.notify();
    }

    pub(super) fn update_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find.visible
            && !self.find.running
            && !self.find.query.trim().is_empty()
            && self
                .find
                .index
                .as_ref()
                .is_none_or(|i| i.generation != self.document.generation())
        {
            self.find.index = None;
            self.request_find(cx);
            // Editing refreshes counts, never navigates the caret back to a
            // result merely because a search bar remains open.
            self.find.cancel_navigation();
        }
        let Some(found) = self.find.pending.take() else {
            return;
        };
        if self.document.composition_active() {
            return;
        }
        self.stop_momentum();
        self.jump_generation = self.jump_generation.wrapping_add(1);
        self.html_selection = None;
        if let Some(source) = &found.html {
            if !matches!(self.document.snapshot().node(found.node), Some(BlockNode::PreservedSource { source: current, .. }) if current == source)
            {
                return;
            }
            // A previous find result may still be selected. Collapse that
            // selection before requesting disclosure geometry: ordinary
            // selection/IME guards correctly prohibit reflow while selecting.
            if let Some(segment) = self.projection.segment_for_node(found.node) {
                self.move_to(segment.projection_start(), window, cx);
            }
            self.find.read_only_match = Some(found.text.clone());
            self.find.status = "Opening result…".into();
            let ancestors = found.disclosures.clone();
            if !ancestors.is_empty() {
                let states = Arc::make_mut(&mut self.projection.html_disclosures);
                let state =
                    states
                        .entry(found.node)
                        .or_insert_with(|| crate::html::DisclosureState {
                            source: source.clone(),
                            overrides: Default::default(),
                        });
                if state.source != *source {
                    state.source = source.clone();
                    state.overrides.clear();
                }
                for ordinal in ancestors {
                    state.overrides.insert(ordinal, true);
                }
                self.geometry_generation = self.geometry_generation.wrapping_add(1);
                self.layout_replan_pending = true;
            }
            self.find.reveal = Some((found, self.jump_generation));
            self.finish_find_reveal(cx);
        } else if let Some(segment) = self.projection.segment_for_node(found.node) {
            let range = segment.projection_start() + found.range.start
                ..segment.projection_start() + found.range.end;
            self.set_selection(range.clone(), false, window, cx);
            _ = self.reveal_find_horizontal();
            if let Some(line) = self
                .visual_lines
                .iter()
                .find(|line| line.projected_range().contains(&range.start))
            {
                self.set_scroll_y((line.y - 24.).max(0.), cx);
            }
        }
        self.hide_find_toolbar();
        cx.notify();
    }

    pub(super) fn finish_find_reveal(&mut self, cx: &mut Context<Self>) {
        let Some((found, generation)) = self.find.reveal.take() else {
            return;
        };
        if !self.find.navigate || generation != self.jump_generation {
            return;
        }
        if !matches!(self.document.snapshot().node(found.node), Some(BlockNode::PreservedSource { source, .. }) if Some(source) == found.html.as_ref())
        {
            return;
        }
        let target = self.visual_lines.iter().find_map(|line| {
            (segment_for_line(&self.projection, &line.projected_range())?.node_id == found.node)
                .then_some(line)
        });
        let Some(line) = target else {
            return;
        };
        let mut y = line.y;
        if let Some(preview) = &line.html_preview {
            if Some(&preview.source) != found.html.as_ref() {
                return;
            }
            if let Some(position) = found.position {
                let byte = preview.byte_for_position(position);
                // Closed previews have no verified full-text edit map. Wait
                // for the same source's disclosure reflow before selecting.
                if byte.is_none() && self.layout_replan_pending {
                    self.find.reveal = Some((found, generation));
                    return;
                }
                if let Some(byte) = byte
                    && let Some(bounds) = preview.caret_bounds(byte)
                    && preview
                        .editable_text
                        .get(byte..byte + found.range.len())
                        .is_some()
                {
                    y += bounds[1] * self.zoom_factor;
                    self.html_selection = Some(HtmlSelection {
                        cross: None,
                        node: found.node,
                        preview: preview.clone(),
                        anchor: byte,
                        head: byte + found.range.len(),
                    });
                    self.find.read_only_match = None;
                }
            }
        }
        _ = self.reveal_find_horizontal();
        self.set_scroll_y((y - 24.).max(0.), cx);
        self.find.status = format!(
            "{} / {}{}",
            self.find.requested + 1,
            self.find.count,
            if self.find.read_only_match.is_some() {
                " · read-only"
            } else {
                ""
            }
        );
    }

    /// Resolve only the selected line, including lines not painted because
    /// their table column is clipped. Geometry uses the renderer's shaping and
    /// local overflow owner, never the document's horizontal scroll offset.
    fn reveal_find_horizontal(&mut self) -> Option<()> {
        let container = self.element_bounds?;
        let width = f32::from(container.size.width);
        let selected = self.selected_byte_range().0;
        let html = self.html_selection.as_ref();
        let line = self.visual_lines.iter().find(|line| {
            html.map_or_else(
                || line.projected_range().contains(&selected.start),
                |selection| {
                    segment_for_line(&self.projection, &line.projected_range())
                        .is_some_and(|segment| segment.node_id == selection.node)
                },
            )
        })?;
        let segment = segment_for_line(&self.projection, &line.projected_range())?;
        let owner = horizontal_scroll_owner(&self.projection, segment)?;
        let is_code = matches!(
            self.projection.block(segment.node_id),
            Some(BlockNode::CodeBlock(_))
        );
        let bounds = visual_line_bounds(self, line, container, is_code, 0.);
        let (viewport_left, viewport) = if line.table_cell.is_some() {
            table_viewport_geometry(line, &self.projection, width, self.zoom_factor)
        } else if html.is_some() {
            // The HTML body has its own clip inside the fragment inset.
            (
                width * line.x_fraction + line.inset,
                (width * line.width_fraction - line.inset - 8. * self.zoom_factor).max(1.),
            )
        } else {
            (width * line.x_fraction, width * line.width_fraction)
        };
        let (start, end, content) = if let Some(selection) = html {
            let range = selection.range();
            let first = selection.preview.caret_bounds(range.start)?;
            let last = selection.preview.caret_bounds(range.end)?;
            // Reveal the first visual line of a multiline match. Avoid a huge
            // union of wrapped lines that could hide the match's beginning.
            let right = if (first[1] - last[1]).abs() < 1. {
                last[0]
            } else {
                first[2]
            };
            let inset = width * line.x_fraction + line.inset - viewport_left;
            (
                inset + first[0] * self.zoom_factor,
                inset + right * self.zoom_factor,
                selection.preview.width * self.zoom_factor,
            )
        } else {
            let layout = self.measurement.shape_unwrapped(
                &self.projection,
                line.projected_range(),
                line.style.font_size,
            )?;
            let layout = if let Some(inline) = &line.inline_math {
                inline_math::compose(
                    layout,
                    &(0..line.projected_range().len()),
                    &inline.attachments,
                    line.style.font_size,
                )
            } else {
                layout
            };
            let left = f32::from(
                aligned_text_left(
                    bounds,
                    &layout,
                    table_column_alignment(&self.projection, Some(segment)),
                ) - container.left(),
            ) - viewport_left;
            let start = selected
                .start
                .saturating_sub(line.projected_start())
                .min(line.projected_range().len());
            let end = selected
                .end
                .saturating_sub(line.projected_start())
                .min(line.projected_range().len());
            let content = f32::from(layout.width()) + line.inset + 8. * self.zoom_factor;
            (
                left + f32::from(shaped_x_for_index(&layout, start)),
                left + f32::from(shaped_x_for_index(&layout, end)),
                content,
            )
        };
        let content = if line.table_cell.is_some() {
            width * self.components.get(&owner)?.right_fraction - viewport_left
        } else {
            content
        };
        let current = self.horizontal_scrolls.get(&owner).copied().unwrap_or(0.);
        let next = horizontal_find_offset(
            current,
            start,
            end,
            viewport,
            content,
            8. * self.zoom_factor,
        );
        self.horizontal_scrolls.insert(owner, next);
        Some(())
    }

    pub(super) fn find_bar(
        &mut self,
        palette: TachyonPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        self.find.visible.then(|| {
            div()
                .id("document-find")
                .flex()
                .items_center()
                .gap(px(4.))
                .h(px(44.))
                .flex_shrink_0()
                .px(px(8.))
                .bg(rgb(palette.surface))
                .border_b_1()
                .border_color(rgb(palette.border))
                .on_action(cx.listener(|this, _: &Escape, window, cx| this.close_find(window, cx)))
                .child(
                    div().id("find-input-container").flex_1().min_w_0().child(
                        Input::new(&self.find.input)
                            .aria_label("Find in document")
                            .small(),
                    ),
                )
                .child(
                    div()
                        .id("find-result-count")
                        .role(Role::Status)
                        .aria_label(self.find.status.clone())
                        .text_sm()
                        .child(self.find.status.clone()),
                )
                .child(named_button(
                    Button::new("find-previous")
                        .ghost()
                        .small()
                        .icon(IconName::ChevronUp)
                        .tooltip("Previous result — Shift+Enter")
                        .disabled(self.find.count == 0)
                        .on_click(cx.listener(|this, _, _, cx| this.find_next(true, cx))),
                    "Previous result",
                ))
                .child(named_button(
                    Button::new("find-next")
                        .ghost()
                        .small()
                        .icon(IconName::ChevronDown)
                        .tooltip("Next result — Enter")
                        .disabled(self.find.count == 0)
                        .on_click(cx.listener(|this, _, _, cx| this.find_next(false, cx))),
                    "Next result",
                ))
                .child(named_button(
                    Button::new("find-close")
                        .ghost()
                        .small()
                        .icon(IconName::Close)
                        .tooltip("Close find — Escape")
                        .on_click(cx.listener(|this, _, window, cx| this.close_find(window, cx))),
                    "Close find",
                ))
                .into_any_element()
        })
    }
}

fn horizontal_find_offset(
    current: f32,
    start: f32,
    end: f32,
    viewport: f32,
    content: f32,
    margin: f32,
) -> f32 {
    let margin = margin.min(viewport * 0.1);
    let left = start.min(end);
    let right = start.max(end);
    let next = if right - left > viewport - 2. * margin {
        // Too wide to show at once: retain the logical beginning (also RTL).
        if start <= end {
            start - margin
        } else {
            start - viewport + margin
        }
    } else if left < current + margin {
        left - margin
    } else if right > current + viewport - margin {
        right - viewport + margin
    } else {
        current
    };
    clamped_horizontal_scroll(0., next, viewport, content)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str =
        include_str!("../../../../performance/layout-fixtures/41-find-document.md");

    #[test]
    fn canonical_find_preserves_source_order_formatting_and_unicode_offsets() {
        let document = Document::from_markdown(SOURCE).unwrap();
        let index = Index::build(&document.snapshot(), 7);
        let (count, first) = index.find("needle", 0);
        assert_eq!(count, 7);
        let first = first.unwrap();
        assert!(first.html.is_none());
        let (_, deep) = index.find("with formatted words", 0);
        let deep = deep.unwrap();
        assert!(deep.html.is_some());
        assert_eq!(deep.disclosures, [0, 1]);
        assert!(deep.position.is_some());
        let (_, last) = index.find("needle", 6);
        assert!(last.unwrap().html.is_none());
        let (_, fallback) = index.find("needle", usize::MAX);
        assert_eq!(fallback.unwrap().node, first.node);
        assert_eq!(index.find("not present", 0).0, 0);
        assert_eq!(index.find("", 0).0, 0);
        assert_eq!(index.find("CAFÉ", 0).0, 2);
        assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        let text = "İSTANBUL café 東京";
        let mut ranges = Vec::new();
        for_matches(text, "café", |range| ranges.push(range));
        assert_eq!(&text[ranges[0].clone()], "café");
        ranges.clear();
        for_matches(text, "i", |range| ranges.push(range));
        assert_eq!(&text[ranges[0].clone()], "İ");
    }

    #[test]
    fn repeated_html_text_maps_to_its_own_containing_disclosures() {
        let source = "<details><summary>Repeat</summary><p>Repeat</p><details><summary>Repeat</summary><p>Repeat</p></details></details><details><summary>Sibling</summary><p>Repeat</p></details>";
        for (text_node, expected) in [
            (0, vec![]),
            (1, vec![0]),
            (2, vec![0]),
            (3, vec![0, 1]),
            (5, vec![2]),
        ] {
            assert_eq!(
                document_core::html_text_disclosures(
                    source,
                    document_core::HtmlTextPosition {
                        text_node,
                        byte_offset: 0
                    }
                ),
                Some(expected)
            );
        }
        assert!(
            document_core::html_text_disclosures(
                source,
                document_core::HtmlTextPosition {
                    text_node: 99,
                    byte_offset: 0
                }
            )
            .is_none()
        );
    }

    #[gpui::test]
    fn find_reveals_html_and_preserves_content_undo(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                // Navigate from a prior nonempty result, not only a caret.
                let start = editor.projection.text().find("Needle").unwrap();
                editor.set_selection(start..start + 6, false, window, cx);
                let index = Arc::new(Index::build(
                    &editor.document.snapshot(),
                    editor.document.generation(),
                ));
                editor.find.visible = true;
                editor.find.query = "deep needle".into();
                editor.find.pending = index.find("deep needle", 0).1;
                editor.find.index = Some(index);
                editor.find.navigate = true;
                editor.update_find(window, cx);
            })
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            let selection = editor.html_selection.as_ref().unwrap_or_else(|| panic!(
                "canonical HTML match selected: pending={}, reveal={}, navigate={}, replan={}, previews={:?}",
                editor.find.pending.is_some(), editor.find.reveal.is_some(), editor.find.navigate,
                editor.layout_replan_pending, editor.visual_lines.iter().filter_map(|l| l.html_preview.as_ref())
                    .map(|p| (&p.editable_text, p.text_hits.len(), p.disclosures.iter().map(|d| (d.ordinal, d.open)).collect::<Vec<_>>())).collect::<Vec<_>>()
            ));
            assert_eq!(
                &selection.preview.editable_text[selection.range()],
                "Deep needle"
            );
            assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
            assert!(matches!(
                editor.document.undo(),
                Err(DocumentError::NothingToUndo)
            ));
        });
    }

    #[gpui::test]
    fn closing_find_cancels_late_navigation(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        cx.run_until_parked();
        let original_selection = editor.read_with(cx, |editor, _| editor.selection.clone());
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.find.query = "needle".into();
                editor.open_find(window, cx);
                assert!(editor.find.running);
                editor.close_find(window, cx);
            })
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(!editor.find.visible);
            assert!(!editor.find.running);
            assert!(editor.find.pending.is_none());
            assert_eq!(editor.selection, original_selection);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
        });
    }

    #[gpui::test]
    fn opaque_find_match_copies_without_editing_an_unrelated_caret(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        // A spanned HTML table is a real preserved block whose Markdown
        // conversion would lose relationships. An inline custom tag can be
        // parsed as an ordinary Markdown paragraph instead.
        let source = "# Keep this heading\n\n<table><tr><td colspan='2'>Opaque needle</td></tr></table>\n\nKeep this paragraph.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let index = Arc::new(Index::build(
                    &editor.document.snapshot(),
                    editor.document.generation(),
                ));
                editor.find.query = "opaque needle".into();
                editor.find.visible = true;
                let (count, found) = index.find(&editor.find.query, 0);
                assert!(found.as_ref().is_some_and(|found| found.html.is_some()));
                assert!(found.as_ref().unwrap().position.is_none());
                editor.find.count = count;
                editor.find.index = Some(index);
                editor.find.pending = found;
                editor.find.navigate = true;
                editor.update_find(window, cx);
                assert_eq!(
                    editor.find.read_only_match.as_deref(),
                    Some("Opaque needle")
                );
                assert!(editor.find.status.ends_with("read-only"));
                editor.close_find(window, cx);
                editor.copy(&Copy, window, cx);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some("Opaque needle")
                );
                assert!(
                    editor
                        .apply_command(EditCommand::ReplaceSelection {
                            text: "wrong target".into(),
                            typing: true
                        })
                        .is_err()
                );
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                editor.move_to(0, window, cx);
                assert!(editor.find.read_only_match.is_none());
            })
        });
    }

    #[gpui::test]
    fn find_reveals_a_clipped_table_cell_without_page_scroll(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        let source = concat!(
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[2000.0,2000.0]} -->\n",
            "| first | last |\n| --- | --- |\n| left | right target |\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                assert!(
                    editor
                        .horizontal_metrics
                        .values()
                        .any(|(viewport, content)| content > viewport),
                    "fixture must have real measured overflow"
                );
                editor.find.query = "right target".into();
                editor.open_find(window, cx);
            })
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            let range = editor.selected_byte_range().0;
            assert_eq!(&editor.projection.text()[range.clone()], "right target");
            let line = editor
                .painted_lines
                .iter()
                .find(|line| line.range.contains(&range.start))
                .expect("find must paint the clipped target column");
            let viewport = line.content_mask.as_ref().unwrap().bounds;
            let start = aligned_text_left(line.bounds, &line.layout, line.alignment)
                + line.layout.x_for_index(range.start - line.range.start);
            let end = start + line.layout.width();
            assert!(
                start >= viewport.left() && end <= viewport.right(),
                "match must fit inside its local viewport: {start:?}..{end:?}, {viewport:?}"
            );
            assert_eq!(editor.scroll_handle.offset().x, px(0.));
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            assert!(matches!(
                editor.document.undo(),
                Err(DocumentError::NothingToUndo)
            ));
        });
    }

    #[gpui::test]
    fn find_reveals_wide_html_text_and_its_actual_caret(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        let source = "<details open><summary>Wide table</summary><table style='width:1800px'><tr><th>Start</th><th style='text-align:right'>right html target</th></tr><tr><td>A</td><td>B</td></tr></table></details>\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        editor.update(cx, |editor, cx| editor.set_zoom_factor(2., cx));
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.find.query = "right html target".into();
                editor.open_find(window, cx);
            })
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            let selection = editor.html_selection.as_ref().unwrap_or_else(|| {
                panic!(
                    "verified HTML selection: status={}, query={}, count={}, previews={:?}",
                    editor.find.status,
                    editor.find.query,
                    editor.find.count,
                    editor
                        .visual_lines
                        .iter()
                        .filter_map(|l| l.html_preview.as_ref())
                        .map(|p| (p.width, p.can_convert, &p.editable_text, p.text_hits.len()))
                        .collect::<Vec<_>>()
                )
            });
            assert_eq!(
                &selection.preview.editable_text[selection.range()],
                "right html target"
            );
            assert!(
                editor.horizontal_scrolls[&selection.node] > 0.,
                "HTML must actually overflow"
            );
            let caret = editor.html_caret_bounds(selection.anchor).unwrap();
            let container = editor.element_bounds.unwrap();
            assert!(
                caret.left() >= container.left() && caret.right() <= container.right(),
                "HTML caret must follow local scroll: {caret:?}, container={container:?}"
            );
            assert_eq!(editor.scroll_handle.offset().x, px(0.));
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn find_cancels_delayed_formatting_toolbar(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.find.query = "needle".into();
                editor.open_find(window, cx);
            })
        });
        for _ in 0..15 {
            cx.run_until_parked();
            cx.executor().advance_clock(Duration::from_millis(50));
        }
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.find.count, 7);
            assert!(editor.find.visible);
            assert!(
                !editor.toolbar_visible,
                "find selection must not open a delayed format toolbar"
            );
            assert_eq!(editor.toolbar_opacity, 0.);
        });
    }

    #[test]
    fn horizontal_find_uses_minimal_motion_and_keeps_overwide_logical_start() {
        assert_eq!(
            horizontal_find_offset(100., 120., 160., 300., 1000., 8.),
            100.
        );
        assert_eq!(
            horizontal_find_offset(0., 420., 460., 300., 1000., 8.),
            168.
        );
        assert_eq!(horizontal_find_offset(400., 20., 60., 300., 1000., 8.), 12.);
        assert_eq!(horizontal_find_offset(0., 60., 20., 300., 1000., 8.), 0.);
        assert_eq!(horizontal_find_offset(0., 100., 800., 300., 1000., 8.), 92.);
        assert_eq!(
            horizontal_find_offset(0., 800., 100., 300., 1000., 8.),
            508.
        );
        assert_eq!(
            horizontal_find_offset(0., 980., 1000., 300., 1000., 8.),
            700.
        );
    }

    #[gpui::test]
    fn editing_with_find_open_refreshes_counts_without_moving_caret(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.find.query = "needle".into();
                editor.open_find(window, cx);
            })
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        let caret = cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                assert_eq!(editor.find.count, 7);
                let end = editor.projection.text().trim_end().len();
                editor.move_to(end, window, cx);
                assert!(editor.replace_range(end..end, " needle", false, window, cx));
                editor.selection.clone()
            })
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(100));
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.find.count, 8);
            assert_eq!(editor.selection, caret);
            assert!(!editor.find.navigate);
            assert!(!editor.find.running);
        });
    }

    #[gpui::test]
    fn latest_find_query_wins_without_parallel_workers(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.find.query = "needle".into();
                editor.open_find(window, cx);
                assert!(editor.find.running);
                editor.find.query = "café".into();
                editor.request_find(cx);
            })
        });
        for _ in 0..3 {
            cx.run_until_parked();
            cx.executor().advance_clock(Duration::from_millis(100));
        }
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(!editor.find.running);
            assert_eq!(editor.find.count, 2);
            assert_eq!(editor.find.query, "café");
            let range = editor.selected_byte_range().0;
            assert_eq!(&editor.projection.text()[range], "café");
            assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
        });
    }
}
