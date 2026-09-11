//! Column resizing uses logical document widths, independently of text zoom.

use super::*;

/// Same boundary for hit testing and cursor feedback; a horizontally clipped
/// column must not expose its offscreen resize target over neighboring content.
pub(super) fn hit_bounds(
    line: Bounds<Pixels>,
    zoom: f32,
    mask: Option<ContentMask<Pixels>>,
) -> Option<Bounds<Pixels>> {
    let edge = line.right() + px(12. * zoom);
    if mask.is_some_and(|mask| edge < mask.bounds.left() || edge > mask.bounds.right() + px(0.5)) {
        return None;
    }
    Some(Bounds::new(
        point(edge - px(6.), line.top()),
        size(px(12.), line.size.height),
    ))
}

impl TableResizeDrag {
    pub(super) fn width_at(self, pointer_x: Pixels, zoom: f32) -> f32 {
        (self.initial_width + f32::from(pointer_x - self.pointer_x) / zoom).max(32.)
    }
}

impl RichDocumentEditor {
    fn table_resize_guide(&self) -> Option<Bounds<Pixels>> {
        let drag = self.table_resize_drag?;
        let element = self.element_bounds?;
        let mut rows = self.visual_lines.iter().filter(|line| {
            line.table_cell
                .is_some_and(|(table, _, _, _)| table == drag.table_id)
        });
        let first = rows.next()?;
        let mut top = first.table_row_y;
        let mut bottom = top + first.table_row_height;
        for row in rows {
            top = top.min(row.table_row_y);
            bottom = bottom.max(row.table_row_y + row.table_row_height);
        }
        Some(Bounds::new(
            point(
                drag.edge_x - element.left()
                    + px((drag.current_width - drag.initial_width) * self.zoom_factor),
                px(top),
            ),
            size(px(2.), px(bottom - top)),
        ))
    }

    pub(super) fn table_resize_feedback(&self, palette: MineralPalette) -> Option<AnyElement> {
        let drag = self.table_resize_drag?;
        let guide = self.table_resize_guide()?;
        // The guide is provisional view state. Reflow and source persistence
        // happen once on release, so moving the pointer never floods undo.
        Some(
            div()
                .id("table-column-resize-preview")
                .role(Role::Status)
                .aria_label(format!("Column width: {:.0} px", drag.current_width))
                .aria_description("Logical document pixels. Release to apply; Escape to cancel.")
                .absolute()
                .left(guide.left())
                .top(guide.top())
                .w(guide.size.width)
                .h(guide.size.height)
                .bg(rgb(palette.accent))
                .cursor(CursorStyle::ResizeColumn)
                .child(
                    div()
                        .absolute()
                        .right(px(6.))
                        .top(px(-30.))
                        .px(px(8.))
                        .py(px(4.))
                        .rounded(px(4.))
                        .bg(rgb(palette.floating))
                        .border_1()
                        .border_color(rgb(palette.accent))
                        .text_color(rgb(palette.text))
                        .text_size(px(12.))
                        .line_height(px(16.))
                        .whitespace_nowrap()
                        .child(format!("{:.0} px", drag.current_width)),
                )
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::hit_bounds;
    use crate::init_editor;

    const SOURCE: &str = concat!(
        "<!-- mineral-table:v1 {\"border\":\"LogicalPixel\",\"widths\":[100,180]} -->\n",
        "| A | B |\n| --- | --- |\n| one | two |\n",
    );

    #[test]
    fn column_resize_cursor_matches_visible_boundary_not_clipped_columns() {
        let line = Bounds::new(point(px(12.), px(20.)), size(px(76.), px(28.)));
        let mask = ContentMask {
            bounds: Bounds::new(point(px(0.), px(20.)), size(px(100.), px(28.))),
        };
        let hit = hit_bounds(line, 1., Some(mask)).unwrap();
        assert_eq!(hit.left(), px(94.));
        assert_eq!(hit.right(), px(106.));
        assert!(hit.contains(&point(px(100.), px(30.))));
        let clipped = ContentMask {
            bounds: Bounds::new(mask.bounds.origin, size(px(80.), px(28.))),
        };
        assert!(hit_bounds(line, 1., Some(clipped)).is_none());
    }

    fn start_resize(
        editor: &mut RichDocumentEditor,
        window: &mut Window,
        cx: &mut Context<RichDocumentEditor>,
    ) -> (Point<Pixels>, TableResizeDrag) {
        // Actual painted geometry, away from the centered insertion knob.
        let start = editor
            .painted_lines
            .iter()
            .find_map(|line| {
                let (target, _) = editor.table_cell_geometry(line)?;
                if target.row != 1 || target.column != 0 {
                    return None;
                }
                let point = point(
                    line.bounds.right() + px(12. * editor.zoom_factor),
                    line.bounds.top() + px(1.),
                );
                (editor.table_edge_at(point).is_none()).then_some(point)
            })
            .expect("painted resize boundary away from insertion controls");
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
        (
            start,
            editor
                .table_resize_drag
                .expect("actual mouse down starts resize"),
        )
    }

    #[gpui::test]
    fn column_resize_mouse_release_converts_screen_delta_to_logical_width(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            let mut editor =
                RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx);
            editor.set_zoom_factor(2., cx);
            editor
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let (start, drag) = start_resize(editor, window, cx);
                let initial_guide = editor
                    .table_resize_guide()
                    .expect("visible guide at drag start");
                assert_eq!(drag.initial_width, 100.);
                let end = start + point(px(80.), px(0.));
                editor.on_mouse_move(
                    &MouseMoveEvent {
                        position: end,
                        pressed_button: Some(MouseButton::Left),
                        modifiers: Default::default(),
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    editor.document.snapshot().serialize().unwrap(),
                    SOURCE,
                    "provisional movement must not change source"
                );
                let guide = editor
                    .table_resize_guide()
                    .expect("live guide while dragging");
                assert_eq!(guide.left() - initial_guide.left(), px(80.));
                assert_eq!(editor.table_resize_drag.unwrap().current_width, 140.);
                assert_eq!(guide.size.height, initial_guide.size.height);
                editor.on_mouse_up(
                    &MouseUpEvent {
                        position: end,
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                let BlockNode::Table(table) = editor.projection.block(drag.table_id).unwrap()
                else {
                    panic!("table survives resizing");
                };
                assert_eq!(
                    table.columns[0].width,
                    Some(140.),
                    "80 screen pixels at 200% zoom are 40 logical pixels"
                );
                assert_eq!(table.columns[1].width, Some(180.));
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
            });
        });
    }

    #[gpui::test]
    fn column_resize_cancel_and_no_movement_leave_source_and_history_untouched(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        for action in ["click", "escape", "lost-button", "zoom"] {
            let source = SOURCE.replace("[100,180]", "[null,180]");
            let (editor, view_cx) = cx.add_window_view(|window, cx| {
                let mut editor = RichDocumentEditor::new(
                    Document::from_markdown(source.as_str()).unwrap(),
                    window,
                    cx,
                );
                editor.set_zoom_factor(2., cx);
                editor
            });
            view_cx.run_until_parked();
            view_cx.update(|window, cx| {
                _ = window.draw(cx);
                editor.update(cx, |editor, cx| {
                    let (start, _) = start_resize(editor, window, cx);
                    let end = if action == "click" {
                        start
                    } else {
                        start + point(px(80.), px(0.))
                    };
                    editor.on_mouse_move(
                        &MouseMoveEvent {
                            position: end,
                            pressed_button: Some(MouseButton::Left),
                            modifiers: Default::default(),
                        },
                        window,
                        cx,
                    );
                    match action {
                        "escape" => editor.dismiss(&Dismiss, window, cx),
                        "lost-button" => editor.on_mouse_move(
                            &MouseMoveEvent {
                                position: end,
                                pressed_button: None,
                                modifiers: Default::default(),
                            },
                            window,
                            cx,
                        ),
                        "zoom" => editor.set_zoom_factor(1., cx),
                        _ => {}
                    }
                    editor.on_mouse_up(
                        &MouseUpEvent {
                            position: end,
                            button: MouseButton::Left,
                            ..Default::default()
                        },
                        window,
                        cx,
                    );
                    assert!(editor.table_resize_drag.is_none(), "{action}: drag ended");
                    assert_eq!(
                        editor.document.snapshot().serialize().unwrap(),
                        source,
                        "{action}: canceled or no-op drag must not author automatic widths"
                    );
                    assert!(
                        matches!(editor.document.undo(), Err(DocumentError::NothingToUndo)),
                        "{action}: no undo entry"
                    );
                });
            });
        }
    }
}
