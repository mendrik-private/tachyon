//! Native measurement and rendering of source-contiguous reading bands.
use super::*;
use crate::adaptive::prose::{Flow, Line};

const MAX_ROOTS: usize = 16;
const MAX_BYTES: usize = 64 * 1024;

// Typography is not a relationship: ordinary reference prose may form a
// reading band too. Specialized paragraph roles remain atomic boundaries.
fn ordinary_prose(segment: &crate::ProjectionSegment) -> bool {
    let context = &segment.context;
    segment.node_id == segment.top_level_node_id
        && !context.metadata
        && context.figure_text.is_none()
        && context.bibliography.is_none()
        && context.resource_title_end.is_none()
        && context.metric.is_none()
        && context.color_role.is_none()
        && context.badge.is_none()
        && context.margin_note_anchor.is_none()
        && context.definition.is_none()
        && context.footnote.is_none()
        && context.list_depth == 0
        && context.quote_depth == 0
        && context.table_cell.is_none()
        && context.alert.is_none()
}

pub(super) fn measure(
    plan: &mut AdaptivePlan,
    projection: &TextProjection,
    width: f32,
    viewport: f32,
    previous: Option<&AdaptivePlan>,
    keep_arrangements: bool,
    fonts: &FontMeasurement,
) {
    // A deliberate resize can make a held reading band unusable. Do not keep
    // columns on a narrow or short surface merely because its caret is active.
    if width < 900. || viewport < 480. {
        return;
    }
    if let Some(old) = previous {
        let mut seen = HashSet::new();
        for flow in old.prose_flows.values().filter(|f| seen.insert(f.group)) {
            let held = keep_arrangements
                || plan
                    .editing_node
                    .is_some_and(|node| flow.sources.iter().any(|(id, _)| *id == node));
            let same_environment = old.canvas.to_bits() == width.to_bits()
                && old.measured_rows.viewport.to_bits() == viewport.to_bits()
                && old
                    .measurement_identity
                    .as_ref()
                    .is_some_and(|identity| Arc::ptr_eq(identity, &fonts.identity));
            let first = plan.root_ordinal(flow.group);
            let contiguous = first.is_some_and(|first| {
                flow.sources
                    .iter()
                    .enumerate()
                    .all(|(offset, (id, _))| plan.root_ordinal(*id) == Some(first + offset))
            });
            let valid = contiguous
                && !flow.sources.iter().any(|(id, _)| {
                    plan.figure_flows.contains_key(id)
                        || plan.inline_lists.contains_key(id)
                        // An unfocused cached reading band must not remove a
                        // newly measured row's ownership (for example when a
                        // deferred opening pair becomes available after blur).
                        // Explicit edit locks still keep the active flow stable.
                        || (!held && plan.slots.contains_key(id))
                })
                && flow.canvas <= width + 0.5
                && flow
                    .sources
                    .iter()
                    .enumerate()
                    .all(|(ordinal, (id, text))| {
                        projection.segment_for_node(*id).is_some_and(ordinary_prose)
                            && old.reading_modes.get(id) == plan.reading_modes.get(id)
                            && matches!(projection.block(*id), Some(BlockNode::Paragraph(_)))
                            && (held
                                || (same_environment
                                    && !flow.needs_balance
                                    && flow.revisions[ordinal] == projection.node_revision(*id)
                                    && projection
                                        .block(*id)
                                        .and_then(BlockNode::text)
                                        .is_some_and(|t| t.as_string().as_str() == text.as_ref())))
                    });
            if valid && (held || same_environment) {
                let mut retained = flow.clone();
                if held && !same_environment {
                    Arc::make_mut(&mut retained).needs_balance = true;
                }
                for (node, _) in &flow.sources {
                    let segment = projection.segment_for_node(*node).unwrap();
                    if let Some(next) = retained.rebased(
                        *node,
                        &projection.text()[segment.projection_range()],
                        projection.node_revision(*node),
                    ) {
                        retained = Arc::new(next);
                    }
                }
                for (node, _) in &flow.sources {
                    plan.slots.remove(node);
                    plan.prose_flows.insert(*node, retained.clone());
                }
            }
        }
    }
    if keep_arrangements {
        return;
    }
    let height_limit = (viewport * 0.65).min(560.);
    let roots = projection.roots().collect::<Vec<_>>();
    let eligible = |root: &BlockNode, plan: &AdaptivePlan| {
        let BlockNode::Paragraph(p) = root else {
            return false;
        };
        let Some(segment) = projection.segment_for_node(p.id) else {
            return false;
        };
        ordinary_prose(segment)
            && segment.node_range == (0..p.content.len())
            && plan.measures_root(p.id)
            && plan.has_measured_geometry(p.id)
            && !plan.prose_flows.contains_key(&p.id)
            && !plan.figure_flows.contains_key(&p.id)
            && !plan.inline_lists.contains_key(&p.id)
            && !plan.slots.contains_key(&p.id)
            && plan.lead != Some(p.id)
            && !plan.resources.contains_key(&p.id)
            && !plan.editorials.contains_key(&p.id)
            && plan.editing_node != Some(p.id)
            && p.content.len() <= MAX_BYTES
            && !p.content.runs().iter().any(|run| {
                run.styles.iter().any(|style| {
                    matches!(
                        style,
                        document_core::InlineStyle::Image { .. }
                            | document_core::InlineStyle::PreservedHtml(_)
                            | document_core::InlineStyle::Math { .. }
                    )
                })
            })
            && !projection.text()[segment.projection_range()].contains(['\n', '\r'])
            && !measurement::contains_strong_rtl(&projection.text()[segment.projection_range()])
    };
    // Stay within published planning windows so deferred chapter geometry
    // never depends on an unmeasured neighbour outside its request.
    for window in plan.windows.clone() {
        let mut start = window.start;
        while start < window.end {
            if !eligible(roots[start], plan) {
                start += 1;
                continue;
            }
            let mut end = start;
            let mut bytes = 0;
            let narrative = plan.reading_modes[&roots[start].id()];
            while end < window.end
                && end - start < MAX_ROOTS
                && eligible(roots[end], plan)
                && plan.reading_modes.get(&roots[end].id()) == Some(&narrative)
            {
                bytes += roots[end].text().unwrap().len();
                if bytes > MAX_BYTES {
                    break;
                }
                end += 1;
            }
            if end == start {
                start += 1;
                continue;
            }
            let measure = plan.prose_measures.for_role(narrative, false);
            let minimum = measure * 40. / DocumentStyle::PROSE_CHARACTERS;
            let maximum =
                measure * DocumentStyle::MAX_PROSE_CHARACTERS / DocumentStyle::PROSE_CHARACTERS;
            let columns = if width >= 1440. && (width - 2. * LAYOUT_GAP) / 3. >= minimum {
                3
            } else {
                2
            };
            let column =
                ((width - (columns - 1) as f32 * LAYOUT_GAP) / columns as f32).min(maximum);
            if column < minimum {
                start = end;
                continue;
            }
            let canvas = columns as f32 * column + (columns - 1) as f32 * LAYOUT_GAP;
            let flow = candidate(
                projection,
                &roots[start..end],
                column,
                canvas,
                columns,
                height_limit,
                fonts,
            )
            .or_else(|| {
                // A short passage may support two useful columns but not
                // three. Re-measure at that width rather than leaving an
                // empty third track or falling straight back to a narrow stack.
                if columns != 3 {
                    return None;
                }
                let column = ((width - LAYOUT_GAP) / 2.).min(maximum);
                candidate(
                    projection,
                    &roots[start..end],
                    column,
                    2. * column + LAYOUT_GAP,
                    2,
                    height_limit,
                    fonts,
                )
            });
            if let Some(flow) = flow {
                let flow = Arc::new(flow);
                for (id, _) in &flow.sources {
                    plan.prose_flows.insert(*id, flow.clone());
                }
            }
            start = end;
        }
    }
}

pub(super) fn rebase_after_edit(
    plan: &mut AdaptivePlan,
    projection: &TextProjection,
    node: NodeId,
) {
    let Some(flow) = plan.prose_flows.get(&node) else {
        return;
    };
    let Some(segment) = projection.segment_for_node(node) else {
        return;
    };
    let Some(next) = flow.rebased(
        node,
        &projection.text()[segment.projection_range()],
        projection.node_revision(node),
    ) else {
        return;
    };
    let next = Arc::new(next);
    for (id, _) in &next.sources {
        plan.prose_flows.insert(*id, next.clone());
    }
}

fn candidate(
    projection: &TextProjection,
    roots: &[&BlockNode],
    column: f32,
    canvas: f32,
    columns: usize,
    height_limit: f32,
    fonts: &FontMeasurement,
) -> Option<Flow> {
    let (original, fit) = measured_candidate(
        projection,
        roots,
        column,
        canvas,
        columns,
        height_limit,
        fonts,
    )?;
    if fit.0 <= 1.5 {
        return Some(original);
    }
    // A five-line paragraph cannot split with three lines on either side.
    // Try a few slightly narrower, still readable measures before accepting
    // a large hole. Never trade that hole for a taller document or more bands.
    let narrative = projection
        .segment_for_node(roots.first()?.id())?
        .context
        .narrative;
    let minimum =
        fonts.prose_measures().for_role(narrative, false) * 40. / DocumentStyle::PROSE_CHARACTERS;
    for scale in [0.96, 0.92, 0.88] {
        let narrower = column * scale;
        if narrower < minimum {
            break;
        }
        if let Some((alternative, next)) = measured_candidate(
            projection,
            roots,
            narrower,
            narrower * columns as f32 + (columns - 1) as f32 * LAYOUT_GAP,
            columns,
            height_limit,
            fonts,
        ) && next.0 <= 1.5
            && next.1 <= fit.1 + 0.5
            && alternative.starts.len() <= original.starts.len()
        {
            return Some(alternative);
        }
    }
    Some(original)
}

/// Worst within-band imbalance and total occupied band height, including gaps.
fn band_fit(lines: &[Line], starts: &[usize], columns: usize) -> (f32, f32) {
    let heights = starts
        .iter()
        .enumerate()
        .map(|(i, &start)| {
            lines[start..starts.get(i + 1).copied().unwrap_or(lines.len())]
                .iter()
                .map(|line| line.height + line.gap)
                .sum::<f32>()
                - lines[start].gap
        })
        .collect::<Vec<_>>();
    heights
        .chunks(columns)
        .fold((1., 0.), |(ratio, total), band| {
            let shortest = band.iter().copied().fold(f32::INFINITY, f32::min);
            let tallest = band.iter().copied().fold(0., f32::max);
            (ratio.max(tallest / shortest.max(1.)), total + tallest)
        })
}

fn measured_candidate(
    projection: &TextProjection,
    roots: &[&BlockNode],
    column: f32,
    canvas: f32,
    columns: usize,
    height_limit: f32,
    fonts: &FontMeasurement,
) -> Option<(Flow, (f32, f32))> {
    let mut lines = Vec::new();
    let mut anchors = Vec::new();
    let mut sources = Vec::new();
    for (paragraph, root) in roots.iter().enumerate() {
        let segment = projection.segment_for_node(root.id())?;
        let measured = build_visual_lines_for_segment(
            projection,
            segment,
            &HashMap::new(),
            column + 8.,
            &[],
            Some(fonts),
            None,
        );
        for (index, line) in measured.iter().enumerate() {
            if fonts.line_width(projection, line.projected_range(), line.style.font_size)?
                > column + 0.5
            {
                return None;
            }
            lines.push(Line {
                height: line.style.line_height,
                gap: if paragraph > 0 && index == 0 { 24. } else { 0. },
                paragraph,
            });
            anchors.push((
                root.id(),
                line.projected_start() - segment.projection_start(),
            ));
        }
        sources.push((
            root.id(),
            Arc::from(&projection.text()[segment.projection_range()]),
        ));
    }
    // Keep short reference instructions in normal flow. A substantial passage
    // needs multiple paragraphs, six shaped lines per column and three lines
    // per paragraph on average. Many one-line instructions are not sustained
    // prose merely because their combined line count fills a band.
    if !projection
        .segment_for_node(roots.first()?.id())?
        .context
        .narrative
        && (roots.len() < 2 || lines.len() < columns * 6 || lines.len() < roots.len() * 3)
    {
        return None;
    }
    let starts = crate::adaptive::prose::breaks(&lines, height_limit, columns)?;
    let fit = band_fit(&lines, &starts, columns);
    Some((
        Flow {
            group: roots.first()?.id(),
            canvas,
            columns,
            needs_balance: false,
            revisions: roots
                .iter()
                .map(|root| projection.node_revision(root.id()))
                .collect(),
            sources,
            starts: starts.into_iter().map(|i| anchors[i]).collect(),
        },
        fit,
    ))
}

pub(super) fn build(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    flow: &Flow,
    width: f32,
    fonts: Option<&FontMeasurement>,
) -> Vec<VisualLineSpec> {
    let text = &projection.text()[segment.projection_range()];
    let mut output = Vec::new();
    for (range, column) in flow.fragments(segment.node_id, text) {
        let slot = LayoutSlot {
            align_components: false,
            group: flow.group,
            item: column,
            row: column / flow.columns,
            columns: flow.columns,
            cards: false,
            card_accent: crate::adaptive::CardAccent::None,
            track_start: (column % flow.columns * (12 / flow.columns)) as u8,
            span: (12 / flow.columns) as u8,
            fixed_canvas: Some(flow.canvas),
        };
        let mut fragment = segment.clone();
        fragment.node_range =
            segment.node_range.start + range.start..segment.node_range.start + range.end;
        fragment.set_projection_range(
            segment.projection_start() + range.start..segment.projection_start() + range.end,
        );
        let mut lines = build_visual_lines_for_segment(
            projection,
            &fragment,
            &HashMap::new(),
            slot.width(width) + 8.,
            &[],
            fonts,
            None,
        );
        for line in &mut lines {
            line.set_slot(Some(slot));
            line.x_fraction = slot.left(width) / width;
            line.width_fraction = slot.width(width) / width;
            line.style.space_above = 0.;
            line.style.space_below = 0.;
        }
        if let Some(first) = lines.first_mut() {
            let opens_column = flow
                .starts
                .get(column)
                .is_some_and(|&(id, offset)| id == segment.node_id && offset == 0);
            first.gap_before = if range.start == 0 && !opens_column {
                LAYOUT_GAP
            } else {
                0.
            };
        }
        output.extend(lines);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    const SENTENCE: &str = "A reading surface should give an argument room to develop, while keeping each sentence connected to the one before it. ";

    #[gpui::test]
    fn reference_paragraphs_after_math_use_a_source_order_reading_band(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            // The test platform checks source/geometry invariants; native
            // captures independently qualify actual fonts and column balance.
            let source =
                include_str!("../../../../performance/layout-fixtures/27-accessible-document.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let width = 1314.;
            let plan = arrangement::build_measured_adaptive_plan(
                &projection,
                width,
                1166.,
                None,
                false,
                &fonts,
            );
            let paragraph = projection
                .segments()
                .iter()
                .find(|s| {
                    projection.text()[s.projection_range()].starts_with("This paragraph keeps")
                })
                .unwrap();
            assert!(
                !paragraph.context.narrative,
                "keep the reference typography"
            );
            let flow = plan
                .prose_flows
                .get(&paragraph.node_id)
                .expect("ordinary reference prose should also use the available canvas");
            assert_eq!(flow.sources.len(), 4);
            assert_eq!(flow.columns, 2);
            assert!(flow.canvas >= width * 0.9);
            let lines = arrangement::build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                width,
                &plan,
                Some(&fonts),
            );
            for segment in projection
                .segments()
                .iter()
                .filter(|segment| flow.sources.iter().any(|(id, _)| *id == segment.node_id))
            {
                let rendered = lines
                    .iter()
                    .filter(|line| segment.projection_range().contains(&line.projected_start()))
                    .map(|line| &projection.text()[line.projected_range()])
                    .collect::<String>();
                assert_eq!(rendered, projection.text()[segment.projection_range()]);
            }
            for line in lines
                .iter()
                .filter(|line| line.slot.is_some_and(|slot| slot.group == flow.group))
            {
                assert_eq!(line.style.font_size, DocumentStyle::REFERENCE_SIZE);
                let slot = line.slot.unwrap();
                assert!(slot.left(width) + slot.width(width) <= width + 0.5);
            }
            let scope = diagnostics::MeasurementScope::new();
            let warm = arrangement::build_measured_adaptive_plan(
                &projection,
                width,
                1166.,
                Some(&plan),
                false,
                &fonts,
            );
            assert_eq!(warm.prose_flows, plan.prose_flows);
            assert_eq!(scope.take_stage().shaping_calls, 0);
            for (canvas, viewport) in [(650., 1166.), (width, 304.)] {
                let narrow = arrangement::build_edit_locked_adaptive_plan(
                    &projection,
                    canvas,
                    viewport,
                    Some(&plan),
                    false,
                    &fonts,
                    Some(paragraph.node_id),
                );
                assert!(narrow.prose_flows.is_empty());
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }
    fn source() -> String {
        format!(
            "# Reading together\n\nA short introduction.\n\n## The long view\n\n{}\n\n{}\n\n## A new subject\n\nThe next section stays outside the flow.\n",
            SENTENCE.repeat(2),
            SENTENCE.repeat(23)
        )
    }

    #[test]
    fn native_line_counts_explain_the_hole_without_weakening_widows() {
        let measured = |counts: [usize; 3]| {
            counts
                .into_iter()
                .enumerate()
                .flat_map(|(paragraph, count)| {
                    (0..count).map(move |index| Line {
                        height: 28.,
                        gap: if paragraph > 0 && index == 0 { 24. } else { 0. },
                        paragraph,
                    })
                })
                .collect::<Vec<_>>()
        };
        let original = measured([4, 5, 3]);
        let starts = crate::adaptive::prose::breaks(&original, 560., 2).unwrap();
        assert_eq!(starts, [0, 4]);
        assert_eq!(band_fit(&original, &starts, 2), (248. / 112., 248.));
        let narrower = measured([4, 6, 3]);
        let starts = crate::adaptive::prose::breaks(&narrower, 560., 2).unwrap();
        assert_eq!(
            starts,
            [0, 7],
            "three lines of the middle paragraph on each side"
        );
        assert_eq!(band_fit(&narrower, &starts, 2), (220. / 192., 220.));
    }

    #[test]
    fn reading_fit_counts_each_band_and_omits_opening_paragraph_gaps() {
        for columns in [2, 3] {
            let lines = (0..columns * 8)
                .map(|paragraph| Line {
                    height: 28.,
                    gap: 24.,
                    paragraph,
                })
                .collect::<Vec<_>>();
            let starts = (0..lines.len()).step_by(4).collect::<Vec<_>>();
            assert_eq!(band_fit(&lines, &starts, columns), (1., 368.));
        }
    }

    #[gpui::test]
    fn real_argument_preserves_source_in_reading_columns(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            // GPUI's test platform uses NoopTextSystem. Native fixture113
            // glyph checks, not this mock, qualify the actual font balance.
            let source =
                include_str!("../../../../performance/layout-fixtures/113-reading-balance.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let width = 1314.;
            let plan = arrangement::build_measured_adaptive_plan(
                &projection,
                width,
                1700.,
                None,
                false,
                &fonts,
            );
            let node = projection
                .segments()
                .iter()
                .find(|s| {
                    projection.text()[s.projection_range()]
                        .starts_with("The three-crate foundation")
                })
                .unwrap()
                .node_id;
            let flow = plan
                .prose_flows
                .get(&node)
                .expect("the sustained argument should form a reading band");
            assert_eq!(flow.sources.len(), 3);
            let lines = arrangement::build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                width,
                &plan,
                Some(&fonts),
            );
            let heights = (0..flow.columns)
                .map(|column| {
                    let members = lines
                        .iter()
                        .filter(|line| {
                            line.slot
                                .is_some_and(|s| s.group == flow.group && s.item == column)
                        })
                        .collect::<Vec<_>>();
                    let top = members
                        .iter()
                        .map(|line| line.y)
                        .fold(f32::INFINITY, f32::min);
                    let bottom = members
                        .iter()
                        .map(|line| line.y + line.style.line_height)
                        .fold(0., f32::max);
                    bottom - top
                })
                .collect::<Vec<_>>();
            let shortest = heights.iter().copied().fold(f32::INFINITY, f32::min);
            let tallest = heights.iter().copied().fold(0., f32::max);
            assert!(
                tallest <= shortest * 1.5,
                "a legal split should avoid the accidental column hole: {heights:?}, starts={:?}",
                flow.starts
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn broad_reading_bands_use_three_columns_without_losing_source(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown(source().as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let width = 1600.;
            let plan = arrangement::build_measured_adaptive_plan(
                &projection,
                width,
                900.,
                None,
                false,
                &fonts,
            );
            let flow = plan.prose_flows.values().next().expect("reading band");
            assert!(
                flow.canvas >= width * 0.95,
                "wide band should use the canvas"
            );
            let lines = arrangement::build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                width,
                &plan,
                Some(&fonts),
            );
            let first_band = lines
                .iter()
                .filter(|line| {
                    line.slot
                        .is_some_and(|slot| slot.group == flow.group && slot.item < 3)
                })
                .collect::<Vec<_>>();
            for column in 0..3 {
                let line = first_band
                    .iter()
                    .find(|line| line.slot.unwrap().item == column)
                    .unwrap();
                let slot = line.slot.unwrap();
                assert_eq!(slot.columns, 3);
                assert_eq!(line.y, first_band[0].y);
                assert!(slot.left(width) + slot.width(width) <= width + 0.01);
                if column > 0 {
                    let previous = first_band
                        .iter()
                        .find(|line| line.slot.unwrap().item == column - 1)
                        .unwrap()
                        .slot
                        .unwrap();
                    assert!(
                        (slot.left(width)
                            - previous.left(width)
                            - previous.width(width)
                            - LAYOUT_GAP)
                            .abs()
                            < 0.01
                    );
                }
            }
            for segment in projection.segments() {
                let rendered = lines
                    .iter()
                    .filter(|line| segment.projection_range().contains(&line.projected_start()))
                    .map(|line| &projection.text()[line.projected_range()])
                    .collect::<String>();
                assert_eq!(rendered, projection.text()[segment.projection_range()]);
            }
        });
    }

    #[gpui::test]
    fn measured_prose_flows_across_paragraphs_and_preserves_every_byte(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = source();
            let document = Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let plan = arrangement::build_measured_adaptive_plan(
                &projection,
                1280.,
                900.,
                None,
                false,
                &fonts,
            );
            assert_eq!(
                plan.prose_flows.len(),
                2,
                "both paragraphs should share one source-contiguous flow"
            );
            let flow = plan.prose_flows.values().next().unwrap();
            assert!(flow.starts.len() >= 2);
            let lines = arrangement::build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1280.,
                &plan,
                Some(&fonts),
            );
            for segment in projection.segments() {
                let text = lines
                    .iter()
                    .filter(|l| segment.projection_range().contains(&l.projected_start()))
                    .map(|l| &projection.text()[l.projected_range()])
                    .collect::<String>();
                assert_eq!(text, projection.text()[segment.projection_range()]);
            }
            assert!(
                lines
                    .iter()
                    .any(|l| l.slot.is_some_and(|s| s.item % 2 == 1))
            );
            assert!(
                lines
                    .windows(2)
                    .any(|p| p[0].projected_end() == p[1].projected_start()
                        && p[0]
                            .slot
                            .zip(p[1].slot)
                            .is_some_and(|(a, b)| a.item != b.item)),
                "a real paragraph must continue into another column"
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            let mut previous_bottom = None;
            for band in 0..flow.starts.len() / 2 {
                let columns = (0..2)
                    .map(|column| {
                        lines
                            .iter()
                            .filter(|l| {
                                l.slot.is_some_and(|s| {
                                    s.group == flow.group && s.item == 2 * band + column
                                })
                            })
                            .collect::<Vec<_>>()
                    })
                    .collect::<Vec<_>>();
                assert_eq!(columns[0][0].y, columns[1][0].y);
                let left = columns[0][0].slot.unwrap();
                let right = columns[1][0].slot.unwrap();
                assert!(
                    (right.left(1280.) - left.left(1280.) - left.width(1280.) - LAYOUT_GAP).abs()
                        < 0.01
                );
                let top = columns[0][0].y;
                let bottom = columns
                    .iter()
                    .map(|column| {
                        let last = column.last().unwrap();
                        last.y + last.style.line_height
                    })
                    .fold(top, f32::max);
                assert!(bottom - top <= 560.5);
                if let Some(previous) = previous_bottom {
                    assert_eq!(top - previous, LAYOUT_GAP);
                }
                previous_bottom = Some(bottom);
            }
            let scope = diagnostics::MeasurementScope::new();
            let warm = arrangement::build_measured_adaptive_plan(
                &projection,
                1280.,
                900.,
                Some(&plan),
                false,
                &fonts,
            );
            assert_eq!(warm.prose_flows, plan.prose_flows);
            assert_eq!(
                scope.take_stage().shaping_calls,
                0,
                "unchanged flow reuses native measurements"
            );
        });
    }

    #[gpui::test]
    fn focused_prose_preserves_boundaries_and_localized_edit_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        for (canvas, reference) in [(1280., false), (1600., false), (1280., true), (1600., true)] {
            cx.update(|cx| {
                let source = if reference {
                    include_str!(
                        "../../../../performance/layout-fixtures/27-accessible-document.md"
                    )
                    .to_owned()
                } else {
                    source()
                };
                let mut document = Document::from_markdown(source.as_str()).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Tachyon".into(),
                    1.,
                );
                let plan = arrangement::build_measured_adaptive_plan(
                    &projection,
                    canvas,
                    900.,
                    None,
                    false,
                    &fonts,
                );
                let flow = plan.prose_flows.values().next().unwrap();
                let node = flow.sources[1].0;
                let mut lines = arrangement::build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    canvas,
                    &plan,
                    Some(&fonts),
                );
                let mut paint = visual_line_paint_order(&lines);
                let first_other = lines[0].clone();
                let old = flow.fragments(node, &flow.sources[1].1);
                let inserted = "東京 café. ".repeat(32);
                document
                    .apply(EditCommand::ReplaceText {
                        node_id: node,
                        range: 0..0,
                        text: inserted.clone(),
                        typing: true,
                        selection_after: None,
                    })
                    .unwrap();
                refresh_arranged_text_node_geometry(
                    &mut projection,
                    &mut lines,
                    &mut paint,
                    &plan,
                    TextRefreshRequest {
                        snapshot: &document.snapshot(),
                        node_id: node,
                        image_dimensions: &HashMap::new(),
                        layout_width: canvas,
                        zoom_factor: 1.,
                        measurement: Some(&fonts),
                    },
                )
                .expect("flow edits use the localized geometry path");
                assert_eq!(lines[0].projected_range(), first_other.projected_range());
                assert_eq!(lines[0].y, first_other.y);
                assert_eq!(paint, visual_line_paint_order(&lines));
                let text = projection.block(node).unwrap().text().unwrap().as_string();
                let new = flow.fragments(node, &text);
                assert_eq!(old.len(), new.len());
                for ((before, column), (after, retained)) in old.iter().zip(&new) {
                    assert_eq!(column, retained);
                    assert_eq!(
                        after.start,
                        if before.start == 0 {
                            0
                        } else {
                            before.start + inserted.len()
                        }
                    );
                    assert_eq!(after.end, before.end + inserted.len());
                }
                let held = arrangement::build_edit_locked_adaptive_plan(
                    &projection,
                    canvas,
                    900.,
                    Some(&plan),
                    false,
                    &fonts,
                    Some(node),
                );
                assert_eq!(held.prose_flows[&node].fragments(node, &text), new);
                let rebuilt = arrangement::build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    canvas,
                    &held,
                    Some(&fonts),
                );
                assert_eq!(lines.len(), rebuilt.len());
                for (a, b) in lines.iter().zip(&rebuilt) {
                    assert_eq!(a.projected_range(), b.projected_range());
                    assert_eq!(a.slot, b.slot);
                    assert!(
                        (a.y - b.y).abs() < 0.01,
                        "localized flow agrees with full placement"
                    );
                }
                assert!(held.prose_flows[&node].needs_balance);
                let released = arrangement::build_measured_adaptive_plan(
                    &projection,
                    canvas,
                    900.,
                    Some(&held),
                    false,
                    &fonts,
                );
                assert!(!released.prose_flows[&node].needs_balance);
                for (width, height) in [(500., 900.), (1280., 250.)] {
                    let resized = arrangement::build_edit_locked_adaptive_plan(
                        &projection,
                        width,
                        height,
                        Some(&held),
                        false,
                        &fonts,
                        Some(node),
                    );
                    assert!(resized.prose_flows.is_empty());
                }
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            });
        }
    }

    #[gpui::test]
    fn short_reference_instructions_and_hard_breaks_stay_stacked(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for body in [
                "Read the configuration first.\n\nSave the file when finished.".to_owned(),
                "After the images.\n\n".repeat(25),
                SENTENCE.repeat(24),
                format!(
                    "{}  \n{}\n\n{}  \n{}",
                    SENTENCE.repeat(3),
                    SENTENCE,
                    SENTENCE.repeat(3),
                    SENTENCE
                ),
            ] {
                let source = format!("# Reference\n\n$$x^2$$\n\n{body}\n");
                let document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Tachyon".into(),
                    1.,
                );
                let plan = arrangement::build_measured_adaptive_plan(
                    &projection,
                    1314.,
                    1166.,
                    None,
                    false,
                    &fonts,
                );
                assert!(
                    plan.prose_flows.is_empty(),
                    "not a sustained multi-paragraph passage"
                );
            }
        });
    }

    #[gpui::test]
    fn a_new_heading_breaks_a_retained_reading_flow(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let mut document = Document::from_markdown(source()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let plan = arrangement::build_measured_adaptive_plan(
                &projection,
                1280.,
                900.,
                None,
                false,
                &fonts,
            );
            let flow = plan.prose_flows.values().next().unwrap();
            let node = flow.sources[1].0;
            let mut heading = Document::from_markdown("## An intervening subject\n")
                .unwrap()
                .snapshot()
                .blocks()
                .iter()
                .next()
                .unwrap()
                .clone();
            if let BlockNode::Heading(heading) = Arc::make_mut(&mut heading) {
                heading.id = NodeId::new_unchecked(1000);
            }
            document
                .apply(EditCommand::InsertBlock {
                    index: plan.root_ordinal(node).unwrap(),
                    block: heading,
                })
                .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let held = arrangement::build_edit_locked_adaptive_plan(
                &projection,
                1280.,
                900.,
                Some(&plan),
                true,
                &fonts,
                Some(node),
            );
            assert!(
                held.prose_flows.is_empty(),
                "a flow cannot span an inserted heading"
            );
        });
    }

    #[gpui::test]
    fn prose_formatting_remeasures_even_when_plain_text_is_unchanged(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let mut document = Document::from_markdown(source()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let plan = arrangement::build_measured_adaptive_plan(
                &projection,
                1280.,
                900.,
                None,
                false,
                &fonts,
            );
            let flow = plan.prose_flows.values().next().unwrap();
            let node = flow.sources[1].0;
            document
                .apply(EditCommand::ToggleInline {
                    node_id: node,
                    range: 0..flow.sources[1].1.len(),
                    format: document_core::InlineFormat::Bold,
                })
                .unwrap();
            let after = TextProjection::from_snapshot(&document.snapshot());
            assert_eq!(after.text(), projection.text());
            let held = arrangement::build_edit_locked_adaptive_plan(
                &after,
                1280.,
                900.,
                Some(&plan),
                false,
                &fonts,
                Some(node),
            );
            assert!(held.prose_flows[&node].needs_balance);
            assert_eq!(held.prose_flows[&node].starts, flow.starts);
            let released = arrangement::build_measured_adaptive_plan(
                &after,
                1280.,
                900.,
                Some(&held),
                false,
                &fonts,
            );
            assert!(!released.prose_flows[&node].needs_balance);
            assert_eq!(
                released.prose_flows[&node].revisions[1],
                after.node_revision(node)
            );
            assert_ne!(released.prose_flows[&node].revisions, flow.revisions);
        });
    }

    #[gpui::test]
    fn narrow_and_short_surfaces_keep_prose_in_one_column(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown(source()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            for (width, height) in [(550., 900.), (899., 900.), (1280., 250.)] {
                let plan = arrangement::build_measured_adaptive_plan(
                    &projection,
                    width,
                    height,
                    None,
                    false,
                    &fonts,
                );
                assert!(plan.prose_flows.is_empty());
            }
        });
    }
}
