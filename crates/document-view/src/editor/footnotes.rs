//! Note affordances share canonical destinations across pointer, keyboard and
//! accessibility. The note body remains ordinary editable document content.
use super::*;

pub(super) const INSET: f32 = 40.;

#[derive(Clone, Copy)]
pub(super) enum Navigation {
    Definition(NodeId),
    FirstReference(NodeId),
}

impl Navigation {
    fn position(self, projection: &TextProjection) -> Option<DocumentPosition> {
        let (Self::Definition(id) | Self::FirstReference(id)) = self;
        let note = projection.footnotes.definition(id)?;
        match self {
            Self::Definition(_) => Some(DocumentPosition::new(note.body, 0, Affinity::Downstream)),
            Self::FirstReference(_) => note.references.first().copied(),
        }
    }
}

pub(super) struct ReferencePresentation {
    pub key: (usize, usize),
    pub number: usize,
    pub action: Navigation,
    pub bounds: Bounds<Pixels>,
    pub font_size: f32,
}

impl ReferencePresentation {
    pub fn element(
        self,
        palette: TachyonPalette,
        cx: &mut Context<RichDocumentEditor>,
    ) -> AnyElement {
        div()
            .id((
                gpui::SharedString::from(format!("footnote-ref-{}", self.key.0)),
                self.key.1,
            ))
            .absolute()
            .left(self.bounds.left())
            .top(self.bounds.top())
            .w(self.bounds.size.width)
            .h(self.bounds.size.height)
            .font_family("Spline Sans Tachyon")
            .text_size(px(self.font_size * 0.7))
            .line_height(px(self.font_size * 0.8))
            .text_color(rgb(palette.accent))
            .cursor_pointer()
            .rounded(px(2.))
            .hover(|style| style.bg(rgb(palette.selection)))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(format!("Go to footnote {}", self.number))
                    .build(window, cx)
            })
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |editor, _, window, cx| {
                editor.navigate_footnote(self.action, window, cx)
            }))
            .child(self.number.to_string())
            .into_any_element()
    }
}

impl RichDocumentEditor {
    pub(super) fn snap_footnote_selection(&self, mut range: Range<usize>) -> Range<usize> {
        let interior = |offset| {
            let position = self.projection.position_at(offset, Affinity::Downstream)?;
            let segment = self.projection.segment_for_node(position.node_id)?;
            let text = self.projection.block(position.node_id)?.text()?;
            let run = text.runs().iter().find(|run| {
                run.range.start < position.text_offset
                    && position.text_offset < run.range.end
                    && run
                        .styles
                        .iter()
                        .any(|style| matches!(style, InlineStyle::FootnoteReference(_)))
            })?;
            let base = segment.projection_start() - segment.node_range.start;
            Some(base + run.range.start..base + run.range.end)
        };
        if range.is_empty() {
            if let Some(span) = interior(range.start) {
                let edge = if range.start < self.cursor_offset() {
                    span.start
                } else {
                    span.end
                };
                return edge..edge;
            }
        } else {
            if let Some(span) = interior(range.start) {
                range.start = span.start;
            }
            if let Some(span) = interior(range.end) {
                range.end = span.end;
            }
        }
        range
    }

    pub(super) fn footnote_boundary(&self, offset: usize, forward: bool) -> Option<usize> {
        if self.html_selection.is_some() {
            return None;
        }
        let position = self.projection.position_at(
            offset,
            if forward {
                Affinity::Downstream
            } else {
                Affinity::Upstream
            },
        )?;
        let segment = self.projection.segment_for_node(position.node_id)?;
        let text = self.projection.block(position.node_id)?.text()?;
        let run = text.runs().iter().find(|run| {
            (if forward {
                run.range.start <= position.text_offset && position.text_offset < run.range.end
            } else {
                run.range.start < position.text_offset && position.text_offset <= run.range.end
            }) && run
                .styles
                .iter()
                .any(|style| matches!(style, InlineStyle::FootnoteReference(_)))
        })?;
        Some(
            segment.projection_start()
                + if forward {
                    run.range.end
                } else {
                    run.range.start
                }
                - segment.node_range.start,
        )
    }

    pub(super) fn navigate_footnote(
        &mut self,
        action: Navigation,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document.composition_active() {
            return;
        }
        let Some(position) = action.position(&self.projection) else {
            return;
        };
        let Some(offset) = self.projection.offset_of(position) else {
            return;
        };
        self.html_selection = None;
        self.focus_handle.focus(window, cx);
        self.move_to(offset, window, cx);
        if let Some(index) = visual_line_index_at_offset(&self.visual_lines, offset)
            && let Some(line) = self.visual_lines.get(index)
        {
            self.set_scroll_y((line.y - 24. * self.zoom_factor).max(0.), cx);
        }
    }

    pub(super) fn footnote_at_offset(&self, offset: usize) -> Option<Navigation> {
        let position = self.position_for_offset(offset, Affinity::Downstream)?;
        let segment = self.projection.segment_for_node(position.node_id)?;
        let text = self.projection.block(position.node_id)?.text()?;
        for run in text
            .runs()
            .iter()
            .filter(|run| run.range.contains(&position.text_offset))
        {
            if let Some(note) = run.styles.iter().find_map(|style| match style {
                InlineStyle::FootnoteReference(label) => self.projection.footnotes.label(label),
                _ => None,
            }) {
                return Some(Navigation::Definition(note.definition));
            }
        }
        segment.context.footnote.map(Navigation::FirstReference)
    }

    pub(super) fn render_footnote_numbers(
        &self,
        visible: &[usize],
        palette: TachyonPalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        visible
            .iter()
            .filter_map(|index| {
                let line = &self.visual_lines[*index];
                let segment = segment_for_line(&self.projection, &line.projected_range())?;
                if !segment.context.footnote_first
                    || line.projected_start() != segment.projection_start()
                {
                    return None;
                }
                let note = self
                    .projection
                    .footnotes
                    .definition(segment.context.footnote?)?;
                let number = note.number?;
                let action = Navigation::FirstReference(note.definition);
                Some(
                    div()
                        .absolute()
                        .left(px(line.x_fraction * self.layout_width + line.inset
                            - INSET * self.zoom_factor))
                        .top(px(line.y - 2. * self.zoom_factor))
                        .w(px(INSET * self.zoom_factor))
                        .h(px(24. * self.zoom_factor))
                        .child(
                            // Like inline references, this is a source-linked
                            // editor affordance. `controls` owns its accessible
                            // link; OpenLink at the note supplies keyboard access.
                            // A second component Button duplicates that action
                            // outside the document's semantic reading order.
                            div()
                                .id(("footnote-return", note.definition.get()))
                                .flex()
                                .items_center()
                                .justify_center()
                                .cursor_pointer()
                                .rounded(px(4. * self.zoom_factor))
                                .hover(|style| style.bg(rgb(palette.selection)))
                                .active(|style| style.bg(rgb(palette.selection)))
                                .tooltip(move |window, cx| {
                                    gpui_component::tooltip::Tooltip::new(format!(
                                        "Return to first reference of footnote {number}"
                                    ))
                                    .build(window, cx)
                                })
                                .w(px(36. * self.zoom_factor))
                                .h(px(24. * self.zoom_factor))
                                .text_color(rgb(palette.accent))
                                // All inner metrics belong to the zoomed document,
                                // not the surrounding application's control scale.
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(4. * self.zoom_factor))
                                        .font_family("Spline Sans Tachyon")
                                        .text_size(px(13. * self.zoom_factor))
                                        .line_height(px(16. * self.zoom_factor))
                                        .child(
                                            Icon::new(IconName::Undo2)
                                                .size(px(12. * self.zoom_factor)),
                                        )
                                        .child(number.to_string()),
                                )
                                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                                .on_click(cx.listener(move |editor, _, window, cx| {
                                    editor.navigate_footnote(action, window, cx)
                                })),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }
}

pub(super) struct Control {
    pub label: String,
    pub action: Navigation,
    pub bounds: SemanticBounds,
}

/// Retained with the semantic tree, not rescanned on scroll. Coordinates use
/// the same prepared advances and table alignment as native painting.
pub(super) fn controls(
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    width: f32,
) -> HashMap<NodeId, Vec<Control>> {
    let mut controls = HashMap::<NodeId, Vec<Control>>::new();
    for line in lines {
        let Some(segment) = segment_for_line(projection, &line.projected_range()) else {
            continue;
        };
        if let Some(inline) = &line.inline_math {
            let scale = line.style.font_size / inline.font_size;
            let block = projection.block(segment.node_id);
            let zoom = block.map_or(1., |block| {
                line.style.font_size
                    / visual_line_style_for(
                        projection,
                        block,
                        segment,
                        &line.projected_range(),
                        None,
                    )
                    .font_size
            });
            let trailing = if line.table_cell.is_some() {
                12. * zoom
            } else {
                line.slot.map_or(8., |slot| slot.inset()) * zoom
            };
            let slack =
                (line.width_fraction * width - line.inset - trailing - inline.width * scale)
                    .max(0.);
            let align = match table_column_alignment(projection, Some(segment)) {
                ColumnAlignment::Right => slack,
                ColumnAlignment::Center => slack * 0.5,
                _ => 0.,
            };
            for attachment in &inline.attachments {
                let inline_math::Content::Reference {
                    label,
                    number,
                    advance_em,
                } = &attachment.content
                else {
                    continue;
                };
                let Some(note) = projection.footnotes.label(label) else {
                    continue;
                };
                controls.entry(segment.node_id).or_default().push(Control {
                    label: format!("Footnote {number}"),
                    action: Navigation::Definition(note.definition),
                    bounds: SemanticBounds {
                        x_fraction: line.x_fraction
                            + (line.inset + align + attachment.x * scale) / width,
                        width_fraction: advance_em * line.style.font_size / width,
                        y: line.y,
                        height: line.style.line_height,
                    },
                });
            }
        }
        if segment.context.footnote_first
            && line.projected_start() == segment.projection_start()
            && let Some(note) = segment
                .context
                .footnote
                .and_then(|id| projection.footnotes.definition(id))
            && let Some(number) = note.number
        {
            // Every reference has a forward action; this explicitly labelled
            // backlink returns to the first source occurrence, never a column.
            let zoom = projection.block(segment.node_id).map_or(1., |block| {
                line.style.font_size
                    / visual_line_style_for(
                        projection,
                        block,
                        segment,
                        &line.projected_range(),
                        None,
                    )
                    .font_size
            });
            controls.entry(note.definition).or_default().push(Control {
                label: format!("Return to first reference of footnote {number}"),
                action: Navigation::FirstReference(note.definition),
                bounds: SemanticBounds {
                    x_fraction: line.x_fraction + (line.inset - INSET * zoom) / width,
                    width_fraction: INSET * zoom / width,
                    y: line.y,
                    height: line.style.line_height,
                },
            });
        }
    }
    controls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn footnote_never_strands_punctuation_and_reference_at_the_next_line_start(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document = Document::from_markdown(include_str!(
                "../../../../performance/layout-fixtures/66-footnotes.md"
            ))
            .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            for segment in projection
                .segments()
                .iter()
                .filter(|segment| segment.context.footnote.is_none())
            {
                for width in (180..=600).step_by(4) {
                    let Some(lines) = inline_math::layout(
                        &projection,
                        segment,
                        segment.projection_range(),
                        width as f32,
                        18.,
                        &fonts,
                    ) else {
                        continue;
                    };
                    for line in lines {
                        for attachment in &line.attachments {
                            if matches!(attachment.content, inline_math::Content::Reference { .. })
                            {
                                let before = &projection.text()
                                    [line.range.start..line.range.start + attachment.range.start];
                                assert!(
                                    before.chars().any(char::is_alphanumeric),
                                    "stranded reference at width {width}: {before:?}"
                                );
                            }
                        }
                    }
                }
            }
        });
    }

    #[gpui::test]
    fn footnote_superscripts_measure_actual_digits_and_keep_canonical_ranges(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "# References\n\nA long source label[^descriptive-source-label] belongs to its word. Another[^b] and again[^descriptive-source-label].\n\n[^b]: Second.\n\n[^descriptive-source-label]: First.\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[1];
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), zoom);
                for width in [140., 360., 760., 1200.] {
                    let lines = inline_math::layout(&projection, segment, segment.projection_range(), width, 16., &fonts).unwrap();
                    assert_eq!(lines.first().unwrap().range.start, segment.projection_start());
                    assert_eq!(lines.last().unwrap().range.end, segment.projection_end());
                    assert!(lines.windows(2).all(|pair| pair[0].range.end == pair[1].range.start));
                    let mut numbers = Vec::new();
                    for line in lines {
                        assert!(line.width <= width + 0.05, "{} > {width}", line.width);
                        let raw = fonts.shape_unwrapped(&projection, line.range.clone(), 16.).unwrap();
                        let shaped = inline_math::compose(raw, &(0..line.range.len()), &line.attachments, 16.);
                        assert_eq!(shaped.len(), line.range.len());
                        for attachment in line.attachments {
                            let inline_math::Content::Reference { number, advance_em, .. } = attachment.content else { panic!("unexpected attachment") };
                            numbers.push(number);
                            assert!(advance_em * 16. < 20., "reference must occupy a digit, not its Markdown label");
                            let text = &projection.text()[line.range.start + attachment.range.start..line.range.start + attachment.range.end];
                            assert!(text.starts_with("[^"));
                            assert!(text.ends_with(']'));
                            let x = shaped.x_for_index(attachment.range.start);
                            let end = shaped.x_for_index(attachment.range.end);
                            assert!((f32::from(end - x) - advance_em * 16.).abs() < 0.05);
                        }
                    }
                    assert_eq!(numbers, [1, 2, 1]);
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn footnote_navigation_editing_numbering_and_undo_keep_source_identity(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init_editor);
        let source = "# References\n\nClaim[^later] and another[^first].\n\n[^first]: Review evidence.\n\n[^later]: Primary evidence.\n\n    Keep this paragraph attached.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| editor.update(cx, |editor, cx| {
            editor.refresh_projection();
            let note = editor.projection.footnotes.label("later").unwrap().clone();
            let reference = editor.projection.offset_of(note.references[0]).unwrap();
            editor.set_selection(reference + 1..reference + 4, false, window, cx);
            assert_eq!(editor.selected_byte_range().0, reference..reference + "[^later]".len());
            assert_eq!(editor.next_boundary(reference), reference + "[^later]".len());
            assert_eq!(editor.previous_boundary(reference + "[^later]".len()), reference);
            editor.move_to(reference, window, cx);
            editor.open_link(&OpenLink, window, cx);
            assert!(matches!(editor.selection, Selection::Text(ref selection) if selection.head.node_id == note.body));
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            let body_start = editor.cursor_offset();
            editor.replace_range(body_start..body_start, "Verified ", true, window, cx);
            assert!(editor.document.snapshot().serialize().unwrap().contains("[^later]: Verified Primary evidence"));
            assert!(editor.document.snapshot().serialize().unwrap().starts_with("# References\n\nClaim[^later] and another[^first].\n"));
            editor.undo(&Undo, window, cx);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            editor.navigate_footnote(Navigation::FirstReference(note.definition), window, cx);
            assert_eq!(editor.cursor_offset(), reference);
            let end = reference + "[^later]".len();
            editor.replace_range(reference..end, "", true, window, cx);
            assert_eq!(editor.projection.footnotes.label("first").unwrap().number, Some(1));
            assert_eq!(editor.projection.footnotes.label("later").unwrap().number, None);
            editor.undo(&Undo, window, cx);
            assert_eq!(editor.projection.footnotes.label("later").unwrap().number, Some(1));
            assert_eq!(editor.projection.footnotes.label("first").unwrap().number, Some(2));
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            let note_lines = editor.visual_lines.iter().filter(|line|
                segment_for_line(&editor.projection, &line.projected_range()).is_some_and(|s| s.context.footnote == Some(note.definition))).collect::<Vec<_>>();
            assert!(note_lines.iter().all(|line| line.inset == INSET));
            assert!(note_lines.iter().all(|line| line.style.font_size == 14. && line.style.line_height == 20.));
            let controls = controls(&editor.projection, &editor.visual_lines, editor.layout_width);
            assert_eq!(controls.values().map(Vec::len).sum::<usize>(), 4);
        }));
    }
}
