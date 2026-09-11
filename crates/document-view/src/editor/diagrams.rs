use super::*;

const SCHEMA: &str = include_str!("../../../../performance/layout-fixtures/100-schema-tree.md");

#[gpui::test]
fn schema_explanation_uses_available_width_without_losing_peer_candidates(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        let document = Document::from_markdown(SCHEMA).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let font = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
        let plan = build_measured_adaptive_plan(&projection, 1314., 1666., None, false, &font);
        let row = plan
            .measured_rows
            .chosen
            .iter()
            .find(|row| {
                row.kind == crate::adaptive::rows::RowKind::Explanation && row.roots.start == 2
            })
            .expect("complete two-paragraph explanation pairs with the tree on a wide canvas");
        assert_eq!(row.parts, vec![3..5, 5..6]);
        assert!(
            plan.measured_rows
                .candidates
                .iter()
                .any(|row| row.kind == crate::adaptive::rows::RowKind::Technical),
            "peer alternatives must still compete"
        );
        assert_eq!(plan.measured_rows.validation_fallbacks, 0);
        for width in [360., 760.] {
            let narrow =
                build_measured_adaptive_plan(&projection, width, 1666., None, false, &font);
            assert!(
                narrow
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.roots.start != 2
                        || row.kind != crate::adaptive::rows::RowKind::Explanation)
            );
        }
        assert_eq!(document.snapshot().serialize().unwrap(), SCHEMA);
    });
}

#[gpui::test]
fn schema_previews_preserve_source_geometry_and_accessible_hierarchy(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        let document = Document::from_markdown(SCHEMA).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        for zoom in [1., 1.5, 2.] {
            let font =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), zoom);
            for width in [360., 760., 1312., 1632.] {
                let plan =
                    build_measured_adaptive_plan(&projection, width, 1000., None, false, &font);
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&font),
                );
                let previews: Vec<_> = lines.iter().filter(|line| line.diagram.is_some()).collect();
                assert_eq!(previews.len(), 2);
                for line in previews {
                    assert!(line.rendered_code_preview());
                    assert!(line.code_line.is_none());
                    let segment = segment_for_line(&projection, &line.projected_range()).unwrap();
                    assert_eq!(line.projected_range(), segment.projection_range());
                    assert!(line.style.line_height >= line.diagram.as_ref().unwrap().light.height);
                }
                assert!(
                    lines.iter().any(|line| line.code_line.is_some()),
                    "ordinary JSON remains code"
                );
                let tree = accessibility::SemanticCache::default().get(
                    &projection,
                    &Arc::new(lines),
                    width,
                );
                let label = tree.label_for_role(Role::Figure).unwrap();
                assert!(label.contains("Level 4: \"address\" = object."));
                assert!(label.contains("\"x-retention\" = 1.00e+5"));
            }
        }
        assert_eq!(document.snapshot().serialize().unwrap(), SCHEMA);
    });
}

#[gpui::test]
fn schema_preview_opens_canonical_source_and_undo_restores_tree(cx: &mut gpui::TestAppContext) {
    cx.update(crate::init_editor);
    let source = "# Schema\n\n```json\n{\"$schema\":\"https://json-schema.org/draft/2020-12/schema\",\"type\":\"object\"}\n```\n";
    let (editor, cx) = cx.add_window_view(|window, cx| {
        RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
    });
    let cx: &mut gpui::VisualTestContext = cx;
    cx.simulate_resize(size(px(1400.), px(900.)));
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }
    let image = cx.debug_bounds("diagram-image").unwrap();
    let viewport = cx.debug_bounds("diagram-viewport").unwrap();
    assert!(
        (f32::from(image.left() - viewport.left())).abs() < 1.,
        "schema labels align with the component leading edge"
    );
    cx.simulate_click(image.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    assert!(cx.debug_bounds("code-copy-command").is_some());
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.replace_text_in_range(None, " ", window, cx)
        })
    });
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.document.snapshot().serialize().unwrap(),
            source.replacen('{', " {", 1)
        )
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
    assert!(cx.debug_bounds("diagram-image").is_some());
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.document.snapshot().serialize().unwrap(), source)
    });
}

const SOURCE: &str = "# Review route\n\n```mermaid\nflowchart LR\nA[Input] --> B[Process]\nB --> C{Decision}\nC -->|Yes| D[Output A]\nC -->|No| E[Output B]\n```\n\nThe input is processed before the decision chooses an output.\n";

#[gpui::test]
fn diagram_geometry_is_atomic_zoomable_and_source_owned(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let document = Document::from_markdown(SOURCE).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        for (width, zoom) in [(1000., 1.), (360., 1.), (640., 2.)] {
            let font =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), zoom);
            let plan = build_measured_adaptive_plan(&projection, width, 900., None, false, &font);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                width,
                &plan,
                Some(&font),
            );
            let diagrams: Vec<_> = lines.iter().filter(|l| l.diagram.is_some()).collect();
            assert_eq!(diagrams.len(), 1);
            let line = diagrams[0];
            assert!(line.rendered_code_preview());
            assert!(line.code_line.is_none());
            let segment = segment_for_line(&projection, &line.projected_range()).unwrap();
            assert_eq!(line.projected_range(), segment.projection_range());
            let preview = line.diagram.as_ref().unwrap();
            assert!(line.style.line_height >= preview.light.height);
            let tree =
                accessibility::SemanticCache::default().get(&projection, &Arc::new(lines), width);
            let figure = tree.label_for_role(Role::Figure).unwrap();
            assert!(figure.contains("Decision leads to Output A — Yes."));
            assert!(figure.contains("Decision leads to Output B — No."));
        }
        assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
    });
}

#[gpui::test]
fn diagram_preview_click_source_edit_and_undo(cx: &mut gpui::TestAppContext) {
    cx.update(crate::init_editor);
    let (editor, cx) = cx.add_window_view(|window, cx| {
        RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
    });
    let cx: &mut gpui::VisualTestContext = cx;
    cx.simulate_resize(size(px(1400.), px(900.)));
    for _ in 0..2 {
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
    }
    let viewport = cx.debug_bounds("diagram-viewport").unwrap();
    let image = cx.debug_bounds("diagram-image").unwrap();
    assert!(image.size.width <= viewport.size.width);
    assert!((f32::from(image.center().x - viewport.center().x)).abs() < 1.);
    assert!(cx.debug_bounds("code-copy-command").is_none());
    cx.simulate_click(image.center(), gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    assert!(cx.debug_bounds("code-copy-command").is_some());
    editor.read_with(cx, |editor, _| {
        assert!(
            editor
                .visual_lines
                .iter()
                .any(|line| line.diagram.as_ref().is_some_and(|d| d.source_visible))
        );
        assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
    });
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.replace_text_in_range(None, "x", window, cx)
        })
    });
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.document.snapshot().serialize().unwrap(),
            SOURCE.replacen("flowchart", "xflowchart", 1)
        )
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
    assert!(cx.debug_bounds("diagram-image").is_some());
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE)
    });
}
