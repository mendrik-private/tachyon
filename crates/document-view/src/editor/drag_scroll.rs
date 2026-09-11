//! One cancellable frame chain for pointer-driven selection at viewport edges.
use super::*;

pub(super) fn register_outside_pointer(editor: &Entity<RichDocumentEditor>, window: &mut Window) {
    let editor = editor.downgrade();
    window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
        if phase != gpui::DispatchPhase::Bubble {
            return;
        }
        _ = editor.update(cx, |editor, cx| {
            if (editor.is_selecting || editor.table_resize_drag.is_some())
                && !editor.scroll_handle.bounds().contains(&event.position)
            {
                editor.on_mouse_move(event, window, cx);
            }
        });
    });
}

pub(super) struct DragScroll {
    pointer: Point<Pixels>,
    pub last_frame: Instant,
    revision: document_core::Revision,
    document_generation: u64,
}

fn edge_velocity(pointer: Point<Pixels>, bounds: Bounds<Pixels>) -> f32 {
    let band = f32::from(bounds.size.height).min(64.) * 0.5;
    if band <= 1. || pointer.x < bounds.left() || pointer.x > bounds.right() {
        return 0.;
    }
    let y = f32::from(pointer.y);
    let top = f32::from(bounds.top()) + band;
    let bottom = f32::from(bounds.bottom()) - band;
    if y < top {
        -(top - y).min(96.) * 18.
    } else if y > bottom {
        (y - bottom).min(96.) * 18.
    } else {
        0.
    }
}

impl RichDocumentEditor {
    pub(super) fn stop_drag_scroll(&mut self) {
        self.drag_scroll = None;
        self.drag_scroll_generation = self.drag_scroll_generation.wrapping_add(1);
    }

    pub(super) fn extend_drag_selection(
        &mut self,
        pointer: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = self.scroll_handle.bounds();
        let pointer = if bounds.size.height > px(2.) {
            point(
                pointer.x,
                pointer
                    .y
                    .clamp(bounds.top() + px(1.), bounds.bottom() - px(1.)),
            )
        } else {
            pointer
        };
        if self.extend_preview_selection(pointer) {
            cx.notify();
        } else if self.html_selection.is_none() {
            self.select_to(self.index_for_mouse_position(pointer), window, cx);
        }
    }

    pub(super) fn update_drag_scroll(
        &mut self,
        pointer: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.is_selecting || edge_velocity(pointer, self.scroll_handle.bounds()) == 0. {
            self.stop_drag_scroll();
            return;
        }
        if let Some(drag) = &mut self.drag_scroll {
            drag.pointer = pointer;
        } else {
            self.drag_scroll = Some(DragScroll {
                pointer,
                last_frame: Instant::now(),
                revision: self.document.snapshot().revision(),
                document_generation: self.projected_generation,
            });
            self.schedule_drag_scroll(window, cx);
        }
    }

    fn schedule_drag_scroll(&self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = cx.entity().downgrade();
        let generation = self.drag_scroll_generation;
        window.on_next_frame(move |window, cx| {
            _ = editor.update(cx, |editor, cx| {
                if editor.drag_scroll_generation != generation {
                    return;
                }
                if editor.advance_drag_scroll(Instant::now(), window, cx) {
                    editor.schedule_drag_scroll(window, cx);
                } else {
                    editor.stop_drag_scroll();
                }
            });
        });
        // A stationary pointer produces no input-driven invalidation.
        cx.notify();
    }

    fn advance_drag_scroll(
        &mut self,
        now: Instant,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(drag) = &mut self.drag_scroll else {
            return false;
        };
        if !self.is_selecting
            || !self.focus_handle.is_focused(window)
            || !window.is_window_active()
            || self.document.composition_active()
            || drag.revision != self.document.snapshot().revision()
            || drag.document_generation != self.projected_generation
        {
            return false;
        }
        let pointer = drag.pointer;
        let dt = now
            .saturating_duration_since(drag.last_frame)
            .as_secs_f32()
            .min(0.05);
        drag.last_frame = now;
        let bounds = self.scroll_handle.bounds();
        let velocity = edge_velocity(pointer, bounds);
        if velocity == 0. {
            return false;
        }
        // Hit-test the geometry from the just-painted frame, before moving
        // its viewport. Keep the original anchor; only extend the head.
        let hit = point(
            pointer.x,
            pointer
                .y
                .clamp(bounds.top() + px(1.), bounds.bottom() - px(1.)),
        );
        self.extend_drag_selection(hit, window, cx);
        let offset = self.scroll_handle.offset();
        let next = (offset.y - px(velocity * dt)).clamp(-self.scroll_handle.max_offset().y, px(0.));
        if next == offset.y {
            return false;
        }
        self.jump_generation = self.jump_generation.wrapping_add(1);
        self.scroll_handle.set_offset(point(offset.x, next));
        self.animate_toolbar(false, cx);
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edge_scroll_velocity_is_bounded_and_only_tracks_vertical_edges() {
        let bounds = Bounds::new(point(px(100.), px(30.)), size(px(600.), px(400.)));
        assert_eq!(edge_velocity(point(px(200.), px(200.)), bounds), 0.);
        assert_eq!(edge_velocity(point(px(90.), px(440.)), bounds), 0.);
        let down = edge_velocity(point(px(200.), bounds.bottom() - px(2.)), bounds);
        let up = edge_velocity(point(px(200.), bounds.top() + px(2.)), bounds);
        assert_eq!(down, -up);
        assert!(down > 0.);
        assert_eq!(
            edge_velocity(point(px(200.), px(1_000_000.)), bounds),
            1728.
        );
    }

    #[gpui::test]
    fn ordinary_drag_scroll_cancels_on_reentry_blur_revision_and_dismiss(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init_editor);
        for cancel in ["reentry", "blur", "revision", "dismiss", "lost_release"] {
            let source = "Ordinary paragraph.\n\n".repeat(100);
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
                    let line = &editor.painted_lines[0];
                    let start = point(line.bounds.left() + px(2.), line.bounds.top() + px(8.));
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
                    let edge = point(start.x, bounds.bottom() - px(2.));
                    editor.on_mouse_move(
                        &MouseMoveEvent {
                            position: edge,
                            pressed_button: Some(MouseButton::Left),
                            ..Default::default()
                        },
                        window,
                        cx,
                    );
                    let generation = editor.drag_scroll_generation;
                    let now = editor.drag_scroll.as_ref().unwrap().last_frame;
                    editor.on_mouse_move(
                        &MouseMoveEvent {
                            position: edge,
                            pressed_button: Some(MouseButton::Left),
                            ..Default::default()
                        },
                        window,
                        cx,
                    );
                    assert_eq!(editor.drag_scroll_generation, generation);
                    assert_eq!(editor.drag_scroll.as_ref().unwrap().last_frame, now);
                    assert!(editor.advance_drag_scroll(
                        now + Duration::from_millis(16),
                        window,
                        cx
                    ));
                    assert!((editor.scroll_metrics().0 - 8.64).abs() < 0.01);
                    match cancel {
                        "reentry" => editor.update_drag_scroll(bounds.center(), window, cx),
                        "blur" => window.blur(),
                        "revision" => {
                            editor
                                .apply_command(EditCommand::ReplaceSelection {
                                    text: "X".into(),
                                    typing: false,
                                })
                                .unwrap();
                        }
                        "dismiss" => editor.dismiss(&Dismiss, window, cx),
                        "lost_release" => editor.on_mouse_move(
                            &MouseMoveEvent {
                                position: edge,
                                pressed_button: None,
                                ..Default::default()
                            },
                            window,
                            cx,
                        ),
                        _ => unreachable!(),
                    }
                    assert!(!editor.advance_drag_scroll(
                        now + Duration::from_millis(32),
                        window,
                        cx
                    ));
                    if cancel != "revision" {
                        assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                    }
                });
                let stopped = editor.read(cx).scroll_metrics().0;
                window.simulate_next_frame(cx);
                assert_eq!(editor.read(cx).scroll_metrics().0, stopped);
                assert!(editor.read(cx).drag_scroll.is_none());
            });
        }
    }

    #[gpui::test]
    fn outside_pointer_events_extend_a_drag_without_hovering_the_editor(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Paragraph.\n\n".repeat(100)).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        let (start, outside) = cx.update(|window, cx| {
            _ = window.draw(cx);
            let editor = editor.read(cx);
            let line = &editor.painted_lines[0];
            (
                point(line.bounds.left() + px(2.), line.bounds.top() + px(8.)),
                point(
                    line.bounds.left() + px(10.),
                    editor.scroll_handle.bounds().bottom() + px(12.),
                ),
            )
        });
        cx.simulate_event(MouseDownEvent {
            position: start,
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        cx.simulate_event(MouseMoveEvent {
            position: outside,
            pressed_button: Some(MouseButton::Left),
            ..Default::default()
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, _| {
                editor
                    .drag_scroll
                    .as_mut()
                    .expect("outside drag reached its owner")
                    .last_frame = Instant::now() - Duration::from_millis(16);
            });
            window.simulate_next_frame(cx);
            assert!(editor.read(cx).scroll_metrics().0 > 0.);
        });
        cx.simulate_event(MouseUpEvent {
            position: outside,
            button: MouseButton::Left,
            ..Default::default()
        });
        assert!(editor.read_with(cx, |editor, _| editor.drag_scroll.is_none()));
    }
}
