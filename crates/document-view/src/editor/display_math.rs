//! Display equations use one source-owned atomic row until explicitly edited.
//! The deterministic math renderer and canonical code node remain unchanged.
use super::*;

pub(super) fn has_editable_preview(projection: &TextProjection, node: NodeId) -> bool {
    projection.block(node).is_some_and(crate::math::is_math)
        || projection
            .block(node)
            .is_some_and(crate::diagram::is_diagram)
        || inline_math::has_math(projection, node)
}

impl RichDocumentEditor {
    pub(super) fn edit_code_preview_source(
        &mut self,
        node: NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(segment) = self.projection.segment_for_node(node) {
            // Dollar-delimited math may retain the authored opening newline.
            // Place the caret at the first token, not before that delimiter gap.
            let text = &self.projection.text()[segment.projection_range()];
            let offset = segment.projection_start() + text.len() - text.trim_start().len();
            self.focus_handle.focus(window, cx);
            self.set_selection(offset..offset, false, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hidden_equation_has_no_source_runs_for_the_native_shaper() {
        let document = Document::from_markdown("$$\nx\n$$\n").unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let range = projection.segments()[0].projection_range();
        let style = gpui::TextStyle::default();
        let hidden =
            styled_projection_runs(&projection, &range, 0, &style, false, TachyonPalette::LIGHT);
        assert_eq!(hidden.iter().map(|run| run.len).sum::<usize>(), 0);
        let editing = styled_projection_runs(
            &projection,
            &range,
            range.len(),
            &style,
            false,
            TachyonPalette::LIGHT,
        );
        assert_eq!(
            editing.iter().map(|run| run.len).sum::<usize>(),
            range.len()
        );
    }

    #[test]
    fn display_equations_are_atomic_source_owned_rows_with_open_spacing() {
        for source in [
            "Before\n\n$$\n\\frac{1}{2}\n$$\n\nAfter\n",
            "Before\n\n```math\n\\begin{pmatrix}a & b \\\\ c & d\\end{pmatrix}\n```\n\nAfter\n",
        ] {
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = projection
                .segments()
                .iter()
                .find(|s| {
                    projection
                        .block(s.node_id)
                        .is_some_and(crate::math::is_math)
                })
                .unwrap();
            for width in [280., 640., 1280.] {
                let lines = build_visual_lines_for_segment(
                    &projection,
                    segment,
                    &HashMap::new(),
                    width,
                    &[],
                    None,
                    None,
                );
                assert_eq!(lines.len(), 1);
                let line = &lines[0];
                assert_eq!(line.projected_range(), segment.projection_range());
                assert!(line.rendered_code_preview());
                assert!(line.code_line.is_none());
                assert_eq!(line.inset, 0.);
                assert_eq!(line.style.space_above, 24.);
                assert_eq!(line.style.space_below, 24.);
                let math = line.display_math.as_ref().unwrap();
                assert_eq!(line.style.line_height, math.light.height);
                assert!(math.light.semantics.is_some());
                assert!(
                    std::str::from_utf8(&math.light.image.bytes)
                        .unwrap()
                        .contains("<path")
                );
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn malformed_display_equations_retain_recoverable_source() {
        let document = Document::from_markdown("$$\n\\frac{\n$$\n").unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let segment = &projection.segments()[0];
        let lines = build_visual_lines_for_segment(
            &projection,
            segment,
            &HashMap::new(),
            640.,
            &[],
            None,
            None,
        );
        assert!(
            lines
                .iter()
                .all(|line| line.display_math.is_none() && !line.rendered_code_preview())
        );
        assert!(
            lines
                .iter()
                .any(|line| projection.text()[line.projected_range()].contains("\\frac{"))
        );
    }

    #[gpui::test]
    fn display_equation_spans_and_adjacent_gaps_use_actual_glyph_sizes(
        cx: &mut gpui::TestAppContext,
    ) {
        let source =
            include_str!("../../../../performance/layout-fixtures/72-display-equations.md");
        cx.update(|cx| {
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for (width, zoom) in [(1280., 1.), (400., 1.), (640., 2.)] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Tachyon".into(),
                    zoom,
                );
                let plan =
                    build_measured_adaptive_plan(&projection, width, 1000., None, false, &fonts);
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                let mut rendered = 0;
                for (index, line) in lines
                    .iter()
                    .enumerate()
                    .filter(|(_, line)| line.rendered_code_preview())
                {
                    rendered += 1;
                    let math = line.display_math.as_ref().unwrap();
                    let segment = segment_for_line(&projection, &line.projected_range()).unwrap();
                    assert_eq!(line.projected_range(), segment.projection_range());
                    let measure =
                        plan.prose_measures
                            .fit_width(width, segment.context.narrative, false);
                    let expected = if math.light.width <= measure - 8. {
                        width.min(measure)
                    } else {
                        width
                    };
                    assert!((line.width_fraction * width - expected).abs() < 0.1);
                    let overflow = math.light.width > expected - 8.;
                    assert!(
                        (line.style.line_height
                            - math.light.height
                            - if overflow { 16. } else { 0. })
                        .abs()
                            < 0.1
                    );
                    if let Some(before) = index.checked_sub(1).and_then(|i| lines.get(i)) {
                        assert!((line.y - before.y - before.style.line_height - 24.).abs() < 0.1);
                    }
                    if let Some(after) = lines.get(index + 1) {
                        let next = segment_for_line(&projection, &after.projected_range()).unwrap();
                        let gap = if matches!(
                            projection.block(next.node_id),
                            Some(BlockNode::Heading(_))
                        ) {
                            64.
                        } else {
                            24.
                        };
                        assert!((after.y - line.y - line.style.line_height - gap).abs() < 0.1);
                    }
                }
                assert_eq!(rendered, 5);
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn display_equation_centering_click_edit_and_return_preserve_source(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init_editor);
        let source = "# Calculation\n\n$$\n\\frac{1}{2}\n$$\n\nDefinitions remain beside the expression in source order.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        for (width, height, zoom) in [(1200., 800., 1.), (400., 600., 1.), (1000., 600., 2.)] {
            cx.simulate_resize(size(px(width), px(height)));
            editor.update(cx, |editor, cx| editor.set_zoom_factor(zoom, cx));
            for _ in 0..2 {
                cx.run_until_parked();
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
            }
            let viewport = cx.debug_bounds("display-math-viewport").unwrap();
            let image = cx.debug_bounds("display-math-image").unwrap();
            assert!((f32::from(image.center().x - viewport.center().x)).abs() < 1.);
            assert!(image.top() >= viewport.top() && image.bottom() <= viewport.bottom() + px(1.));
            assert!(
                cx.debug_bounds("code-copy-command").is_none(),
                "reading equations must not retain source-panel controls"
            );
            assert!(editor.read_with(cx, |editor, _| {
                editor
                    .visual_lines
                    .iter()
                    .any(VisualLineSpec::rendered_code_preview)
            }));
        }
        let image = cx.debug_bounds("display-math-image").unwrap();
        cx.simulate_click(image.center(), gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("code-copy-command").is_some());
        editor.read_with(cx, |editor, _| {
            let math = editor
                .visual_lines
                .iter()
                .find_map(|line| line.display_math.as_ref())
                .unwrap();
            assert!(math.source_visible);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.replace_text_in_range(None, "x", window, cx);
            })
        });
        editor.read_with(cx, |editor, _| {
            let changed = editor.document.snapshot().serialize().unwrap();
            assert!(
                changed.contains("x\\frac{1}{2}"),
                "changed: {changed}; selection: {:?}",
                editor.selection
            );
        });
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.perform_undo(window, cx)));
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_selection(0..0, false, window, cx)
            })
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("code-copy-command").is_none());
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source)
        });
    }
}
