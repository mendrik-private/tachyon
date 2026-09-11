//! Code-pane source-preserving presentation and interaction.
use super::*;

/// Reserved command-control track includes both Copy and Copied, so feedback
/// never changes the source viewport. The language track is natively measured.
pub(super) const STRIP_TRAILING: f32 = 96.;

pub(super) fn strip_leading(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    width: f32,
    fonts: Option<&FontMeasurement>,
) -> Option<f32> {
    if segment.node_id != segment.top_level_node_id || segment.context.table_cell.is_some() {
        return None;
    }
    let BlockNode::CodeBlock(code) = projection.block(segment.node_id)? else {
        return None;
    };
    let language = code.language.as_deref()?;
    if !matches!(
        language,
        "sh" | "shell" | "bash" | "zsh" | "fish" | "console"
    ) {
        return None;
    }
    if let Some((_, locked)) = projection
        .command_strip_lock
        .filter(|(id, _)| *id == segment.node_id)
    {
        return locked
            .map(f32::from_bits)
            .filter(|leading| width - leading - STRIP_TRAILING - 2. * CODE_BLOCK_PADDING >= 80.);
    }
    let text = &projection.text()[segment.projection_range()];
    let value = text.strip_suffix('\n').unwrap_or(text);
    let value = value.strip_suffix('\r').unwrap_or(value);
    if value.is_empty() || value.len() > 4096 || value.contains(['\n', '\r', '\t']) {
        return None;
    }
    let fonts = fonts?;
    let leading = fonts.command_language_width(language) + 16.;
    let source_width = fonts.line_width(
        projection,
        segment.projection_start()..segment.projection_start() + value.len(),
        DocumentStyle::CODE_SIZE,
    )?;
    let available = width - leading - STRIP_TRAILING - 2. * CODE_BLOCK_PADDING;
    (available >= 80. && source_width <= available).then_some(leading)
}

pub(super) fn active_strip(
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    node: Option<NodeId>,
    zoom: f32,
) -> Option<(NodeId, Option<u32>)> {
    let node = node?;
    let segment = projection.segment_for_node(node)?;
    let first = lines.partition_point(|line| line.projected_start() < segment.projection_start());
    let code = lines.get(first)?.code_line?;
    Some((node, code.strip.then_some((code.width / zoom).to_bits())))
}

pub(super) fn hide_terminal_row(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    block: &BlockNode,
) -> bool {
    matches!(block, BlockNode::CodeBlock(_))
        && !crate::math::is_math(block)
        && projection.expanded_code_tail != Some(segment.node_id)
        && projection.text()[segment.projection_range()].ends_with('\n')
}

/// Navigation still visits every authored line even when its final empty row
/// has no idle footprint. Selecting this endpoint expands its normal geometry.
pub(super) fn terminal_after_line(
    projection: &TextProjection,
    line: &VisualLineSpec,
) -> Option<usize> {
    let segment = projection.segment_for_range(&line.projected_range())?;
    let block = projection.block(segment.node_id)?;
    (line.projected_end() + 1 == segment.projection_end()
        && hide_terminal_row(projection, segment, block))
    .then_some(segment.projection_end())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Hsla;

    fn contrast(a: Hsla, b: Hsla) -> f32 {
        let luminance = |color: Hsla| {
            let color = color.to_rgb();
            let linear = |c: f32| {
                if c <= 0.04045 {
                    c / 12.92
                } else {
                    ((c + 0.055) / 1.055).powf(2.4)
                }
            };
            0.2126 * linear(color.r) + 0.7152 * linear(color.g) + 0.0722 * linear(color.b)
        };
        let (a, b) = (luminance(a), luminance(b));
        (a.max(b) + 0.05) / (a.min(b) + 0.05)
    }

    #[gpui::test]
    fn selected_code_glyphs_contrast_with_the_painted_selection(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        for language in ["rust", "json", "toml", ""] {
            let source = format!("```{language}\nlet value = (42, \"label\"); // note\n```\n");
            let (editor, cx) = cx.add_window_view(|window, cx| {
                RichDocumentEditor::new(
                    Document::from_markdown(source.as_str()).unwrap(),
                    window,
                    cx,
                )
            });
            let cx: &mut gpui::VisualTestContext = cx;
            cx.run_until_parked();
            for dark in [false, true] {
                for zoom in [1., 2.] {
                    cx.update(|window, cx| {
                        window.activate_window();
                        gpui_component::Theme::change(
                            if dark {
                                gpui_component::ThemeMode::Dark
                            } else {
                                gpui_component::ThemeMode::Light
                            },
                            Some(window),
                            cx,
                        );
                        editor.update(cx, |editor, cx| {
                            editor.set_zoom_factor(zoom, cx);
                            editor.select_all(&SelectAll, window, cx);
                        });
                    });
                    cx.run_until_parked();
                    for inactive in [false, true] {
                        if inactive {
                            cx.deactivate_window();
                        }
                        cx.update(|window, cx| {
                            _ = window.draw(cx);
                            let editor = editor.read(cx);
                            let line = editor.painted_lines.iter().find(|line| line.layout.text.contains("let value")).unwrap();
                            let scale = window.scale_factor();
                            let fill = window.painted_quads().iter().rev().find(|quad| {
                                (quad.bounds.top().as_f32() - f32::from(line.bounds.top()) * scale).abs() < 0.01
                                    && (quad.bounds.size.height.as_f32() - f32::from(line.bounds.size.height) * scale).abs() < 0.01
                                    && quad.bounds.size.width.as_f32() > 10.
                            }).unwrap().background.as_solid().unwrap();
                            let runs = styled_runs(editor, &line.range, line.range.len(), &window.text_style(), false, TachyonPalette::for_dark(dark));
                            assert!(runs.len() >= 3, "exercise syntax categories, not only plain text");
                            for run in runs {
                                assert!(contrast(run.color, fill) >= 4.5,
                                    "selected {language} contrast {} dark={dark} zoom={zoom} inactive={inactive}: {:?} on {:?}",
                                    contrast(run.color, fill), run.color, fill);
                            }
                            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                        });
                    }
                }
            }
        }
    }

    #[gpui::test]
    fn selected_rich_cell_keeps_code_and_prose_readable(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let source = "<table><tr><td><p>Before</p><pre><code class=\"language-rust\">let value = 42;</code></pre><p>After</p></td><td>Neighbor</td></tr></table>\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            gpui_component::Theme::change(gpui_component::ThemeMode::Light, Some(window), cx);
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let id = editor
                    .projection
                    .roots()
                    .find(|root| matches!(root, BlockNode::Table(_)))
                    .unwrap()
                    .id();
                editor
                    .apply_command(EditCommand::SetSelection(Selection::Table(
                        document_core::RectangularSelection {
                            table_id: id,
                            anchor_row: 0,
                            anchor_column: 0,
                            head_row: 0,
                            head_column: 0,
                        },
                    )))
                    .unwrap();
                editor.focus_handle.focus(window, cx);
                cx.notify();
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            let editor = editor.read(cx);
            for label in ["Before", "let value", "After"] {
                let line = editor
                    .painted_lines
                    .iter()
                    .find(|line| line.layout.text.contains(label))
                    .unwrap();
                let scale = window.scale_factor();
                let sample = point(
                    gpui::ScaledPixels((f32::from(line.bounds.left()) + 1.) * scale),
                    gpui::ScaledPixels((f32::from(line.bounds.top()) + 1.) * scale),
                );
                let fill = window
                    .painted_quads()
                    .iter()
                    .rev()
                    .filter(|quad| quad.bounds.contains(&sample))
                    .filter_map(|quad| quad.background.as_solid())
                    .find(|color| color.a == 1.)
                    .unwrap();
                for run in styled_runs(
                    editor,
                    &line.range,
                    line.range.len(),
                    &window.text_style(),
                    false,
                    TachyonPalette::LIGHT,
                ) {
                    assert!(
                        contrast(run.color, fill) >= 4.5,
                        "selected cell {label} contrast {}",
                        contrast(run.color, fill)
                    );
                }
            }
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn short_shell_command_uses_one_measured_strip(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown("```sh\npwd\n```\n").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let lines = build_visual_lines_for_segment(
                &projection,
                &projection.segments()[0],
                &HashMap::new(),
                760.,
                &[],
                Some(&fonts),
                None,
            );
            assert_eq!(lines.len(), 1);
            assert_eq!(
                lines[0].style.space_above,
                8. + CODE_BLOCK_PADDING,
                "short command should not reserve a separate header row"
            );
        });
    }

    #[gpui::test]
    fn command_strips_fit_source_and_controls_or_keep_a_full_pane(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                for (source, width, strip) in [
                    ("```sh\npwd\n```\n", 240., true),
                    ("```sh\npwd\n```\n", 200., false),
                    ("```bash\ngit status --short\n```\n", 360., true),
                    ("```bash\ngit status --short\n```\n", 260., false),
                    ("```sh\npwd\nls\n```\n", 760., false),
                    ("```sh\npwd\t-x\n```\n", 760., false),
                    ("```rust\nlet ready = true;\n```\n", 760., false),
                    ("```\npwd\n```\n", 760., false),
                    ("- ```sh\n  pwd\n  ```\n", 760., false),
                    ("```sh\r\npwd\r\n```\r\n", 760., true),
                ] {
                    let document = Document::from_markdown(source).unwrap();
                    let projection = TextProjection::from_snapshot(&document.snapshot());
                    let segment = &projection.segments()[0];
                    let lines = build_visual_lines_for_segment(
                        &projection,
                        segment,
                        &HashMap::new(),
                        width,
                        &[],
                        Some(&fonts),
                        None,
                    );
                    assert_eq!(
                        lines[0].command_strip(),
                        strip,
                        "{source:?}, width {width}, zoom {zoom}"
                    );
                    if strip {
                        assert_eq!(lines.len(), 1);
                        let line = &lines[0];
                        assert_eq!(line.inset, CODE_BLOCK_PADDING);
                        assert_eq!(line.style.space_above, 8. + CODE_BLOCK_PADDING);
                        let end = line.projected_start()
                            + projection.text()[line.projected_range()]
                                .trim_end_matches(['\r', '\n'])
                                .len();
                        let actual = fonts
                            .line_width(
                                &projection,
                                line.projected_start()..end,
                                line.style.font_size,
                            )
                            .unwrap();
                        assert!(
                            actual + line.code_gutter() + STRIP_TRAILING + 2. * CODE_BLOCK_PADDING
                                <= width + 0.01
                        );
                    }
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                }
            }
        });
    }

    #[gpui::test]
    fn command_strip_choices_stay_inside_measured_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = (0..100)
                .map(|index| format!("# Section {index}\n\n```sh\npwd\n```\n\n"))
                .collect::<String>();
            let document = Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = arrangement::build_edit_locked_adaptive_plan(
                &projection,
                760.,
                800.,
                None,
                false,
                arrangement::LayoutMeasurement {
                    text: &fonts,
                    images: None,
                    scope: Some(0..2),
                    resource_generation: 0,
                },
                None,
            );
            assert!(!plan.command_strips.is_empty());
            assert!(plan.command_strips.len() < 100);
            for segment in projection.segments() {
                if matches!(
                    projection.block(segment.node_id),
                    Some(BlockNode::CodeBlock(_))
                ) {
                    assert_eq!(
                        plan.command_strips.contains_key(&segment.node_id),
                        plan.has_measured_geometry(segment.top_level_node_id)
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn command_strip_growth_retains_origin_until_blur_and_worker_publication(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init_editor);
        let source = "# Commands\n\n```sh\npwd\n```\n\nAfter.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                editor.refresh_projection();
                let segment = code_segment(editor);
                let id = segment.node_id;
                let first = |lines: &[VisualLineSpec]| {
                    lines
                        .iter()
                        .find(|line| line.projected_start() == segment.projection_start())
                        .unwrap()
                        .clone()
                };
                assert!(first(&editor.visual_lines).command_strip());
                let before = first(&editor.visual_lines);
                editor.set_selection(
                    segment.projection_start()..segment.projection_start(),
                    false,
                    window,
                    cx,
                );
                editor.replace_text_in_range(None, &"long-command ".repeat(40), window, cx);
                let after = first(&editor.visual_lines);
                assert!(after.command_strip());
                assert_eq!(
                    (after.y, after.inset, after.code_gutter()),
                    (before.y, before.inset, before.code_gutter())
                );
                editor.refresh_projection();
                assert!(first(&editor.visual_lines).command_strip());
                let snapshot = editor.document.snapshot();
                let view = PreparedDocumentView::prepare_snapshot_with_images(
                    &snapshot,
                    &HashMap::new(),
                    None,
                    ReflowViewport {
                        published_geometry: editor.published_geometry.clone(),
                        width: editor.layout_width,
                        height: 800.,
                        zoom: editor.zoom_factor,
                        preview_edit_node: None,
                        expanded_code_tail: None,
                        editing_node: Some(id),
                        table_layout_lock: None,
                        html_disclosures: Arc::default(),
                        html_loaded_images: Arc::default(),
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: None,
                        resource_generation: 0,
                    },
                    &editor.adaptive,
                    &editor.measurement,
                )
                .0;
                assert!(first(&view.visual_lines).command_strip());
                assert_eq!(
                    first(&view.visual_lines).code_gutter(),
                    before.code_gutter()
                );
                editor.set_selection(0..0, false, window, cx);
                editor.refresh_projection();
                assert!(!first(&editor.visual_lines).command_strip());
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        });
    }

    const SOURCE: &str = "# Before\n\n```yaml\napp:\n  name: tachyon\n  enabled: true\n  port: 8080\n```\n\nAfter.\n";
    const PAYLOAD: &str = "app:\n  name: tachyon\n  enabled: true\n  port: 8080\n";

    fn code_segment(editor: &RichDocumentEditor) -> crate::ProjectionSegment {
        editor
            .projection
            .segments()
            .iter()
            .find(|segment| {
                matches!(
                    editor.projection.block(segment.node_id),
                    Some(BlockNode::CodeBlock(_))
                )
            })
            .unwrap()
            .clone()
    }

    fn row_count(editor: &RichDocumentEditor, node: NodeId) -> usize {
        editor
            .visual_lines
            .iter()
            .filter(|line| {
                editor
                    .projection
                    .segment_for_range(&line.projected_range())
                    .is_some_and(|segment| segment.node_id == node)
            })
            .count()
    }

    #[gpui::test]
    fn terminal_newline_remains_editable_and_exact_through_undo(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        for prefix in ["# Before\n\n", "# Before\n\n## Example: Configuration\n\n"] {
            for zoom in [1., 2.] {
                let source = format!("{prefix}```yaml\n{PAYLOAD}\n```\n\nAfter.\n");
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
                        editor.set_zoom_factor(zoom, cx);
                        editor.refresh_projection();
                        let segment = code_segment(editor);
                        let end = segment.projection_end();
                        // Four populated rows plus one authored blank row survive.
                        assert_eq!(row_count(editor, segment.node_id), 5);
                        editor.set_selection(end - 1..end - 1, false, window, cx);
                        editor.right(&Right, window, cx);
                        assert_eq!(editor.cursor_offset(), end);
                        assert_eq!(editor.projection.expanded_code_tail, Some(segment.node_id));
                        assert_eq!(row_count(editor, segment.node_id), 6);
                        assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                        editor.replace_text_in_range(None, "next: true", window, cx);
                        assert_eq!(editor.projection.expanded_code_tail, None);
                        assert_eq!(row_count(editor, segment.node_id), 6);
                        assert!(
                            editor
                                .document
                                .snapshot()
                                .serialize()
                                .unwrap()
                                .contains("\n\nnext: true\n```")
                        );
                        editor.undo(&Undo, window, cx);
                        assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                        assert_eq!(editor.projection.expanded_code_tail, Some(segment.node_id));
                        editor.backspace(&Backspace, window, cx);
                        assert_eq!(row_count(editor, segment.node_id), 5);
                        editor.undo(&Undo, window, cx);
                        assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                        editor.set_selection(0..0, false, window, cx);
                        assert_eq!(editor.projection.expanded_code_tail, None);
                        assert_eq!(row_count(editor, segment.node_id), 5);
                        // Re-enter after a compact cache entry was populated.
                        editor.set_selection(end..end, false, window, cx);
                        assert_eq!(row_count(editor, segment.node_id), 6);
                        editor.refresh_projection();
                        assert_eq!(row_count(editor, segment.node_id), 6);
                        assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                    });
                });
            }
        }
    }

    #[gpui::test]
    fn down_and_shift_down_visit_terminal_code_row_before_next_block(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let segment = code_segment(editor);
                let end = segment.projection_end();
                for selecting in [false, true] {
                    editor.set_selection(end - 2..end - 2, false, window, cx);
                    if selecting {
                        editor.select_down(&SelectDown, window, cx);
                    } else {
                        editor.down(&Down, window, cx);
                    }
                    assert_eq!(
                        editor.cursor_offset(),
                        end,
                        "Down must visit the code's terminal newline before leaving its block"
                    );
                    assert_eq!(row_count(editor, segment.node_id), 5);
                    if selecting {
                        assert_eq!(editor.selected_byte_range().0, end - 2..end);
                    }
                    editor.down(&Down, window, cx);
                    assert!(editor.cursor_offset() > end);
                    assert_eq!(row_count(editor, segment.node_id), 4);
                    editor.up(&Up, window, cx);
                    assert_eq!(
                        editor.cursor_offset(),
                        end,
                        "Up must enter the preceding code's terminal row"
                    );
                    assert_eq!(row_count(editor, segment.node_id), 5);
                }
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
            });
        });
    }

    #[gpui::test]
    fn pointer_copy_keeps_caret_and_selection(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        for (source, payload) in [
            (SOURCE, PAYLOAD),
            ("# Before\n\n```sh\npwd\n```\n", "pwd\n"),
        ] {
            let (editor, cx) = cx.add_window_view(|window, cx| {
                RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
            });
            let cx: &mut gpui::VisualTestContext = cx;
            for selection in [0..0, 0..5] {
                editor.update_in(cx, |editor, window, cx| {
                    editor.set_selection(selection.clone(), false, window, cx)
                });
                cx.run_until_parked();
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                let button = cx.debug_bounds("code-copy-command").unwrap();
                cx.simulate_click(button.center(), gpui::Modifiers::default());
                editor.read_with(cx, |editor, cx| {
                    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), payload);
                    assert!(editor.copied_code.is_some());
                    assert_eq!(
                        editor.selected_byte_range().0,
                        selection,
                        "Copy must not retarget the editor selection"
                    );
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                });
            }
        }
    }

    #[gpui::test]
    fn background_reflow_and_caches_keep_terminal_rows_view_local(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let segment = code_segment(editor);
                let end = segment.projection_end();
                editor.set_selection(end..end, false, window, cx);
                // Shared source selection remains at the code tail. Each worker
                // must use its own view state, including on a published-cache hit.
                let snapshot = editor.document.snapshot();
                let mut published = None;
                for expanded in [false, true, true, false, false] {
                    let tail = expanded.then_some(segment.node_id);
                    let prepare = |measurement: &FontMeasurement, published_geometry| {
                        PreparedDocumentView::prepare_snapshot_with_images(
                            &snapshot,
                            &HashMap::new(),
                            None,
                            ReflowViewport {
                                published_geometry,
                                width: editor.layout_width,
                                height: 800.,
                                zoom: 1.,
                                preview_edit_node: None,
                                expanded_code_tail: tail,
                                editing_node: tail,
                                table_layout_lock: None,
                                html_disclosures: Arc::default(),
                                html_loaded_images: Arc::default(),
                                trace_mode: LayoutTraceMode::Off,
                                visible_roots: None,
                                resource_generation: 0,
                            },
                            &editor.adaptive,
                            measurement,
                        )
                        .0
                    };
                    let view = prepare(&editor.measurement, published.take());
                    let fresh = FontMeasurement::new(
                        cx.text_system().clone(),
                        "Public Sans Tachyon".into(),
                        1.,
                    );
                    let oracle = prepare(&fresh, None);
                    let geometry = |view: &PreparedDocumentView| {
                        view.visual_lines
                            .iter()
                            .map(|line| {
                                (
                                    line.projected_range(),
                                    line.y,
                                    line.style.line_height,
                                    line.style.space_above,
                                    line.style.space_below,
                                )
                            })
                            .collect::<Vec<_>>()
                    };
                    assert_eq!(geometry(&view), geometry(&oracle));
                    assert_eq!(view.projection.expanded_code_tail, tail);
                    assert_eq!(
                        view.visual_lines
                            .iter()
                            .filter(|line| view
                                .projection
                                .segment_for_range(&line.projected_range())
                                .is_some_and(|owner| owner.node_id == segment.node_id))
                            .count(),
                        4 + usize::from(expanded)
                    );
                    published = view.published_geometry;
                }
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
            });
        });
    }

    #[gpui::test]
    fn code_footer_has_one_padding_inset_after_the_last_source_line(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.read_with(cx, |editor, _| {
                let line = editor
                    .painted_lines
                    .iter()
                    .rev()
                    .find(|line| line.horizontal_owner.is_some() && !line.layout.text.is_empty())
                    .unwrap();
                let owner = line.horizontal_owner.unwrap();
                let frame = visual_component_bounds(
                    editor,
                    editor.element_bounds.unwrap(),
                    owner,
                    CODE_BLOCK_PADDING,
                    0.,
                    CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING,
                    CODE_BLOCK_PADDING,
                )
                .unwrap();
                assert_eq!(
                    f32::from(frame.bottom() - line.bounds.bottom()),
                    CODE_BLOCK_PADDING
                );
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
            });
        });
    }
}
