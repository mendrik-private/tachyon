//! Source-addressed keyboard movement between retained HTML and Markdown text.
use super::*;
use document_core::PreviewPosition;

impl RichDocumentEditor {
    fn navigation_preview_for_segment(
        &self,
        segment: &crate::ProjectionSegment,
    ) -> Option<Arc<crate::html::HtmlPreview>> {
        if !segment.context.preserved_source {
            return None;
        }
        self.visual_lines.iter().find_map(|line| {
            (self
                .projection
                .segment_for_range(&line.projected_range())?
                .node_id
                == segment.node_id)
                .then_some(line.html_preview.as_ref())
                .flatten()
                .filter(|preview| preview.can_convert)
                .cloned()
        })
    }

    /// Logical source-order movement. Ordinary text-only movement stays with
    /// the existing navigator; preview boundaries never traverse placeholder
    /// labels or manufacture editable positions inside opaque HTML.
    pub(super) fn navigate_preview_horizontal(
        &mut self,
        direction: isize,
        word: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let active = self.html_selection.is_some();
        let (range, _) = self.selected_byte_range();
        let collapse = !extend && !range.is_empty();
        let origin = if collapse {
            let byte = if direction < 0 {
                range.start
            } else {
                range.end
            };
            if let Some(selection) = &self.html_selection {
                selection.position(byte, Affinity::Downstream)
            } else {
                self.projection
                    .position_at(byte, Affinity::Downstream)
                    .map(PreviewPosition::Document)
            }
        } else {
            self.navigation_head()
        };
        let Some(origin) = origin else { return active };
        if collapse && !word {
            if active {
                self.navigate_preview_position(origin, false, window, cx);
            }
            return active;
        }
        let node = match &origin {
            PreviewPosition::Document(position) => position.node_id,
            PreviewPosition::Html { node_id, .. } => *node_id,
        };
        let segments = self.projection.segments();
        let Some(index) = segments.iter().position(|segment| {
            segment.node_id == node
                && match &origin {
                    PreviewPosition::Document(position) => {
                        segment.node_range.contains(&position.text_offset)
                            || segment.node_range.end == position.text_offset
                    }
                    PreviewPosition::Html { .. } => true,
                }
        }) else {
            return active;
        };
        let segment = &segments[index];
        let preview = self.navigation_preview_for_segment(segment);
        let local = match &origin {
            PreviewPosition::Document(position) => position.text_offset - segment.node_range.start,
            PreviewPosition::Html {
                expected_source,
                position,
                ..
            } => {
                let Some(byte) = preview
                    .as_ref()
                    .filter(|preview| preview.source == *expected_source)
                    .and_then(|preview| preview.byte_for_position(*position))
                else {
                    return true;
                };
                byte
            }
        };
        let text = preview.as_ref().map_or_else(
            || &self.projection.text()[segment.projection_range()],
            |preview| preview.editable_text.as_str(),
        );
        let at_edge = if direction < 0 {
            local == 0
        } else {
            local == text.len()
        };
        let target = if at_edge {
            let neighbor = index
                .checked_add_signed(direction)
                .and_then(|i| segments.get(i));
            if !active && !neighbor.is_some_and(|s| s.context.preserved_source) {
                return false;
            }
            let Some(neighbor) = neighbor else {
                return true;
            };
            let preview = self.navigation_preview_for_segment(neighbor);
            if neighbor.context.preserved_source && preview.is_none() {
                return true;
            }
            let text = preview.as_ref().map_or_else(
                || &self.projection.text()[neighbor.projection_range()],
                |preview| preview.editable_text.as_str(),
            );
            let edge = if direction < 0 { text.len() } else { 0 };
            let byte = if word {
                horizontal_boundary(text, edge, direction, true)
            } else {
                edge
            };
            horizontal_position(neighbor, preview.as_deref(), byte, direction)
        } else {
            if !active {
                return false;
            }
            horizontal_position(
                segment,
                preview.as_deref(),
                horizontal_boundary(text, local, direction, word),
                direction,
            )
        };
        if let Some(target) = target {
            self.navigate_preview_position(target, extend, window, cx);
        }
        true
    }

    pub(super) fn navigate_line_boundary(
        &mut self,
        end: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferred_x = None;
        let range = self
            .html_line_range()
            .unwrap_or_else(|| visual_line_range_at(self, self.cursor_offset()));
        let offset = if end { range.end } else { range.start };
        if let Some(selection) = &self.html_selection {
            if let Some(position) = selection.position(
                offset,
                if end {
                    Affinity::Upstream
                } else {
                    Affinity::Downstream
                },
            ) {
                self.navigate_preview_position(position, extend, window, cx);
            }
        } else {
            if extend {
                self.select_to(offset, window, cx);
            } else {
                self.move_to(offset, window, cx);
            }
            self.keep_offset_visible(offset);
        }
    }

    fn navigation_head(&self) -> Option<PreviewPosition> {
        if let Some(selection) = &self.html_selection {
            selection.position(selection.head, Affinity::Downstream)
        } else if let Selection::Text(selection) = &self.selection {
            Some(PreviewPosition::Document(selection.head))
        } else {
            None
        }
    }

    fn navigation_line(&self, position: &PreviewPosition) -> Option<usize> {
        match position {
            PreviewPosition::Document(position) => {
                let offset = self.projection.offset_of(*position)?;
                self.visual_lines
                    .iter()
                    .position(|line| line.projected_range().contains(&offset))
                    .or_else(|| {
                        self.visual_lines
                            .iter()
                            .position(|line| line.projected_end() == offset)
                    })
            }
            PreviewPosition::Html {
                node_id,
                expected_source,
                ..
            } => self.visual_lines.iter().position(|line| {
                line.html_preview
                    .as_ref()
                    .is_some_and(|preview| preview.source == *expected_source)
                    && self
                        .projection
                        .segment_for_range(&line.projected_range())
                        .is_some_and(|s| s.node_id == *node_id)
            }),
        }
    }

    fn navigation_line_left(&self, line: &VisualLineSpec) -> Option<Pixels> {
        let bounds = self.element_bounds?;
        let scroll = segment_for_line(&self.projection, &line.projected_range())
            .and_then(|segment| horizontal_scroll_owner(&self.projection, segment))
            .and_then(|owner| self.horizontal_scrolls.get(&owner).copied())
            .unwrap_or(0.);
        Some(
            bounds.left()
                + px(line.x_fraction * f32::from(bounds.size.width) + line.inset - scroll),
        )
    }

    pub(super) fn navigate_preview_position(
        &mut self,
        target: PreviewPosition,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.is_selecting = false;
        self.stop_drag_scroll();
        self.stop_momentum();
        if extend && self.extend_to_preview_position(target.clone()) {
            self.keep_offset_visible(self.cursor_offset());
            self.schedule_selection_toolbar(cx);
            cx.notify();
            return;
        }
        match target {
            PreviewPosition::Document(position) => {
                let Some(offset) = self.projection.offset_of(position) else {
                    return;
                };
                // A collapsed ordinary caret is ordinary editing, including
                // composition. Don't leave it in the cross-preview buffer.
                self.html_selection = None;
                if extend {
                    self.select_to(offset, window, cx);
                } else {
                    self.move_to(offset, window, cx);
                }
                self.keep_offset_visible(offset);
            }
            PreviewPosition::Html {
                node_id,
                expected_source,
                position,
            } => {
                let Some(preview) = self.visual_lines.iter().find_map(|line| {
                    (self
                        .projection
                        .segment_for_range(&line.projected_range())?
                        .node_id
                        == node_id)
                        .then_some(line.html_preview.as_ref())
                        .flatten()
                        .filter(|preview| preview.source == expected_source && preview.can_convert)
                        .cloned()
                }) else {
                    return;
                };
                let Some(byte) = preview.byte_for_position(position) else {
                    return;
                };
                self.html_selection = Some(HtmlSelection {
                    node: node_id,
                    preview,
                    anchor: byte,
                    head: byte,
                    cross: None,
                });
                self.sync_layout_focus(window, cx);
                self.keep_offset_visible(byte);
                self.schedule_selection_toolbar(cx);
                cx.notify();
            }
        }
    }

    /// Return false only when the normal Markdown navigator owns this move.
    pub(super) fn navigate_preview_vertical(
        &mut self,
        direction: isize,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(head) = self.navigation_head() else {
            return self.html_selection.is_some();
        };
        let Some(index) = self.navigation_line(&head) else {
            return self.html_selection.is_some();
        };
        let line = &self.visual_lines[index];
        let neighbor = visual_vertical_neighbor(&self.visual_lines, index, direction);
        if self.html_selection.is_none()
            && !neighbor.is_some_and(|index| self.visual_lines[index].html_preview.is_some())
        {
            return false;
        }
        let current_x = match &head {
            PreviewPosition::Html { position, .. } => {
                let Some(preview) = &line.html_preview else {
                    return true;
                };
                let Some(byte) = preview.byte_for_position(*position) else {
                    return true;
                };
                let Some(caret) = preview.caret_bounds(byte) else {
                    return true;
                };
                self.navigation_line_left(line)
                    .map(|left| left + px(caret[0] * self.zoom_factor))
            }
            PreviewPosition::Document(position) => {
                self.projection.offset_of(*position).and_then(|offset| {
                    self.painted_lines
                        .iter()
                        .find(|painted| painted.range == line.projected_range())
                        .map(|painted| {
                            aligned_text_left(painted.bounds, &painted.layout, painted.alignment)
                                + shaped_x_for_index(
                                    &painted.layout,
                                    offset
                                        .saturating_sub(painted.range.start)
                                        .min(painted.range.len()),
                                )
                        })
                })
            }
        };
        let Some(x) = self.preferred_x.or(current_x) else {
            return true;
        };
        self.preferred_x = Some(x);
        if let PreviewPosition::Html {
            node_id,
            expected_source,
            position,
        } = &head
        {
            let preview = line.html_preview.as_ref().unwrap();
            let local_x =
                f32::from(x - self.navigation_line_left(line).unwrap()) / self.zoom_factor;
            if let Some(position) =
                preview.vertical_position(preview.byte_for_position(*position), direction, local_x)
            {
                self.navigate_preview_position(
                    PreviewPosition::Html {
                        node_id: *node_id,
                        expected_source: expected_source.clone(),
                        position,
                    },
                    extend,
                    window,
                    cx,
                );
                return true;
            }
        }
        let Some(neighbor) = neighbor else {
            return true;
        };
        let target = &self.visual_lines[neighbor];
        let position = if let Some(preview) = &target.html_preview {
            let Some(left) = self.navigation_line_left(target) else {
                return true;
            };
            let Some(position) =
                preview.vertical_position(None, direction, f32::from(x - left) / self.zoom_factor)
            else {
                return true;
            };
            let Some(segment) = self.projection.segment_for_range(&target.projected_range()) else {
                return true;
            };
            PreviewPosition::Html {
                node_id: segment.node_id,
                expected_source: preview.source.clone(),
                position,
            }
        } else {
            let offset = if let Some(painted) = self
                .painted_lines
                .iter()
                .find(|line| line.range == target.projected_range())
            {
                target.projected_start()
                    + shaped_index_for_x(
                        &painted.layout,
                        x - aligned_text_left(painted.bounds, &painted.layout, painted.alignment),
                    )
                    .min(target.projected_range().len())
            } else {
                // Match the existing offscreen Markdown navigation fallback;
                // the next paint supplies exact shaping after caret reveal.
                let left = self.navigation_line_left(target).unwrap_or(x);
                let width = (self.layout_width * target.width_fraction - target.inset).max(1.);
                let ratio = (f32::from(x - left) / width).clamp(0., 1.);
                snap_offset_to_grapheme(
                    self.projection.text(),
                    target.projected_range(),
                    target.projected_start()
                        + (ratio * target.projected_range().len() as f32).round() as usize,
                )
            };
            let Some(position) = self.projection.position_at(offset, Affinity::Downstream) else {
                return true;
            };
            PreviewPosition::Document(position)
        };
        self.navigate_preview_position(position, extend, window, cx);
        true
    }
}

fn horizontal_boundary(text: &str, byte: usize, direction: isize, word: bool) -> usize {
    if word {
        if direction < 0 {
            previous_word_boundary(text, byte)
        } else {
            next_word_boundary(text, byte)
        }
    } else if direction < 0 {
        text[..byte]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(i, _)| i)
    } else {
        text[byte..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(text.len(), |(i, _)| byte + i)
    }
}

fn horizontal_position(
    segment: &crate::ProjectionSegment,
    preview: Option<&crate::html::HtmlPreview>,
    byte: usize,
    direction: isize,
) -> Option<PreviewPosition> {
    if let Some(preview) = preview {
        Some(PreviewPosition::Html {
            node_id: segment.node_id,
            expected_source: preview.source.clone(),
            position: preview.position_for_byte(byte)?,
        })
    } else if !segment.context.preserved_source {
        Some(PreviewPosition::Document(DocumentPosition::new(
            segment.node_id,
            segment.node_range.start + byte.min(segment.node_range.len()),
            if direction < 0 {
                Affinity::Upstream
            } else {
                Affinity::Downstream
            },
        )))
    } else {
        None
    }
}
