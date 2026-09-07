//! Native controls over retained Blitz summary geometry. Open choices are
//! per-view rendering inputs, never HTML edits or content-undo entries.
use super::*;

gpui::actions!(
    html_disclosure,
    [
        ActivateDisclosure,
        NextDisclosureControl,
        PreviousDisclosureControl,
        RestoreAuthoredDisclosures
    ]
);

pub(super) fn init_controls(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("enter", ActivateDisclosure, Some("HtmlDisclosure")),
        KeyBinding::new("space", ActivateDisclosure, Some("HtmlDisclosure")),
        KeyBinding::new("tab", NextDisclosureControl, Some("HtmlDisclosure")),
        KeyBinding::new(
            "shift-tab",
            PreviousDisclosureControl,
            Some("HtmlDisclosure"),
        ),
    ]);
}

pub(super) struct DisclosureAnchor {
    node: NodeId,
    ordinal: usize,
    viewport_y: f32,
    jump_generation: u64,
}

pub(super) struct HtmlAnchorJump {
    node: NodeId,
    source: Arc<str>,
    name: String,
    jump_generation: u64,
}

impl RichDocumentEditor {
    pub(super) fn navigate_to_html_anchor(
        &mut self,
        fragment: &str,
        heading_offset: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<bool> {
        let target = self.visual_lines.iter().find_map(|line| {
            let preview = line.html_preview.as_ref()?;
            let anchor = preview
                .anchors
                .iter()
                .find(|anchor| anchor.name == fragment)?;
            let segment = segment_for_line(&self.projection, &line.range)?;
            (heading_offset.is_none_or(|offset| segment.projection_range.start < offset))
                .then(|| (segment.node_id, preview.source.clone(), anchor.clone()))
        });
        let (node, source, anchor) = target?;
        if !matches!(self.document.snapshot().node(node), Some(BlockNode::PreservedSource { source: current, .. }) if current == &source)
        {
            return Some(false);
        }
        if anchor.y.is_none() && anchor.closed_ancestors.is_empty() {
            return Some(false);
        }
        self.html_selection = None;
        self.move_to(self.cursor_offset(), window, cx);
        self.jump_generation = self.jump_generation.saturating_add(1);
        self.disclosure_anchor = None;
        self.html_anchor_jump = Some(HtmlAnchorJump {
            node,
            source: source.clone(),
            name: fragment.to_owned(),
            jump_generation: self.jump_generation,
        });
        if anchor.closed_ancestors.is_empty() {
            self.finish_html_anchor_jump(cx);
        } else {
            let states = Arc::make_mut(&mut self.projection.html_disclosures);
            let state = states
                .entry(node)
                .or_insert_with(|| crate::html::DisclosureState {
                    source: source.clone(),
                    overrides: Default::default(),
                });
            if state.source != source {
                state.source = source;
                state.overrides.clear();
            }
            for ordinal in anchor.closed_ancestors {
                state.overrides.insert(ordinal, true);
            }
            self.geometry_generation = self.geometry_generation.wrapping_add(1);
            self.layout_replan_pending = true;
            cx.notify();
        }
        Some(true)
    }

    pub(super) fn finish_html_anchor_jump(&mut self, cx: &mut Context<Self>) {
        let Some(jump) = self.html_anchor_jump.take() else {
            return;
        };
        if jump.jump_generation != self.jump_generation
            || !matches!(self.document.snapshot().node(jump.node), Some(BlockNode::PreservedSource { source, .. }) if source == &jump.source)
        {
            return;
        }
        let target = self.visual_lines.iter().find_map(|line| {
            if segment_for_line(&self.projection, &line.range)?.node_id != jump.node {
                return None;
            }
            let preview = line.html_preview.as_ref()?;
            let anchor = preview
                .anchors
                .iter()
                .find(|anchor| anchor.name == jump.name)?;
            Some((
                line.y + (32. + anchor.y?) * self.zoom_factor,
                preview.clone(),
                anchor.text_byte,
            ))
        });
        if let Some((y, preview, text_byte)) = target {
            if let Some(byte) = text_byte {
                self.html_selection = Some(HtmlSelection {
                    cross: None,
                    node: jump.node,
                    preview,
                    anchor: byte,
                    head: byte,
                });
            }
            self.set_scroll_y(
                y.min((self.document_height - self.scroll_metrics().1).max(0.)),
                cx,
            );
        } else {
            cx.emit(EditorEvent::LinkFailed(format!(
                "Cannot display anchor #{}. HTML source remains available.",
                jump.name
            )));
        }
    }

    pub(super) fn restore_authored_disclosures(
        &mut self,
        _: &RestoreAuthoredDisclosures,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document.composition_active() || !self.selected_byte_range().0.is_empty() {
            window.play_system_bell();
            return;
        }
        self.projection.html_disclosures = Arc::default();
        self.html_anchor_jump = None;
        self.disclosure_anchor = None;
        self.geometry_generation = self.geometry_generation.wrapping_add(1);
        self.layout_replan_pending = true;
        cx.notify();
    }

    pub(super) fn html_disclosure_chrome(
        &self,
        node: NodeId,
        preview: &crate::html::HtmlPreview,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        preview.disclosures.iter().map(|disclosure| {
            let ordinal = disclosure.ordinal;
            let id = ElementId::Name(format!("html-disclosure-{}-{ordinal}", node.get()).into());
            let focus = window.use_keyed_state(id.clone(), cx, |_, cx| cx.focus_handle().tab_stop(true)).read(cx).clone();
            let mouse_focus = focus.clone();
            let action_focus = focus.clone();
            let action_editor = cx.entity().downgrade();
            let source = preview.source.clone();
            let keyboard_source = source.clone();
            let action_source = source.clone();
            let [left, top, right, bottom] = disclosure.bounds;
            let label = if disclosure.label.trim().is_empty() { "Details".to_owned() } else { disclosure.label.clone() };
            div().id(id).role(Role::Button).key_context("HtmlDisclosure")
                .aria_label(label.clone()).aria_expanded(disclosure.open)
                .aria_keyshortcuts("Enter Space")
                .aria_description("Expand or collapse this disclosure for reading. Original HTML is preserved.")
                .track_focus(&focus).tab_stop(true)
                .absolute().left(px(left * self.zoom_factor)).top(px(top * self.zoom_factor))
                .w(px((right - left) * self.zoom_factor)).h(px((bottom - top) * self.zoom_factor))
                .border_2().border_color(rgba(0)).rounded(px(3.))
                .focus(|style| style.border_color(rgb(MineralPalette::LIGHT.accent)))
                .hover(|style| style.bg(rgba(0x256f5010)))
                .cursor_pointer()
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new("Toggle disclosure · Enter / Space")
                        .build(window, cx)
                })
                .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                    mouse_focus.focus(window, cx);
                    cx.stop_propagation();
                })
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.toggle_html_disclosure(node, &source, ordinal, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(cx.listener(move |this, _: &ActivateDisclosure, window, cx| {
                    window.prevent_default();
                    this.toggle_html_disclosure(node, &keyboard_source, ordinal, window, cx);
                    cx.stop_propagation();
                }))
                .on_action(|_: &NextDisclosureControl, window, cx| { window.focus_next(cx); cx.stop_propagation(); })
                .on_action(|_: &PreviousDisclosureControl, window, cx| { window.focus_prev(cx); cx.stop_propagation(); })
                .on_a11y_action(gpui::accesskit::Action::Click, move |_, window, cx| {
                    action_focus.focus(window, cx);
                    let _ = action_editor.update(cx, |this, cx| this.toggle_html_disclosure(node, &action_source, ordinal, window, cx));
                })
                .into_any_element()
        }).collect()
    }

    pub(super) fn toggle_html_disclosure(
        &mut self,
        node: NodeId,
        source: &Arc<str>,
        ordinal: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document.composition_active() || !self.selected_byte_range().0.is_empty() {
            window.play_system_bell();
            return;
        }
        if self.projected_generation != self.document.generation()
            || !matches!(self.projection.block(node), Some(BlockNode::PreservedSource { source: current, .. }) if current == source)
        {
            return;
        }
        let Some((y, disclosure)) = self.visual_lines.iter().find_map(|line| {
            let preview = line.html_preview.as_ref()?;
            (segment_for_line(&self.projection, &line.range)?.node_id == node
                && &preview.source == source)
                .then(|| {
                    preview
                        .disclosures
                        .iter()
                        .find(|d| d.ordinal == ordinal)
                        .map(|d| (line.y, d))
                })?
        }) else {
            return;
        };
        let authored_open = disclosure.authored_open;
        self.html_anchor_jump = None;
        let open = self
            .projection
            .html_disclosure_overrides(node)
            .and_then(|state| state.get(&ordinal))
            .copied()
            .unwrap_or(disclosure.open);
        self.disclosure_anchor = Some(DisclosureAnchor {
            node,
            ordinal,
            viewport_y: y + (32. + disclosure.bounds[1]) * self.zoom_factor
                - self.scroll_metrics().0,
            jump_generation: self.jump_generation,
        });
        let states = Arc::make_mut(&mut self.projection.html_disclosures);
        let state = states
            .entry(node)
            .or_insert_with(|| crate::html::DisclosureState {
                source: source.clone(),
                overrides: Default::default(),
            });
        if state.source != *source {
            state.source = source.clone();
            state.overrides.clear();
        }
        if open != authored_open {
            state.overrides.remove(&ordinal);
        } else {
            state.overrides.insert(ordinal, !open);
        }
        if state.overrides.is_empty() {
            states.remove(&node);
        }
        self.stop_momentum();
        self.layout_focus = None;
        self.projection.table_layout_lock = None;
        self.geometry_generation = self.geometry_generation.wrapping_add(1);
        self.layout_replan_pending = true;
        cx.notify();
    }

    pub(super) fn restore_disclosure_anchor(&mut self) -> Option<f32> {
        let anchor = self.disclosure_anchor.take()?;
        if anchor.jump_generation != self.jump_generation {
            return None;
        }
        let y = self.visual_lines.iter().find_map(|line| {
            if segment_for_line(&self.projection, &line.range)?.node_id != anchor.node {
                return None;
            }
            let summary = line
                .html_preview
                .as_ref()?
                .disclosures
                .iter()
                .find(|d| d.ordinal == anchor.ordinal)?;
            Some(line.y + (32. + summary.bounds[1]) * self.zoom_factor)
        })?;
        let x = self.scroll_handle.offset().x;
        self.scroll_handle
            .set_offset(point(x, px(-(y - anchor.viewport_y).max(0.))));
        Some(y - self.scroll_metrics().0 - anchor.viewport_y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn html_id_navigation_reveals_nested_details_without_editing(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        let source = format!(
            "[Go](#caf%C3%A9)\n\n<details><summary>Outer</summary><details><summary>Inner</summary><p id='café'>Target body.</p></details></details>\n\n{}",
            "After.\n\n".repeat(30)
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
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
                    editor.navigate_to_heading("café", window, cx),
                    "authored HTML IDs must resolve"
                );
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
            });
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            let preview = editor
                .visual_lines
                .iter()
                .find_map(|line| line.html_preview.as_ref())
                .unwrap();
            assert_eq!(preview.disclosures.len(), 2);
            assert!(preview.disclosures.iter().all(|d| d.open));
            let line = editor
                .visual_lines
                .iter()
                .find(|line| line.html_preview.is_some())
                .unwrap();
            let y = preview
                .anchors
                .iter()
                .find(|a| a.name == "café")
                .unwrap()
                .y
                .unwrap();
            assert!(
                (editor.scroll_metrics().0 - line.y - (32. + y) * editor.zoom_factor).abs() < 1.
            );
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn html_and_markdown_duplicate_anchors_follow_source_order(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        for prefix in [
            "<div><p id='repeat'>First HTML</p><p id='repeat'>Second HTML</p></div>\n\n# Repeat\n\n",
            "# Repeat\n\n<div><p id='repeat'>Later HTML</p></div>\n\n",
        ] {
            let source = format!("{prefix}{}", "After.\n\n".repeat(25));
            let (editor, cx) = cx.add_window_view(|window, cx| {
                RichDocumentEditor::new(
                    Document::from_markdown(source.as_str()).unwrap(),
                    window,
                    cx,
                )
            });
            for _ in 0..3 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            cx.update(|window, cx| {
                cx.set_reduce_motion(true);
                editor.update(cx, |editor, cx| {
                    assert!(editor.navigate_to_heading("repeat", window, cx));
                    let first = editor.document.snapshot().blocks().get(0).unwrap().id();
                    assert_eq!(
                        editor
                            .html_selection
                            .as_ref()
                            .map(|selection| selection.node)
                            .unwrap_or_else(|| editor
                                .position_for_offset(editor.cursor_offset(), Affinity::Downstream)
                                .unwrap()
                                .node_id),
                        first
                    );
                    if let Some(preview) = editor.visual_lines[0].html_preview.as_ref() {
                        assert!(
                            (editor.scroll_metrics().0
                                - editor.visual_lines[0].y
                                - 32.
                                - preview.anchors[0].y.unwrap())
                            .abs()
                                < 1.
                        );
                    }
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                    editor.replace_text_in_range(None, "x", window, cx);
                    let edited = editor.document.snapshot().serialize().unwrap();
                    assert!(
                        if prefix.starts_with('<') {
                            edited.contains("xFirst HTML")
                        } else {
                            edited.contains("# xRepeat")
                        },
                        "typing must edit the resolved destination: {edited}"
                    );
                    editor.undo(&Undo, window, cx);
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                });
            });
        }
    }

    #[gpui::test]
    fn html_anchor_reveal_does_not_restore_over_an_intervening_scroll(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init);
        let source = format!(
            "Before.\n\n<details><summary>Closed</summary><p id='target'>Body</p></details>\n\n{}",
            "After.\n\n".repeat(25)
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                assert!(editor.navigate_to_heading("target", window, cx));
                assert!(editor.html_anchor_jump.is_some());
                editor.set_scroll_y(20., cx);
            })
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(editor.html_anchor_jump.is_none());
            assert!((editor.scroll_metrics().0 - 20.).abs() < 1.);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    const SOURCE: &str = "# Before\n\n<details><summary>Read more</summary><p>Body text that is initially closed.</p></details>\n\nAfter.\n";

    #[gpui::test]
    fn disclosure_pointer_and_keyboard_actions_do_not_edit_source(cx: &mut gpui::TestAppContext) {
        assert_disclosure_actions_preserve_source(SOURCE, cx);
    }

    #[gpui::test]
    fn implicit_disclosure_pointer_and_keyboard_actions_do_not_edit_source(
        cx: &mut gpui::TestAppContext,
    ) {
        assert_disclosure_actions_preserve_source(
            "# Before\n\n<details>Direct text.<p>Body text that is initially closed.</p></details>\n\nAfter.\n",
            cx,
        );
    }

    fn assert_disclosure_actions_preserve_source(
        source: &'static str,
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let point = editor.read_with(cx, |editor, _| {
            let bounds = editor.element_bounds.unwrap();
            let line = editor
                .visual_lines
                .iter()
                .find(|line| line.html_preview.is_some())
                .unwrap();
            let d = &line.html_preview.as_ref().unwrap().disclosures[0];
            point(
                bounds.left() + px(line.inset + d.bounds[0] + 12.),
                bounds.top() + px(line.y + 32. + (d.bounds[1] + d.bounds[3]) * 0.5),
            )
        });
        let revision = editor.read_with(cx, |editor, _| editor.document.snapshot().revision());
        cx.simulate_click(point, gpui::Modifiers::default());
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.projection.html_disclosures.len(), 1);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
        cx.simulate_keystrokes("space");
        editor.read_with(cx, |editor, _| {
            assert!(editor.projection.html_disclosures.is_empty());
            assert_eq!(editor.document.snapshot().revision(), revision);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
        cx.simulate_keystrokes("enter");
        editor.read_with(cx, |editor, _| {
            assert_eq!(
                editor.projection.html_disclosures.len(),
                1,
                "after Enter: {}",
                editor.document.snapshot().serialize().unwrap()
            )
        });
        let summary_focus = cx.update(|window, cx| window.focused(cx).unwrap());
        cx.simulate_keystrokes("tab");
        cx.update(|window, cx| {
            assert_ne!(window.focused(cx), Some(summary_focus.clone()));
        });
        cx.simulate_keystrokes("shift-tab");
        cx.update(|window, cx| {
            assert_eq!(window.focused(cx), Some(summary_focus));
            window.dispatch_action(Box::new(RestoreAuthoredDisclosures), cx);
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(editor.projection.html_disclosures.is_empty());
            assert_eq!(editor.document.snapshot().revision(), revision);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn disclosure_geometry_cache_and_source_binding_follow_view_state(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let node = editor.document.snapshot().blocks().get(1).unwrap().id();
                let source: Arc<str> = match editor.projection.block(node).unwrap() {
                    BlockNode::PreservedSource { source, .. } => source.clone(),
                    _ => unreachable!(),
                };
                let height = editor
                    .visual_lines
                    .iter()
                    .find_map(|line| line.html_preview.as_ref().map(|p| p.height))
                    .unwrap();
                let selection = editor.selection.clone();
                let revision = editor.document.snapshot().revision();
                editor.toggle_html_disclosure(node, &source, 0, window, cx);
                assert!(editor.layout_replan_pending);
                assert_eq!(
                    editor
                        .visual_lines
                        .iter()
                        .find_map(|line| line.html_preview.as_ref().map(|p| p.height))
                        .unwrap(),
                    height,
                    "input must not rasterize synchronously"
                );
                let prepared = PreparedDocumentView::prepare_snapshot_with_images(
                    &editor.document.snapshot(),
                    &HashMap::new(),
                    None,
                    ReflowViewport {
                        published_geometry: None,
                        width: 760.,
                        height: 1000.,
                        zoom: 1.,
                        math_edit_node: None,
                        editing_node: None,
                        table_layout_lock: None,
                        html_disclosures: editor.projection.html_disclosures.clone(),
                        html_loaded_images: Arc::default(),
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: None,
                        resource_generation: 0,
                    },
                    &editor.adaptive,
                    &editor.measurement,
                )
                .0;
                assert!(
                    prepared
                        .visual_lines
                        .iter()
                        .find_map(|line| line.html_preview.as_ref().map(|p| p.height))
                        .unwrap()
                        > height
                );
                editor.install_prepared(prepared);
                assert_eq!(editor.restore_disclosure_anchor(), Some(0.));
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().revision(), revision);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
                // Unrelated source edits retain the override; a changed fragment
                // with a reused node ID must not acquire it.
                editor.move_to(0, window, cx);
                editor.replace_text_in_range(None, "x", window, cx);
                assert!(editor.projection.html_disclosure_overrides(node).is_some());
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
                editor.toggle_html_disclosure(
                    node,
                    &Arc::from("<details>different</details>"),
                    0,
                    window,
                    cx,
                );
                assert_eq!(
                    editor
                        .projection
                        .html_disclosure_overrides(node)
                        .unwrap()
                        .get(&0),
                    Some(&true)
                );
                editor.projection.html_disclosures = Arc::new(
                    [(
                        node,
                        crate::html::DisclosureState {
                            source: Arc::from("stale source"),
                            overrides: [(0, true)].into(),
                        },
                    )]
                    .into_iter()
                    .collect(),
                );
                editor.projection.retain_html_disclosures();
                assert!(editor.projection.html_disclosures.is_empty());
            })
        });
    }
}
