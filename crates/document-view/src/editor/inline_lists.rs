//! Native measured presentation of one source-owned enumerated paragraph.
use super::*;
use crate::adaptive::{candidates, inline_lists::InlineList, prose::Flow};

fn fragment_lines(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    range: Range<usize>,
    width: f32,
    fonts: Option<&FontMeasurement>,
) -> Vec<VisualLineSpec> {
    let mut fragment = segment.clone();
    fragment.node_range =
        segment.node_range.start + range.start..segment.node_range.start + range.end;
    fragment.set_projection_range(
        segment.projection_start() + range.start..segment.projection_start() + range.end,
    );
    fragment.context.narrative = false;
    build_visual_lines_for_segment(
        projection,
        &fragment,
        &HashMap::new(),
        width + 8.,
        &[],
        fonts,
        None,
    )
}

pub(super) fn measure(
    plan: &mut AdaptivePlan,
    projection: &TextProjection,
    previous: Option<&AdaptivePlan>,
    keep: bool,
    fonts: &FontMeasurement,
) {
    for root in projection.roots() {
        let BlockNode::Paragraph(p) = root else {
            continue;
        };
        let Some(segment) = projection.segment_for_node(p.id) else {
            continue;
        };
        let text = &projection.text()[segment.projection_range()];
        if let Some(old) = previous.and_then(|old| old.inline_lists.get(&p.id))
            && old.flow.canvas <= plan.canvas + 0.01
            && text.len() <= 64 * 1024
            && (keep
                || plan.editing_node == Some(p.id)
                || (!plan.measures_root(p.id) && old.flow.sources[0].1.as_ref() == text))
        {
            let mut held = old.as_ref().clone();
            if let Some(next) = held
                .flow
                .rebased(p.id, text, projection.node_revision(p.id))
            {
                held.flow = next;
            }
            // A focus-locked outer stack can install a generic paragraph
            // slot. The published internal fragments own this node instead.
            plan.slots.remove(&p.id);
            plan.inline_lists.insert(p.id, Arc::new(held));
            if let Some(decision) = previous.and_then(|p| p.measured_inline_lists.get(&root.id())) {
                let mut decision = decision.clone();
                decision.retained_previous = true;
                plan.measured_inline_lists.insert(p.id, decision);
            }
            continue;
        }
        if plan.slots.contains_key(&p.id)
            || plan.lead == Some(p.id)
            || plan.resources.contains_key(&p.id)
            || plan.editorials.contains_key(&p.id)
            || segment.node_id != segment.top_level_node_id
            || segment.context.figure_text.is_some()
            || segment.context.bibliography.is_some()
            || keep
            || !plan.measures_root(p.id)
            || measurement::contains_strong_rtl(text)
        {
            continue;
        }
        let Some(starts) = crate::adaptive::inline_lists::starts(&p.content) else {
            continue;
        };
        if p.content.runs().iter().any(|run| {
            run.styles.iter().any(|s| {
                matches!(
                    s,
                    InlineStyle::Image { .. }
                        | InlineStyle::Math { .. }
                        | InlineStyle::PreservedHtml(_)
                )
            })
        }) {
            continue;
        }
        let decision = candidates::choose_list(
            starts.len() - 1,
            plan.canvas,
            plan.prose_measures.reference,
            None,
            false,
            true,
            |item, width, _| {
                let index = item + 1;
                let range = starts[index]..starts.get(index + 1).copied().unwrap_or(text.len());
                let lines = fragment_lines(projection, segment, range.clone(), width, Some(fonts));
                let preferred = fonts.line_width(
                    projection,
                    segment.projection_start() + range.start
                        ..segment.projection_start() + range.end,
                    DocumentStyle::REFERENCE_SIZE,
                )?;
                Some(candidates::ItemMeasurement {
                    lines: lines.len(),
                    height: lines.iter().map(|l| l.style.line_height).sum(),
                    preferred_width: preferred,
                    overflow: lines.iter().any(|line| {
                        fonts
                            .line_width(projection, line.projected_range(), line.style.font_size)
                            .is_none_or(|w| w > width + 0.5)
                    }),
                })
            },
        );
        let publish = matches!(decision.layout, ListLayout::Grid(_))
            && decision.is_valid(starts.len() - 1, plan.canvas, None);
        let rows = decision.row_columns.clone();
        plan.measured_inline_lists.insert(p.id, decision);
        if !publish {
            continue;
        }
        plan.inline_lists.insert(
            p.id,
            Arc::new(InlineList {
                rows,
                flow: Flow {
                    group: p.id,
                    canvas: plan.canvas,
                    columns: starts.len(),
                    needs_balance: false,
                    sources: vec![(p.id, Arc::from(text))],
                    revisions: vec![projection.node_revision(p.id)],
                    starts: starts.into_iter().map(|offset| (p.id, offset)).collect(),
                },
            }),
        );
    }
}

pub(super) fn build(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    list: &InlineList,
    width: f32,
    fonts: Option<&FontMeasurement>,
) -> Vec<VisualLineSpec> {
    let mut output = Vec::new();
    for (range, item) in list.flow.fragments(
        segment.node_id,
        &projection.text()[segment.projection_range()],
    ) {
        let (row, column, columns) = if item == 0 {
            (0, 0, 1)
        } else {
            list.placement(item - 1).unwrap()
        };
        let slot = LayoutSlot {
            align_components: false,
            group: segment.node_id,
            item,
            row,
            columns,
            cards: false,
            card_accent: crate::adaptive::CardAccent::Open,
            track_start: (column * (12 / columns)) as u8,
            span: (12 / columns) as u8,
            fixed_canvas: Some(list.flow.canvas),
        };
        let mut lines = fragment_lines(projection, segment, range, slot.width(width), fonts);
        for line in &mut lines {
            // The introduction is ordinary full-width flow. The first clause
            // owns its 16 px attachment gap; subsequent rows use the shared
            // grid gutter, never a second locally added margin.
            line.set_slot((item > 0).then_some(slot));
            line.x_fraction = slot.left(width) / width;
            line.width_fraction = slot.width(width) / width;
            line.style.space_above = 0.;
            line.style.space_below = 0.;
            line.gap_before = 0.;
        }
        if item > 0
            && let Some(first) = lines.first_mut()
        {
            first.gap_before = if row == 1 { 16. } else { 0. };
        }
        output.extend(lines);
    }
    output
}

pub(super) fn rebase_after_edit(
    plan: &mut AdaptivePlan,
    projection: &TextProjection,
    node: NodeId,
) {
    let Some(list) = plan.inline_lists.get_mut(&node) else {
        return;
    };
    let Some(segment) = projection.segment_for_node(node) else {
        return;
    };
    if let Some(next) = list.flow.rebased(
        node,
        &projection.text()[segment.projection_range()],
        projection.node_revision(node),
    ) {
        Arc::make_mut(list).flow = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str =
        include_str!("../../../../performance/layout-fixtures/89-inline-enumerations.md");

    #[gpui::test]
    fn enumerations_measure_complete_source_ranges_and_reflow(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown(SOURCE).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Tachyon".into(),
                    zoom,
                );
                for width in [360., 620., 1280.] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1000.,
                        None,
                        false,
                        &fonts,
                    );
                    if width == 1280. {
                        assert_eq!(plan.inline_lists.len(), 3);
                        let key = plan.geometry_key();
                        let mut changed = plan.clone();
                        changed.inline_lists.clear();
                        assert!(
                            !key.matches(&changed),
                            "fragment placement is a renderer input"
                        );
                        let mut changed = plan.clone();
                        changed.measured_inline_lists.clear();
                        assert!(
                            key.matches(&changed),
                            "candidate diagnostics are not geometry"
                        );
                    }
                    if width == 360. {
                        assert!(plan.inline_lists.is_empty());
                    }
                    let lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    assert_eq!(
                        lines
                            .iter()
                            .map(|line| &projection.text()[line.projected_range()])
                            .collect::<String>(),
                        projection
                            .segments()
                            .iter()
                            .map(|s| &projection.text()[s.projection_range()])
                            .collect::<String>()
                    );
                    for (&node, list) in &plan.inline_lists {
                        let focused = build_edit_locked_adaptive_plan(
                            &projection,
                            width,
                            1000.,
                            Some(&plan),
                            false,
                            &fonts,
                            Some(node),
                        );
                        assert_eq!(
                            focused.inline_lists.get(&node),
                            Some(list),
                            "focus must retain clause geometry"
                        );
                        assert!(
                            !focused.slots.contains_key(&node),
                            "a generic focused stack must not replace internal columns"
                        );
                        let segment = projection.segment_for_node(node).unwrap();
                        let intro = lines
                            .iter()
                            .find(|line| line.projected_start() == segment.projection_start())
                            .unwrap();
                        assert!(intro.slot.is_none());
                        let placed = lines
                            .iter()
                            .filter(|line| line.slot.is_some_and(|s| s.group == node))
                            .collect::<Vec<_>>();
                        assert!(
                            (placed[0].y - intro.y - intro.style.line_height - 16.).abs() < 0.01
                        );
                        let rows = placed
                            .chunk_by(|a, b| a.slot.unwrap().same_row(b.slot.unwrap()))
                            .collect::<Vec<_>>();
                        assert_eq!(rows.len(), list.rows.len());
                        for pair in rows.windows(2) {
                            let bottom = pair[0]
                                .iter()
                                .map(|line| line.y + line.style.line_height)
                                .fold(0., f32::max);
                            assert!((pair[1][0].y - bottom - LAYOUT_GAP).abs() < 0.01);
                        }
                        for line in placed {
                            let slot = line.slot.unwrap();
                            assert!(slot.left(width) + slot.width(width) <= width + 0.01);
                        }
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }
}
