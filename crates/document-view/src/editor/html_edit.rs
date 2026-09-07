//! Temporary selection on an immutable, source-verified HTML preview. The first
//! mutation goes through document-core; this is not a second editable document.
use super::*;

fn merge_selection_rects(mut rects: Vec<[f32; 4]>) -> Vec<[f32; 4]> {
    rects.sort_by(|a, b| a[1].total_cmp(&b[1]).then_with(|| a[0].total_cmp(&b[0])));
    let mut merged: Vec<[f32; 4]> = Vec::new();
    for rect in rects {
        if let Some(last) = merged.last_mut()
            && (last[1] - rect[1]).abs() < 0.5
            && (last[3] - rect[3]).abs() < 0.5
            && rect[0] - last[2] <= (rect[3] - rect[1]) * 0.8
        {
            last[2] = last[2].max(rect[2]);
        } else {
            merged.push(rect);
        }
    }
    merged
}

#[derive(Clone)]
pub(super) struct HtmlSelection {
    pub node: NodeId,
    pub preview: Arc<crate::html::HtmlPreview>,
    pub anchor: usize,
    pub head: usize,
    pub cross: Option<Arc<super::preview_selection::PreviewSelectionProjection>>,
}

/// Only the view holding this state may update/commit the session's preedit.
pub(super) enum CompositionOrigin {
    Markdown,
    Html(HtmlSelection),
}

impl HtmlSelection {
    pub fn text(&self) -> &str {
        self.cross
            .as_ref()
            .map_or(&self.preview.editable_text, |cross| &cross.text)
    }

    pub fn position(
        &self,
        byte: usize,
        affinity: Affinity,
    ) -> Option<document_core::PreviewPosition> {
        if let Some(cross) = &self.cross {
            return cross.position(byte, affinity);
        }
        Some(document_core::PreviewPosition::Html {
            node_id: self.node,
            expected_source: self.preview.source.clone(),
            position: self.preview.position_for_byte(byte)?,
        })
    }

    pub fn range(&self) -> Range<usize> {
        self.anchor.min(self.head)..self.anchor.max(self.head)
    }

    pub fn command(&self, edit: document_core::HtmlTextEdit) -> Result<EditCommand, DocumentError> {
        if let Some(cross) = &self.cross {
            return Ok(EditCommand::EditPreviewSelection {
                selection: cross
                    .selection(self.anchor, self.head)
                    .ok_or_else(|| DocumentError::Html("Choose the preview text again".into()))?,
                edit,
            });
        }
        let position = |byte| {
            self.preview
                .position_for_byte(byte)
                .ok_or_else(|| DocumentError::Html("Choose the HTML text again".into()))
        };
        Ok(EditCommand::EditHtmlSelection {
            node_id: self.node,
            expected_source: self.preview.source.to_string(),
            anchor: position(self.anchor)?,
            head: position(self.head)?,
            edit,
        })
    }
}

impl RichDocumentEditor {
    pub(super) fn html_toolbar(
        &self,
        node: NodeId,
        preview: &crate::html::HtmlPreview,
        available: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let source = preview.source.clone();
        let text = preview.text.clone();
        let can_convert = preview.can_convert;
        let row = div()
            .h(px(32. * self.zoom_factor))
            .flex()
            .items_center()
            .gap_2();
        // Native buttons retain their UI font size during document zoom. Use
        // their actual available on-screen width, not unscaled prose width.
        if available < 220. {
            let editor = cx.entity().downgrade();
            let focus = self.focus_handle.clone();
            return row
                .child(
                    Button::new(("html-actions", node.get() as usize))
                        .ghost()
                        .small()
                        .label("HTML")
                        .when(available >= 76., |button| button.icon(IconName::Ellipsis))
                        .when(available < 76., |button| button.px_0())
                        .accessibility_id(format!("html-actions-{}", node.get()))
                        .tooltip("HTML actions: copy source, copy text, or edit text")
                        .dropdown_menu(move |menu, _, _| {
                            let source = source.clone();
                            let text = text.clone();
                            let editor = editor.clone();
                            menu.min_w(px(240.))
                                .max_w(px(240.))
                                .action_context(focus.clone())
                                .item(
                                    PopupMenuItem::new("Copy original HTML")
                                        .icon(IconName::Copy)
                                        .on_click(move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                source.to_string(),
                                            ))
                                        }),
                                )
                                .item(
                                    PopupMenuItem::new("Copy fragment text")
                                        .icon(IconName::CaseSensitive)
                                        .on_click(move |_, _, cx| {
                                            cx.write_to_clipboard(ClipboardItem::new_string(
                                                text.clone(),
                                            ))
                                        }),
                                )
                                .when(can_convert, |menu| {
                                    menu.separator().item(
                                        PopupMenuItem::new("Edit text")
                                            .icon(IconName::CaseSensitive)
                                            .on_click(move |_, window, cx| {
                                                let _ = editor.update(cx, |editor, cx| {
                                                    editor.apply_structural_command(
                                                        EditCommand::ConvertHtmlToMarkdown {
                                                            node_id: node,
                                                        },
                                                        window,
                                                        cx,
                                                    );
                                                });
                                            }),
                                    )
                                })
                        }),
                )
                .into_any_element();
        }
        row.child(Button::new(("copy-html", node.get() as usize))
            .ghost().small().icon(IconName::Copy).label("HTML")
            .tooltip("Copy original HTML source")
            .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(source.to_string()))))
            .child(Button::new(("copy-html-text", node.get() as usize))
                .ghost().small().icon(IconName::Copy)
                .tooltip("Copy fragment text")
                .on_click(move |_, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(text.clone()))))
            .when(can_convert, |row| row.child(Button::new(("edit-html", node.get() as usize))
                .ghost().small().label("Edit text")
                .tooltip("Convert to editable Markdown; replaces HTML styling and disclosure controls, retaining all body text. Or click rendered text and type to convert with your first edit. Undo restores the original HTML.")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.apply_structural_command(EditCommand::ConvertHtmlToMarkdown { node_id: node }, window, cx);
                }))))
            .into_any_element()
    }

    pub(super) fn html_link_chrome(
        &self,
        node: NodeId,
        preview: &crate::html::HtmlPreview,
    ) -> Vec<AnyElement> {
        let mut elements = Vec::new();
        for (index, link) in preview.links.iter().enumerate() {
            for (part, rect) in link.bounds.iter().enumerate() {
                let hint = if self.can_open_link(&link.target) {
                    format!(
                        "{} — Ctrl-click to open; Alt+Enter at the caret. Click to edit text.",
                        link.target
                    )
                } else {
                    format!(
                        "{} — Right-click to copy this address. Opening this link type is not supported.",
                        link.target
                    )
                };
                elements.push(
                    div()
                        .id(ElementId::Name(
                            format!("html-link-{}-{index}-{part}", node.get()).into(),
                        ))
                        .absolute()
                        .left(px(rect[0] * self.zoom_factor))
                        .top(px(rect[1] * self.zoom_factor))
                        .w(px((rect[2] - rect[0]) * self.zoom_factor))
                        .h(px((rect[3] - rect[1]) * self.zoom_factor))
                        .cursor_pointer()
                        .tooltip(move |window, cx| {
                            let text = hint.clone();
                            let width =
                                (f32::from(window.viewport_size().width) - 48.).clamp(120., 280.);
                            gpui_component::tooltip::Tooltip::element(move |_, _| {
                                div().w(px(width)).whitespace_normal().child(text.clone())
                            })
                            .build(window, cx)
                        })
                        .into_any_element(),
                );
            }
        }
        elements
    }

    pub(super) fn validate_html_range(&self, range: &Range<usize>) -> Result<(), DocumentError> {
        let valid = self.html_selection.as_ref().is_some_and(|selection| {
            range.start <= range.end
                && selection
                    .position(range.start, Affinity::Downstream)
                    .is_some()
                && selection.position(range.end, Affinity::Upstream).is_some()
        });
        if valid {
            Ok(())
        } else {
            Err(DocumentError::Html(
                "The HTML selection range is unavailable; select the text again".into(),
            ))
        }
    }

    pub(super) fn html_line_range(&self) -> Option<Range<usize>> {
        use document_core::PreviewPosition;
        let selection = self.html_selection.as_ref()?;
        let Some(cross) = &selection.cross else {
            return selection.preview.line_range(selection.head);
        };
        match cross.position(selection.head, Affinity::Downstream)? {
            PreviewPosition::Document(position) => {
                let offset = self.projection.offset_of(position)?;
                let range = visual_line_range_at(self, offset);
                let start = self
                    .projection
                    .position_at(range.start, Affinity::Downstream)?;
                let end = self.projection.position_at(range.end, Affinity::Upstream)?;
                Some(
                    cross.byte(&PreviewPosition::Document(start))?
                        ..cross.byte(&PreviewPosition::Document(end))?,
                )
            }
            PreviewPosition::Html {
                node_id,
                expected_source,
                position,
            } => {
                let preview = self.visual_lines.iter().find_map(|line| {
                    (self.projection.segment_for_range(&line.range)?.node_id == node_id)
                        .then_some(line.html_preview.as_ref())
                        .flatten()
                        .filter(|preview| preview.source == expected_source)
                })?;
                let range = preview.line_range(preview.byte_for_position(position)?)?;
                let map = |byte| {
                    cross.byte(&PreviewPosition::Html {
                        node_id,
                        expected_source: expected_source.clone(),
                        position: preview.position_for_byte(byte)?,
                    })
                };
                Some(map(range.start)?..map(range.end)?)
            }
        }
    }

    pub(super) fn editing_text(&self) -> &str {
        self.html_selection
            .as_ref()
            .map_or_else(|| self.projection.text(), |selection| selection.text())
    }

    pub(super) fn refresh_html_selection(&mut self) {
        if self
            .html_selection
            .as_ref()
            .is_some_and(|selection| selection.cross.is_some())
        {
            let next = super::preview_selection::PreviewSelectionProjection::build(self);
            let selection = self.html_selection.as_mut().unwrap();
            let previous = selection.cross.as_ref().unwrap();
            if previous.revision == next.revision && previous.text == next.text {
                selection.cross = Some(Arc::new(next));
            } else {
                self.html_selection = None;
            }
            return;
        }
        let Some(selection) = &mut self.html_selection else {
            return;
        };
        let preview = self.visual_lines.iter().find_map(|line| {
            let segment = self.projection.segment_for_range(&line.range)?;
            (segment.node_id == selection.node)
                .then_some(line.html_preview.as_ref())
                .flatten()
        });
        if let Some(preview) = preview.filter(|preview| {
            preview.source == selection.preview.source
                && preview.editable_text == selection.preview.editable_text
                && !preview.text_hits.is_empty()
        }) {
            selection.preview = preview.clone();
        } else {
            self.html_selection = None;
        }
    }

    pub(super) fn replace_html_range(
        &mut self,
        range: Range<usize>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.stop_momentum();
        if let Err(error) = self.validate_html_range(&range) {
            self.record_error(error, window);
            return false;
        }
        if range.is_empty() && text.is_empty() {
            return false;
        }
        self.set_selection(range, false, window, cx);
        match self.apply_command(EditCommand::ReplaceSelection {
            text: text.to_owned(),
            typing: false,
        }) {
            Ok(result) => {
                self.refresh_after_transaction(&result);
                self.last_error = None;
                self.keep_offset_visible(self.cursor_offset());
                cx.emit(EditorEvent::Changed);
                cx.notify();
                true
            }
            Err(error) => {
                self.record_error(error, window);
                false
            }
        }
    }

    pub(super) fn html_caret_bounds(&self, byte: usize) -> Option<Bounds<Pixels>> {
        let selection = self.html_selection.as_ref()?;
        let (node, preview, byte) = if let Some(cross) = &selection.cross {
            match cross.position(byte, Affinity::Downstream)? {
                document_core::PreviewPosition::Document(position) => {
                    let offset = self.projection.offset_of(position)?;
                    let line = painted_line_for_offset(&self.painted_lines, offset)?;
                    return Some(Bounds::new(
                        point(
                            aligned_text_left(line.bounds, &line.layout, line.alignment)
                                + line.layout.x_for_index(
                                    offset
                                        .saturating_sub(line.range.start)
                                        .min(line.range.len()),
                                ),
                            line.bounds.top(),
                        ),
                        size(px(1.5), line.bounds.size.height),
                    ));
                }
                document_core::PreviewPosition::Html {
                    node_id, position, ..
                } => {
                    let preview = self.visual_lines.iter().find_map(|line| {
                        (self.projection.segment_for_range(&line.range)?.node_id == node_id)
                            .then_some(line.html_preview.as_ref())
                            .flatten()
                    })?;
                    (node_id, preview, preview.byte_for_position(position)?)
                }
            }
        } else {
            (selection.node, &selection.preview, byte)
        };
        let line = self.visual_lines.iter().find(|line| {
            self.projection
                .segment_for_range(&line.range)
                .is_some_and(|s| s.node_id == node)
        })?;
        let [x, y, right, bottom] = preview.caret_bounds(byte)?;
        let bounds = self.element_bounds?;
        let scroll = segment_for_line(&self.projection, &line.range)
            .and_then(|segment| horizontal_scroll_owner(&self.projection, segment))
            .and_then(|owner| self.horizontal_scrolls.get(&owner).copied())
            .unwrap_or(0.);
        let left = bounds.left()
            + px(line.x_fraction * f32::from(bounds.size.width) + line.inset - scroll);
        Some(Bounds::new(
            point(
                left + px(x * self.zoom_factor),
                bounds.top() + px(line.y + (32. + y) * self.zoom_factor),
            ),
            size(
                px((right - x) * self.zoom_factor),
                px((bottom - y) * self.zoom_factor),
            ),
        ))
    }

    pub(super) fn html_selection_chrome(
        &self,
        node: NodeId,
        palette: MineralPalette,
        active: bool,
    ) -> Vec<AnyElement> {
        let Some(selection) = self.html_selection.as_ref() else {
            return Vec::new();
        };
        let (preview, range, head) = if let Some(cross) = &selection.cross {
            let Some(local) = cross.html_range(node, selection.range(), selection.head) else {
                return Vec::new();
            };
            if local.1.is_empty() && !selection.range().is_empty() {
                return Vec::new();
            }
            local
        } else if selection.node == node {
            (selection.preview.clone(), selection.range(), selection.head)
        } else {
            return Vec::new();
        };
        if range.is_empty() && !active {
            return Vec::new();
        }
        let rects = if range.is_empty() {
            preview.caret_bounds(head).into_iter().collect::<Vec<_>>()
        } else {
            preview
                .text_hits
                .iter()
                .filter_map(|hit| {
                    let a = preview.byte_for_position(hit.left)?;
                    let b = preview.byte_for_position(hit.right)?;
                    (a.min(b) < range.end && a.max(b) > range.start).then_some(hit.bounds)
                })
                .collect()
        };
        let rects = if range.is_empty() {
            rects
        } else {
            merge_selection_rects(rects)
        };
        rects
            .into_iter()
            .map(|[left, top, right, bottom]| {
                div()
                    .absolute()
                    .left(px(left * self.zoom_factor))
                    .top(px(top * self.zoom_factor))
                    .w(px((right - left) * self.zoom_factor))
                    .h(px((bottom - top) * self.zoom_factor))
                    // The raster is opaque, so selection lies over its pixels.
                    // A light selection token could match authored backgrounds
                    // and simply wash out the glyphs. Use a restrained accent
                    // wash plus a crisp baseline cue, keeping text readable.
                    .bg(rgba(MineralPalette::with_alpha(
                        palette.accent,
                        if range.is_empty() {
                            255
                        } else if active {
                            32
                        } else {
                            18
                        },
                    )))
                    .when(!range.is_empty(), |this| {
                        this.border_b_2()
                            .border_color(rgba(MineralPalette::with_alpha(
                                palette.accent,
                                if active { 255 } else { 128 },
                            )))
                    })
                    .into_any_element()
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::merge_selection_rects;

    #[test]
    fn highlight_bridges_word_spaces_but_not_other_rows_or_columns() {
        assert_eq!(
            merge_selection_rects(vec![
                [20., 0., 30., 20.],
                [0., 0., 10., 20.],
                [100., 0., 110., 20.],
                [0., 24., 10., 44.],
            ]),
            vec![
                [0., 0., 30., 20.],
                [100., 0., 110., 20.],
                [0., 24., 10., 44.]
            ]
        );
    }
}
