//! Resource typography and fit share this single geometry constructor.
use super::*;

pub(super) const ICON_GUTTER: f32 = 24.;

fn source_split(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    resource: crate::adaptive::resource::Resource,
) -> Option<usize> {
    let text = &projection.text()[segment.projection_range()];
    // Retain a safe canonical split when the delimiter is temporarily edited.
    match projection.block(segment.node_id) {
        Some(BlockNode::Paragraph(p)) => crate::adaptive::resource::classify(p)
            .and_then(|r| r.body_start)
            .or(resource.body_start),
        _ => resource.body_start,
    }
    .map(|end| segment.projection_start() + text.floor_char_boundary(end.min(text.len())))
}

pub(super) fn build(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    resource: crate::adaptive::resource::Resource,
    width: f32,
    fonts: Option<&FontMeasurement>,
) -> Vec<VisualLineSpec> {
    let split = source_split(projection, segment, resource);
    let inset = resource.inset();
    let available = (width - 2. * inset - ICON_GUTTER).max(1.);
    let breaks = split.into_iter().collect::<Vec<_>>();
    let mut lines = build_visual_lines_for_segment(
        projection,
        segment,
        &HashMap::new(),
        available + container_inset(segment) + 8.,
        &breaks,
        fonts,
        None,
    );
    let count = lines.len();
    for (index, line) in lines.iter_mut().enumerate() {
        line.inset = inset + ICON_GUTTER;
        line.style.space_above = if index == 0 {
            inset
        } else if split == Some(line.projected_start()) {
            8.
        } else {
            0.
        };
        line.style.space_below = if index + 1 == count { inset } else { 0. };
    }
    lines
}

pub(super) fn height(lines: &[VisualLineSpec]) -> f32 {
    lines
        .iter()
        .map(|line| line.style.space_above + line.style.line_height + line.style.space_below)
        .sum()
}

pub(super) fn measure(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    resource: crate::adaptive::resource::Resource,
    width: f32,
    fonts: &FontMeasurement,
) -> Option<crate::adaptive::candidates::ItemMeasurement> {
    if segment.projection_len() > 4096 {
        return None;
    }
    let lines = build(projection, segment, resource, width, Some(fonts));
    let available = (width - 2. * resource.inset() - ICON_GUTTER).max(1.);
    let mut overflow = false;
    for line in &lines {
        overflow |= fonts.line_width(projection, line.projected_range(), line.style.font_size)?
            > available + 0.5;
    }
    // Preferred width is an intrinsic title/description measure, not the
    // already-wrapped width. It affects scoring, never the hard fit oracle.
    let mut preferred = 0_f32;
    let start = segment.projection_start();
    let end = segment.projection_end();
    let split = source_split(projection, segment, resource).unwrap_or(end);
    for range in [start..split, split..end] {
        preferred =
            preferred.max(fonts.line_width(projection, range, DocumentStyle::REFERENCE_SIZE)?);
    }
    Some(crate::adaptive::candidates::ItemMeasurement {
        lines: lines.len(),
        height: height(&lines),
        overflow,
        preferred_width: preferred + width - available,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::{CardAccent, ListLayout};
    const SOURCE: &str = "# Library\n\n## Guides\n\n- [First handbook](https://example.org/one): Readable text and useful space.\n- [Second handbook](https://example.org/two): Clear keyboard navigation.\n- [Third handbook](https://example.org/three): Useful examples with careful attention to typography.\n\n## Next section\n\n[Single destination](guide.md)\n\n[Ordinary link](guide.md) belongs in flowing prose.\n";

    #[gpui::test]
    fn retained_resource_split_handles_shortened_multibyte_text(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let doc = Document::from_markdown("短文").unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            let segment = &projection.segments()[0];
            for body_start in [Some(1), Some(999), None] {
                let old = crate::adaptive::resource::Resource { body_start };
                let measured = measure(&projection, segment, old, 230., &fonts).unwrap();
                assert!(measured.height > 0. && !measured.overflow);
                let lines = build(&projection, segment, old, 230., Some(&fonts));
                assert_eq!(
                    lines
                        .iter()
                        .map(|l| &projection.text()[l.projected_range()])
                        .collect::<String>(),
                    "短文"
                );
            }
        });
    }

    #[gpui::test]
    fn resource_cards_fit_actual_text_keep_natural_heights_and_source_order(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let document = Document::from_markdown(SOURCE).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for width in [1200., 760., 480., 230.] {
                let plan = arrangement::build_measured_adaptive_plan(
                    &projection,
                    width,
                    900.,
                    None,
                    false,
                    &fonts,
                );
                assert_eq!(plan.resources.len(), 4);
                let lines = arrangement::build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                let resources = projection
                    .segments()
                    .iter()
                    .filter(|s| plan.resources.contains_key(&s.node_id))
                    .collect::<Vec<_>>();
                for segment in &resources {
                    let resource = plan.resources[&segment.node_id];
                    let slot = plan.slots[&segment.node_id];
                    let fragments = lines
                        .iter()
                        .filter(|l| segment.projection_range().contains(&l.projected_start()))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        fragments
                            .iter()
                            .map(|line| &projection.text()[line.projected_range()])
                            .collect::<String>(),
                        &projection.text()[segment.projection_range()]
                    );
                    let available = slot.width(width) - 2. * resource.inset() - ICON_GUTTER;
                    for line in &fragments {
                        assert_eq!(line.inset, resource.inset() + ICON_GUTTER);
                        assert!(
                            fonts
                                .line_width(
                                    &projection,
                                    line.projected_range(),
                                    line.style.font_size
                                )
                                .unwrap()
                                <= available + 0.5
                        );
                    }
                    assert_eq!(fragments[0].y - fragments[0].table_row_y, resource.inset());
                    let last = fragments.last().unwrap();
                    assert!(
                        (last.table_row_y + last.table_row_height
                            - last.y
                            - last.style.line_height
                            - resource.inset())
                        .abs()
                            < 0.01
                    );
                    if let Some(start) = resource.body_start {
                        let body = fragments
                            .iter()
                            .position(|line| {
                                line.projected_start() == segment.projection_start() + start
                            })
                            .unwrap();
                        assert_eq!(
                            fragments[body].y
                                - fragments[body - 1].y
                                - fragments[body - 1].style.line_height,
                            8.
                        );
                    }
                }
                if width < 560. {
                    assert!(
                        plan.resources
                            .keys()
                            .all(|node| plan.slots[node].columns == 1)
                    );
                }
                if width == 1200. {
                    assert!(
                        plan.lists
                            .values()
                            .any(|list| matches!(list.layout, ListLayout::Grid(_))),
                        "{:?}",
                        plan.measured_lists
                    );
                    let first = &lines[lines
                        .iter()
                        .position(|l| {
                            l.slot
                                .is_some_and(|s| matches!(s.card_accent, CardAccent::Resource(_)))
                        })
                        .unwrap()];
                    let following = resources[1..3]
                        .iter()
                        .map(|s| {
                            lines
                                .iter()
                                .find(|l| s.projection_range().contains(&l.projected_start()))
                                .unwrap()
                        })
                        .collect::<Vec<_>>();
                    assert!(
                        following
                            .iter()
                            .all(|line| line.table_row_y == first.table_row_y)
                    );
                    assert!(
                        following
                            .iter()
                            .any(|line| line.table_row_height != first.table_row_height),
                        "natural heights, not a stretched row"
                    );
                }
                let heading = projection
                    .segments()
                    .iter()
                    .find(|s| &projection.text()[s.projection_range()] == "Next section")
                    .unwrap();
                let heading = lines
                    .iter()
                    .find(|l| l.projected_start() == heading.projection_start())
                    .unwrap();
                let cards_bottom = lines
                    .iter()
                    .filter(|l| {
                        resources[..3]
                            .iter()
                            .any(|s| s.projection_range().contains(&l.projected_start()))
                    })
                    .map(|l| l.table_row_y + l.table_row_height)
                    .fold(0_f32, f32::max);
                assert_eq!(heading.y - cards_bottom, 64.);
                for zoom in [0.75, 2.] {
                    let mut scaled = lines.clone();
                    scale_visual_lines(&mut scaled, zoom);
                    for (before, after) in lines.iter().zip(scaled) {
                        assert!((after.inset - before.inset * zoom).abs() < 0.01);
                        assert!(
                            (after.table_row_height - before.table_row_height * zoom).abs() < 0.01
                        );
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }

    #[gpui::test]
    fn resource_grid_is_stable_while_typing_and_releases_after_blur(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let mut document = Document::from_markdown(SOURCE).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = arrangement::build_measured_adaptive_plan(
                &projection,
                1200.,
                900.,
                None,
                false,
                &fonts,
            );
            let segment = projection
                .segments()
                .iter()
                .find(|s| projection.text()[s.projection_range()].starts_with("Second handbook"))
                .unwrap();
            let node = segment.node_id;
            let slot = plan.slots[&node];
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: segment.projection_len()..segment.projection_len(),
                    text: " More detailed evidence.".repeat(30),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let locked = arrangement::build_edit_locked_adaptive_plan(
                &projection,
                1280.,
                900.,
                Some(&plan),
                false,
                &fonts,
                Some(node),
            );
            assert_eq!(locked.slots[&node].width(1280.), slot.width(1200.));
            assert_eq!(locked.slots[&node].left(1280.), slot.left(1200.));
            let lines = arrangement::build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1280.,
                &locked,
                Some(&fonts),
            );
            let segment = projection.segment_for_node(node).unwrap();
            assert_eq!(
                lines
                    .iter()
                    .filter(|l| segment.projection_range().contains(&l.projected_start()))
                    .map(|l| &projection.text()[l.projected_range()])
                    .collect::<String>(),
                &projection.text()[segment.projection_range()]
            );
            let released = arrangement::build_edit_locked_adaptive_plan(
                &projection,
                1280.,
                900.,
                Some(&locked),
                false,
                &fonts,
                None,
            );
            assert_eq!(released.slots[&node].columns, 1);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }
}
