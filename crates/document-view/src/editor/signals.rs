//! Native-shaped color/signal contracts; source order is independent of tracks.
use super::*;
use crate::signals::ColorRole;

const SOURCE: &str = include_str!("../../../../performance/layout-fixtures/74-literal-signals.md");

#[gpui::test]
fn color_objects_have_measured_natural_geometry_and_exact_source(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let document = Document::from_markdown(SOURCE).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        assert_eq!(
            projection
                .segments()
                .iter()
                .filter(|s| s.context.badge.is_some())
                .count(),
            11
        );
        for (width, height, zoom) in [
            (1280., 1600., 1.),
            (760., 1000., 1.),
            (420., 1000., 1.),
            (230., 600., 1.),
            (640., 1000., 2.),
            (960., 300., 1.),
        ] {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), zoom);
            let plan =
                build_measured_adaptive_plan(&projection, width, height, None, false, &fonts);
            if width == 1280. {
                assert!(
                    projection
                        .segments()
                        .iter()
                        .filter(|s| s.context.metadata)
                        .all(|s| plan.label_rows[&s.node_id]
                            .slot()
                            .is_some_and(|slot| slot.columns == 3)),
                    "short metadata must use the available width before stacking"
                );
            }
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                width,
                &plan,
                Some(&fonts),
            );
            let owners = projection
                .segments()
                .iter()
                .filter(|s| s.context.color_role == Some(ColorRole::Label))
                .map(|s| s.node_id)
                .collect::<Vec<_>>();
            assert_eq!(owners.len(), 5);
            for segment in projection.segments() {
                let own = lines
                    .iter()
                    .filter(|l| {
                        projection
                            .segment_for_range(&l.projected_range())
                            .unwrap()
                            .node_id
                            == segment.node_id
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    own.iter()
                        .map(|l| &projection.text()[l.projected_range()])
                        .collect::<String>(),
                    projection.text()[segment.projection_range()]
                );
                let Some(role) = segment.context.color_role else {
                    continue;
                };
                let slot = plan.slots[&segment.node_id];
                for line in &own {
                    assert_eq!(
                        (line.style.font_size, line.style.line_height),
                        if role == ColorRole::Label {
                            (18., 24.)
                        } else {
                            (14., 20.)
                        }
                    );
                    assert_eq!(
                        line.inset,
                        24. + editorial::color_leading(slot.width(width))
                    );
                    assert!(
                        fonts
                            .line_width(&projection, line.projected_range(), line.style.font_size)
                            .unwrap()
                            <= slot.width(width) - line.inset - 24. + 0.5
                    );
                }
                if role == ColorRole::Literal {
                    assert_eq!(own.len(), 1);
                }
                if width < 560. {
                    assert_eq!(slot.columns, 1);
                }
            }
            if width < 560. {
                for pair in owners[..3].windows(2) {
                    let last = lines
                        .iter()
                        .rfind(|l| l.slot.is_some_and(|s| s.group == pair[0]))
                        .unwrap();
                    let first = lines
                        .iter()
                        .find(|l| l.slot.is_some_and(|s| s.group == pair[1]))
                        .unwrap();
                    assert!(
                        (first.y
                            - first.style.space_above
                            - last.y
                            - last.style.line_height
                            - last.style.space_below
                            - 24.)
                            .abs()
                            < 0.01
                    );
                }
            }
        }
        assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
    });
}

#[gpui::test]
fn incomplete_color_edit_keeps_tracks_not_a_stale_swatch(cx: &mut gpui::TestAppContext) {
    cx.update(|cx| {
        let mut document = Document::from_markdown(SOURCE).unwrap();
        let before = TextProjection::from_snapshot(&document.snapshot());
        let fonts =
            FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
        let initial = build_measured_adaptive_plan(&before, 1280., 1600., None, false, &fonts);
        let node = before
            .segments()
            .iter()
            .find(|s| s.context.color_rgba == Some(0x3f6247ff))
            .unwrap()
            .node_id;
        document
            .apply(EditCommand::ReplaceText {
                node_id: node,
                range: 1..2,
                text: "Z".into(),
                typing: true,
                selection_after: None,
            })
            .unwrap();
        let mut after = TextProjection::from_snapshot(&document.snapshot());
        after.retain_value_roles(&initial.editorials, Some(node));
        let held = arrangement::build_edit_locked_adaptive_plan(
            &after,
            1280.,
            1600.,
            Some(&initial),
            false,
            &fonts,
            Some(node),
        );
        assert_eq!(held.slots[&node], initial.slots[&node]);
        assert_eq!(
            after.segment_for_node(node).unwrap().context.color_role,
            Some(ColorRole::Literal)
        );
        assert_eq!(
            after.segment_for_node(node).unwrap().context.color_rgba,
            None
        );
        assert_eq!(
            document.snapshot().serialize().unwrap(),
            SOURCE.replacen("`#3F6247`", "`#ZF6247`", 1)
        );
        let released = TextProjection::from_snapshot(&document.snapshot());
        assert_eq!(
            released.segment_for_node(node).unwrap().context.color_role,
            None
        );
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
    });
}

#[gpui::test]
fn color_pointer_edit_and_undo_use_the_literal_source_range(cx: &mut gpui::TestAppContext) {
    cx.update(crate::init_editor);
    let (editor, cx) = cx.add_window_view(|window, cx| {
        RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
    });
    let cx: &mut gpui::VisualTestContext = cx;
    cx.simulate_resize(size(px(1600.), px(1600.)));
    cx.run_until_parked();
    cx.update(|window, cx| {
        _ = window.draw(cx);
    });
    let (point, node, slot) = editor.read_with(cx, |editor, _| {
        let s = editor
            .projection
            .segments()
            .iter()
            .find(|s| s.context.color_rgba == Some(0x3f6247ff))
            .unwrap();
        let line = editor
            .painted_lines
            .iter()
            .find(|l| l.range == s.projection_range())
            .unwrap();
        (
            line.bounds.origin + point(line.layout.x_for_index(2), line.line_height / 2.),
            s.node_id,
            editor.adaptive.slots[&s.node_id],
        )
    });
    cx.simulate_click(point, gpui::Modifiers::default());
    cx.run_until_parked();
    cx.update(|window, cx| {
        editor.update(cx, |editor, cx| {
            editor.replace_text_in_range(None, "a", window, cx)
        })
    });
    editor.read_with(cx, |editor, _| {
        assert_eq!(
            editor.document.snapshot().serialize().unwrap(),
            SOURCE.replacen("`#3F6247`", "`#3aF6247`", 1)
        );
        assert_eq!(editor.adaptive.slots[&node], slot);
        assert_eq!(
            editor
                .projection
                .segment_for_node(node)
                .unwrap()
                .context
                .color_rgba,
            None
        );
    });
    cx.update(|window, cx| editor.update(cx, |editor, cx| editor.perform_undo(window, cx)));
    editor.read_with(cx, |editor, _| {
        assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE)
    });
}
