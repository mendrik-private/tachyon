//! Font-measured property rows. All visible text remains in its source node.
use super::*;
use crate::adaptive::{
    CardAccent, LabelColumns, LabelPresentation, LayoutSlot, metadata::Placement,
};

pub(super) fn measure(
    projection: &TextProjection,
    list: &document_core::ListBlock,
    width: f32,
    allow_strip: bool,
    fonts: &FontMeasurement,
) -> Option<Vec<(NodeId, LabelColumns)>> {
    let mut entries = Vec::with_capacity(list.items.len());
    for item in list.items.iter() {
        let BlockNode::Paragraph(p) = item.blocks.get(0)?.as_ref() else {
            return None;
        };
        let end = crate::adaptive::authored_label_end(p)?;
        let segment = projection.segment_for_node(p.id)?;
        if segment.context.table_cell.is_some()
            || segment.context.list_depth != 1
            || inline_math::has_math(projection, p.id)
            || measurement::contains_strong_rtl(&projection.text()[segment.projection_range()])
        {
            return None;
        }
        let start = segment.projection_start();
        let label_width = fonts
            .line_width(projection, start..start + end, DocumentStyle::METADATA_SIZE)?
            .ceil();
        let value_start = start + end;
        let mut value_width = Some(0_f32);
        for_each_display_line_range(
            &projection.text()[value_start..segment.projection_end()],
            |range| {
                value_width = value_width
                    .zip(fonts.line_width(
                        projection,
                        value_start + range.start..value_start + range.end,
                        DocumentStyle::METADATA_SIZE,
                    ))
                    .map(|(before, next)| before.max(next.ceil()));
            },
        );
        let value_width = value_width?
            + if segment.context.badge.is_some() {
                2. * DocumentStyle::BADGE_INSET
            } else {
                0.
            };
        entries.push((p.id, end, label_width, value_width));
    }
    if entries.is_empty() {
        return None;
    }
    let preferred = fonts.prose_width(false, DocumentStyle::REFERENCE_SIZE);
    let required = entries.iter().map(|e| e.2 + 8. + e.3).sum::<f32>()
        + LAYOUT_GAP * entries.len().saturating_sub(1) as f32;
    let canvas = width.min(preferred.max(required));
    if allow_strip
        && width >= DocumentStyle::SINGLE_COLUMN_WIDTH
        && entries.len() <= 6
        && let Some(canvas) = strip_canvas(&entries, canvas, width)
        && let Some(rows) = strip(projection, list.id, &entries, canvas, fonts)
    {
        return Some(rows);
    }
    let available = width.min(preferred).max(1.);
    let label = entries.iter().map(|e| e.2).fold(0., f32::max);
    let stacked = label > available * 0.4 || available - label - LAYOUT_GAP < 160.;
    let rows = entries
        .iter()
        .map(|&(node, end, _, _)| {
            (
                node,
                LabelColumns {
                    label_end: end,
                    label_width: if stacked { available } else { label },
                    body_width: if stacked {
                        available
                    } else {
                        available - label - LAYOUT_GAP
                    },
                    presentation: LabelPresentation::Metadata(Placement {
                        slot: None,
                        stacked,
                        badge_inset: if projection
                            .segment_for_node(node)
                            .is_some_and(|s| s.context.badge.is_some())
                        {
                            DocumentStyle::BADGE_INSET
                        } else {
                            0.
                        },
                    }),
                },
            )
        })
        .collect::<Vec<_>>();
    // Unknown native metrics must not publish an unverified arrangement.
    for (node, columns) in &rows {
        label_rows::build(
            projection,
            projection.segment_for_node(*node)?,
            *columns,
            Some(fonts),
        )?;
    }
    Some(rows)
}

/// The sum of text widths is not the width of a twelve-track arrangement:
/// rounding every item's minimum up to whole tracks can require a little
/// more room. Choose the smallest feasible canvas from the finite set of
/// track-boundary widths, before falling back to rows. No extra shaping and
/// no per-pixel search; at most 6 * 12 candidates, only during preparation.
fn strip_canvas(entries: &[(NodeId, usize, f32, f32)], preferred: f32, limit: f32) -> Option<f32> {
    let need = |entry: &(NodeId, usize, f32, f32)| entry.2 + 8. + entry.3 + LAYOUT_GAP;
    std::iter::once(preferred)
        .chain(entries.iter().flat_map(|entry| {
            (1..=12).map(move |span| need(entry) * 12. / span as f32 - LAYOUT_GAP + 0.01)
        }))
        .filter(|width| *width >= preferred && *width <= limit)
        .filter(|width| {
            let pitch = (*width + LAYOUT_GAP) / 12.;
            entries
                .iter()
                .map(|e| (need(e) / pitch).ceil().max(1.) as usize)
                .sum::<usize>()
                <= 12
        })
        .min_by(f32::total_cmp)
}

fn strip(
    projection: &TextProjection,
    group: NodeId,
    entries: &[(NodeId, usize, f32, f32)],
    canvas: f32,
    fonts: &FontMeasurement,
) -> Option<Vec<(NodeId, LabelColumns)>> {
    // Allocate actual minimum widths on the shared twelve-track grammar.
    // Residual tracks go to the most tightly fitted property; source order
    // never changes and every entry remains on this one bounded strip.
    let pitch = (canvas + LAYOUT_GAP) / 12.;
    let mut spans = entries
        .iter()
        .map(|e| ((e.2 + 8. + e.3 + LAYOUT_GAP) / pitch).ceil().max(1.) as usize)
        .collect::<Vec<_>>();
    let used = spans.iter().sum::<usize>();
    if used > 12 {
        return None;
    }
    for _ in used..12 {
        let index = entries
            .iter()
            .enumerate()
            .max_by(|(a, x), (b, y)| {
                ((x.2 + 8. + x.3) / spans[*a] as f32)
                    .total_cmp(&((y.2 + 8. + y.3) / spans[*b] as f32))
            })?
            .0;
        spans[index] += 1;
    }
    let mut track = 0;
    let mut rows = Vec::with_capacity(entries.len());
    for (index, &(node, end, label, _)) in entries.iter().enumerate() {
        let slot = LayoutSlot {
            align_components: false,
            group,
            item: index,
            row: 0,
            columns: entries.len(),
            cards: false,
            card_accent: CardAccent::Open,
            track_start: track,
            span: spans[index] as u8,
            fixed_canvas: Some(canvas),
        };
        track += slot.span;
        let columns = LabelColumns {
            label_end: end,
            label_width: label,
            body_width: slot.width(canvas) - label - 8.,
            presentation: LabelPresentation::Metadata(Placement {
                slot: Some(slot),
                stacked: false,
                badge_inset: if projection
                    .segment_for_node(node)
                    .is_some_and(|s| s.context.badge.is_some())
                {
                    DocumentStyle::BADGE_INSET
                } else {
                    0.
                },
            }),
        };
        let lines = label_rows::build(
            projection,
            projection.segment_for_node(node)?,
            columns,
            Some(fonts),
        )?;
        if lines.len() != 2
            || lines.iter().any(|line| {
                fonts
                    .line_width(projection, line.projected_range(), line.style.font_size)
                    .is_none_or(|w| w > line.label_row.unwrap().1 + 0.5)
            })
        {
            return None;
        }
        rows.push((node, columns));
    }
    Some(rows)
}

/// Hairline separators use the published text geometry, with 16 px breathing
/// room above/below. No background box and no invented status icon.
pub(super) fn rules(
    component: &arrangement::ComponentGeometry,
    slot: LayoutSlot,
    origin: Point<Pixels>,
    width: f32,
    zoom: f32,
    outer: bool,
) -> Vec<Bounds<Pixels>> {
    let left = origin.x + px(slot.left(width / zoom) * zoom);
    let top = origin.y + px(component.top - 16. * zoom);
    let bottom = origin.y + px(component.bottom + 16. * zoom);
    let mut rules = Vec::with_capacity(3);
    if outer {
        let span = slot.fixed_canvas.unwrap_or(width / zoom) * zoom;
        rules.push(Bounds::new(point(origin.x, top), size(px(span), px(zoom))));
        rules.push(Bounds::new(
            point(origin.x, bottom - px(zoom)),
            size(px(span), px(zoom)),
        ));
    }
    if slot.item > 0 {
        rules.push(Bounds::new(
            point(left - px(12. * zoom), top + px(12. * zoom)),
            size(px(zoom), (bottom - top - px(24. * zoom)).max(px(1.))),
        ));
    }
    rules
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = "# Guide\n\nA useful reference.\n\n- **Status:** In review\n- **Version:** `2.1`\n- **Owner:** Editorial\n\n## Next section\n\nThe source stays editable.\n";

    #[gpui::test]
    fn metadata_uses_measured_strips_rows_and_stacks(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown(SOURCE).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            for (width, strip, stacked) in [
                (1280., true, false),
                (480., false, false),
                (230., false, true),
            ] {
                let plan =
                    build_measured_adaptive_plan(&projection, width, 1000., None, false, &fonts);
                assert_eq!(plan.label_rows.len(), 3, "width {width}");
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                let mut last_range_end = 0;
                for line in &lines {
                    assert!(line.projected_start() >= last_range_end);
                    last_range_end = line.projected_end();
                }
                let components = component_geometry(
                    &projection,
                    &lines,
                    width,
                    1.,
                    &visual_line_paint_order(&lines),
                );
                for (&node, &columns) in &plan.label_rows {
                    assert_eq!(columns.slot().is_some(), strip, "width {width}");
                    assert_eq!(columns.stacked(), stacked, "width {width}");
                    let segment = projection.segment_for_node(node).unwrap();
                    let style = gpui::TextStyle {
                        font_family: "Spline Sans Tachyon".into(),
                        ..Default::default()
                    };
                    let label =
                        segment.projection_start()..segment.projection_start() + columns.label_end;
                    let runs = styled_projection_runs(
                        &projection,
                        &label,
                        label.len(),
                        &style,
                        false,
                        TachyonPalette::LIGHT,
                    );
                    assert!(
                        runs.iter()
                            .all(|run| run.font.family.as_ref() == "Spline Sans Tachyon"),
                        "metadata labels must not inherit feature serif"
                    );
                    let own = lines
                        .iter()
                        .filter(|l| segment.projection_range().contains(&l.projected_start()))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        own.iter()
                            .map(|l| &projection.text()[l.projected_range()])
                            .collect::<String>(),
                        &projection.text()[segment.projection_range()]
                    );
                    assert!(
                        own.iter().all(
                            |line| line.style.font_size == 14. && line.style.line_height == 20.
                        )
                    );
                    assert!(own.iter().all(|line| {
                        fonts
                            .line_width(&projection, line.projected_range(), 14.)
                            .unwrap()
                            <= line.label_row.unwrap().1 + 0.5
                    }));
                    if !stacked {
                        assert_eq!(own[0].y, own[1].y);
                    }
                    if let Some(slot) = columns.slot() {
                        assert!(
                            own.iter().all(
                                |l| l.inset + l.label_row.unwrap().1 <= slot.width(width) + 0.5
                            )
                        );
                        let geometry = components.get(&slot.group).unwrap();
                        for zoom in [1., 2.] {
                            let mut scaled = lines.clone();
                            scale_visual_lines(&mut scaled, zoom);
                            let scaled_components = component_geometry(
                                &projection,
                                &scaled,
                                width * zoom,
                                zoom,
                                &visual_line_paint_order(&scaled),
                            );
                            let borders = rules(
                                scaled_components.get(&slot.group).unwrap(),
                                slot,
                                point(px(0.), px(0.)),
                                width * zoom,
                                zoom,
                                true,
                            );
                            assert_eq!(borders[0].size.height, px(zoom));
                            assert_eq!(borders[0].top(), px((geometry.top - 16.) * zoom));
                            assert_eq!(borders[1].bottom(), px((geometry.bottom + 16.) * zoom));
                        }
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }

    #[gpui::test]
    fn metadata_grows_without_rearranging_the_focused_property(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let mut document = Document::from_markdown(SOURCE).unwrap();
            let before = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let initial = build_measured_adaptive_plan(&before, 1280., 1000., None, false, &fonts);
            let node = before
                .segments()
                .iter()
                .find(|s| before.text()[s.projection_range()].starts_with("Owner:"))
                .unwrap()
                .node_id;
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: 7..7,
                    text: "The complete editorial and engineering review team. ".repeat(4),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let after = TextProjection::from_snapshot(&document.snapshot());
            let locked = arrangement::build_edit_locked_adaptive_plan(
                &after,
                1280.,
                1000.,
                Some(&initial),
                false,
                &fonts,
                Some(node),
            );
            assert_eq!(locked.label_rows, initial.label_rows);
            let lines =
                build_measured_visual_lines(&after, &HashMap::new(), 1280., &locked, Some(&fonts));
            let segment = after.segment_for_node(node).unwrap();
            assert_eq!(
                lines
                    .iter()
                    .filter(|l| segment.projection_range().contains(&l.projected_start()))
                    .map(|l| &after.text()[l.projected_range()])
                    .collect::<String>(),
                &after.text()[segment.projection_range()]
            );
            let released =
                build_measured_adaptive_plan(&after, 1280., 1000., Some(&locked), false, &fonts);
            assert_eq!(released.label_rows.len(), 3);
            assert!(released.label_rows.values().all(|row| row.slot().is_none()));
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }

    #[gpui::test]
    fn property_hard_breaks_and_many_values_stay_complete_rows(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let source = "## Properties\n\n- **Location:** North wing  \n  Second floor\n- **Owner:** Editorial\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = build_measured_adaptive_plan(&projection, 1280., 1000., None, false, &fonts);
            assert_eq!(plan.label_rows.len(), 2);
            assert!(plan.label_rows.values().all(|row| row.slot().is_none()));
            let lines = build_measured_visual_lines(&projection, &HashMap::new(), 1280., &plan, Some(&fonts));
            let node = projection.segments().iter().find(|s| projection.text()[s.projection_range()].starts_with("Location:")).unwrap().node_id;
            let segment = projection.segment_for_node(node).unwrap();
            let values = lines.iter().filter(|l| segment.projection_range().contains(&l.projected_start()) && l.label_row.is_some_and(|(part, _)| part == label_rows::Part::Body)).collect::<Vec<_>>();
            assert_eq!(values.len(), 2);
            assert_eq!(values[1].y - values[0].y, 20.);
            assert_eq!(&projection.text()[values[0].projected_range()], "North wing  ");
            assert_eq!(&projection.text()[values[1].projected_range()], "Second floor");
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            let source = format!("## Metadata\n\n{}", (1..=8).map(|i| format!("- Field {i}: Value {i}\n")).collect::<String>());
            let document = Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = build_measured_adaptive_plan(&projection, 1280., 1000., None, false, &fonts);
            assert_eq!(plan.label_rows.len(), 8);
            assert!(plan.label_rows.values().all(|row| row.slot().is_none()));
        });
    }
}
