//! Metric variants share the editorial object's source-owned placement.

/// A quiet document symbol can accompany a roomy card, but never consume the
/// remaining reading measure in a narrow track. Values never imply success.
pub(super) fn leading(width: f32) -> f32 {
    if width >= 420. { 72. } else { 0. }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::leading;
    use crate::metrics::TextRole;

    const SOURCE: &str =
        include_str!("../../../../performance/layout-fixtures/73-contextual-metrics.md");

    #[gpui::test]
    fn metric_pointer_edit_and_undo_keep_the_same_source_owned_card(cx: &mut gpui::TestAppContext) {
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
        let (position, node, slot) = editor.read_with(cx, |editor, _| {
            let node = editor
                .projection
                .segments()
                .iter()
                .find(|s| s.context.metric == Some(TextRole::Value))
                .unwrap()
                .node_id;
            let range = editor
                .projection
                .segment_for_node(node)
                .unwrap()
                .projection_range();
            let line = editor
                .painted_lines
                .iter()
                .find(|l| l.range == range)
                .unwrap();
            (
                line.bounds.origin + point(line.layout.x_for_index(1), line.line_height / 2.),
                node,
                editor.adaptive.slots[&node],
            )
        });
        cx.simulate_click(position, gpui::Modifiers::default());
        cx.run_until_parked();
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.replace_text_in_range(None, "x", window, cx)
            });
            _ = window.draw(cx);
        });
        editor.read_with(cx, |editor, _| {
            assert_eq!(
                editor
                    .projection
                    .block(node)
                    .unwrap()
                    .text()
                    .unwrap()
                    .as_cow(),
                "7x5%"
            );
            // The serializer conservatively escapes punctuation in the edited
            // node. All surrounding authored bytes must remain untouched.
            assert_eq!(
                editor.document.snapshot().serialize().unwrap(),
                SOURCE.replacen("\n75%\n", "\n7x5\\%\n", 1)
            );
            assert_eq!(editor.adaptive.slots[&node], slot);
            assert_eq!(
                editor
                    .projection
                    .segment_for_node(node)
                    .unwrap()
                    .context
                    .metric,
                Some(TextRole::Value)
            );
        });
        cx.update(|window, cx| editor.update(cx, |editor, cx| editor.perform_undo(window, cx)));
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE)
        });
    }

    #[gpui::test]
    fn metrics_use_actual_type_and_natural_card_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown(SOURCE).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for (width, height, zoom) in [
                (1280., 1600., 1.),
                (760., 1000., 1.),
                (420., 1000., 1.),
                (230., 600., 1.),
                (640., 1000., 2.),
                (960., 300., 1.),
            ] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Tachyon".into(),
                    zoom,
                );
                let plan =
                    build_measured_adaptive_plan(&projection, width, height, None, false, &fonts);
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                if width < 560. {
                    let owners = projection
                        .roots()
                        .filter_map(|root| {
                            plan.editorials
                                .get(&root.id())
                                .filter(|m| m.owner == root.id() && m.metric_value.is_some())
                                .map(|m| m.owner)
                        })
                        .take(3)
                        .collect::<Vec<_>>();
                    for pair in owners.windows(2) {
                        let before = lines
                            .iter()
                            .rfind(|l| l.slot.is_some_and(|s| s.group == pair[0]))
                            .unwrap();
                        let after = lines
                            .iter()
                            .find(|l| l.slot.is_some_and(|s| s.group == pair[1]))
                            .unwrap();
                        assert!(
                            (after.y
                                - after.style.space_above
                                - before.y
                                - before.style.line_height
                                - before.style.space_below
                                - 24.)
                                .abs()
                                < 0.01,
                            "stacked metrics must share the 24px modular rhythm"
                        );
                    }
                }
                assert_eq!(
                    plan.editorials
                        .iter()
                        .filter(|(id, m)| **id == m.owner && m.metric_value.is_some())
                        .count(),
                    5
                );
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
                    let Some(role) = segment.context.metric else {
                        continue;
                    };
                    let expected = match role {
                        TextRole::Label => (18., 24.),
                        TextRole::Value => (40., 44.),
                        TextRole::Context => (14., 20.),
                    };
                    let slot = plan.slots[&segment.node_id];
                    if width < 560. {
                        assert_eq!(slot.columns, 1);
                    }
                    for line in &own {
                        assert_eq!((line.style.font_size, line.style.line_height), expected);
                        assert_eq!(line.inset, 24. + leading(slot.width(width)));
                        let actual = fonts
                            .line_width(&projection, line.projected_range(), line.style.font_size)
                            .unwrap();
                        assert!(
                            actual <= slot.width(width) - line.inset - 24. + 0.5,
                            "{width}/{zoom}: {actual}, slot {slot:?}"
                        );
                    }
                    if role == TextRole::Value && slot.columns > 1 {
                        assert_eq!(own.len(), 1);
                    }
                }
                if width == 1280. {
                    let values = projection
                        .segments()
                        .iter()
                        .filter(|s| s.context.metric == Some(TextRole::Value))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        plan.slots[&values[0].node_id].columns, 3,
                        "{:?}",
                        plan.measured_rows.chosen
                    );
                    assert_eq!(
                        plan.slots[&values[0].node_id].group,
                        plan.slots[&values[2].node_id].group
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }

    #[gpui::test]
    fn metric_roles_and_tracks_survive_incomplete_typing_then_release(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let mut document = Document::from_markdown(SOURCE).unwrap();
            let before = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let initial = build_measured_adaptive_plan(&before, 1280., 1600., None, false, &fonts);
            let node = before
                .segments()
                .iter()
                .find(|s| s.context.metric == Some(TextRole::Value))
                .unwrap()
                .node_id;
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: 0..3,
                    text: "pending".into(),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let mut after = TextProjection::from_snapshot(&document.snapshot());
            assert_eq!(after.segment_for_node(node).unwrap().context.metric, None);
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
                after.segment_for_node(node).unwrap().context.metric,
                Some(TextRole::Value)
            );
            let lines =
                build_measured_visual_lines(&after, &HashMap::new(), 1280., &held, Some(&fonts));
            assert!(
                lines
                    .iter()
                    .filter(|l| after
                        .segment_for_range(&l.projected_range())
                        .unwrap()
                        .node_id
                        == node)
                    .all(|l| l.style.font_size == 40.)
            );
            let released_projection = TextProjection::from_snapshot(&document.snapshot());
            let released = build_measured_adaptive_plan(
                &released_projection,
                1280.,
                1600.,
                Some(&held),
                false,
                &fonts,
            );
            assert!(!released.editorials.contains_key(&node));
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }
}
