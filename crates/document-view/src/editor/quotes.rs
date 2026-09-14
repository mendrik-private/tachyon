//! Quotation geometry consumes the same source-owned lines as editing and
//! accessibility. Full-span editorial surfaces do not stretch their prose.
use super::*;

pub(super) fn has_no_outer_paragraph_margin(
    segment: &crate::ProjectionSegment,
    block: &BlockNode,
) -> bool {
    matches!(block, BlockNode::Paragraph(_))
        && (segment.context.quote_attribution
            || segment.context.quote.is_some()
                && segment.context.list_depth == 0
                && segment.context.table_cell.is_none())
}

pub(super) fn panel(
    component: &arrangement::ComponentGeometry,
    container: Bounds<Pixels>,
    zoom: f32,
    pull: bool,
) -> Option<Bounds<Pixels>> {
    let width = f32::from(container.size.width);
    let left = if pull {
        0.
    } else {
        width * component.left_fraction - DocumentStyle::QUOTE_INSET * zoom
    };
    let right = if pull {
        width
    } else {
        width * component.right_fraction
    };
    let top = component.top - DocumentStyle::QUOTE_PADDING * zoom;
    let bottom = component.bottom + DocumentStyle::QUOTE_PADDING * zoom;
    (left.is_finite() && right > left && top.is_finite() && bottom > top).then(|| {
        Bounds::from_corners(
            container.origin + point(px(left), px(top)),
            container.origin + point(px(right), px(bottom)),
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn editing_attribution_retains_its_role_until_blur_and_undo_is_exact(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let mut document = Document::from_markdown(SOURCE).unwrap();
            let before = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&before, 1280., 1000., None, false, &fonts);
            let id = before
                .segments()
                .iter()
                .find(|s| s.context.quote_attribution)
                .unwrap()
                .node_id;
            document
                .apply(EditCommand::ReplaceText {
                    node_id: id,
                    range: 0..4,
                    text: "".into(),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let snapshot = document.snapshot();
            let mut changed = TextProjection::from_snapshot(&snapshot);
            assert!(
                !changed
                    .segment_for_node(id)
                    .unwrap()
                    .context
                    .quote_attribution
            );
            changed.retain_quote_roles(&plan.quote_roles, Some(id));
            assert!(
                changed
                    .segment_for_node(id)
                    .unwrap()
                    .context
                    .quote_attribution
            );
            let held = build_edit_locked_adaptive_plan(
                &changed,
                1280.,
                1000.,
                Some(&plan),
                true,
                &fonts,
                Some(id),
            );
            let lines = arrangement::build_measured_visual_lines(
                &changed,
                &HashMap::new(),
                1280.,
                &held,
                Some(&fonts),
            );
            let line = lines
                .iter()
                .find(|l| {
                    changed
                        .segment_for_range(&l.projected_range())
                        .unwrap()
                        .node_id
                        == id
                })
                .unwrap();
            assert_eq!(
                (line.style.font_size, line.style.line_height),
                (DocumentStyle::BODY_SIZE, DocumentStyle::BODY_LEADING)
            );
            let text_style = gpui::TextStyle {
                font_family: "Public Sans Tachyon".into(),
                ..Default::default()
            };
            let runs = styled_projection_runs(
                &changed,
                &line.projected_range(),
                line.projected_range().len(),
                &text_style,
                false,
                TachyonPalette::LIGHT,
            );
            assert!(
                runs.iter()
                    .all(|run| run.font.family.as_ref() == "Public Sans Tachyon")
            );
            let blurred = TextProjection::from_snapshot(&snapshot);
            assert!(
                !blurred
                    .segment_for_node(id)
                    .unwrap()
                    .context
                    .quote_attribution
            );
            let original_other_quote = before
                .roots()
                .filter(|r| matches!(r, BlockNode::BlockQuote { .. }))
                .nth(1)
                .unwrap()
                .plain_text();
            assert_eq!(
                blurred
                    .roots()
                    .filter(|r| matches!(r, BlockNode::BlockQuote { .. }))
                    .nth(1)
                    .unwrap()
                    .plain_text(),
                original_other_quote
            );
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }
    const SOURCE: &str =
        include_str!("../../../../performance/layout-fixtures/70-quotation-grammar.md");

    #[gpui::test]
    fn quotes_keep_attribution_insets_and_reading_measure_at_each_scale(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document = Document::from_markdown(SOURCE).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for (width, zoom) in [(1280., 1.), (420., 1.), (640., 2.)] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                let plan =
                    build_measured_adaptive_plan(&projection, width, 1000., None, false, &fonts);
                let mut lines = arrangement::build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                let unscaled = lines.clone();
                for heading in projection
                    .roots()
                    .filter(|r| matches!(r, BlockNode::Heading(_)))
                {
                    let held = build_edit_locked_adaptive_plan(
                        &projection,
                        width,
                        1000.,
                        Some(&plan),
                        false,
                        &fonts,
                        Some(heading.id()),
                    );
                    let focused = arrangement::build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        width,
                        &held,
                        Some(&fonts),
                    );
                    assert_eq!(
                        focused.len(),
                        unscaled.len(),
                        "focus may not rewrap a quote"
                    );
                    for (before, after) in unscaled.iter().zip(&focused) {
                        assert_eq!(before.projected_range(), after.projected_range());
                        for (a, b) in [
                            (before.x_fraction, after.x_fraction),
                            (before.width_fraction, after.width_fraction),
                            (before.inset, after.inset),
                            (before.y, after.y),
                        ] {
                            assert!(
                                (a - b).abs() < 0.01,
                                "focus changed quote geometry: {a} -> {b}"
                            );
                        }
                    }
                }
                scale_visual_lines(&mut lines, zoom);
                let components = component_geometry(
                    &projection,
                    &lines,
                    width * zoom,
                    zoom,
                    &visual_line_paint_order(&lines),
                );
                let container = Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(width * zoom), px(3000. * zoom)),
                );
                let mut attributions = 0;
                for segment in projection.segments() {
                    let own: Vec<_> = lines
                        .iter()
                        .filter(|l| {
                            projection
                                .segment_for_range(&l.projected_range())
                                .unwrap()
                                .node_id
                                == segment.node_id
                        })
                        .collect();
                    assert_eq!(
                        own.iter()
                            .map(|l| &projection.text()[l.projected_range()])
                            .collect::<String>(),
                        projection.text()[segment.projection_range()]
                    );
                    if segment.context.quote_attribution {
                        attributions += 1;
                        assert!(own.iter().all(|l| {
                            l.style.font_size == DocumentStyle::BODY_SIZE * zoom
                                && l.style.line_height == DocumentStyle::BODY_LEADING * zoom
                        }));
                        let i = lines
                            .iter()
                            .position(|l| l.projected_start() == segment.projection_start())
                            .unwrap();
                        let before = &lines[i - 1];
                        assert!(
                            (own[0].y - before.y - before.style.line_height - 8. * zoom).abs()
                                < 0.01
                        );
                        let component = components.get(&segment.context.quote.unwrap()).unwrap();
                        let surface =
                            panel(component, container, zoom, segment.context.quote_pull).unwrap();
                        assert!(
                            (f32::from(surface.bottom())
                                - own.last().unwrap().y
                                - DocumentStyle::BODY_LEADING * zoom
                                - 16. * zoom)
                                .abs()
                                < 0.01
                        );
                        if !segment.context.quote_pull {
                            assert!(
                                (own[0].inset + own[0].x_fraction * width * zoom
                                    - f32::from(surface.left())
                                    - 24. * zoom)
                                    .abs()
                                    < 0.01
                            );
                        }
                        if segment.context.quote_pull {
                            assert_eq!(f32::from(surface.right()), width * zoom);
                            assert!(
                                own[0].width_fraction * width
                                    <= plan.prose_measures.narrative * 84. / 72. + 0.1
                            );
                            assert_eq!(own[0].x_fraction, 0.);
                            assert_eq!(own[0].inset, DocumentStyle::QUOTE_INSET * zoom);
                        }
                    }
                }
                assert_eq!(attributions, 2);
                for line in &unscaled {
                    let segment = projection
                        .segment_for_range(&line.projected_range())
                        .unwrap();
                    if segment.context.quote.is_none() {
                        continue;
                    }
                    let available =
                        segment_text_width(segment, &projection, line.width_fraction * width);
                    let actual = fonts
                        .line_width(&projection, line.projected_range(), line.style.font_size)
                        .unwrap();
                    assert!(
                        actual <= available + 0.5,
                        "every rendered quote line fits: {actual} <= {available}"
                    );
                }
                let nested = projection
                    .segments()
                    .iter()
                    .find(|s| s.context.quote_depth == 3)
                    .unwrap();
                let panels: Vec<_> = nested
                    .context
                    .quote_ancestors
                    .iter()
                    .map(|id| panel(components.get(id).unwrap(), container, zoom, false).unwrap())
                    .collect();
                for pair in panels.windows(2) {
                    assert!((f32::from(pair[1].left() - pair[0].left()) - 24. * zoom).abs() < 0.01);
                    assert!(
                        (f32::from(pair[0].right() - pair[1].right()) - 24. * zoom).abs() < 0.01
                    );
                    assert!(pair[0].top() < pair[1].top() && pair[0].bottom() > pair[1].bottom());
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }
}
