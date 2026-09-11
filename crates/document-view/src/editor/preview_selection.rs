//! Lazy, immutable text mapping for selections crossing preview boundaries.
//! It is never edited; authoring resolves its addresses in document-core.
use super::*;
use document_core::{PreviewPosition, PreviewSelection, Revision};

#[derive(Clone)]
struct Segment {
    range: Range<usize>,
    source: crate::ProjectionSegment,
    html: Option<Arc<crate::html::HtmlPreview>>,
}

pub(super) struct PreviewSelectionProjection {
    pub revision: Revision,
    pub text: String,
    segments: Vec<Segment>,
}

impl PreviewSelectionProjection {
    pub fn build(editor: &RichDocumentEditor) -> Self {
        let previews: HashMap<_, _> = editor
            .visual_lines
            .iter()
            .filter_map(|line| {
                let preview = line.html_preview.as_ref()?;
                let segment = editor
                    .projection
                    .segment_for_range(&line.projected_range())?;
                Some((segment.node_id, preview.clone()))
            })
            .collect();
        let mut text = String::new();
        let mut segments = Vec::new();
        let mut previous = 0;
        for source in editor.projection.segments() {
            text.push_str(&editor.projection.text()[previous..source.projection_start()]);
            let start = text.len();
            let html = previews
                .get(&source.node_id)
                .filter(|preview| preview.can_convert)
                .cloned();
            text.push_str(html.as_ref().map_or_else(
                || &editor.projection.text()[source.projection_range()],
                |preview| preview.editable_text.as_str(),
            ));
            segments.push(Segment {
                range: start..text.len(),
                source: source.clone(),
                html,
            });
            previous = source.projection_end();
        }
        text.push_str(&editor.projection.text()[previous..]);
        Self {
            revision: editor.document.snapshot().revision(),
            text,
            segments,
        }
    }

    fn segment_at(&self, byte: usize, affinity: Affinity) -> Option<&Segment> {
        if byte > self.text.len() || !self.text.is_char_boundary(byte) {
            return None;
        }
        self.segments
            .iter()
            .find(|segment| {
                segment.range.contains(&byte)
                    || (affinity == Affinity::Upstream && segment.range.end == byte)
                    || (affinity == Affinity::Downstream && segment.range.start == byte)
            })
            .or_else(|| {
                self.segments.iter().min_by_key(|segment| {
                    segment.range.start.saturating_sub(byte)
                        + byte.saturating_sub(segment.range.end)
                })
            })
    }

    pub fn position(&self, byte: usize, affinity: Affinity) -> Option<PreviewPosition> {
        let segment = self.segment_at(byte, affinity)?;
        let local = byte
            .saturating_sub(segment.range.start)
            .min(segment.range.len());
        if let Some(preview) = &segment.html {
            Some(PreviewPosition::Html {
                node_id: segment.source.node_id,
                expected_source: preview.source.clone(),
                position: preview.position_for_byte(local)?,
            })
        } else if !segment.source.context.preserved_source {
            Some(PreviewPosition::Document(DocumentPosition::new(
                segment.source.node_id,
                segment.source.node_range.start + local.min(segment.source.node_range.len()),
                affinity,
            )))
        } else {
            None
        }
    }

    pub fn byte(&self, position: &PreviewPosition) -> Option<usize> {
        self.segments.iter().find_map(|segment| {
            let local = match position {
                PreviewPosition::Html {
                    node_id,
                    expected_source,
                    position,
                } if *node_id == segment.source.node_id => {
                    let preview = segment
                        .html
                        .as_ref()
                        .filter(|preview| preview.source == *expected_source)?;
                    preview.byte_for_position(*position)?
                }
                PreviewPosition::Document(position)
                    if position.node_id == segment.source.node_id
                        && (segment.source.node_range.contains(&position.text_offset)
                            || position.text_offset == segment.source.node_range.end) =>
                {
                    position
                        .text_offset
                        .checked_sub(segment.source.node_range.start)?
                }
                _ => return None,
            };
            Some(segment.range.start + local)
        })
    }

    pub fn selection(&self, anchor: usize, head: usize) -> Option<PreviewSelection> {
        Some(PreviewSelection {
            revision: self.revision,
            anchor: self.position(anchor, Affinity::Downstream)?,
            head: self.position(head, Affinity::Upstream)?,
        })
    }

    pub fn projected_range(&self, range: Range<usize>) -> Option<Range<usize>> {
        let map = |byte, end| {
            let segment = self.segment_at(
                byte,
                if end {
                    Affinity::Upstream
                } else {
                    Affinity::Downstream
                },
            )?;
            if segment.html.is_some() {
                if range.is_empty() {
                    return None;
                }
                Some(if end {
                    segment.source.projection_end()
                } else {
                    segment.source.projection_start()
                })
            } else {
                Some(
                    segment.source.projection_start()
                        + byte
                            .saturating_sub(segment.range.start)
                            .min(segment.range.len()),
                )
            }
        };
        Some(map(range.start, false)?..map(range.end, true)?)
    }

    pub fn html_range(
        &self,
        node: NodeId,
        range: Range<usize>,
        head: usize,
    ) -> Option<(Arc<crate::html::HtmlPreview>, Range<usize>, usize)> {
        let segment = self
            .segments
            .iter()
            .find(|segment| segment.source.node_id == node)?;
        if range.end < segment.range.start || range.start > segment.range.end {
            return None;
        }
        Some((
            segment.html.clone()?,
            range
                .start
                .saturating_sub(segment.range.start)
                .min(segment.range.len())
                ..range
                    .end
                    .saturating_sub(segment.range.start)
                    .min(segment.range.len()),
            head.saturating_sub(segment.range.start)
                .min(segment.range.len()),
        ))
    }
}

impl RichDocumentEditor {
    pub(super) fn preview_position_at(&self, point: Point<Pixels>) -> Option<PreviewPosition> {
        if let Some((node_id, preview, position)) = self.html_target_at(point) {
            return Some(PreviewPosition::Html {
                node_id,
                expected_source: preview.source.clone(),
                position,
            });
        }
        self.projection
            .position_at(self.index_for_mouse_position(point), Affinity::Downstream)
            .map(PreviewPosition::Document)
    }

    pub(super) fn extend_preview_selection(&mut self, point: Point<Pixels>) -> bool {
        let Some(target) = self.preview_position_at(point) else {
            return self.html_selection.is_some();
        };
        self.extend_to_preview_position(target)
    }

    pub(super) fn extend_to_preview_position(&mut self, target: PreviewPosition) -> bool {
        if let Some(selection) = &mut self.html_selection {
            if let Some(cross) = &selection.cross {
                if let Some(byte) = cross.byte(&target) {
                    selection.head = byte;
                }
                return true;
            }
            if let PreviewPosition::Html {
                node_id, position, ..
            } = &target
                && *node_id == selection.node
            {
                if let Some(byte) = selection.preview.byte_for_position(*position) {
                    selection.head = byte;
                }
                return true;
            }
        }
        let anchor = if let Some(selection) = &self.html_selection {
            selection.position(selection.anchor, Affinity::Downstream)
        } else if let Selection::Text(selection) = &self.selection {
            Some(PreviewPosition::Document(selection.anchor))
        } else {
            None
        };
        let Some(anchor) = anchor else {
            return false;
        };
        // Ordinary text-only drags keep the existing cheap projection path.
        if let (PreviewPosition::Document(a), PreviewPosition::Document(b)) = (&anchor, &target) {
            let Some(a) = self.projection.offset_of(*a) else {
                return false;
            };
            let Some(b) = self.projection.offset_of(*b) else {
                return false;
            };
            let segments = self.projection.segments();
            let first = segments.partition_point(|segment| segment.projection_end() <= a.min(b));
            if !segments[first..]
                .iter()
                .take_while(|segment| segment.projection_start() < a.max(b))
                .any(|segment| {
                    segment.context.preserved_source
                        && segment.projection_start() < a.max(b)
                        && segment.projection_end() > a.min(b)
                })
            {
                return false;
            }
        }
        let cross = Arc::new(PreviewSelectionProjection::build(self));
        let Some((anchor, head)) = cross.byte(&anchor).zip(cross.byte(&target)) else {
            return false;
        };
        let Some(segment) = cross.segments.iter().find(|segment| {
            segment.html.is_some()
                && segment.range.start <= anchor.max(head)
                && segment.range.end >= anchor.min(head)
        }) else {
            return false;
        };
        self.html_selection = Some(HtmlSelection {
            node: segment.source.node_id,
            preview: segment.html.as_ref().unwrap().clone(),
            anchor,
            head,
            cross: Some(cross),
        });
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::init_editor;

    const SOURCE: &str = "Before\n\n<div><p>Alpha <strong>café</strong></p></div>\n\nMiddle\n\n<div><p>Beta <em>tail</em></p></div>\n\nAfter\n";

    fn html_point(editor: &RichDocumentEditor, node: NodeId, byte: usize) -> Point<Pixels> {
        let bounds = editor.element_bounds.unwrap();
        let line = editor
            .visual_lines
            .iter()
            .find(|line| {
                line.html_preview.is_some()
                    && editor
                        .projection
                        .segment_for_range(&line.projected_range())
                        .is_some_and(|segment| segment.node_id == node)
            })
            .unwrap_or_else(|| panic!("missing HTML line for {node:?}"));
        let hit = line
            .html_preview
            .as_ref()
            .unwrap()
            .text_hits
            .iter()
            .find(|hit| {
                (hit.left.text_node == 0 && hit.left.byte_offset == byte)
                    || (hit.right.text_node == 0 && hit.right.byte_offset == byte)
            })
            .unwrap_or_else(|| {
                panic!(
                    "missing byte {byte} in {:?}",
                    line.html_preview
                        .as_ref()
                        .unwrap()
                        .text_hits
                        .iter()
                        .map(|hit| (hit.left, hit.right))
                        .collect::<Vec<_>>()
                )
            });
        let [left, top, right, bottom] = hit.bounds;
        let fraction = if hit.left.byte_offset == byte {
            0.1
        } else {
            0.9
        };
        point(
            bounds.left()
                + px(line.x_fraction * f32::from(bounds.size.width)
                    + line.inset
                    + (left + (right - left) * fraction) * editor.zoom_factor),
            bounds.top() + px(line.y + ((top + bottom) * 0.5) * editor.zoom_factor),
        )
    }

    fn text_point(editor: &RichDocumentEditor, node: NodeId, byte: usize) -> Point<Pixels> {
        if matches!(
            editor.document.snapshot().node(node),
            Some(BlockNode::PreservedSource { .. })
        ) {
            return html_point(editor, node, byte);
        }
        let offset = editor
            .projection
            .offset_of(DocumentPosition::new(node, byte, Affinity::Downstream))
            .unwrap();
        let line = painted_line_for_offset(&editor.painted_lines, offset).unwrap();
        let point = point(
            aligned_text_left(line.bounds, &line.layout, line.alignment)
                + shaped_x_for_index(&line.layout, offset - line.range.start),
            line.bounds.top() + line.bounds.size.height * 0.5,
        );
        assert_eq!(
            editor.index_for_mouse_position(point),
            offset,
            "test point must resolve to the requested byte"
        );
        point
    }

    #[gpui::test]
    fn horizontal_keys_cross_html_edges_without_conversion(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let snapshot = editor.document.snapshot();
                let start = html_point(editor, snapshot.blocks().get(1).unwrap().id(), 0);
                editor.on_mouse_down(
                    &MouseDownEvent {
                        position: start,
                        button: MouseButton::Left,
                        click_count: 1,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                editor.on_mouse_up(
                    &MouseUpEvent {
                        position: start,
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                // Word moves use the same word classes as ordinary Markdown.
                // Plain moves cross a block boundary without consuming a glyph.
                for (key, block, offset) in [
                    ("left", 0, 6),
                    ("right", 1, 0),
                    ("word-right", 1, 5),
                    ("word-right", 1, 11),
                    ("left", 1, 9),
                    ("right", 1, 11),
                    ("right", 2, 0),
                    ("left", 1, 11),
                    ("word-right", 2, 6),
                    ("word-left", 2, 0),
                    ("word-left", 1, 6),
                    ("word-left", 1, 0),
                    ("word-left", 0, 0),
                ] {
                    match key {
                        "left" => editor.left(&Left, window, cx),
                        "right" => editor.right(&Right, window, cx),
                        "word-left" => editor.word_left(&WordLeft, window, cx),
                        "word-right" => editor.word_right(&WordRight, window, cx),
                        _ => unreachable!(),
                    }
                    let expected = snapshot.blocks().get(block).unwrap().id();
                    if let Some(selection) = &editor.html_selection {
                        let PreviewPosition::Html {
                            node_id, position, ..
                        } = selection
                            .position(selection.head, Affinity::Downstream)
                            .unwrap()
                        else {
                            panic!("ordinary caret must leave preview selection")
                        };
                        assert_eq!(node_id, expected, "{key}");
                        assert_eq!(
                            selection.preview.byte_for_position(position),
                            Some(offset),
                            "{key}"
                        );
                        assert_eq!(selection.anchor, selection.head);
                    } else {
                        let Selection::Text(selection) = &editor.selection else {
                            panic!("text caret")
                        };
                        assert_eq!(selection.head.node_id, expected, "{key}");
                        assert_eq!(selection.head.text_offset, offset, "{key}");
                    }
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
                    assert_eq!(editor.document.snapshot().revision(), snapshot.revision());
                }
            });
        });
    }

    #[gpui::test]
    fn horizontal_word_selection_retains_anchor_and_collapses_to_real_endpoints(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let snapshot = editor.document.snapshot();
                let html = snapshot.blocks().get(1).unwrap().id();
                let start = html_point(editor, html, 0);
                editor.on_mouse_down(
                    &MouseDownEvent {
                        position: start,
                        button: MouseButton::Left,
                        click_count: 1,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                editor.on_mouse_up(
                    &MouseUpEvent {
                        position: start,
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                for _ in 0..5 {
                    editor.select_word_right(&SelectWordRight, window, cx);
                }
                let selection = editor.html_selection.as_ref().expect("cross selection");
                let cross = selection.cross.as_ref().unwrap();
                let selected = cross.selection(selection.anchor, selection.head).unwrap();
                assert_eq!(
                    snapshot
                        .preview_clipboard_payload(&selected)
                        .unwrap()
                        .unwrap()
                        .plain_text
                        .as_deref(),
                    Some("Alpha **café**\n\nMiddle\n\nBeta *tail*")
                );
                let original_anchor = selected.anchor;
                editor.left(&Left, window, cx);
                let selection = editor.html_selection.as_ref().unwrap();
                assert_eq!(
                    selection.position(selection.head, Affinity::Downstream),
                    Some(original_anchor)
                );
                assert!(selection.range().is_empty());
                for _ in 0..5 {
                    editor.select_word_right(&SelectWordRight, window, cx);
                }
                editor.right(&Right, window, cx);
                let selection = editor.html_selection.as_ref().unwrap();
                assert_eq!(selection.node, snapshot.blocks().get(3).unwrap().id());
                assert_eq!(selection.head, 9);
                assert!(selection.range().is_empty());
                for _ in 0..5 {
                    editor.select_word_left(&SelectWordLeft, window, cx);
                }
                let selection = editor.html_selection.as_ref().unwrap();
                assert!(selection.head < selection.anchor);
                let selected = selection
                    .cross
                    .as_ref()
                    .unwrap()
                    .selection(selection.anchor, selection.head)
                    .unwrap();
                assert_eq!(
                    snapshot
                        .preview_clipboard_payload(&selected)
                        .unwrap()
                        .unwrap()
                        .plain_text
                        .as_deref(),
                    Some("Alpha **café**\n\nMiddle\n\nBeta *tail*")
                );
                editor.right(&Right, window, cx);
                editor.select_right(&SelectRight, window, cx);
                editor.select_right(&SelectRight, window, cx);
                let selection = editor.html_selection.as_ref().unwrap();
                let selected = selection
                    .cross
                    .as_ref()
                    .unwrap()
                    .selection(selection.anchor, selection.head)
                    .unwrap();
                assert_eq!(
                    snapshot
                        .preview_clipboard_payload(&selected)
                        .unwrap()
                        .unwrap()
                        .plain_text
                        .as_deref(),
                    Some("\n\nA")
                );
                editor.right(&Right, window, cx);
                assert!(
                    editor.html_selection.is_none(),
                    "collapse into Markdown restores ordinary editing"
                );
                let Selection::Text(selection) = &editor.selection else {
                    panic!("text caret")
                };
                assert_eq!(
                    selection.head.node_id,
                    snapshot.blocks().get(4).unwrap().id()
                );
                assert_eq!(selection.head.text_offset, 1);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
                assert_eq!(editor.document.snapshot().revision(), snapshot.revision());
            });
        });
    }

    #[gpui::test]
    fn vertical_keys_leave_html_and_enter_adjacent_markdown_without_conversion(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let snapshot = editor.document.snapshot();
                let start = html_point(editor, snapshot.blocks().get(1).unwrap().id(), 0);
                editor.on_mouse_down(
                    &MouseDownEvent {
                        position: start,
                        button: MouseButton::Left,
                        click_count: 1,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                editor.on_mouse_up(
                    &MouseUpEvent {
                        position: start,
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                for (direction, index) in
                    [(1, 2), (1, 3), (1, 4), (-1, 3), (-1, 2), (-1, 1), (-1, 0)]
                {
                    if direction > 0 {
                        editor.down(&Down, window, cx);
                    } else {
                        editor.up(&Up, window, cx);
                    }
                    let actual = if let Some(selection) = &editor.html_selection {
                        selection
                            .position(selection.head, Affinity::Downstream)
                            .unwrap()
                    } else {
                        let Selection::Text(selection) = &editor.selection else {
                            panic!("text caret")
                        };
                        PreviewPosition::Document(selection.head)
                    };
                    let expected = snapshot.blocks().get(index).unwrap().id();
                    match actual {
                        PreviewPosition::Document(position) => {
                            assert_eq!(position.node_id, expected);
                            assert_eq!(position.text_offset, 0);
                            assert!(
                                editor.html_selection.is_none(),
                                "ordinary carets must regain normal editing/IME"
                            );
                        }
                        PreviewPosition::Html {
                            node_id, position, ..
                        } => {
                            assert_eq!(node_id, expected);
                            assert_eq!(position.byte_offset, 0);
                        }
                    }
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
                    assert_eq!(editor.document.snapshot().revision(), snapshot.revision());
                }
            });
        });
    }

    #[gpui::test]
    fn cross_preview_home_end_use_wrapped_markdown_lines(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = SOURCE.replace(
            "Middle",
            &"A measured paragraph with many words. ".repeat(30),
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let snapshot = editor.document.snapshot();
                let html = snapshot.blocks().get(1).unwrap().id();
                let middle = snapshot.blocks().get(2).unwrap().id();
                let line = editor
                    .painted_lines
                    .iter()
                    .filter(|line| {
                        editor
                            .projection
                            .segment_for_range(&line.range)
                            .is_some_and(|s| s.node_id == middle)
                    })
                    .nth(1)
                    .expect("paragraph must wrap");
                let from = editor
                    .projection
                    .position_at(line.range.start, Affinity::Downstream)
                    .unwrap();
                let to = editor
                    .projection
                    .position_at(line.range.end, Affinity::Upstream)
                    .unwrap();
                let head =
                    DocumentPosition::new(middle, from.text_offset + 4, Affinity::Downstream);
                let cross = Arc::new(PreviewSelectionProjection::build(editor));
                let preview = editor
                    .visual_lines
                    .iter()
                    .find_map(|line| line.html_preview.clone())
                    .unwrap();
                let anchor = cross
                    .byte(&PreviewPosition::Html {
                        node_id: html,
                        expected_source: preview.source.clone(),
                        position: preview.position_for_byte(6).unwrap(),
                    })
                    .unwrap();
                let expected = cross.byte(&PreviewPosition::Document(from)).unwrap()
                    ..cross.byte(&PreviewPosition::Document(to)).unwrap();
                for (end, extend) in [(false, false), (true, false), (false, true), (true, true)] {
                    editor.html_selection = Some(HtmlSelection {
                        node: html,
                        preview: preview.clone(),
                        anchor,
                        head: cross.byte(&PreviewPosition::Document(head)).unwrap(),
                        cross: Some(cross.clone()),
                    });
                    assert_eq!(editor.html_line_range().unwrap(), expected);
                    match (end, extend) {
                        (false, false) => editor.home(&Home, window, cx),
                        (true, false) => editor.end(&End, window, cx),
                        (false, true) => editor.select_home(&SelectHome, window, cx),
                        (true, true) => editor.select_end(&SelectEnd, window, cx),
                    }
                    let target = if end { to } else { from };
                    if extend {
                        let selection = editor.html_selection.as_ref().expect("cross selection");
                        assert_eq!(
                            selection.anchor, anchor,
                            "Shift must retain the HTML anchor"
                        );
                        assert_eq!(
                            selection.head,
                            cross.byte(&PreviewPosition::Document(target)).unwrap()
                        );
                    } else {
                        assert!(editor.html_selection.is_none());
                        let Selection::Text(selection) = &editor.selection else {
                            panic!("ordinary caret")
                        };
                        assert_eq!(selection.head.node_id, target.node_id);
                        assert_eq!(selection.head.text_offset, target.text_offset);
                        assert_eq!(selection.anchor.node_id, selection.head.node_id);
                        assert_eq!(selection.anchor.text_offset, selection.head.text_offset);
                    }
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                    assert_eq!(editor.document.snapshot().revision(), snapshot.revision());
                }
            });
        });
    }

    #[gpui::test]
    fn html_drag_at_viewport_edge_scrolls_without_more_pointer_events(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = format!(
            "{SOURCE}\n{}",
            "More paragraphs for scrolling.\n\n".repeat(100)
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let snapshot = editor.document.snapshot();
                let start = html_point(editor, snapshot.blocks().get(1).unwrap().id(), 6);
                editor.on_mouse_down(
                    &MouseDownEvent {
                        position: start,
                        button: MouseButton::Left,
                        click_count: 1,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                let bounds = editor.scroll_handle.bounds();
                editor.on_mouse_move(
                    &MouseMoveEvent {
                        position: point(start.x, bounds.bottom() - px(2.)),
                        pressed_button: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    window,
                    cx,
                );
            });
            let before = editor.read(cx).scroll_metrics().0;
            assert!(window.is_window_active(), "test window must be active");
            editor.update(cx, |editor, _| {
                assert!(editor.focus_handle.is_focused(window));
                editor
                    .drag_scroll
                    .as_mut()
                    .expect("edge drag scheduled")
                    .last_frame = Instant::now() - Duration::from_millis(16);
            });
            window.simulate_next_frame(cx);
            assert!(
                editor.read(cx).scroll_metrics().0 > before,
                "HTML drag must scroll on a frame without another pointer event"
            );
            assert_eq!(
                editor.read(cx).document.snapshot().serialize().unwrap(),
                source
            );
            let anchor = editor.read(cx).html_selection.as_ref().unwrap().anchor;
            let old_head = editor.read(cx).html_selection.as_ref().unwrap().head;
            for _ in 0..12 {
                _ = window.draw(cx);
                editor.update(cx, |editor, _| {
                    editor.drag_scroll.as_mut().unwrap().last_frame =
                        Instant::now() - Duration::from_millis(50);
                });
                window.simulate_next_frame(cx);
            }
            let selection = editor.read(cx).html_selection.as_ref().unwrap();
            assert_eq!(selection.anchor, anchor);
            assert!(
                selection.head > old_head,
                "scrolling must extend the selected range"
            );
            editor.update(cx, |editor, cx| {
                let bounds = editor.scroll_handle.bounds();
                editor.on_mouse_up(
                    &MouseUpEvent {
                        position: point(bounds.left() + px(100.), bounds.bottom() - px(2.)),
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                assert!(editor.drag_scroll.is_none());
            });
            let stopped = editor.read(cx).scroll_metrics().0;
            window.simulate_next_frame(cx);
            assert_eq!(editor.read(cx).scroll_metrics().0, stopped);
            assert_eq!(
                editor.read(cx).document.snapshot().serialize().unwrap(),
                source
            );
        });
    }

    #[gpui::test]
    fn native_pointer_cross_preview_selection_copies_and_edits_one_transaction(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        for (start_index, end_index) in [(1, 3), (0, 3), (1, 4), (0, 4)] {
            for reverse in [false, true] {
                let (editor, cx) = cx.add_window_view(|window, cx| {
                    RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
                });
                let cx: &mut gpui::VisualTestContext = cx;
                cx.run_until_parked();
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.update(|window, cx| {
                    editor.update(cx, |editor, cx| {
                        let snapshot = editor.document.snapshot();
                        let a = snapshot.blocks().get(1).unwrap().id();
                        let b = snapshot.blocks().get(3).unwrap().id();
                        let start_node = snapshot.blocks().get(start_index).unwrap().id();
                        let end_node = snapshot.blocks().get(end_index).unwrap().id();
                        let start_byte = if start_index == 1 { 6 } else { 2 };
                        let start_point = text_point(editor, start_node, start_byte);
                        let end_point = text_point(editor, end_node, 4);
                        let (start, end) = if reverse {
                            (end_point, start_point)
                        } else {
                            (start_point, end_point)
                        };
                        editor.on_mouse_down(
                            &MouseDownEvent {
                                position: start,
                                button: MouseButton::Left,
                                click_count: 1,
                                ..Default::default()
                            },
                            window,
                            cx,
                        );
                        editor.on_mouse_move(
                            &MouseMoveEvent {
                                position: end,
                                pressed_button: Some(MouseButton::Left),
                                ..Default::default()
                            },
                            window,
                            cx,
                        );
                        editor.on_mouse_up(
                            &MouseUpEvent {
                                position: end,
                                button: MouseButton::Left,
                                ..Default::default()
                            },
                            window,
                            cx,
                        );
                        let selection = editor.html_selection.as_ref().unwrap();
                        assert!(
                            selection.cross.is_some(),
                            "drag must cross the fragment boundary"
                        );
                        assert_eq!(selection.head < selection.anchor, reverse);
                        assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
                        assert_eq!(editor.document.snapshot().revision(), snapshot.revision());
                        assert!(
                            !editor
                                .html_selection_chrome(a, MineralPalette::LIGHT, true)
                                .is_empty()
                        );
                        assert!(
                            !editor
                                .html_selection_chrome(b, MineralPalette::LIGHT, true)
                                .is_empty()
                        );
                        let selected = selection
                            .cross
                            .as_ref()
                            .unwrap()
                            .projected_range(selection.range())
                            .unwrap();
                        assert!(editor.projection.text()[selected].contains("Middle"));
                        editor.copy(&Copy, window, cx);
                        let words = ["Before", "Alpha café", "Middle", "Beta tail", "After"];
                        let mut fragments = words[start_index..=end_index]
                            .iter()
                            .map(|text| text.to_string())
                            .collect::<Vec<_>>();
                        fragments[0] = words[start_index][start_byte..].to_string();
                        *fragments.last_mut().unwrap() = words[end_index][..4].to_string();
                        assert_eq!(
                            cx.read_from_clipboard().unwrap().text().unwrap(),
                            fragments
                                .iter()
                                .map(|text| text
                                    .replace("café", "**café**")
                                    .replace("tail", "*tail*"))
                                .collect::<Vec<_>>()
                                .join("\n\n")
                        );
                        editor.replace_text_in_range(None, "X", window, cx);
                        assert!(editor.html_selection.is_none());
                        assert_eq!(
                            editor
                                .document
                                .snapshot()
                                .blocks()
                                .get(start_index)
                                .unwrap()
                                .plain_text(),
                            format!(
                                "{}X{}",
                                &words[start_index][..start_byte],
                                &words[end_index][4..]
                            )
                        );
                        editor.undo(&Undo, window, cx);
                        assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
                    })
                });
            }
        }
    }

    #[gpui::test]
    fn cross_preview_selection_survives_reflow_and_composition_cancellation(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let snapshot = editor.document.snapshot();
                let start = html_point(editor, snapshot.blocks().get(1).unwrap().id(), 6);
                let end = html_point(editor, snapshot.blocks().get(3).unwrap().id(), 4);
                editor.on_mouse_down(
                    &MouseDownEvent {
                        position: start,
                        button: MouseButton::Left,
                        click_count: 1,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                editor.is_selecting = true;
                editor.on_mouse_move(
                    &MouseMoveEvent {
                        position: end,
                        pressed_button: Some(MouseButton::Left),
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                editor.on_mouse_up(
                    &MouseUpEvent {
                        position: end,
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                let selected = editor.selected_text_range(false, window, cx).unwrap();
                let mut actual = None;
                assert_eq!(
                    editor
                        .text_for_range(selected.range.clone(), &mut actual, window, cx)
                        .unwrap(),
                    "café\nMiddle\nBeta"
                );
                editor.replace_and_mark_text_in_range(None, "仮", None, window, cx);
                assert!(editor.document.composition_active());
                assert_eq!(
                    editor.document.snapshot().serialize().unwrap(),
                    "Before\n\nAlpha 仮 *tail*\n\nAfter\n"
                );
                editor.replace_and_mark_text_in_range(None, "確定", Some(0..2), window, cx);
                assert_eq!(
                    editor.document.snapshot().serialize().unwrap(),
                    "Before\n\nAlpha 確定 *tail*\n\nAfter\n"
                );
                editor.cancel_pending_composition(cx).unwrap();
                assert!(!editor.document.composition_active());
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
                assert_eq!(
                    editor.selected_text_range(false, window, cx).unwrap().range,
                    selected.range
                );
                editor.set_zoom_factor(1.5, cx);
                editor.refresh_projection();
                assert!(editor.html_selection.as_ref().unwrap().cross.is_some());
                editor.copy(&Copy, window, cx);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().unwrap(),
                    "**café**\n\nMiddle\n\nBeta"
                );
                editor.select_left(&SelectLeft, window, cx);
                assert!(
                    editor
                        .html_selection
                        .as_ref()
                        .unwrap()
                        .position(editor.cursor_offset(), Affinity::Upstream)
                        .is_some()
                );
                editor.select_all(&SelectAll, window, cx);
                editor.copy(&Copy, window, cx);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().unwrap(),
                    "Before\n\nAlpha **café**\n\nMiddle\n\nBeta *tail*\n\nAfter"
                );
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
                editor.replace_and_mark_text_in_range(None, "全", Some(0..1), window, cx);
                assert!(editor.document.composition_active());
                assert_eq!(
                    editor.projection.segments().len(),
                    1,
                    "removed blocks must leave the rendered projection"
                );
                assert!(!editor.projection.text().contains("Middle"));
                editor.replace_and_mark_text_in_range(None, "全部", Some(0..2), window, cx);
                editor.replace_text_in_range(None, "確定", window, cx);
                assert!(!editor.document.composition_active());
                assert!(editor.marked_range.is_none());
                assert_eq!(editor.document.snapshot().serialize().unwrap(), "確定\n");
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
            })
        });
    }
}
