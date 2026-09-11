//! Aligned label–description rows, measured and painted from the same ranges.
//! Source paragraphs remain intact; this is geometry, not a table conversion.
use super::*;
use crate::adaptive::{LabelColumns, LabelPresentation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Part {
    Label,
    Body,
    StackedLabel,
    StackedBody,
}

impl Part {
    fn is_body(self) -> bool {
        matches!(self, Self::Body | Self::StackedBody)
    }
    pub(super) fn stacked(self) -> bool {
        matches!(self, Self::StackedLabel | Self::StackedBody)
    }
}

pub(super) fn measure(
    projection: &TextProjection,
    list: &document_core::ListBlock,
    width: f32,
    allow_horizontal: bool,
    fonts: &FontMeasurement,
) -> Option<Vec<(NodeId, LabelColumns)>> {
    if !matches!(list.kind, document_core::ListKind::Unordered)
        || !(2..=64).contains(&list.items.len())
    {
        return None;
    }
    let mut entries = Vec::with_capacity(list.items.len());
    let dated = crate::adaptive::timeline::is_timeline(list);
    let mut label_width = 56_f32;
    let mut available = f32::INFINITY;
    for item in list.items.iter() {
        if item.checked.is_some() || (!dated && item.blocks.len() != 1) {
            return None;
        }
        let BlockNode::Paragraph(paragraph) = item.blocks.get(0)?.as_ref() else {
            return None;
        };
        let end = if dated {
            crate::adaptive::timeline::date_end(paragraph)?
        } else {
            crate::adaptive::authored_label_end(paragraph)?
        };
        let segment = projection.segment_for_node(paragraph.id)?;
        if segment.context.table_cell.is_some()
            || (!dated && segment.context.list_depth != 1)
            // A pressure-reanchored deep tree already supplies explicit parent
            // context. Parallel date rails would cross those reanchored branches;
            // retain the complete readable tree instead at that width.
            || (dated && compact_tree::prepare(segment, width).context.compact_outline)
            || inline_math::has_math(projection, paragraph.id)
            || measurement::contains_strong_rtl(
                &projection.text()[segment.projection_range()],
            )
        {
            return None;
        }
        let label = segment.projection_start()..segment.projection_start() + end;
        if projection.text()[label.clone()].contains('\n') {
            return None;
        }
        label_width = label_width.max(
            fonts
                .line_width(projection, label, DocumentStyle::REFERENCE_SIZE)?
                .ceil(),
        );
        available = available.min(segment_text_width(segment, projection, width));
        entries.push((paragraph.id, end));
    }
    // A date rail is navigation furniture, not prose. Apply the reading cap
    // to the summary after container insets and the shared date rail, so wide
    // quoted events do not wrap early just to keep their furniture in the cap.
    let reading_width = fonts.prose_width(false, DocumentStyle::REFERENCE_SIZE);
    let body_width = (available - label_width - LAYOUT_GAP).min(if dated {
        reading_width
    } else {
        f32::INFINITY
    });
    if dated
        && !crate::adaptive::timeline::has_supporting_blocks(list)
        && allow_horizontal
        && width >= DocumentStyle::SINGLE_COLUMN_WIDTH
        && (2..=4).contains(&entries.len())
        && let Some(rows) = horizontal_timeline(projection, list.id, &entries, width, fonts)
    {
        return Some(rows);
    }
    // Terms are compact reference rows, not narrow narrative columns. Require
    // a useful description measure and reject long/uneven pseudo-definitions.
    if !dated && (body_width < 160. || label_width > available * 0.40) {
        return None;
    }
    let stacked = dated && (body_width < 160. || label_width > available * 0.40);
    let mut rows = Vec::with_capacity(entries.len());
    for (index, &(node, end)) in entries.iter().enumerate() {
        let columns = LabelColumns {
            label_end: end,
            label_width: if stacked {
                available.min(reading_width).max(1.)
            } else {
                label_width
            },
            body_width: if stacked {
                available.min(reading_width).max(1.)
            } else {
                body_width
            },
            presentation: if dated {
                LabelPresentation::Timeline(crate::adaptive::timeline::Placement {
                    next: entries.get(index + 1).map(|(node, _)| *node),
                    slot: None,
                    stacked,
                })
            } else {
                LabelPresentation::Terms
            },
        };
        let lines = build(
            projection,
            projection.segment_for_node(node)?,
            columns,
            Some(fonts),
        )?;
        if !dated
            && (lines
                .iter()
                .filter(|line| line.label_row.is_some_and(|(part, _)| part == Part::Body))
                .count()
                > 4
                || lines.iter().any(|line| {
                    fonts
                        .line_width(projection, line.projected_range(), line.style.font_size)
                        .is_none_or(|w| {
                            w > if line.label_row.is_some_and(|(part, _)| part == Part::Label) {
                                label_width + 0.5
                            } else {
                                body_width + 0.5
                            }
                        })
                }))
        {
            return None;
        }
        rows.push((node, columns));
    }
    Some(rows)
}

fn horizontal_timeline(
    projection: &TextProjection,
    group: NodeId,
    entries: &[(NodeId, usize)],
    width: f32,
    fonts: &FontMeasurement,
) -> Option<Vec<(NodeId, LabelColumns)>> {
    let mut rows = Vec::with_capacity(entries.len());
    let mut shortest = f32::INFINITY;
    let mut tallest = 0_f32;
    for (item, &(node, end)) in entries.iter().enumerate() {
        let slot = crate::adaptive::LayoutSlot {
            align_components: false,
            group,
            item,
            row: 0,
            columns: entries.len(),
            cards: false,
            card_accent: crate::adaptive::CardAccent::Open,
            track_start: (item * (12 / entries.len())) as u8,
            span: (12 / entries.len()) as u8,
            fixed_canvas: Some(width),
        };
        let measure = slot.width(width) - 8.;
        if measure < 160. {
            return None;
        }
        let columns = LabelColumns {
            label_end: end,
            label_width: measure,
            body_width: measure,
            presentation: LabelPresentation::Timeline(crate::adaptive::timeline::Placement {
                next: entries.get(item + 1).map(|(node, _)| *node),
                slot: Some(slot),
                stacked: true,
            }),
        };
        let lines = build(
            projection,
            projection.segment_for_node(node)?,
            columns,
            Some(fonts),
        )?;
        let date_lines = lines
            .iter()
            .filter(|line| !line.label_row.unwrap().0.is_body())
            .count();
        let body_lines = lines.len() - date_lines;
        if date_lines != 1
            || body_lines > 3
            || lines.iter().any(|line| {
                fonts
                    .line_width(projection, line.projected_range(), line.style.font_size)
                    .is_none_or(|w| w > measure + 0.5)
            })
        {
            return None;
        }
        let height = height(&lines);
        shortest = shortest.min(height);
        tallest = tallest.max(height);
        rows.push((node, columns));
    }
    (tallest <= shortest * 1.5).then_some(rows)
}

pub(super) fn build(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    columns: LabelColumns,
    fonts: Option<&FontMeasurement>,
) -> Option<Vec<VisualLineSpec>> {
    let BlockNode::Paragraph(paragraph) = projection.block(segment.node_id)? else {
        return None;
    };
    let text = &projection.text()[segment.projection_range()];
    // Widths stay fixed while editing. Re-resolve an authored delimiter after
    // typing inside the label; if temporarily deleted, retain a safe old split.
    let end = if columns.timeline().is_some() {
        crate::adaptive::timeline::date_end(paragraph)
    } else {
        crate::adaptive::authored_label_end(paragraph)
    }
    .unwrap_or_else(|| text.floor_char_boundary(columns.label_end.min(text.len())));
    if end == 0 || end >= text.len() {
        return None;
    }
    let mut output = Vec::new();
    let stacked = columns.stacked();
    let horizontal = columns.slot().is_some();
    let metadata = columns.metadata().is_some();
    let badge_padding = columns.metadata().map_or(0., |m| m.badge_inset);
    let (font_size, line_height) = if metadata {
        (
            DocumentStyle::METADATA_SIZE,
            DocumentStyle::METADATA_LEADING,
        )
    } else {
        (
            DocumentStyle::REFERENCE_SIZE,
            DocumentStyle::REFERENCE_LEADING,
        )
    };
    for (part, local, width, left) in [
        (
            if stacked {
                Part::StackedLabel
            } else {
                Part::Label
            },
            0..end,
            columns.label_width,
            0.,
        ),
        (
            if stacked {
                Part::StackedBody
            } else {
                Part::Body
            },
            end..text.len(),
            (columns.body_width - badge_padding * 2.).max(1.),
            badge_padding
                + if stacked {
                    0.
                } else {
                    columns.label_width
                        + if metadata && horizontal {
                            8.
                        } else {
                            LAYOUT_GAP
                        }
                },
        ),
    ] {
        let mut failed = false;
        for_each_display_line_range(&text[local.clone()], |logical| {
            let range = segment.projection_start() + local.start + logical.start
                ..segment.projection_start() + local.start + logical.end;
            let ranges = if let Some(fonts) = fonts {
                fonts.wrap(projection, segment, range, width, font_size)
            } else {
                let mut ranges = Vec::new();
                for_each_wrap_line_range(
                    projection.text(),
                    range,
                    (width / 9.).floor().max(1.) as usize,
                    |range| ranges.push(range),
                );
                Some(ranges)
            };
            let Some(ranges) = ranges else {
                failed = true;
                return;
            };
            for range in ranges {
                let source = LineSourceRange::new(projection, segment.node_id, range)
                    .expect("label row belongs to its projection segment");
                output.push(VisualLineSpec {
                    compact_tree: false,
                    source,
                    payload: visual_line_payload(VisualLinePayload {
                        label_row: Some((part, width)),
                        ..VisualLinePayload::default()
                    }),
                    style: VisualLineStyle {
                        font_size,
                        line_height,
                        space_above: 0.,
                        space_below: 0.,
                    },
                    inset: if horizontal || metadata {
                        left
                    } else {
                        container_inset(segment) + left
                    },
                    gap_before: 0.,
                    y: 0.,
                    x_fraction: 0.,
                    width_fraction: 1.,
                    table_cell_first: false,
                    flow_geometry: false,
                });
            }
        });
        if failed {
            return None;
        }
    }
    if horizontal {
        output.first_mut()?.style.space_above = 16.;
    }
    if stacked
        && let Some(body) = output
            .iter_mut()
            .find(|line| line.label_row.unwrap().0.is_body())
    {
        body.style.space_above = 8.;
    }
    // The source-order final line owns the row's following gap. Positioning
    // uses max(label height, body height), never their sum.
    output.last_mut()?.style.space_below = if metadata && horizontal {
        16.
    } else if horizontal {
        0.
    } else {
        12.
    };
    Some(output)
}

pub(super) fn height(lines: &[VisualLineSpec]) -> f32 {
    if lines
        .first()
        .is_some_and(|line| line.label_row.unwrap().0.stacked())
    {
        return lines
            .iter()
            .map(|line| line.style.line_height + line.style.space_above)
            .sum();
    }
    [Part::Label, Part::Body]
        .into_iter()
        .map(|part| {
            lines
                .iter()
                .filter(|line| line.label_row.is_some_and(|(role, _)| role == part))
                .map(|line| line.style.line_height)
                .sum::<f32>()
        })
        .fold(0., f32::max)
}

/// Date nodes and connecting rails consume already-published component bounds.
/// A scrolled event still paints its rail; no source walk or shaping occurs.
pub(super) fn timeline_marks(
    component: &arrangement::ComponentGeometry,
    next: Option<&arrangement::ComponentGeometry>,
    horizontal: bool,
    origin: Point<Pixels>,
    width: f32,
    zoom: f32,
) -> (Bounds<Pixels>, Option<Bounds<Pixels>>) {
    let x = origin.x
        + px(component.left_fraction * width + if horizontal { 4. * zoom } else { -16. * zoom });
    let y = origin.y + px(component.top + if horizontal { -12. * zoom } else { 12. * zoom });
    let dot = Bounds::new(
        point(x - px(4. * zoom), y - px(4. * zoom)),
        size(px(8. * zoom), px(8. * zoom)),
    );
    let rail = next.map(|next| {
        if horizontal {
            Bounds::new(
                point(x, y - px(0.5 * zoom)),
                size(
                    px(((next.left_fraction - component.left_fraction) * width).max(0.)),
                    px(zoom),
                ),
            )
        } else {
            Bounds::new(
                point(x - px(0.5 * zoom), y),
                size(px(zoom), px((next.top - component.top).max(0.))),
            )
        }
    });
    (dot, rail)
}

#[cfg(test)]
mod tests {
    use super::*;
    const DATES: &str = "# Milestones\n\n- **2022:** Initial research\n- **2023:** Prototype and testing\n- **2024:** Public release\n- **2025:** Continued improvement\n\n## Afterwards\n\nThe document continues.\n";

    #[gpui::test]
    fn nested_timelines_preserve_every_dated_list(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!("../../../../performance/layout-fixtures/121-nested-timelines.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), zoom);
                for width in [1400., 230.] {
                    let plan = build_measured_adaptive_plan(&projection, width, 1000., None, false, &fonts);
                    assert_eq!(plan.label_rows.len(), 8, "every explicit-date list is measured, including two nested histories and an event inside an event");
                    assert!(plan.label_rows.values().all(|row| row.timeline().unwrap().slot.is_none()));
                    let node = |prefix: &str| projection.segments().iter().find(|s| projection.text()[s.projection_range()].starts_with(prefix)).unwrap().node_id;
                    let outer = node("2026:");
                    let inner = node("2026-09-08:");
                    assert_eq!(plan.timeline_chain(node("tachyon release")).collect::<Vec<_>>(), vec![inner, outer]);
                    assert_eq!(plan.timeline_chain(node("The release summary")).collect::<Vec<_>>(), vec![outer], "leaving the child list returns to the parent event");
                    assert_eq!(plan.timeline_parents.len(), 2);
                    let mut changed = plan.clone();
                    changed.timeline_parents.clear();
                    assert!(!plan.geometry_key().matches(&changed), "ancestor rail ownership is published geometry");
                    for segment in projection.segments() {
                        let text = &projection.text()[segment.projection_range()];
                        if text.starts_with("The research branch") || text.starts_with("Unrelated content") || text == "Working methods" {
                            assert!(!plan.timeline_owners.contains_key(&segment.node_id), "ownership cannot leak out of a nested list");
                        }
                    }
                    let lines = build_measured_visual_lines(&projection, &HashMap::new(), width, &plan, Some(&fonts));
                    assert_eq!(lines.iter().map(|line| &projection.text()[line.projected_range()]).collect::<String>(), projection.segments().iter().map(|s| projection.text()[s.projection_range()].replace('\n', "")).collect::<String>());
                    for line in &lines {
                        if let Some((_, measured)) = line.label_row {
                            assert!(line.inset + measured <= line.width_fraction * width + 0.01);
                        }
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn deep_dated_hierarchy_uses_complete_readable_tree_under_pressure(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "- Root\n  - Branch\n    - Context\n      - 2024: First event.\n      - 2025: Second event.\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let wide = build_measured_adaptive_plan(&projection, 1400., 1000., None, false, &fonts);
            assert_eq!(wide.label_rows.len(), 2);
            let narrow = build_measured_adaptive_plan(&projection, 280., 1000., Some(&wide), false, &fonts);
            assert!(narrow.label_rows.is_empty());
            assert!(narrow.timeline_owners.is_empty() && narrow.timeline_parents.is_empty());
            let lines = build_measured_visual_lines(&projection, &HashMap::new(), 280., &narrow, Some(&fonts));
            assert!(lines.iter().any(|line| line.compact_tree));
            assert_eq!(lines.iter().map(|line| &projection.text()[line.projected_range()]).collect::<String>(), projection.segments().iter().map(|s| &projection.text()[s.projection_range()]).collect::<String>());
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn nested_event_code_growth_retains_both_rails_and_exact_undo(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/121-nested-timelines.md");
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let initial =
                build_measured_adaptive_plan(&projection, 1400., 1000., None, false, &fonts);
            let code = projection
                .segments()
                .iter()
                .find(|s| projection.text()[s.projection_range()].starts_with("tachyon release"))
                .unwrap()
                .node_id;
            let owners = initial.timeline_chain(code).collect::<Vec<_>>();
            assert_eq!(owners.len(), 2);
            document
                .apply(EditCommand::ReplaceText {
                    node_id: code,
                    range: 0..0,
                    text: "# Retained nested event\n".repeat(60),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let edited = TextProjection::from_snapshot(&document.snapshot());
            let locked = arrangement::build_edit_locked_adaptive_plan(
                &edited,
                1400.,
                1000.,
                Some(&initial),
                false,
                &fonts,
                Some(code),
            );
            assert_eq!(locked.label_rows, initial.label_rows);
            assert_eq!(locked.timeline_chain(code).collect::<Vec<_>>(), owners);
            let lines =
                build_measured_visual_lines(&edited, &HashMap::new(), 1400., &locked, Some(&fonts));
            let components =
                component_geometry(&edited, &lines, 1400., 1., &visual_line_paint_order(&lines));
            let code_bounds = components.get(&code).unwrap();
            assert!(code_bounds.bottom - code_bounds.top > 1000.);
            for owner in owners {
                let next = locked.label_rows[&owner].timeline().unwrap().next.unwrap();
                let (_, rail) = timeline_marks(
                    components.get(&owner).unwrap(),
                    components.get(&next),
                    false,
                    point(px(0.), px(0.)),
                    1400.,
                    1.,
                );
                assert!(f32::from(rail.unwrap().bottom()) > code_bounds.bottom);
            }
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn enclosed_timelines_keep_container_and_event_boundaries(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/120-enclosed-timelines.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Tachyon".into(),
                    zoom,
                );
                for width in [1400., 230.] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1000.,
                        None,
                        false,
                        &fonts,
                    );
                    let mut headers = 0;
                    for segment in projection.segments() {
                        let text = &projection.text()[segment.projection_range()];
                        if ["2024:", "2025:", "2026-09-08:", "2026-09-10:"]
                            .iter()
                            .any(|date| text.starts_with(date))
                        {
                            headers += 1;
                            let row = plan.label_rows.get(&segment.node_id).unwrap_or_else(|| {
                                panic!("enclosed authored dates retain their timeline: {text}")
                            });
                            assert!(
                                row.timeline().unwrap().slot.is_none(),
                                "enclosed timelines stay vertical"
                            );
                            assert_eq!(row.stacked(), width < 300.);
                            if width > 1000. {
                                assert!(
                                    (row.body_width
                                        - fonts.prose_width(false, DocumentStyle::REFERENCE_SIZE))
                                    .abs()
                                        < 0.01,
                                    "date rails must not consume the readable summary measure"
                                );
                            }
                        }
                        if segment.context.list_depth == 0 || text.contains("archive entry") {
                            assert!(
                                !plan.timeline_owners.contains_key(&segment.node_id),
                                "event ownership must not leak beyond its list: {text}"
                            );
                        }
                        if text.starts_with("Supporting evidence") || text.starts_with("tachyon") {
                            let owner = plan.timeline_owners[&segment.node_id];
                            assert!(
                                projection.text()[projection
                                    .segment_for_node(owner)
                                    .unwrap()
                                    .projection_range()]
                                .starts_with("2024:")
                            );
                        }
                        if text.starts_with("Keep the original")
                            || text.starts_with("Record the unanswered")
                        {
                            let owner = plan.timeline_owners[&segment.node_id];
                            assert!(
                                projection.text()[projection
                                    .segment_for_node(owner)
                                    .unwrap()
                                    .projection_range()]
                                .starts_with("2025:")
                            );
                        }
                    }
                    assert_eq!(headers, 4);
                    let lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    for line in &lines {
                        if let Some((_, measured)) = line.label_row {
                            assert!(
                                line.inset + measured <= line.width_fraction * width + 0.01,
                                "published event bounds must include its measured columns"
                            );
                        }
                    }
                    assert_eq!(
                        lines
                            .iter()
                            .map(|line| &projection.text()[line.projected_range()])
                            .collect::<String>(),
                        projection
                            .segments()
                            .iter()
                            .map(|s| projection.text()[s.projection_range()].replace('\n', ""))
                            .collect::<String>()
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn enclosed_timeline_edit_retains_columns_and_exact_undo(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/120-enclosed-timelines.md");
            for prefix in ["Supporting evidence", "tachyon", "2026-09-08:"] {
                let mut document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Tachyon".into(),
                    1.,
                );
                let initial =
                    build_measured_adaptive_plan(&projection, 1400., 1000., None, false, &fonts);
                let segment = projection
                    .segments()
                    .iter()
                    .find(|s| projection.text()[s.projection_range()].starts_with(prefix))
                    .unwrap();
                let node = segment.node_id;
                let offset = segment.projection_len();
                document
                    .apply(EditCommand::ReplaceText {
                        node_id: node,
                        range: offset..offset,
                        text: " More supporting evidence.".repeat(20),
                        typing: true,
                        selection_after: None,
                    })
                    .unwrap();
                let edited = TextProjection::from_snapshot(&document.snapshot());
                let locked = arrangement::build_edit_locked_adaptive_plan(
                    &edited,
                    1400.,
                    1000.,
                    Some(&initial),
                    false,
                    &fonts,
                    Some(node),
                );
                assert_eq!(locked.label_rows, initial.label_rows);
                assert_eq!(locked.timeline_owners, initial.timeline_owners);
                let narrow =
                    build_measured_adaptive_plan(&edited, 230., 300., Some(&locked), false, &fonts);
                let owner = narrow.timeline_owners[&node];
                assert!(narrow.label_rows[&owner].stacked());
                let reopened =
                    Document::from_markdown(document.snapshot().serialize().unwrap()).unwrap();
                assert!(
                    TextProjection::from_snapshot(&reopened.snapshot())
                        .text()
                        .contains(&" More supporting evidence.".repeat(20))
                );
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn rich_dated_events_keep_supporting_blocks_in_vertical_source_order(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/119-rich-timelines.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 1400., 1000., None, false, &fonts);
            let leads = projection
                .segments()
                .iter()
                .filter(|s| {
                    projection.text()[s.projection_range()].starts_with("2026-09-")
                        && s.context.task_checked.is_none()
                })
                .collect::<Vec<_>>();
            assert_eq!(leads.len(), 3);
            for lead in &leads {
                let placement = plan
                    .label_rows
                    .get(&lead.node_id)
                    .and_then(|row| row.timeline());
                assert!(
                    placement.is_some(),
                    "every rich event retains an explicit date header"
                );
                assert!(
                    placement.unwrap().slot.is_none(),
                    "rich events never form a horizontal strip"
                );
            }
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), zoom);
                for width in [1400., 230.] {
                    let plan = build_measured_adaptive_plan(&projection, width, 1000., None, false, &fonts);
                    let root = leads[0].top_level_node_id;
                    let mut owner = leads[0].node_id;
                    for segment in projection.segments().iter().filter(|s| s.top_level_node_id == root) {
                        if leads.iter().any(|lead| lead.node_id == segment.node_id) {
                            owner = segment.node_id;
                            let row = plan.label_rows[&owner];
                            assert!(row.timeline().unwrap().slot.is_none());
                            assert_eq!(row.stacked(), width < 300.);
                        }
                        assert_eq!(plan.timeline_owners.get(&segment.node_id), Some(&owner), "supporting content must retain its source event, including nested leaves");
                        assert!(!plan.slots.contains_key(&segment.node_id), "rich events cannot acquire compact peer slots");
                    }
                    let lines = build_measured_visual_lines(&projection, &HashMap::new(), width, &plan, Some(&fonts));
                    // Newlines delimit visual code rows; they are not painted
                    // glyphs. Exact underlying bytes are checked separately.
                    assert_eq!(lines.iter().map(|line| &projection.text()[line.projected_range()]).collect::<String>(), projection.segments().iter().map(|s| projection.text()[s.projection_range()].replace('\n', "")).collect::<String>());
                    let components = component_geometry(&projection, &lines, width, 1., &visual_line_paint_order(&lines));
                    for (index, lead) in leads.iter().enumerate() {
                        let next = leads.get(index + 1).map(|s| components.get(&s.node_id).unwrap());
                        let (_, rail) = timeline_marks(components.get(&lead.node_id).unwrap(), next, false, point(px(0.), px(0.)), width, 1.);
                        if let Some(next) = next {
                            let rail = rail.unwrap();
                            assert!((f32::from(rail.bottom()) - (next.top + 12.)).abs() < 0.01);
                            let supports = projection.segments().iter().filter(|s| s.top_level_node_id == root && s.node_id != lead.node_id && plan.timeline_owners[&s.node_id] == lead.node_id);
                            for support in supports {
                                assert!(components.get(&support.node_id).unwrap().bottom <= next.top, "next event cannot overlap preceding supporting content");
                            }
                        }
                    }
                    let mut different = plan.clone();
                    different.timeline_owners.clear();
                    assert!(!plan.geometry_key().matches(&different), "paint ownership is part of the prepared scene identity");
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn rich_timeline_code_growth_retains_dates_support_ownership_and_exact_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/119-rich-timelines.md");
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let initial =
                build_measured_adaptive_plan(&projection, 1400., 1000., None, false, &fonts);
            let code = projection
                .segments()
                .iter()
                .find(|s| matches!(projection.block(s.node_id), Some(BlockNode::CodeBlock(_))))
                .unwrap()
                .node_id;
            document
                .apply(EditCommand::ReplaceText {
                    node_id: code,
                    range: 0..0,
                    text: "# Retained event evidence\n".repeat(60),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let edited = TextProjection::from_snapshot(&document.snapshot());
            let locked = arrangement::build_edit_locked_adaptive_plan(
                &edited,
                1400.,
                1000.,
                Some(&initial),
                false,
                &fonts,
                Some(code),
            );
            assert_eq!(locked.label_rows, initial.label_rows);
            assert_eq!(
                locked.timeline_owners[&code],
                initial.timeline_owners[&code]
            );
            let lines =
                build_measured_visual_lines(&edited, &HashMap::new(), 1400., &locked, Some(&fonts));
            let segment = edited.segment_for_node(code).unwrap();
            assert_eq!(
                lines
                    .iter()
                    .filter(|l| segment.projection_range().contains(&l.projected_start()))
                    .map(|l| &edited.text()[l.projected_range()])
                    .collect::<String>(),
                edited.text()[segment.projection_range()].replace('\n', "")
            );
            assert_eq!(
                lines
                    .iter()
                    .filter(|line| line.code_line.is_some()
                        && segment.projection_range().contains(&line.projected_start()))
                    .count(),
                61
            );
            let components =
                component_geometry(&edited, &lines, 1400., 1., &visual_line_paint_order(&lines));
            let owner = locked.timeline_owners[&code];
            let header = components.get(&owner).unwrap();
            let code_bounds = components.get(&code).unwrap();
            assert!(
                code_bounds.bottom - header.top > 1000.,
                "exercise support well beyond a viewport"
            );
            let next = locked.label_rows[&owner].timeline().unwrap().next.unwrap();
            let (_, rail) = timeline_marks(
                header,
                components.get(&next),
                false,
                point(px(0.), px(0.)),
                1400.,
                1.,
            );
            assert!(f32::from(rail.unwrap().bottom()) > code_bounds.bottom);
            let narrow =
                build_measured_adaptive_plan(&edited, 230., 300., Some(&locked), false, &fonts);
            assert_eq!(narrow.timeline_owners[&code], owner);
            assert!(narrow.label_rows[&owner].stacked());
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn timeline_edit_keeps_columns_until_blur_and_undo_is_exact(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let mut document = Document::from_markdown(DATES).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let initial =
                build_measured_adaptive_plan(&projection, 1200., 900., None, false, &fonts);
            let node = projection
                .segments()
                .iter()
                .find(|s| projection.text()[s.projection_range()].starts_with("2023:"))
                .unwrap()
                .node_id;
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: 6..6,
                    text: "Additional evidence remains visible. ".repeat(30),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let locked = arrangement::build_edit_locked_adaptive_plan(
                &projection,
                1200.,
                900.,
                Some(&initial),
                false,
                &fonts,
                Some(node),
            );
            assert_eq!(locked.label_rows, initial.label_rows);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1200.,
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
                projection.text()[segment.projection_range()]
            );
            let released = build_measured_adaptive_plan(
                &projection,
                1200.,
                900.,
                Some(&locked),
                false,
                &fonts,
            );
            assert_eq!(released.label_rows.len(), 4);
            assert!(
                released
                    .label_rows
                    .values()
                    .all(|row| row.timeline().unwrap().slot.is_none())
            );
            let vertical = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1200.,
                &released,
                Some(&fonts),
            );
            for line in vertical.iter().filter(|line| line.label_row.is_some()) {
                assert!(
                    line.inset + line.label_row.unwrap().1 <= 1200. * line.width_fraction + 0.5,
                    "timeline text width must stay inside its published outer span"
                );
            }
            // Reversing chronology is authored order, not permission to sort.
            let descending =
                Document::from_markdown("- 2026: Current event\n- 2024: Earlier event\n").unwrap();
            let descending_projection = TextProjection::from_snapshot(&descending.snapshot());
            let plan = build_measured_adaptive_plan(
                &descending_projection,
                900.,
                900.,
                None,
                false,
                &fonts,
            );
            let first = descending_projection.segments()[0].node_id;
            assert_eq!(
                plan.label_rows[&first].timeline().unwrap().next,
                Some(descending_projection.segments()[1].node_id)
            );
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), DATES);
        });
    }

    #[gpui::test]
    fn timeline_measures_horizontal_vertical_and_narrow_stacks(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown(DATES).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            for (width, horizontal, stacked) in [
                (1200., true, true),
                (560., false, false),
                (230., false, true),
            ] {
                let plan =
                    build_measured_adaptive_plan(&projection, width, 900., None, false, &fonts);
                assert_eq!(plan.label_rows.len(), 4, "width {width}");
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                let components = component_geometry(
                    &projection,
                    &lines,
                    width,
                    1.,
                    &visual_line_paint_order(&lines),
                );
                let following = projection
                    .segments()
                    .iter()
                    .find(|segment| &projection.text()[segment.projection_range()] == "Afterwards")
                    .unwrap();
                let events_bottom = plan
                    .label_rows
                    .keys()
                    .map(|node| components.get(node).unwrap().bottom)
                    .fold(0., f32::max);
                assert_eq!(
                    components.get(&following.node_id).unwrap().top - events_bottom,
                    64.,
                    "internal event gaps must not accumulate outside the timeline"
                );
                let mut events = plan
                    .label_rows
                    .keys()
                    .map(|node| {
                        (
                            projection
                                .segment_for_node(*node)
                                .unwrap()
                                .projection_start(),
                            *node,
                        )
                    })
                    .collect::<Vec<_>>();
                events.sort_unstable();
                for (index, (_, node)) in events.iter().enumerate() {
                    let row = plan.label_rows[node];
                    let timeline = row.timeline().unwrap();
                    assert_eq!(timeline.slot.is_some(), horizontal, "width {width}");
                    assert_eq!(timeline.stacked, stacked, "width {width}");
                    assert_eq!(timeline.next, events.get(index + 1).map(|(_, node)| *node));
                    let segment = projection.segment_for_node(*node).unwrap();
                    let own = lines
                        .iter()
                        .filter(|l| segment.projection_range().contains(&l.projected_start()))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        own.iter()
                            .map(|l| &projection.text()[l.projected_range()])
                            .collect::<String>(),
                        projection.text()[segment.projection_range()]
                    );
                    assert!(own.iter().all(|line| {
                        fonts
                            .line_width(&projection, line.projected_range(), line.style.font_size)
                            .unwrap()
                            <= line.label_row.unwrap().1 + 0.5
                    }));
                    let label = own
                        .iter()
                        .find(|l| !l.label_row.unwrap().0.is_body())
                        .unwrap();
                    let body = own
                        .iter()
                        .find(|l| l.label_row.unwrap().0.is_body())
                        .unwrap();
                    assert_eq!(
                        body.y,
                        label.y
                            + if stacked {
                                label.style.line_height + 8.
                            } else {
                                0.
                            }
                    );
                    let component = components.get(node).unwrap();
                    let next = timeline.next.and_then(|node| components.get(&node));
                    let (dot, rail) = timeline_marks(
                        component,
                        next,
                        horizontal,
                        point(px(0.), px(0.)),
                        width,
                        1.,
                    );
                    assert_eq!(dot.size, size(px(8.), px(8.)));
                    if let Some(next) = next {
                        let (next_dot, _) = timeline_marks(
                            next,
                            None,
                            horizontal,
                            point(px(0.), px(0.)),
                            width,
                            1.,
                        );
                        if horizontal {
                            assert_eq!(dot.center().y, next_dot.center().y);
                            assert_eq!(rail.unwrap().right(), next_dot.center().x);
                        } else {
                            assert_eq!(dot.center().x, next_dot.center().x);
                            assert_eq!(rail.unwrap().bottom(), next_dot.center().y);
                        }
                    }
                }
                for zoom in [0.75, 2.] {
                    let mut scaled = lines.clone();
                    scale_visual_lines(&mut scaled, zoom);
                    let geometry = component_geometry(
                        &projection,
                        &scaled,
                        width * zoom,
                        zoom,
                        &visual_line_paint_order(&scaled),
                    );
                    let node = events[0].1;
                    let (dot, _) = timeline_marks(
                        geometry.get(&node).unwrap(),
                        None,
                        horizontal,
                        point(px(0.), px(0.)),
                        width * zoom,
                        zoom,
                    );
                    assert_eq!(dot.size, size(px(8. * zoom), px(8. * zoom)));
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), DATES);
        });
    }
    const SOURCE: &str = "# Terms\n\n- API: A programmatic interface for accessing a service.\n- Command line: A text interface for working with local tools.\n\n## Afterwards\n\nThe source stays intact.\n";

    #[gpui::test]
    fn aligned_labels_share_columns_and_keep_every_source_byte(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown(SOURCE).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 760., 900., None, false, &fonts);
            assert_eq!(plan.label_rows.len(), 2);
            let widths = plan
                .label_rows
                .values()
                .map(|c| c.label_width)
                .collect::<Vec<_>>();
            assert_eq!(widths[0], widths[1]);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                760.,
                &plan,
                Some(&fonts),
            );
            for (&node, columns) in &plan.label_rows {
                let segment = projection.segment_for_node(node).unwrap();
                let own = lines
                    .iter()
                    .filter(|line| {
                        projection
                            .segment_for_range(&line.projected_range())
                            .unwrap()
                            .node_id
                            == node
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    own.iter()
                        .map(|line| &projection.text()[line.projected_range()])
                        .collect::<String>(),
                    projection.text()[segment.projection_range()]
                );
                let label = own
                    .iter()
                    .find(|line| line.label_row.unwrap().0 == Part::Label)
                    .unwrap();
                let body = own
                    .iter()
                    .find(|line| line.label_row.unwrap().0 == Part::Body)
                    .unwrap();
                assert_eq!(label.y, body.y);
                assert_eq!(body.inset - label.inset, columns.label_width + 24.);
                assert!(own.iter().all(|line| {
                    fonts
                        .line_width(&projection, line.projected_range(), line.style.font_size)
                        .unwrap()
                        <= line.label_row.unwrap().1 + 0.5
                }));
            }
            let narrow =
                build_measured_adaptive_plan(&projection, 270., 900., Some(&plan), false, &fonts);
            assert!(narrow.label_rows.is_empty());
            let stack = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                270.,
                &narrow,
                Some(&fonts),
            );
            assert!(stack.iter().all(|line| line.label_row.is_none()));
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }

    #[gpui::test]
    fn label_columns_stay_put_while_the_description_is_edited(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let mut document = Document::from_markdown(SOURCE).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 760., 900., None, false, &fonts);
            let segment = projection
                .segments()
                .iter()
                .find(|s| projection.text()[s.projection_range()].starts_with("API:"))
                .unwrap();
            let node = segment.node_id;
            let columns = plan.label_rows[&node];
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: 5..5,
                    text: "Additional explanation. ".repeat(40),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let locked = arrangement::build_edit_locked_adaptive_plan(
                &projection,
                760.,
                900.,
                Some(&plan),
                false,
                &fonts,
                Some(node),
            );
            assert_eq!(locked.label_rows[&node], columns);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                760.,
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
                projection.text()[segment.projection_range()]
            );
            let released =
                build_measured_adaptive_plan(&projection, 760., 900., Some(&locked), false, &fonts);
            assert!(
                released.label_rows.is_empty(),
                "long descriptions must leave the compact aligned template after blur"
            );
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }

    #[gpui::test]
    fn label_rows_do_not_reinterpret_order_tasks_or_unlabelled_prose(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            for source in [
                "1. First: Do this.\n2. Second: Do that.",
                "- [x] Done: Known result.\n- [ ] Next: Pending result.",
                "- Heading only\n- Another plain point",
                "- One: Description.\n  - Child\n- Two: Description.",
            ] {
                let document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let plan =
                    build_measured_adaptive_plan(&projection, 1100., 900., None, false, &fonts);
                assert!(plan.label_rows.is_empty(), "{source}");
            }
        });
    }
}
