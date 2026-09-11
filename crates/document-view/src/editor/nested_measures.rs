//! Keep ancestry gutters separate from the measure of nested reading text.
use super::*;

pub(super) fn reading_width(
    plan: &AdaptivePlan,
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    available: f32,
) -> f32 {
    if let Some(columns) = plan.label_rows.get(&segment.node_id)
        && columns
            .timeline()
            .is_some_and(|timeline| timeline.slot.is_none())
    {
        let text = if columns.stacked() {
            columns.label_width.max(columns.body_width)
        } else {
            columns.label_width + LAYOUT_GAP + columns.body_width
        };
        // Measurement, painting, container chrome and accessibility must all
        // own the same outer extent, including the date rail and source insets.
        let insets = available - segment_text_width(segment, projection, available);
        return (text + insets).min(available);
    }
    let nested = if segment.context.table_cell.is_none()
        && matches!(
            projection.block(segment.node_id),
            Some(BlockNode::Paragraph(_))
        ) {
        let root_ordered = segment.context.list_ancestors.first().is_some_and(|id| {
            matches!(projection.block(*id), Some(BlockNode::List(list)) if matches!(list.kind, document_core::ListKind::Ordered { .. }))
        });
        if segment.context.compact_outline && segment.context.list_depth > 2 {
            compact_tree::INSET
                - 24.
                - if root_ordered {
                    NUMBERED_LIST_EXTRA_GAP
                } else {
                    0.
                }
        } else {
            segment.context.list_depth.saturating_sub(1) as f32 * 24.
                + segment
                    .context
                    .ordered_list_depth
                    .saturating_sub(usize::from(root_ordered)) as f32
                    * NUMBERED_LIST_EXTRA_GAP
        }
    } else {
        0.
    };
    (plan.prose_measures.fit_width(
        (available - nested).max(0.),
        segment.context.narrative
            || segment.context.quote.is_some()
            || segment.context.bibliography.is_some(),
        plan.lead == Some(segment.node_id),
    ) + nested)
        .min(available)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn deep_outline_keeps_readable_space_on_a_narrow_canvas(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for marker in ["- ", "1. "] {
                let source = (0..12).map(|depth| format!("{}{marker}Level {} keeps its original evidence and explanation together.\n", " ".repeat(depth * marker.len()), depth + 1)).collect::<String>();
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                for zoom in [1., 1.5, 2.] {
                    let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), zoom);
                    let plan = build_measured_adaptive_plan(&projection, 360., 1000., None, false, &fonts);
                    let lines = build_measured_visual_lines(&projection, &HashMap::new(), 360., &plan, Some(&fonts));
                    let deepest = projection.segments().last().unwrap();
                    let line = lines.iter().find(|l| l.projected_start() == deepest.projection_start()).unwrap();
                    assert!(line.width_fraction * 360. - line.inset >= 280., "deep hierarchy must not squeeze prose into a sliver: {}", line.width_fraction * 360. - line.inset);
                    assert_eq!(lines.iter().map(|l| &projection.text()[l.projected_range()]).collect::<String>(), projection.segments().iter().map(|s| &projection.text()[s.projection_range()]).collect::<String>());
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn wide_nested_paragraphs_keep_their_reading_measure(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for marker in ["- ", "1. "] {
                let source = (0..8)
                    .map(|depth| {
                        format!(
                            "{}{marker}{}\n",
                            " ".repeat(depth * marker.len()),
                            "The explanation keeps its evidence and qualifications together. "
                                .repeat(4)
                        )
                    })
                    .collect::<String>();
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                assert_eq!(projection.segments().len(), 8);
                for zoom in [1., 1.5, 2.] {
                    let fonts = FontMeasurement::new(
                        cx.text_system().clone(),
                        "Public Sans Tachyon".into(),
                        zoom,
                    );
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        1600.,
                        1000.,
                        None,
                        false,
                        &fonts,
                    );
                    let lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        1600.,
                        &plan,
                        Some(&fonts),
                    );
                    let first = &lines[0];
                    let expected = first.width_fraction * 1600. - first.inset;
                    for segment in projection.segments() {
                        let line = lines
                            .iter()
                            .find(|line| line.projected_start() == segment.projection_start())
                            .unwrap();
                        assert!(
                            (line.width_fraction * 1600. - line.inset - expected).abs() < 0.01,
                            "depth {} loses reading measure",
                            segment.context.list_depth
                        );
                    }
                    assert_eq!(
                        lines
                            .iter()
                            .map(|l| &projection.text()[l.projected_range()])
                            .collect::<String>(),
                        projection
                            .segments()
                            .iter()
                            .map(|s| &projection.text()[s.projection_range()])
                            .collect::<String>()
                    );
                    for width in [360., 620., 1000.] {
                        let narrow = build_measured_adaptive_plan(
                            &projection,
                            width,
                            1000.,
                            Some(&plan),
                            false,
                            &fonts,
                        );
                        let lines = build_measured_visual_lines(
                            &projection,
                            &HashMap::new(),
                            width,
                            &narrow,
                            Some(&fonts),
                        );
                        for segment in projection.segments() {
                            let line = lines
                                .iter()
                                .find(|line| line.projected_start() == segment.projection_start())
                                .unwrap();
                            assert!(
                                line.width_fraction * width <= width + 0.01,
                                "nesting cannot widen the viewport"
                            );
                            assert!(line.width_fraction * width - line.inset > 0.);
                            let held = build_edit_locked_adaptive_plan(
                                &projection,
                                width,
                                1000.,
                                Some(&narrow),
                                false,
                                &fonts,
                                Some(segment.node_id),
                            );
                            let held_lines = build_measured_visual_lines(
                                &projection,
                                &HashMap::new(),
                                width,
                                &held,
                                Some(&fonts),
                            );
                            let focused = held_lines
                                .iter()
                                .find(|l| l.projected_start() == line.projected_start())
                                .unwrap();
                            assert_eq!(focused.width_fraction, line.width_fraction);
                            assert_eq!(focused.inset, line.inset);
                        }
                    }
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }
}
