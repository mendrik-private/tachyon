//! Visual code coordinates are decorations, never editable document text.
use super::*;

const GAP: f32 = 16.;

#[derive(Clone, Copy)]
pub(super) struct CodeLine {
    /// Zero denotes the caret-only line after the final source newline.
    pub number: usize,
    pub width: f32,
    pub strip: bool,
}

pub(super) fn width(block: &BlockNode, text: &str, fonts: Option<&FontMeasurement>) -> Option<f32> {
    if !matches!(block, BlockNode::CodeBlock(_)) || crate::math::is_math(block) {
        return None;
    }
    let count = text.lines().count();
    Some(if count < 4 {
        0.
    } else {
        // Leave one spare digit so the common 9→10 and 99→100 typing
        // transitions don't need a new text origin. Recomputed only at layout.
        let digits = (count.ilog10() + 2).max(2) as f32;
        (digits * fonts.map_or(DocumentStyle::CODE_SIZE * 0.65, |f| f.code_digit_width)).ceil()
            + GAP
    })
}

pub(super) fn retain_width(
    lines: &mut [VisualLineSpec],
    projection: &TextProjection,
    node: NodeId,
    previous: Option<(f32, bool)>,
) {
    let Some((width, strip)) = previous else {
        return;
    };
    let Some(segment) = projection.segment_for_node(node) else {
        return;
    };
    let first = lines.partition_point(|line| line.projected_start() < segment.projection_start());
    let end = lines.partition_point(|line| line.projected_start() <= segment.projection_end());
    for line in &mut lines[first..end] {
        if segment.projection_start() <= line.projected_start()
            && line.projected_end() <= segment.projection_end()
            && let Some(code) = &mut line.code_line
            && code.strip == strip
        {
            code.width = width;
        }
    }
}

pub(super) fn active_width(
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    node: Option<NodeId>,
) -> Option<(NodeId, f32, bool)> {
    let node = node?;
    let segment = projection.segment_for_node(node)?;
    let first = lines.partition_point(|line| line.projected_start() < segment.projection_start());
    let line = lines.get(first)?;
    Some((
        node,
        line.code_line?.width / line.style.font_size,
        line.code_line?.strip,
    ))
}

pub(super) fn restore_active_width(
    projection: &TextProjection,
    lines: &mut Arc<Vec<VisualLineSpec>>,
    previous: Option<(NodeId, f32, bool)>,
) -> bool {
    let Some((node, em, strip)) = previous else {
        return false;
    };
    let Some(segment) = projection.segment_for_node(node) else {
        return false;
    };
    let first = lines.partition_point(|line| line.projected_start() < segment.projection_start());
    let Some(line) = lines.get(first) else {
        return false;
    };
    let Some(code) = line.code_line else {
        return false;
    };
    if code.strip != strip {
        return false;
    }
    let width = em * line.style.font_size;
    if (width - code.width).abs() < 0.01 {
        return false;
    }
    retain_width(
        Arc::make_mut(lines).as_mut_slice(),
        projection,
        node,
        Some((width, strip)),
    );
    true
}

pub(super) fn paint(
    spec: &VisualLineSpec,
    text_bounds: Bounds<Pixels>,
    offset: f32,
    zoom: f32,
    palette: TachyonPalette,
    window: &mut Window,
) -> Option<(PaintedLine, MaskedQuad)> {
    let code = spec.code_line.filter(|c| c.width > 0. && !c.strip)?;
    let left = text_bounds.left()
        + px(if spec.table_cell.is_none() {
            offset
        } else {
            0.
        })
        - px(code.width);
    let digit_bounds = Bounds::new(
        point(left, text_bounds.top()),
        size(
            px((code.width - GAP * zoom).max(1.)),
            text_bounds.size.height,
        ),
    );
    let number = if code.number == 0 {
        String::new()
    } else {
        code.number.to_string()
    };
    let len = number.len();
    let layout = window.text_system().shape_line(
        number.into(),
        px(spec.style.font_size),
        &[TextRun {
            len,
            font: code_block_font(),
            color: rgb(palette.secondary).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        }],
        None,
    );
    Some((
        PaintedLine {
            range: spec.projected_start()..spec.projected_start(),
            layout,
            bounds: digit_bounds,
            line_height: px(spec.style.line_height),
            horizontal_owner: None,
            content_mask: None,
            alignment: ColumnAlignment::Right,
        },
        MaskedQuad {
            quad: fill(
                Bounds::new(
                    point(
                        digit_bounds.right() + px(GAP * 0.5 * zoom),
                        digit_bounds.top(),
                    ),
                    size(px(1.), digit_bounds.size.height),
                ),
                rgb(palette.border),
            ),
            content_mask: None,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbering_preserves_blank_lines_and_excludes_short_commands_and_math() {
        for (source, expected) in [
            ("```sh\ncargo test\n```\n", vec![]),
            ("```yaml\na\nb\nc\n```\n", vec![]),
            ("```yaml\na\n\nc\nd\n```\n", vec![1, 2, 3, 4]),
            ("    a\n    b\n    c\n    d\n", vec![1, 2, 3, 4]),
            ("```math\na\nb\nc\nd\n```\n", vec![]),
        ] {
            let doc = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            let segment = &projection.segments()[0];
            let lines = build_visual_lines_for_segment(
                &projection,
                segment,
                &HashMap::new(),
                600.,
                &[],
                None,
                None,
            );
            let numbers = lines
                .iter()
                .filter_map(|l| l.code_line)
                .filter(|c| c.width > 0. && c.number > 0)
                .map(|c| c.number)
                .collect::<Vec<_>>();
            assert_eq!(numbers, expected, "{source}");
            assert_eq!(doc.snapshot().serialize().unwrap(), source);
        }
    }

    #[gpui::test]
    fn native_gutter_is_fixed_scaled_and_outside_copied_code(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let payload = format!(
            "first: {}\n\nthird: élan\nfourth: true\n",
            "literal_".repeat(100)
        );
        let source = format!("## Request\n\n```yaml\n{payload}```\n");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        for zoom in [1., 1.3, 2.] {
            editor.update(cx, |editor, cx| editor.set_zoom_factor(zoom, cx));
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
                editor.update(cx, |editor, cx| {
                    let line = editor
                        .painted_lines
                        .iter()
                        .max_by(|a, b| a.layout.width().partial_cmp(&b.layout.width()).unwrap())
                        .unwrap();
                    let owner = line.horizontal_owner.unwrap();
                    let spec = editor
                        .visual_lines
                        .iter()
                        .find(|s| s.projected_range() == line.range)
                        .unwrap();
                    let mask = line.content_mask.unwrap().bounds;
                    let old_offset = editor.horizontal_scrolls.get(&owner).copied().unwrap_or(0.);
                    let palette = code_palette(
                        TachyonPalette::LIGHT,
                        editor.projection.block(owner).unwrap(),
                    );
                    let (number, rail) =
                        paint(spec, line.bounds, old_offset, zoom, palette, window).unwrap();
                    let before = number.bounds;
                    assert_eq!(number.layout.text.as_ref(), "1");
                    assert_eq!(number.alignment, ColumnAlignment::Right);
                    // Remeasured fractional zooms can reach this edge through
                    // different floating-point addition orders.
                    assert!(f32::from(number.bounds.right() + px(GAP * zoom) - mask.left()).abs() < 0.01);
                    assert_eq!(rail.quad.bounds.size.width, px(1.));
                    let (viewport, content) = editor.horizontal_metrics[&owner];
                    assert_eq!(viewport, f32::from(mask.size.width));
                    assert!(content > viewport);
                    let original_end = line.bounds.left() + px(old_offset) + line.layout.width();
                    let range = spec.projected_range();
                    editor.on_scroll_wheel(
                        &ScrollWheelEvent {
                            position: mask.center(),
                            delta: gpui::ScrollDelta::Pixels(point(px(-100000.), px(0.))),
                            ..Default::default()
                        },
                        window,
                        cx,
                    );
                    let offset = editor.horizontal_scrolls[&owner];
                    assert_eq!(offset, content - viewport);
                    let trailing = f32::from(mask.right() - original_end) + offset;
                    assert!((trailing - CODE_BLOCK_PADDING * zoom).abs() < 0.5,
                        "zoom={zoom}, trailing={trailing}, old_offset={old_offset}, offset={offset}, viewport={viewport}, content={content}");
                    let spec = editor
                        .visual_lines
                        .iter()
                        .find(|s| s.projected_range() == range)
                        .unwrap();
                    let moved = visual_line_bounds(
                        editor,
                        spec,
                        editor.element_bounds.unwrap(),
                        true,
                        offset,
                    );
                    let (after, _) = paint(spec, moved, offset, zoom, palette, window).unwrap();
                    assert!((f32::from(before.left() - after.bounds.left())).abs() < 0.01);
                    let range = editor
                        .projection
                        .segment_for_node(owner)
                        .unwrap()
                        .projection_range();
                    editor.set_selection(range, false, window, cx);
                    editor.copy(&Copy, window, cx);
                    assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), payload);
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                });
            });
        }
    }

    #[gpui::test]
    fn typing_keeps_gutter_width_through_local_and_full_refresh(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        for (prefix, count) in [
            ("", 3),
            ("", 9),
            ("## Request\n\n", 3),
            ("## Request\n\n", 9),
        ] {
            let source = format!("{prefix}```yaml\n{}```\n", "value: true\n".repeat(count));
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
                    let node = editor
                        .projection
                        .segments()
                        .iter()
                        .find(|s| {
                            matches!(
                                editor.projection.block(s.node_id),
                                Some(BlockNode::CodeBlock(_))
                            )
                        })
                        .unwrap()
                        .node_id;
                    let before =
                        active_width(&editor.projection, &editor.visual_lines, Some(node)).unwrap();
                    let start = editor
                        .projection
                        .segment_for_node(node)
                        .unwrap()
                        .projection_start();
                    editor.set_selection(start..start, false, window, cx);
                    let result = editor
                        .apply_command(EditCommand::ReplaceText {
                            node_id: node,
                            range: 0..0,
                            text: "new: line\n".into(),
                            typing: true,
                            selection_after: None,
                        })
                        .unwrap();
                    editor.refresh_after_transaction(&result);
                    assert_eq!(
                        active_width(&editor.projection, &editor.visual_lines, Some(node)),
                        Some(before)
                    );
                    editor.refresh_projection();
                    assert_eq!(
                        active_width(&editor.projection, &editor.visual_lines, Some(node)),
                        Some(before)
                    );
                    editor.undo(&Undo, window, cx);
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                });
            });
        }
    }

    #[gpui::test]
    fn multiline_code_reserves_a_number_rail(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let source = "```yaml\napp:\n  name: tachyon\n  enabled: true\n  port: 8080\n```\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, _| {
                let line = editor
                    .painted_lines
                    .iter()
                    .find(|l| l.horizontal_owner.is_some())
                    .unwrap();
                let mask = line.content_mask.unwrap().bounds;
                // The fixed number rail is outside the code text's scroll mask.
                assert!(f32::from(mask.left() - editor.element_bounds.unwrap().left()) >= 40.);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        });
    }
}
