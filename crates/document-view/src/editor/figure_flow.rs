//! Measured, source-owned supporting figures with a full-measure continuation.
use super::*;
use crate::adaptive::prose::{FigureFlow, Flow, supporting_image as supporting};

const MAX_ROOTS: usize = 16;
const MAX_BYTES: usize = 64 * 1024;

fn slot(flow: &FigureFlow, item: usize) -> LayoutSlot {
    let (track_start, span) = match item {
        0 => (0, 4),
        1 => (4, 8),
        _ => (0, 12),
    };
    LayoutSlot {
        align_components: false,
        group: flow.text.group,
        item,
        row: item / 2,
        columns: 2,
        cards: false,
        card_accent: crate::adaptive::CardAccent::None,
        track_start,
        span,
        fixed_canvas: Some(flow.text.canvas),
    }
}

fn prose(projection: &TextProjection, id: NodeId) -> Option<&crate::ProjectionSegment> {
    let BlockNode::Paragraph(p) = projection.block(id)? else {
        return None;
    };
    let segment = projection.segment_for_node(id)?;
    let text = &projection.text()[segment.projection_range()];
    // Ambiguous editorial text stays stacked, while explicit authored labels
    // are consumed separately into the figure's own lane.
    let caption = text.len() <= 240
        && (text.starts_with("Figure ")
            || text.starts_with("Photo ")
            || text.starts_with("Illustration ")
            || !p.content.runs().is_empty()
                && p.content
                    .runs()
                    .iter()
                    .all(|run| run.styles.contains(&InlineStyle::Italic)));
    (segment.node_id == segment.top_level_node_id
        && segment.context.figure_text.is_none()
        && !caption
        && segment.node_range == (0..p.content.len())
        && p.content.len() <= MAX_BYTES
        && !text.contains(['\n', '\r'])
        && !measurement::contains_strong_rtl(text)
        && !p.content.runs().iter().any(|run| {
            run.styles.iter().any(|style| {
                matches!(
                    style,
                    InlineStyle::Image { .. }
                        | InlineStyle::PreservedHtml(_)
                        | InlineStyle::Math { .. }
                )
            })
        }))
    .then_some(segment)
}

fn publish(plan: &mut AdaptivePlan, flow: Arc<FigureFlow>) {
    for id in flow.nodes() {
        plan.slots.remove(&id);
        plan.prose_flows.remove(&id);
        plan.figure_flows.insert(id, flow.clone());
    }
}

/// Compare the complete source unit with the same native wrap geometry used
/// below. A failed wrap can nominate a regular measured pair; it must not force
/// every supporting photograph into a stack. The row planner calls this only
/// inside requested bounded windows, with one cached result per source range.
pub(super) fn prefers_wrap(
    projection: &TextProjection,
    plan: &AdaptivePlan,
    roots: &[&BlockNode],
    viewport: f32,
    resources: &arrangement::LayoutMeasurement<'_>,
) -> bool {
    if plan.canvas < DocumentStyle::SINGLE_COLUMN_WIDTH || viewport < 480. {
        return false;
    }
    let Some(BlockNode::Image(image)) = roots.first() else {
        return false;
    };
    if !supporting(image) {
        return false;
    }
    let empty = HashMap::new();
    let Some(dimensions) = resolved_image_dimensions(image, resources.images.unwrap_or(&empty))
    else {
        return false;
    };
    candidate(
        projection,
        plan,
        image,
        dimensions,
        &roots[1..],
        viewport,
        resources.text,
    )
    .is_some()
}

pub(super) fn measure(
    plan: &mut AdaptivePlan,
    projection: &TextProjection,
    viewport: f32,
    previous: Option<&AdaptivePlan>,
    keep_arrangements: bool,
    resources: &arrangement::LayoutMeasurement<'_>,
) {
    if plan.canvas < DocumentStyle::SINGLE_COLUMN_WIDTH || viewport < 480. {
        return;
    }
    let empty = HashMap::new();
    let images = resources.images.unwrap_or(&empty);
    let roots = projection.roots().collect::<Vec<_>>();
    if let Some(old) = previous {
        let mut seen = HashSet::new();
        for flow in old
            .figure_flows
            .values()
            .filter(|flow| seen.insert(flow.text.group))
        {
            let held = keep_arrangements
                || plan
                    .editing_node
                    .is_some_and(|id| flow.nodes().any(|n| n == id));
            let same_environment = old.canvas.to_bits() == plan.canvas.to_bits()
                && old
                    .measurement_identity
                    .as_ref()
                    .is_some_and(|id| Arc::ptr_eq(id, &resources.text.identity));
            let first = plan.root_ordinal(flow.text.group);
            let valid = flow.text.canvas <= plan.canvas + 0.5
                && flow.height <= (viewport * 0.4).min(336.)
                && first.is_some_and(|start| {
                    flow.nodes()
                        .enumerate()
                        .all(|(i, id)| plan.root_ordinal(id) == Some(start + i))
                })
                && matches!(projection.block(flow.text.group), Some(BlockNode::Image(image))
                    if image.source == flow.image_source && supporting(image)
                        && resolved_image_dimensions(image, images) == Some(flow.dimensions))
                && flow.labels.iter().enumerate().all(|(i, id)| {
                    projection.segment_for_node(*id).is_some_and(|s| {
                        s.context
                            .figure_text
                            .is_some_and(|(owner, _)| owner == flow.text.group)
                            && (held || projection.node_revision(*id) == flow.label_revisions[i])
                    })
                })
                && flow
                    .text
                    .sources
                    .iter()
                    .enumerate()
                    .all(|(i, (id, before))| {
                        !plan.inline_lists.contains_key(id)
                            && prose(projection, *id).is_some_and(|s| {
                                held || (same_environment
                                    && !flow.text.needs_balance
                                    && projection.node_revision(*id) == flow.text.revisions[i]
                                    && &projection.text()[s.projection_range()] == before.as_ref())
                            })
                    });
            if valid && (held || same_environment) {
                // A caption edit locks the figure's enclosing source group.
                // Its preceding heading did not join the float and must not
                // acquire a new one-column row merely because its label did.
                if held
                    && let Some(heading) = first
                        .and_then(|i| i.checked_sub(1))
                        .and_then(|i| roots.get(i))
                    && matches!(heading, BlockNode::Heading(_))
                    && plan
                        .slots
                        .get(&heading.id())
                        .is_some_and(|s| s.columns == 1)
                {
                    if let Some(slot) = old.slots.get(&heading.id()) {
                        plan.slots.insert(heading.id(), *slot);
                    } else {
                        plan.slots.remove(&heading.id());
                    }
                }
                let mut retained = flow.as_ref().clone();
                for (i, id) in flow.labels.iter().enumerate() {
                    let revision = projection.node_revision(*id);
                    retained.text.needs_balance |= revision != flow.label_revisions[i];
                    retained.label_revisions[i] = revision;
                }
                for (id, _) in &flow.text.sources {
                    let s = projection.segment_for_node(*id).unwrap();
                    if let Some(text) = retained.text.rebased(
                        *id,
                        &projection.text()[s.projection_range()],
                        projection.node_revision(*id),
                    ) {
                        retained.text = text;
                    }
                }
                retained.text.needs_balance |= !same_environment;
                publish(plan, Arc::new(retained));
            }
        }
    }
    if keep_arrangements {
        return;
    }
    for window in plan.windows.clone() {
        for start in window.start..window.end.saturating_sub(1) {
            let BlockNode::Image(image) = roots[start] else {
                continue;
            };
            if !supporting(image)
                || plan.figure_flows.contains_key(&image.id)
                || plan.slots.contains_key(&image.id)
                || plan.editing_node == Some(image.id)
                || !plan.has_measured_geometry(image.id)
                || !plan.measures_root(image.id)
                || start > 0 && matches!(roots[start - 1], BlockNode::Image(_))
            {
                continue;
            }
            let Some(dimensions) = resolved_image_dimensions(image, images) else {
                continue;
            };
            let labels_end = crate::figures::end(&roots, start);
            if roots[start + 1..labels_end].iter().any(|root| {
                plan.editing_node == Some(root.id()) || !plan.has_measured_geometry(root.id())
            }) {
                continue;
            }
            let mut end = labels_end;
            let mut bytes = 0;
            while end < window.end && end - start <= MAX_ROOTS {
                let id = roots[end].id();
                let Some(s) = prose(projection, id) else {
                    break;
                };
                bytes += s.node_range.len();
                if bytes > MAX_BYTES
                    || plan.slots.contains_key(&id)
                    || plan.figure_flows.contains_key(&id)
                    || plan.inline_lists.contains_key(&id)
                    || plan.resources.contains_key(&id)
                    || plan.editorials.contains_key(&id)
                    || plan.lead == Some(id)
                    || plan.editing_node == Some(id)
                    || !plan.measures_root(id)
                    || !plan.has_measured_geometry(id)
                {
                    break;
                }
                end += 1;
            }
            if let Some(flow) = candidate(
                projection,
                plan,
                image,
                dimensions,
                &roots[start + 1..end],
                viewport,
                resources.text,
            ) {
                publish(plan, Arc::new(flow));
            }
        }
    }
}

fn candidate(
    projection: &TextProjection,
    plan: &AdaptivePlan,
    image: &document_core::ImageNode,
    dimensions: (u32, u32),
    roots: &[&BlockNode],
    viewport: f32,
    fonts: &FontMeasurement,
) -> Option<FigureFlow> {
    let labels_end = roots
        .iter()
        .take_while(|root| {
            projection.segment_for_node(root.id()).is_some_and(|s| {
                s.context.figure_text.is_some_and(|(owner, role)| {
                    owner == image.id && role.gallery_start().is_none()
                })
            })
        })
        .count();
    let labels = &roots[..labels_end];
    let roots = &roots[labels_end..];
    let first = projection.segment_for_node(roots.first()?.id())?;
    let measure = plan.prose_measures.for_role(first.context.narrative, false);
    let canvas = plan.canvas.min(measure);
    let mut flow = FigureFlow {
        text: Flow {
            group: image.id,
            canvas,
            columns: 2,
            needs_balance: false,
            sources: Vec::new(),
            revisions: Vec::new(),
            starts: vec![(first.node_id, 0)],
        },
        image_source: image.source.clone(),
        dimensions,
        height: 0.,
        image_height: 0.,
        labels: labels.iter().map(|root| root.id()).collect(),
        label_revisions: labels
            .iter()
            .map(|root| projection.node_revision(root.id()))
            .collect(),
    };
    let figure_width = slot(&flow, 0).width(canvas);
    let narrow = slot(&flow, 1).width(canvas);
    if figure_width < canvas * 0.25
        || figure_width > canvas * 0.35
        || narrow < measure * 40. / DocumentStyle::PROSE_CHARACTERS
        || dimensions.0 == 0
        || dimensions.1 == 0
        || (dimensions.0 as f32) < figure_width
    {
        return None;
    }
    flow.image_height = figure_width * dimensions.1 as f32 / dimensions.0 as f32;
    flow.height = flow.image_height;
    for root in labels {
        let s = projection.segment_for_node(root.id())?;
        if s.node_range.len() > MAX_BYTES {
            return None;
        }
        let lines = build_visual_lines_for_segment(
            projection,
            s,
            &HashMap::new(),
            figure_width + 8.,
            &[],
            Some(fonts),
            None,
        );
        flow.height += s.context.figure_text?.1.gap();
        for line in lines {
            if fonts.line_width(projection, line.projected_range(), line.style.font_size)?
                > figure_width + 0.5
            {
                return None;
            }
            flow.height += line.style.line_height;
        }
    }
    if flow.height > (viewport * 0.4).min(336.) {
        return None;
    }
    let mut height = 0.;
    let mut useful_lines = 0;
    let mut below = false;
    for (paragraph, root) in roots.iter().enumerate() {
        let segment = prose(projection, root.id())?;
        if segment.context.narrative != first.context.narrative {
            return None;
        }
        let lines = build_visual_lines_for_segment(
            projection,
            segment,
            &HashMap::new(),
            narrow + 8.,
            &[],
            Some(fonts),
            None,
        );
        if paragraph > 0 {
            height += LAYOUT_GAP;
        }
        for line in lines {
            if !below && height >= flow.height {
                flow.text.starts.push((
                    root.id(),
                    line.projected_start() - segment.projection_start(),
                ));
                below = true;
            }
            if !below {
                if fonts.line_width(projection, line.projected_range(), line.style.font_size)?
                    > narrow + 0.5
                {
                    return None;
                }
                if !projection.text()[line.projected_range()].trim().is_empty() {
                    useful_lines += 1;
                }
                height += line.style.line_height;
            }
        }
        flow.text.sources.push((
            root.id(),
            Arc::from(&projection.text()[segment.projection_range()]),
        ));
        flow.text
            .revisions
            .push(projection.node_revision(root.id()));
    }
    (below && useful_lines >= 4).then_some(flow)
}

pub(super) fn rebase_after_edit(plan: &mut AdaptivePlan, projection: &TextProjection, id: NodeId) {
    let Some(flow) = plan.figure_flows.get(&id) else {
        return;
    };
    let Some(s) = projection.segment_for_node(id) else {
        return;
    };
    if let Some(i) = flow.labels.iter().position(|node| *node == id) {
        let mut next = flow.as_ref().clone();
        next.text.needs_balance = true;
        next.label_revisions[i] = projection.node_revision(id);
        publish(plan, Arc::new(next));
        return;
    }
    if let Some(text) = flow.text.rebased(
        id,
        &projection.text()[s.projection_range()],
        projection.node_revision(id),
    ) {
        let mut next = flow.as_ref().clone();
        next.text = text;
        publish(plan, Arc::new(next));
    }
}

pub(super) fn build(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    flow: &FigureFlow,
    width: f32,
    fonts: Option<&FontMeasurement>,
) -> Vec<VisualLineSpec> {
    let image = segment.node_id == flow.text.group;
    let label = segment
        .context
        .figure_text
        .filter(|(owner, _)| *owner == flow.text.group);
    let text = &projection.text()[segment.projection_range()];
    let fragments = if image || label.is_some() {
        vec![(0..text.len(), 0)]
    } else {
        flow.text
            .fragments(segment.node_id, text)
            .into_iter()
            .map(|(r, i)| (r, i + 1))
            .collect()
    };
    let mut output = Vec::new();
    for (range, item) in fragments {
        let slot = slot(flow, item);
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
        if image {
            // Alt is one semantic image range, never visible caption text.
            lines.truncate(1);
            if let Some(line) = lines.first_mut() {
                line.set_projected_range(segment.projection_range())
                    .expect("image source belongs to its projection segment");
                line.style.line_height = flow.image_height;
            }
        } else if let Some((_, role)) = label {
            if let Some(line) = lines.first_mut() {
                line.gap_before = role.gap();
            }
        } else if let Some(line) = lines.first_mut() {
            line.gap_before = if range.start == 0 { LAYOUT_GAP } else { 0. };
        }
        output.extend(lines);
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    const FIXTURE: &str =
        include_str!("../../../../performance/layout-fixtures/67-supporting-figure-flow.md");

    fn images(projection: &TextProjection) -> NodeImageDimensions {
        projection
            .image_segments()
            .map(|s| {
                (
                    s.node_id,
                    (s.context.image_source.clone().unwrap(), (600, 600)),
                )
            })
            .collect()
    }

    fn plan(
        projection: &TextProjection,
        fonts: &FontMeasurement,
        width: f32,
        previous: Option<&AdaptivePlan>,
        editing: Option<NodeId>,
    ) -> AdaptivePlan {
        arrangement::build_edit_locked_adaptive_plan(
            projection,
            width,
            1000.,
            previous,
            false,
            arrangement::LayoutMeasurement {
                text: fonts,
                images: Some(&images(projection)),
                scope: None,
                resource_generation: 1,
            },
            editing,
        )
    }

    #[gpui::test]
    fn supporting_image_can_pair_when_its_complete_lane_cannot_wrap(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/111-supporting-image-pair.md"
            );
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let layout = plan(&projection, &fonts, 1314., None, None);
            assert!(
                layout.figure_flows.is_empty(),
                "the full caption lane is too tall for a float"
            );
            let pair = layout
                .measured_rows
                .chosen
                .iter()
                .find(|row| row.kind == crate::adaptive::rows::RowKind::FigureExplanation)
                .expect(
                    "a readable complete figure/explanation pair should use the available width",
                );
            assert_eq!(pair.parts[0].len(), 3);
            assert_eq!(pair.parts[1].len(), 3);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn supporting_pairs_preserve_geometry_across_width_height_and_zoom(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/111-supporting-image-pair.md"
            );
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let dimensions = images(&projection);
            let roots = projection.roots().collect::<Vec<_>>();
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                let wide = plan(&projection, &fonts, 1314., None, None);
                for (width, height, paired) in [
                    (1314., 1366., true),
                    (520., 1366., false),
                    (657., 683., false),
                    (1314., 240., false),
                ] {
                    let layout = arrangement::build_edit_locked_adaptive_plan(
                        &projection,
                        width,
                        height,
                        Some(&wide),
                        false,
                        arrangement::LayoutMeasurement {
                            text: &fonts,
                            images: Some(&dimensions),
                            scope: None,
                            resource_generation: 1,
                        },
                        None,
                    );
                    assert!(layout.figure_flows.is_empty());
                    assert_eq!(layout.measured_rows.validation_fallbacks, 0);
                    let rows = layout
                        .measured_rows
                        .chosen
                        .iter()
                        .filter(|row| row.kind == crate::adaptive::rows::RowKind::FigureExplanation)
                        .collect::<Vec<_>>();
                    assert_eq!(
                        rows.len(),
                        usize::from(paired),
                        "{width}x{height} at {zoom}"
                    );
                    let lines = arrangement::build_measured_visual_lines(
                        &projection,
                        &dimensions,
                        width,
                        &layout,
                        Some(&fonts),
                    );
                    for row in rows {
                        assert!(
                            row.widths[0] <= 600. + 0.01,
                            "never upscale the source image"
                        );
                        let body = projection
                            .segment_for_node(roots[row.parts[1].start].id())
                            .unwrap();
                        assert!(
                            row.widths[1]
                                <= layout.prose_measures.fit_width(
                                    width,
                                    body.context.narrative,
                                    false
                                ) + 0.01
                        );
                        let mut top = None;
                        for (part, expected) in row.parts.iter().zip(&row.heights) {
                            let own = lines
                                .iter()
                                .filter(|line| {
                                    let id = projection
                                        .segment_for_range(&line.projected_range())
                                        .unwrap()
                                        .top_level_node_id;
                                    roots[part.clone()].iter().any(|root| root.id() == id)
                                })
                                .collect::<Vec<_>>();
                            assert!((own[0].y - *top.get_or_insert(own[0].y)).abs() < 0.1);
                            let bottom =
                                own.last().unwrap().y + own.last().unwrap().style.line_height;
                            assert!((bottom - own[0].y - expected).abs() < 0.1);
                            for adjacent in own.windows(2) {
                                assert!(
                                    adjacent[0].y + adjacent[0].style.line_height
                                        <= adjacent[1].y + 0.1
                                );
                            }
                        }
                    }
                    for segment in projection.segments() {
                        let own = lines
                            .iter()
                            .filter(|line| {
                                projection
                                    .segment_for_range(&line.projected_range())
                                    .unwrap()
                                    .node_id
                                    == segment.node_id
                            })
                            .collect::<Vec<_>>();
                        assert_eq!(
                            own.first().unwrap().projected_start(),
                            segment.projection_start()
                        );
                        assert_eq!(
                            own.last().unwrap().projected_end(),
                            segment.projection_end()
                        );
                        assert!(
                            own.windows(2)
                                .all(|pair| pair[0].projected_end() <= pair[1].projected_start())
                        );
                    }
                    let restored = plan(&projection, &fonts, 1314., Some(&layout), None);
                    assert!(
                        restored.measured_rows.chosen.iter().any(
                            |row| row.kind == crate::adaptive::rows::RowKind::FigureExplanation
                        )
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn supporting_pair_waits_for_resources_and_keeps_focused_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/111-supporting-image-pair.md"
            );
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let body = projection
                .segments()
                .iter()
                .find(|s| {
                    projection.text()[s.projection_range()].starts_with("The illustration gives")
                })
                .unwrap()
                .node_id;
            let pending = arrangement::build_edit_locked_adaptive_plan(
                &projection,
                1314.,
                1000.,
                None,
                false,
                &fonts,
                Some(body),
            );
            assert!(
                pending
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.kind != crate::adaptive::rows::RowKind::FigureExplanation)
            );
            let focused = plan(&projection, &fonts, 1314., Some(&pending), Some(body));
            assert!(
                focused
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.kind != crate::adaptive::rows::RowKind::FigureExplanation)
            );
            let ready = plan(&projection, &fonts, 1314., Some(&focused), None);
            let slot = ready.slots[&body];
            assert_eq!(slot.columns, 2);
            document
                .apply(EditCommand::ReplaceText {
                    node_id: body,
                    range: 0..0,
                    text: "More detail about the observation. ".repeat(100),
                    selection_after: None,
                    typing: false,
                })
                .unwrap();
            let changed = TextProjection::from_snapshot(&document.snapshot());
            let held = plan(&changed, &fonts, 1600., Some(&ready), Some(body));
            assert_eq!(held.slots[&body].width(1600.), slot.width(1314.));
            assert_eq!(held.slots[&body].left(1600.), slot.left(1314.));
            assert_eq!(held.slots[&body].group, slot.group);
            assert_eq!(held.slots[&body].item, slot.item);
            assert!(held.figure_flows.is_empty());
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn supporting_image_negotiation_releases_only_after_editing_ends(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let full = include_str!(
                "../../../../performance/layout-fixtures/111-supporting-image-pair.md"
            );
            let source = &full[full.find("## Reading the illustration").unwrap()
                ..full.find("## Return to the field").unwrap()];
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let caption = projection
                .segments()
                .iter()
                .find(|s| {
                    s.context
                        .figure_text
                        .is_some_and(|(_, role)| role == crate::figures::FigureTextRole::Caption)
                })
                .unwrap();
            let id = caption.node_id;
            let image = caption.context.figure_text.unwrap().0;
            let paired = plan(&projection, &fonts, 1314., None, None);
            let caption_slot = paired.slots[&id];
            document
                .apply(EditCommand::ReplaceText {
                    node_id: id,
                    range: caption.node_range.clone(),
                    text: "Caption: The complete branch.".into(),
                    selection_after: None,
                    typing: false,
                })
                .unwrap();
            let changed = TextProjection::from_snapshot(&document.snapshot());
            let held = plan(&changed, &fonts, 1314., Some(&paired), Some(id));
            assert!(held.figure_flows.is_empty());
            assert_eq!(held.slots[&id].width(1314.), caption_slot.width(1314.));
            let wrapped = plan(&changed, &fonts, 1314., Some(&held), None);
            assert!(
                wrapped.figure_flows.contains_key(&image),
                "the shortened complete lane now supports true wrapping"
            );
            assert!(
                wrapped
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.kind != crate::adaptive::rows::RowKind::FigureExplanation)
            );
            let narrow = plan(&changed, &fonts, 520., Some(&wrapped), None);
            assert!(narrow.figure_flows.is_empty());
            assert!(
                narrow
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.kind != crate::adaptive::rows::RowKind::FigureExplanation)
            );
            let restored = plan(&changed, &fonts, 1314., Some(&narrow), None);
            assert!(restored.figure_flows.contains_key(&image));
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            let original = TextProjection::from_snapshot(&document.snapshot());
            let paired_again = plan(&original, &fonts, 1314., Some(&restored), None);
            assert!(paired_again.figure_flows.is_empty());
            assert!(
                paired_again
                    .measured_rows
                    .chosen
                    .iter()
                    .any(|row| row.kind == crate::adaptive::rows::RowKind::FigureExplanation)
            );
        });
    }

    #[gpui::test]
    fn wraps_then_returns_to_full_measure_without_losing_source(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown(FIXTURE).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = plan(&projection, &fonts, 1280., None, None);
            assert_eq!(
                plan.figure_flows.len(),
                3,
                "one image and its two paragraphs"
            );
            let flow = plan.figure_flows.values().next().unwrap();
            assert!(
                flow.text.sources.iter().all(|(id, _)| projection
                    .segment_for_node(*id)
                    .unwrap()
                    .context
                    .narrative)
            );
            assert!(
                projection
                    .image_segments()
                    .filter(|s| s.node_id != flow.text.group)
                    .all(|s| !s.context.narrative)
            );
            let lines = arrangement::build_measured_visual_lines(
                &projection,
                &images(&projection),
                1280.,
                &plan,
                Some(&fonts),
            );
            for s in projection.segments() {
                let reconstructed = lines
                    .iter()
                    .filter(|l| {
                        projection
                            .segment_for_range(&l.projected_range())
                            .is_some_and(|owner| owner.node_id == s.node_id)
                    })
                    .map(|l| &projection.text()[l.projected_range()])
                    .collect::<String>();
                assert_eq!(reconstructed, projection.text()[s.projection_range()]);
            }
            let figure = lines
                .iter()
                .find(|l| {
                    l.slot
                        .is_some_and(|s| s.group == flow.text.group && s.item == 0)
                })
                .unwrap();
            let narrow = lines
                .iter()
                .filter(|l| {
                    l.slot
                        .is_some_and(|s| s.group == flow.text.group && s.item == 1)
                })
                .collect::<Vec<_>>();
            let full = lines
                .iter()
                .filter(|l| {
                    l.slot
                        .is_some_and(|s| s.group == flow.text.group && s.item == 2)
                })
                .collect::<Vec<_>>();
            assert!(narrow.len() >= 4 && !full.is_empty());
            assert_eq!(figure.y, narrow[0].y);
            assert!(
                (narrow[0].x_fraction * 1280. - figure.width_fraction * 1280. - LAYOUT_GAP).abs()
                    < 0.01
            );
            let fraction = figure.width_fraction * 1280. / flow.text.canvas;
            assert!((0.25..=0.35).contains(&fraction));
            assert!((full[0].width_fraction * 1280. - flow.text.canvas).abs() < 0.01);
            assert_eq!(full[0].x_fraction, 0.);
            let last = narrow.last().unwrap();
            assert_eq!(
                full[0].y,
                last.y + last.style.line_height,
                "no invented gap within a paragraph"
            );
            assert!(full[0].y >= figure.y + figure.style.line_height);
            assert_eq!(last.projected_end(), full[0].projected_start());
            let next_heading = lines
                .iter()
                .find(|l| projection.text()[l.projected_range()].contains("A separate subject"))
                .unwrap();
            assert!(
                next_heading.y
                    >= full.last().unwrap().y + full.last().unwrap().style.line_height + 48.
            );
            assert!(next_heading.slot.is_none());
            assert_eq!(document.snapshot().serialize().unwrap(), FIXTURE);
            let warm = super::tests::plan(&projection, &fonts, 1280., Some(&plan), None);
            assert_eq!(warm.figure_flows, plan.figure_flows);
            assert!(
                super::tests::plan(&projection, &fonts, 500., Some(&plan), None)
                    .figure_flows
                    .is_empty()
            );
        });
    }

    #[gpui::test]
    fn held_wrap_edit_matches_full_geometry_and_exact_undo(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let mut document = Document::from_markdown(FIXTURE).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let initial = plan(&projection, &fonts, 1280., None, None);
            let flow = initial.figure_flows.values().next().unwrap();
            let node = flow.text.sources[0].0;
            let mut lines = arrangement::build_measured_visual_lines(
                &projection,
                &images(&projection),
                1280.,
                &initial,
                Some(&fonts),
            );
            let mut paint = visual_line_paint_order(&lines);
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: 0..0,
                    text: "A new observation. ".repeat(12),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let dimensions = images(&projection);
            refresh_arranged_text_node_geometry(
                &mut projection,
                &mut lines,
                &mut paint,
                &initial,
                TextRefreshRequest {
                    snapshot: &document.snapshot(),
                    node_id: node,
                    image_dimensions: &dimensions,
                    layout_width: 1280.,
                    zoom_factor: 1.,
                    measurement: Some(&fonts),
                },
            )
            .expect("the complete float uses localized editing");
            let held = plan(&projection, &fonts, 1280., Some(&initial), Some(node));
            let full = arrangement::build_measured_visual_lines(
                &projection,
                &images(&projection),
                1280.,
                &held,
                Some(&fonts),
            );
            arrangement::tests::assert_same_geometry(&lines, &full);
            assert_eq!(paint, visual_line_paint_order(&lines));
            assert!(held.figure_flows[&node].text.needs_balance);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), FIXTURE);
        });
    }

    #[gpui::test]
    fn rejects_cramped_tall_unknown_and_semantically_essential_figures(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for source in [
                FIXTURE.replace(
                    "Botanical illustration of a flowering branch",
                    "Essential diagram",
                ),
                FIXTURE.replace(
                    "Botanical illustration of a flowering branch",
                    "Unclassified object",
                ),
                FIXTURE.replace("A field notebook connects", "A field notebook  \nconnects"),
                FIXTURE.replace("A field notebook connects", "*An unlabelled editorial aside.*\n\nA field notebook connects"),
                FIXTURE.replace("A field notebook connects", "> [!WARNING]\n> Do not proceed.\n\nA field notebook connects"),
                FIXTURE.replace("A field notebook connects", "| Measurement | Value |\n| --- | --- |\n| Height | 12 cm |\n\nA field notebook connects"),
            ] {
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                assert!(
                    plan(&projection, &fonts, 1280., None, None)
                        .figure_flows
                        .is_empty()
                );
            }
            let document = Document::from_markdown(FIXTURE).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for dimensions in [
                HashMap::new(),
                projection
                    .image_segments()
                    .map(|s| {
                        (
                            s.node_id,
                            (s.context.image_source.clone().unwrap(), (100, 2000)),
                        )
                    })
                    .collect(),
            ] {
                let plan = arrangement::build_edit_locked_adaptive_plan(
                    &projection,
                    1280.,
                    1000.,
                    None,
                    false,
                    arrangement::LayoutMeasurement {
                        text: &fonts,
                        images: Some(&dimensions),
                        scope: None,
                        resource_generation: 0,
                    },
                    None,
                );
                assert!(plan.figure_flows.is_empty());
            }
        });
    }

    #[gpui::test]
    fn gallery_captions_are_not_independent_columns(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = "## Gallery\n\n![Photo A](a.png)\n\nFigure 1. Morning.\n\nCredit: East trail\n\n![Photo B](b.png)\n\nFigure 2. Afternoon.\n\nCredit: West trail\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for dimensions in [(1200, 600), (600, 300)] {
            let images = projection.image_segments().map(|s| (s.node_id, (s.context.image_source.clone().unwrap(), dimensions))).collect();
            let plan = arrangement::build_edit_locked_adaptive_plan(&projection, 1280., 1200., None, false, arrangement::LayoutMeasurement { text: &fonts, images: Some(&images), scope: None, resource_generation: 1 }, None);
            let lines = arrangement::build_measured_visual_lines(&projection, &images, 1280., &plan, Some(&fonts));
            for segment in projection.segments().iter().filter(|s| s.context.figure_text.is_some()) {
                let (owner, role) = segment.context.figure_text.unwrap();
                let slot = plan.slots.get(&owner).expect("gallery has measured peer figures");
                assert_eq!(slot.columns, 2);
                assert_eq!(plan.slots.get(&segment.node_id), Some(slot));
                let index = lines.iter().position(|l| l.projected_start() == segment.projection_start()).unwrap();
                let before = &lines[index - 1];
                let label = &lines[index];
                assert!((label.y - before.y - before.style.line_height - role.gap()).abs() < 0.01);
                assert_eq!(label.width_fraction, before.width_fraction);
            }
            let figures = lines.iter().filter(|line| projection.segment_for_range(&line.projected_range()).unwrap().context.image_source.is_some()).collect::<Vec<_>>();
            assert!(
                (figures[1].x_fraction * 1280.
                    - (figures[0].x_fraction + figures[0].width_fraction) * 1280.
                    - LAYOUT_GAP)
                    .abs()
                    < 0.01
            );
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn captions_and_credits_share_the_image_lane_and_clear_before_full_prose(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = FIXTURE.replace("A field notebook connects", "Figure 1. A botanical study.\n\nCredit: Field notebook\n\nA field notebook connects");
            let mut document = Document::from_markdown(source.as_str()).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let initial = plan(&projection, &fonts, 1280., None, None);
            let flow = initial.figure_flows.values().next().expect("captioned figure can wrap");
            assert_eq!(flow.labels.len(), 2);
            assert_eq!(flow.nodes().count(), 5);
            let lines = arrangement::build_measured_visual_lines(&projection, &images(&projection), 1280., &initial, Some(&fonts));
            let for_node = |id| lines.iter().filter(|l| projection.segment_for_range(&l.projected_range()).unwrap().node_id == id).collect::<Vec<_>>();
            let image = for_node(flow.text.group)[0];
            let caption = for_node(flow.labels[0]);
            let credit = for_node(flow.labels[1]);
            assert!((caption[0].y - image.y - image.style.line_height - 8.).abs() < 0.01);
            assert!((credit[0].y - caption.last().unwrap().y - DocumentStyle::CAPTION_LEADING - 4.).abs() < 0.01);
            for line in caption.iter().chain(&credit) {
                assert_eq!(line.style.font_size, DocumentStyle::CAPTION_SIZE);
                assert_eq!(line.style.line_height, DocumentStyle::CAPTION_LEADING);
                assert_eq!(line.slot, image.slot);
                assert_eq!(line.x_fraction, image.x_fraction);
                assert_eq!(line.width_fraction, image.width_fraction);
            }
            let full = lines.iter().find(|l| l.slot.is_some_and(|s| s.group == flow.text.group && s.item == 2)).unwrap();
            assert!(full.y >= credit.last().unwrap().y + DocumentStyle::CAPTION_LEADING);
            for s in projection.segments() {
                let text = for_node(s.node_id).into_iter().map(|l| &projection.text()[l.projected_range()]).collect::<String>();
                assert_eq!(text, projection.text()[s.projection_range()]);
            }
            let caption_id = flow.labels[0];
            let mut updated = lines.clone();
            let mut paint = visual_line_paint_order(&updated);
            document.apply(EditCommand::ReplaceText { node_id: caption_id, range: 10..10, text: "A closer observation. ".repeat(8), typing: true, selection_after: None }).unwrap();
            let dimensions = images(&projection);
            refresh_arranged_text_node_geometry(&mut projection, &mut updated, &mut paint, &initial, TextRefreshRequest { snapshot: &document.snapshot(), node_id: caption_id, image_dimensions: &dimensions, layout_width: 1280., zoom_factor: 1., measurement: Some(&fonts) }).expect("caption edits refresh the complete media lane");
            let held = plan(&projection, &fonts, 1280., Some(&initial), Some(caption_id));
            let rebuilt = arrangement::build_measured_visual_lines(&projection, &images(&projection), 1280., &held, Some(&fonts));
            arrangement::tests::assert_same_geometry(&updated, &rebuilt);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let narrow = plan(&projection, &fonts, 420., None, None);
            assert!(narrow.figure_flows.is_empty());
            let lines = arrangement::build_measured_visual_lines(&projection, &images(&projection), 420., &narrow, Some(&fonts));
            let caption = projection.segment_for_node(caption_id).unwrap();
            let index = lines.iter().position(|l| l.projected_start() == caption.projection_start()).unwrap();
            let image = &lines[index - 1];
            assert_eq!(lines[index].y - image.y - image.style.line_height, 8.);
            assert_eq!(lines[index].x_fraction, image.x_fraction);
            assert_eq!(lines[index].width_fraction, image.width_fraction);
        });
    }
}
