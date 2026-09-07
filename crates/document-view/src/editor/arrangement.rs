use super::*;

/// Read-only resources already owned by the view. Candidate measurement never
/// starts image loading; absent or stale dimensions retain the stack fallback.
pub(super) struct LayoutMeasurement<'a> {
    pub text: &'a FontMeasurement,
    pub images: Option<&'a NodeImageDimensions>,
    pub scope: Option<Range<usize>>,
    pub resource_generation: u64,
}

impl<'a> From<&'a FontMeasurement> for LayoutMeasurement<'a> {
    fn from(text: &'a FontMeasurement) -> Self {
        Self {
            text,
            images: None,
            scope: None,
            resource_generation: 0,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct RootMeasureKey {
    content: measurement::ContentIdentity,
    lead: bool,
    steps: bool,
    starts_document: bool,
    math_edit: Option<NodeId>,
    html_disclosures: Vec<(NodeId, crate::html::DisclosureOverrides)>,
    html_images: Vec<(NodeId, crate::html::images::ImageKey)>,
    image: Option<(String, (u32, u32))>,
    table: Option<TableMeasureKey>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct TableMeasureKey {
    minimum: Vec<u32>,
    preferred: Vec<u32>,
    fitted: Vec<u32>,
}

/// A footprint, not a chosen layout: prior-plan penalties, editing locks and
/// hysteresis are evaluated again by the planner. The owning FontMeasurement
/// supplies an immutable font/style/zoom environment for the cache lifetime.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(super) struct GroupMeasureKey {
    roots: Vec<RootMeasureKey>,
    width: u32,
    cards: bool,
    gaps: Vec<u32>,
}

impl GroupMeasureKey {
    fn new(
        projection: &TextProjection,
        roots: &[&BlockNode],
        segments: &HashMap<NodeId, Vec<usize>>,
        width: f32,
        cards: bool,
        resources: &LayoutMeasurement<'_>,
        presentation: &AdaptivePlan,
    ) -> Option<Self> {
        // Bound retained groups before collecting keys. Oversized groups use
        // uncached measurement, with the same eligibility and complete renderer.
        if roots.is_empty() || roots.len() > 4 {
            return None;
        }
        let mut bytes = 0_usize;
        let mut count = 0_usize;
        let mut keys = Vec::with_capacity(roots.len());
        for root in roots {
            let indexes = segments.get(&root.id())?;
            count += indexes.len();
            if count > 512 {
                return None;
            }
            for &index in indexes {
                bytes = bytes.checked_add(projection.segments()[index].projection_range.len())?;
                bytes = bytes.checked_add(
                    projection
                        .html_disclosure_overrides(projection.segments()[index].node_id)
                        .map_or(0, |overrides| overrides.len() * 32),
                )?;
            }
            if let BlockNode::Image(image) = root
                && image.source.len() > 4096
            {
                return None;
            }
            if bytes > 64 * 1024 {
                return None;
            }
            let table = if matches!(root, BlockNode::Table(_)) {
                projection.table_measurements(root.id()).map(|measured| {
                    let available = projection.table_available_width(root.id(), width);
                    TableMeasureKey {
                        minimum: measured
                            .minimum
                            .iter()
                            .map(|value| value.to_bits())
                            .collect(),
                        preferred: measured
                            .preferred
                            .iter()
                            .map(|value| value.to_bits())
                            .collect(),
                        fitted: projection
                            .fitted_table_widths(root.id(), available)
                            .unwrap_or_default()
                            .iter()
                            .map(|value| value.to_bits())
                            .collect(),
                    }
                })
            } else {
                None
            };
            keys.push(RootMeasureKey {
                content: measurement::ContentIdentity(projection.block_handle(root.id())?.clone()),
                lead: presentation.lead == Some(root.id()),
                steps: presentation
                    .lists
                    .get(&root.id())
                    .is_some_and(|list| list.layout == ListLayout::Steps),
                starts_document: indexes
                    .first()
                    .is_some_and(|&index| projection.segments()[index].projection_range.start == 0),
                math_edit: projection.math_edit_node.filter(|node| {
                    indexes
                        .iter()
                        .any(|&index| projection.segments()[index].node_id == *node)
                }),
                image: resources
                    .images
                    .and_then(|images| images.get(&root.id()))
                    .cloned(),
                html_disclosures: indexes
                    .iter()
                    .filter_map(|&index| {
                        let node = projection.segments()[index].node_id;
                        Some((node, projection.html_disclosure_overrides(node)?.clone()))
                    })
                    .collect(),
                html_images: indexes
                    .iter()
                    .filter_map(|&index| {
                        let node = projection.segments()[index].node_id;
                        Some((node, projection.html_images(node)?.key.clone()))
                    })
                    .collect(),
                table,
            });
        }
        Some(Self {
            roots: keys,
            width: width.to_bits(),
            cards,
            gaps: roots
                .windows(2)
                .map(|pair| presentation.gap_between(pair[0], pair[1]).to_bits())
                .collect(),
        })
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct ComponentGeometry {
    pub left_fraction: f32,
    pub right_fraction: f32,
    pub top: f32,
    pub bottom: f32,
    pub first_line: usize,
}

pub(super) struct ComponentIndex {
    bounds: HashMap<NodeId, ComponentGeometry>,
    headings: Vec<(f32, NodeId)>,
    paint_bottoms: Vec<f32>,
}

impl ComponentIndex {
    pub fn get(&self, id: &NodeId) -> Option<&ComponentGeometry> {
        self.bounds.get(id)
    }

    pub fn active_heading(&self, scroll_y: f32, height: f32) -> Option<NodeId> {
        let split = self.headings.partition_point(|(y, _)| *y <= scroll_y + 8.);
        split
            .checked_sub(1)
            .and_then(|index| self.headings.get(index))
            .map(|(_, id)| *id)
            .or_else(|| {
                self.headings
                    .get(split)
                    .filter(|(y, _)| *y < scroll_y + height)
                    .map(|(_, id)| *id)
            })
    }

    pub fn visible_range(
        &self,
        lines: &[VisualLineSpec],
        order: &[usize],
        top: f32,
        bottom: f32,
    ) -> Range<usize> {
        // Prefix maxima include tall figures and long table cells that start
        // above several shorter neighbors. A start-y-only search loses them.
        let start = self.paint_bottoms.partition_point(|end| *end < top);
        let end = order.partition_point(|index| lines[*index].y <= bottom);
        start.min(end)..end
    }
}

/// One linear pass at geometry publication, replacing full-document scans for
/// every visible code header, quote, callout and list during scroll frames.
pub(super) fn component_geometry(
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    width: f32,
    paint_order: &[usize],
) -> ComponentIndex {
    let mut components = HashMap::<NodeId, ComponentGeometry>::new();
    let mut headings = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let Some(segment) = projection.segment_for_range(&line.range) else {
            continue;
        };
        let geometry = ComponentGeometry {
            left_fraction: line.x_fraction + line.inset / width.max(1.),
            right_fraction: line.x_fraction + line.width_fraction,
            top: line.y,
            bottom: line.y + line.style.line_height,
            first_line: index,
        };
        if line.range.start == segment.projection_range.start
            && matches!(
                projection.block(segment.node_id),
                Some(BlockNode::Heading(_))
            )
        {
            headings.push((line.y, segment.node_id));
        }
        for id in [
            Some(segment.node_id),
            Some(segment.top_level_node_id),
            segment.context.alert.as_ref().map(|(id, _)| *id),
            segment.context.quote,
            segment.context.table_cell.map(|(id, _, _)| id),
        ]
        .into_iter()
        .flatten()
        {
            let geometry = if line.table_cell.is_some()
                && id != segment.node_id
                && projection.container_cell(id) != segment.context.table_cell
            {
                // Containers enclose the whole cell border, not just its text.
                // Leaf bounds remain text bounds for caret/accessibility use.
                ComponentGeometry {
                    left_fraction: line.x_fraction,
                    right_fraction: geometry.right_fraction
                        + if line.table_cell.is_some_and(|(table, _, _, _)| table != id) {
                            24. / width.max(1.)
                        } else {
                            0.
                        },
                    top: line.table_row_y,
                    bottom: line.table_row_y + line.table_row_height,
                    ..geometry
                }
            } else {
                ComponentGeometry {
                    right_fraction: geometry.right_fraction
                        - if projection.container_cell(id).is_some() {
                            12. / width.max(1.)
                        } else {
                            0.
                        },
                    ..geometry
                }
            };
            components
                .entry(id)
                .and_modify(|existing| {
                    existing.left_fraction = existing.left_fraction.min(geometry.left_fraction);
                    existing.right_fraction = existing.right_fraction.max(geometry.right_fraction);
                    existing.top = existing.top.min(geometry.top);
                    existing.bottom = existing.bottom.max(geometry.bottom);
                })
                .or_insert(geometry);
        }
    }
    headings.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut bottom = 0_f32;
    let paint_bottoms = paint_order
        .iter()
        .copied()
        .map(|index| {
            bottom = bottom.max(lines[index].y + lines[index].style.line_height);
            bottom
        })
        .collect();
    ComponentIndex {
        bounds: components,
        headings,
        paint_bottoms,
    }
}

fn card_presentation_breaks(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    cards: bool,
) -> Vec<usize> {
    let Some(block) = projection.block(segment.node_id) else {
        return Vec::new();
    };
    let label_break = if cards
        && let Some(text) = block.text()
        && let Some(prefix) = text.runs().iter().next()
        && prefix.range.start == 0
        && prefix.styles.contains(&InlineStyle::Bold)
    {
        let end = segment.projection_range.start + prefix.range.end;
        projection.text()[end..segment.projection_range.end]
            .char_indices()
            .find(|(_, character)| !character.is_whitespace())
            .map(|(offset, _)| end + offset)
    } else {
        None
    };
    let mut breaks = label_break.into_iter().collect::<Vec<_>>();
    if cards && segment.context.list_depth > 0 {
        let text = &projection.text()[segment.projection_range.clone()];
        let stages = text.split('→').collect::<Vec<_>>();
        if (3..=6).contains(&stages.len())
            && stages
                .iter()
                .all(|stage| !stage.trim().is_empty() && stage.len() <= 48)
        {
            breaks.extend(
                text.match_indices('→')
                    .map(|(offset, _)| segment.projection_range.start + offset),
            );
        }
    }
    breaks.sort_unstable();
    breaks.dedup();
    breaks
}

#[cfg(test)]
pub(super) fn build_measured_adaptive_plan(
    projection: &TextProjection,
    width: f32,
    viewport: f32,
    previous: Option<&AdaptivePlan>,
    keep_arrangements: bool,
    measurement: &FontMeasurement,
) -> AdaptivePlan {
    build_edit_locked_adaptive_plan(
        projection,
        width,
        viewport,
        previous,
        keep_arrangements,
        measurement,
        None,
    )
}

pub(super) fn build_edit_locked_adaptive_plan<'a>(
    projection: &TextProjection,
    width: f32,
    viewport: f32,
    previous: Option<&AdaptivePlan>,
    keep_arrangements: bool,
    measurement: impl Into<LayoutMeasurement<'a>>,
    editing_node: Option<NodeId>,
) -> AdaptivePlan {
    let resources = measurement.into();
    let measurement = resources.text;
    let mut plan = AdaptivePlan::build(projection, width, previous, keep_arrangements);
    plan.editing_node = editing_node;
    plan.measurement_identity = Some(measurement.identity.clone());
    plan.resource_generation = previous
        .filter(|_| keep_arrangements)
        .map_or(resources.resource_generation, |old| old.resource_generation);
    if let Some(requested) = &resources.scope {
        let mut ranges = plan.windows_for(requested);
        if let Some(ordinal) = editing_node
            .and_then(|node| projection.segment_for_node(node))
            .and_then(|segment| plan.root_ordinal(segment.top_level_node_id))
        {
            ranges.extend(plan.windows_for(&(ordinal..ordinal + 1)));
        }
        ranges.sort_by_key(|range| range.start);
        ranges.dedup();
        plan.measurement_ranges = Some(ranges);
    }
    if keep_arrangements {
        plan.edit_geometry_ranges = plan.measurement_ranges.clone().unwrap_or_default();
    }
    plan.pending_images = projection
        .image_segments()
        .filter(|segment| {
            resources
                .images
                .and_then(|images| images.get(&segment.node_id))
                .is_none_or(|(source, (width, height))| {
                    segment.context.image_source.as_ref() != Some(source)
                        || *width == 0
                        || *height == 0
                })
        })
        .map(|segment| segment.node_id)
        .collect();
    let roots = projection.roots().collect::<Vec<_>>();
    let mut segments = HashMap::<NodeId, Vec<usize>>::new();
    for (index, segment) in projection.segments().iter().enumerate() {
        segments
            .entry(segment.top_level_node_id)
            .or_default()
            .push(index);
    }
    let mut cache = HashMap::new();
    let presentation = plan.clone();
    crate::adaptive::rows::measure_rows(
        &mut plan,
        projection,
        width,
        viewport,
        previous,
        keep_arrangements,
        |range, width, cards| {
            let key = (range.start, range.end, width.to_bits(), cards);
            *cache.entry(key).or_insert_with(|| {
                measure_row_group(
                    projection,
                    &roots[range],
                    &segments,
                    width,
                    cards,
                    &resources,
                    &presentation,
                )
            })
        },
    );
    plan.measure_lists(
        projection,
        width,
        previous,
        keep_arrangements,
        |node, width, cards| {
            let segment = projection.segment_for_node(node)?;
            let block = projection.block(node)?;
            measurement.cached_list_item(projection, node, width, cards, || {
                let available = segment_text_width(
                    segment,
                    projection,
                    if cards {
                        width - CARD_PADDING * 2. + 8.
                    } else {
                        width.min(PROSE_WIDTH)
                    },
                );
                let breaks = card_presentation_breaks(projection, segment, cards);
                let mut ranges = Vec::new();
                for_each_display_line_range(
                    &projection.text()[segment.projection_range.clone()],
                    |local| {
                        let mut start = segment.projection_range.start + local.start;
                        let end = segment.projection_range.start + local.end;
                        for &offset in &breaks {
                            if offset > start && offset < end {
                                ranges.push(start..offset);
                                start = offset;
                            }
                        }
                        ranges.push(start..end);
                    },
                );
                let mut lines = 0;
                let mut height = 0.;
                let mut preferred = 0_f32;
                let mut overflow = false;
                for range in ranges {
                    let style = visual_line_style_for(projection, block, segment, &range, None);
                    preferred = preferred.max(measurement.line_width(
                        projection,
                        range.clone(),
                        style.font_size,
                    )?);
                    for line in
                        measurement.wrap(projection, segment, range, available, style.font_size)?
                    {
                        lines += 1;
                        height += style.line_height;
                        overflow |= measurement.line_width(projection, line, style.font_size)?
                            > available + 0.5;
                    }
                }
                Some(crate::adaptive::candidates::ItemMeasurement {
                    lines,
                    height: height + if cards { CARD_PADDING * 2. } else { 0. },
                    preferred_width: preferred + (width - available),
                    overflow,
                })
            })
        },
    );
    plan
}

/// Same native wraps, type sizes, presentation breaks and component padding as
/// final geometry. Unsupported or oversized groups do not enter row search.
fn measure_row_group(
    projection: &TextProjection,
    roots: &[&BlockNode],
    segments: &HashMap<NodeId, Vec<usize>>,
    width: f32,
    cards: bool,
    resources: &LayoutMeasurement<'_>,
    presentation: &AdaptivePlan,
) -> Option<crate::adaptive::rows::GroupMeasurement> {
    let key = GroupMeasureKey::new(
        projection,
        roots,
        segments,
        width,
        cards,
        resources,
        presentation,
    );
    resources.text.cached_group(key, || {
        measure_row_group_uncached(
            projection,
            roots,
            segments,
            width,
            cards,
            resources,
            presentation,
        )
    })
}

fn measure_row_group_uncached(
    projection: &TextProjection,
    roots: &[&BlockNode],
    segments: &HashMap<NodeId, Vec<usize>>,
    width: f32,
    cards: bool,
    resources: &LayoutMeasurement<'_>,
    presentation: &AdaptivePlan,
) -> Option<crate::adaptive::rows::GroupMeasurement> {
    use crate::adaptive::rows::GroupMeasurement;
    let measurement = resources.text;
    if roots.len() > 4
        || roots.iter().any(|root| {
            !matches!(
                root,
                BlockNode::Paragraph(_)
                    | BlockNode::Heading(_)
                    | BlockNode::List(_)
                    | BlockNode::CodeBlock(_)
                    | BlockNode::Table(_)
                    | BlockNode::Image(_)
            )
        })
    {
        return None;
    }
    let mut result = GroupMeasurement::default();
    let mut segment_count = 0;
    for (root_index, root) in roots.iter().enumerate() {
        let indexes = segments.get(&root.id())?;
        if !cards && root_index > 0 {
            result.height += presentation.gap_between(roots[root_index - 1], root);
        }
        if let BlockNode::Image(image) = root {
            if cards || indexes.len() != 1 {
                return None;
            }
            let empty = HashMap::new();
            let dimensions = resources.images.unwrap_or(&empty);
            let (intrinsic_width, _) = resolved_image_dimensions(image, dimensions)?;
            let segment = &projection.segments()[indexes[0]];
            // Same uncropped, non-upscaled footprint as final geometry. Outer
            // figure margins are handled by the shared relationship gap pass.
            result.height += image_reserved_height(projection, root, segment, dimensions, width)?;
            result.preferred_width = result.preferred_width.max(intrinsic_width as f32 + 16.);
            segment_count += 1;
            continue;
        }
        if matches!(root, BlockNode::Table(_)) {
            if cards {
                return None;
            }
            let table = measure_table_group(projection, root.id(), indexes, width, measurement)?;
            result.height += table.height;
            result.preferred_width = result.preferred_width.max(table.preferred_width);
            result.overflow |= table.overflow;
            segment_count += indexes.len();
            continue;
        }
        if indexes.len() > 12 {
            return None;
        }
        for index in indexes {
            let segment = &projection.segments()[*index];
            let block = projection.block(segment.node_id)?;
            let text = block.text()?;
            if text.len() > 4096 || crate::math::is_math(block) {
                return None;
            }
            let outer = if !cards
                && matches!(block, BlockNode::Paragraph(_))
                && segment.context.table_cell.is_none()
            {
                width.min(PROSE_WIDTH)
            } else {
                width
            };
            let inner = if cards {
                outer - 2. * CARD_PADDING + 8.
            } else {
                outer
            } - if presentation
                .lists
                .get(&segment.top_level_node_id)
                .is_some_and(|list| list.layout == ListLayout::Steps)
            {
                8.
            } else {
                0.
            };
            let lead = presentation.lead == Some(segment.node_id);
            let available = segment_text_width(segment, projection, inner);
            let breaks = card_presentation_breaks(projection, segment, cards);
            let lines = build_visual_lines_for_segment(
                projection,
                segment,
                &HashMap::new(),
                inner,
                &breaks,
                Some(measurement),
                lead.then_some(20.),
            );
            for line in &lines {
                let shaped = measurement.line_width(
                    projection,
                    line.range.clone(),
                    if lead { 20. } else { line.style.font_size },
                )?;
                result.overflow |= shaped > available + 0.5;
                result.height += if lead { 31. } else { line.style.line_height };
                if !cards {
                    result.height += line.style.space_above + line.style.space_below;
                }
            }
            let mut preferred = 0_f32;
            let mut valid = true;
            for_each_display_line_range(
                &projection.text()[segment.projection_range.clone()],
                |range| {
                    let range = segment.projection_range.start + range.start
                        ..segment.projection_range.start + range.end;
                    let style = visual_line_style_for(projection, block, segment, &range, None);
                    // Prose's preferred measure is not its entire unwrapped
                    // paragraph: ordinary wrapping must not itself be scored
                    // as discomfort. Shape up to 60 real graphemes as the
                    // initial prose target; technical lines remain intrinsic.
                    let range = if matches!(block, BlockNode::Paragraph(_)) {
                        let end = projection.text()[range.clone()]
                            .grapheme_indices(true)
                            .nth(60)
                            .map_or(range.end, |(offset, _)| range.start + offset);
                        range.start..end
                    } else {
                        range
                    };
                    if let Some(w) = measurement.line_width(
                        projection,
                        range,
                        if lead { 20. } else { style.font_size },
                    ) {
                        preferred = preferred.max(w);
                    } else {
                        valid = false;
                    }
                },
            );
            if !valid {
                return None;
            }
            result.preferred_width = result.preferred_width.max(preferred + outer - available);
            if cards {
                if segment_count > 0 {
                    result.height += 4.;
                }
                if matches!(block, BlockNode::Heading(_)) {
                    result.height += 4.;
                }
            } else {
                // Remove only outer margins, as apply_group_spacing does;
                // retain code/header insets and inter-item list spacing.
                if Some(index) == indexes.first() {
                    result.height -= match block {
                        BlockNode::Heading(_) => lines.first()?.style.space_above,
                        BlockNode::CodeBlock(_) => 8.,
                        _ => 0.,
                    };
                }
                if Some(index) == indexes.last() {
                    result.height -= if matches!(block, BlockNode::Heading(_)) {
                        7.
                    } else {
                        16.
                    };
                }
            }
            segment_count += 1;
        }
    }
    if cards {
        result.height += 2. * CARD_PADDING;
    }
    Some(result)
}

fn measure_table_group(
    projection: &TextProjection,
    id: NodeId,
    indexes: &[usize],
    width: f32,
    measurement: &FontMeasurement,
) -> Option<crate::adaptive::rows::GroupMeasurement> {
    let measured = projection.table_measurements(id)?;
    let available = projection.table_available_width(id, width);
    let widths = projection.fitted_table_widths(id, available)?;
    let mut lines = Vec::new();
    for &index in indexes {
        lines.extend(build_visual_lines_for_segment(
            projection,
            &projection.segments()[index],
            &HashMap::new(),
            width,
            &[],
            Some(measurement),
            None,
        ));
    }
    let height = position_visual_lines(&mut lines, projection, width, 0.);
    let mut overflow = widths.iter().sum::<f32>() > width + 0.5;
    for line in &lines {
        let available = line.width_fraction * width - line.inset - 12.;
        overflow |= measurement.line_width(projection, line.range.clone(), line.style.font_size)?
            > available + 0.5;
    }
    Some(crate::adaptive::rows::GroupMeasurement {
        height,
        preferred_width: measured.preferred.iter().sum(),
        overflow,
    })
}

pub(super) fn build_arranged_visual_lines(
    projection: &TextProjection,
    image_dimensions: &NodeImageDimensions,
    width: f32,
    plan: &AdaptivePlan,
) -> Vec<VisualLineSpec> {
    build_measured_visual_lines(projection, image_dimensions, width, plan, None)
}

pub(super) fn build_measured_visual_lines(
    projection: &TextProjection,
    image_dimensions: &NodeImageDimensions,
    width: f32,
    plan: &AdaptivePlan,
    measurement: Option<&FontMeasurement>,
) -> Vec<VisualLineSpec> {
    build_measured_visual_lines_for_segments(
        projection,
        image_dimensions,
        width,
        plan,
        measurement,
        0..projection.segments().len(),
    )
}

/// The same renderer geometry for a complete source-order row. Global segment
/// indexes are retained so card boundaries, authored ranges and list semantics
/// agree with a full rebuild; callers restore the row's external leading gap.
pub(super) fn build_measured_visual_lines_for_segments(
    projection: &TextProjection,
    image_dimensions: &NodeImageDimensions,
    width: f32,
    plan: &AdaptivePlan,
    measurement: Option<&FontMeasurement>,
    segments: Range<usize>,
) -> Vec<VisualLineSpec> {
    build_visual_lines_with_extensions(
        projection,
        image_dimensions,
        width,
        plan,
        measurement,
        segments,
        None,
    )
}

pub(super) fn build_visual_lines_with_extensions(
    projection: &TextProjection,
    image_dimensions: &NodeImageDimensions,
    width: f32,
    plan: &AdaptivePlan,
    measurement: Option<&FontMeasurement>,
    segments: Range<usize>,
    mut extensions: Option<&mut geometry_cache::ExtensionReuse<'_>>,
) -> Vec<VisualLineSpec> {
    let width = width.max(1.);
    let mut lines = Vec::with_capacity(segments.len());
    for (local, segment) in projection.segments()[segments.clone()].iter().enumerate() {
        let segment_index = segments.start + local;
        let Some(block) = projection.block(segment.node_id) else {
            continue;
        };
        let slot = plan.slots.get(&segment.node_id).copied();
        let wide = matches!(
            block,
            BlockNode::CodeBlock(_) | BlockNode::Image(_) | BlockNode::Heading(_)
        ) || segment.context.table_cell.is_some();
        let measure = PROSE_WIDTH;
        let segment_width = slot.map_or_else(
            || if wide { width } else { width.min(measure) },
            |slot| slot.width(width),
        );
        let steps = plan
            .lists
            .get(&segment.top_level_node_id)
            .is_some_and(|list| list.layout == ListLayout::Steps);
        let content_width = segment_width
            - if slot.is_some_and(|slot| slot.cards) {
                // The segment helper already subtracts the ordinary 8 px
                // trailing inset. Cards replace it with symmetric padding.
                CARD_PADDING * 2. - 8.
            } else {
                0.
            }
            - if steps { 8. } else { 0. };
        let breaks =
            card_presentation_breaks(projection, segment, slot.is_some_and(|slot| slot.cards));
        let segment_measurement = measurement.filter(|_| {
            let measured = plan.has_measured_geometry(segment.top_level_node_id);
            if !measured {
                diagnostics::count(|counts| counts.deferred_text_segments += 1);
            }
            measured
        });
        let font = (plan.lead == Some(segment.node_id)).then_some(20.);
        let mut node_lines = if let Some(reuse) = extensions.as_deref_mut()
            && (matches!(block, BlockNode::PreservedSource { .. }) || crate::math::is_math(block))
        {
            reuse.segment(
                projection,
                segment,
                image_dimensions,
                content_width.max(1.),
                &breaks,
                segment_measurement,
                font,
            )
        } else {
            build_visual_lines_for_segment(
                projection,
                segment,
                image_dimensions,
                content_width.max(1.),
                &breaks,
                segment_measurement,
                font,
            )
        };
        let count = node_lines.len();
        for (index, line) in node_lines.iter_mut().enumerate() {
            if plan
                .lists
                .get(&segment.top_level_node_id)
                .is_some_and(|list| list.layout == ListLayout::Steps)
            {
                // Keep the full number badge inside the document's clip and
                // leave a little breathing room before the instruction.
                line.inset += 8.;
            }
            line.slot = slot;
            if segment.context.table_cell.is_none() {
                line.width_fraction = segment_width / width;
            }
            if let Some(slot) = slot {
                line.x_fraction = slot.left(width) / width;
                if slot.cards {
                    line.inset += CARD_PADDING;
                    let first = segment_index
                        .checked_sub(1)
                        .and_then(|i| projection.segments().get(i))
                        .and_then(|s| plan.slots.get(&s.node_id))
                        != Some(&slot);
                    let last = projection
                        .segments()
                        .get(segment_index + 1)
                        .and_then(|s| plan.slots.get(&s.node_id))
                        != Some(&slot);
                    let heading = matches!(block, BlockNode::Heading(_));
                    line.style.space_above = if index == 0 && first {
                        CARD_PADDING
                    } else {
                        0.
                    };
                    line.style.space_below = if index + 1 == count {
                        if last {
                            CARD_PADDING
                        } else if heading {
                            8.
                        } else {
                            4.
                        }
                    } else {
                        0.
                    };
                }
            }
            if plan.lead == Some(segment.node_id) {
                line.style.font_size = 20.;
                line.style.line_height = line.style.line_height.max(31.);
                if index + 1 == count {
                    line.style.space_below = 24.;
                }
            }
            if let Some(list) = plan.lists.get(&segment.top_level_node_id)
                && list.layout == ListLayout::Checklist
                && segment.node_id == list.first_node
                && index == 0
            {
                line.style.space_above += LAYOUT_HEADER;
            }
        }
        lines.extend(node_lines);
    }
    apply_group_spacing(&mut lines, projection, plan);
    position_visual_lines(&mut lines, projection, width, 0.);
    lines
}

/// Separate outside whitespace from component insets. In particular, table
/// spacing must move the whole row, not stretch just its first cell/background.
fn apply_group_spacing(
    lines: &mut [VisualLineSpec],
    projection: &TextProjection,
    plan: &AdaptivePlan,
) {
    let mut start = 0;
    let mut previous = None;
    let mut previous_slot = None;
    while start < lines.len() {
        let Some(segment) = projection.segment_for_range(&lines[start].range) else {
            break;
        };
        let root_id = segment.top_level_node_id;
        let Some(root) = projection.block(root_id) else {
            break;
        };
        let end = lines[start..]
            .iter()
            .position(|line| {
                projection
                    .segment_for_range(&line.range)
                    .is_none_or(|segment| segment.top_level_node_id != root_id)
            })
            .map_or(lines.len(), |offset| start + offset);
        let slot = lines[start].slot;
        let continues_slot = slot.is_some_and(|slot: crate::adaptive::LayoutSlot| {
            previous_slot
                .is_some_and(|previous: crate::adaptive::LayoutSlot| previous.group == slot.group)
        });
        if !continues_slot
            || slot
                .zip(previous_slot)
                .is_some_and(|(slot, previous)| !slot.cards && slot.same_column(previous))
        {
            lines[start].gap_before = previous.map_or(
                if matches!(root, BlockNode::Heading(_)) {
                    8.
                } else {
                    0.
                },
                |before| plan.gap_between(before, root),
            );
        }
        if slot.is_none_or(|slot| !slot.cards) {
            // These were the renderer's old per-block external margins. Keep
            // code header/padding, quote/alert insets and table cell padding.
            let first = &mut lines[start];
            if first.table_cell.is_none() {
                let first_segment = projection.segment_for_range(&first.range).unwrap();
                let first_block = projection.block(first_segment.node_id).unwrap();
                let external = match first_block {
                    BlockNode::Heading(_) => {
                        if first.range.start == 0 {
                            8.
                        } else {
                            20.
                        }
                    }
                    BlockNode::CodeBlock(_) | BlockNode::Image(_) => 8.,
                    _ => 0.,
                };
                first.style.space_above = (first.style.space_above - external).max(0.);
            }
            let last = &mut lines[end - 1];
            if last.table_cell.is_none() {
                let last_segment = projection.segment_for_range(&last.range).unwrap();
                let last_block = projection.block(last_segment.node_id).unwrap();
                let external = if matches!(last_block, BlockNode::Heading(_)) {
                    7.
                } else if plan.lead == Some(last_segment.node_id) {
                    24.
                } else {
                    16.
                };
                last.style.space_below = (last.style.space_below - external).max(0.);
            }
        }
        previous = Some(root);
        previous_slot = slot;
        start = end;
    }
}

impl RichDocumentEditor {
    pub(super) fn render_task_summaries(
        &self,
        visible: &[usize],
        palette: MineralPalette,
    ) -> Vec<AnyElement> {
        let mut seen = HashSet::new();
        visible
            .iter()
            .filter_map(|index| {
                let line = &self.visual_lines[*index];
                let segment = self.projection.segment_for_range(&line.range)?;
                let id = segment.top_level_node_id;
                let list = self.adaptive.lists.get(&id)?;
                if list.layout != ListLayout::Checklist || !seen.insert(id) {
                    return None;
                }
                let first = &self.visual_lines[self.components.get(&id)?.first_line];
                Some(
                    div()
                        .absolute()
                        .top(px(first.y - LAYOUT_HEADER * self.zoom_factor))
                        .left(px(first.x_fraction * self.layout_width
                            + if first.slot.is_some_and(|slot| slot.cards) {
                                CARD_PADDING * self.zoom_factor
                            } else {
                                0.
                            }))
                        .text_size(px(12. * self.zoom_factor))
                        .text_color(rgb(palette.secondary))
                        .child(format!("{} of {} complete", list.completed, list.count))
                        .into_any_element(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::adaptive::rows::RowKind;

    pub(in crate::editor) fn assert_same_geometry(a: &[VisualLineSpec], b: &[VisualLineSpec]) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.range, b.range);
            let metrics = |line: &VisualLineSpec| {
                [
                    line.y,
                    line.inset,
                    line.gap_before,
                    line.x_fraction,
                    line.width_fraction,
                    line.table_row_y,
                    line.table_row_height,
                    line.style.font_size,
                    line.style.line_height,
                    line.style.space_above,
                    line.style.space_below,
                ]
            };
            assert_eq!(metrics(a), metrics(b), "range {:?}", a.range);
            assert_eq!(
                (a.slot, a.table_cell, a.table_cell_first),
                (b.slot, b.table_cell, b.table_cell_first)
            );
            assert_eq!(
                a.html_preview
                    .as_ref()
                    .map(|p| (&p.source, p.width, p.height, &p.editable_text)),
                b.html_preview
                    .as_ref()
                    .map(|p| (&p.source, p.width, p.height, &p.editable_text))
            );
            assert_eq!(a.display_math.is_some(), b.display_math.is_some());
            if let Some((a, b)) = a.display_math.as_ref().zip(b.display_math.as_ref()) {
                for (a, b) in [(&a.light, &b.light), (&a.dark, &b.dark)] {
                    assert_eq!(
                        [a.width, a.height, a.baseline],
                        [b.width, b.height, b.baseline]
                    );
                    assert_eq!(a.image.bytes, b.image.bytes);
                }
            }
            assert_eq!(a.inline_math.is_some(), b.inline_math.is_some());
            if let Some((a, b)) = a.inline_math.as_ref().zip(b.inline_math.as_ref()) {
                assert_eq!(a.range, b.range);
                assert_eq!(
                    [a.width, a.ascent, a.descent, a.font_size],
                    [b.width, b.ascent, b.descent, b.font_size]
                );
                assert_eq!(a.attachments.len(), b.attachments.len());
                for (a, b) in a.attachments.iter().zip(&b.attachments) {
                    assert_eq!(a.range, b.range);
                    assert_eq!(
                        [
                            a.x,
                            a.light.width,
                            a.light.height,
                            a.dark.width,
                            a.dark.height
                        ],
                        [
                            b.x,
                            b.light.width,
                            b.light.height,
                            b.dark.width,
                            b.dark.height
                        ]
                    );
                }
            }
        }
    }

    #[gpui::test]
    fn initial_view_stacks_lists_until_native_measurement(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = "# Directions\n\nSix independent directions.\n\n1. North\n2. South\n3. East\n4. West\n5. Above\n6. Below\n";
            let document = Document::from_markdown(source).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            assert!(initial.adaptive.slots.is_empty(), "initial geometry must not publish a guessed grid");
            assert!(initial.visual_lines.iter().all(|line| line.slot.is_none()));
            assert!(initial.visual_lines.windows(2).all(|pair| pair[0].y < pair[1].y));
            assert_eq!(initial.adaptive.lists.values().next().unwrap().layout, ListLayout::List);
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let editing = initial.projection.segments().iter().find(|s| s.context.list_marker.is_some()).unwrap().node_id;
            let focused = build_edit_locked_adaptive_plan(&initial.projection, 1280., 1000., Some(&initial.adaptive), false, &measurement, Some(editing));
            assert_eq!(focused.lists.values().next().unwrap().layout, ListLayout::List,
                "first measurement must not rearrange a list already being edited");
            assert!(focused.slots.is_empty());
            let measured = build_measured_adaptive_plan(&initial.projection, 1280., 1000., Some(&initial.adaptive), false, &measurement);
            assert_eq!(measured.lists.values().next().unwrap().layout, ListLayout::Grid(3));
            assert!(measured.measured_lists.values().all(|decision| decision.candidates.iter().any(|candidate| candidate.columns == 3 && candidate.rejected.is_none())));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn measured_list_resize_and_live_breaks_preserve_the_correct_topology(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "1. North\n2. South\n3. East\n4. West\n5. Above\n6. Below\n";
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let wide =
                build_measured_adaptive_plan(&projection, 1280., 1000., None, false, &measurement);
            assert_eq!(
                wide.lists.values().next().unwrap().layout,
                ListLayout::Grid(3)
            );
            let narrow = build_measured_adaptive_plan(
                &projection,
                696.,
                1000.,
                Some(&wide),
                false,
                &measurement,
            );
            assert_eq!(
                narrow.lists.values().next().unwrap().layout,
                ListLayout::Grid(2),
                "hysteresis must not retain cramped columns"
            );
            let first = projection.segments()[0].node_id;
            document
                .apply(EditCommand::ReplaceText {
                    node_id: first,
                    range: 5..5,
                    text: "\nmore detail".into(),
                    selection_after: None,
                    typing: false,
                })
                .unwrap();
            let changed = TextProjection::from_snapshot(&document.snapshot());
            let live = build_edit_locked_adaptive_plan(
                &changed,
                1280.,
                1000.,
                Some(&wide),
                true,
                &measurement,
                Some(first),
            );
            assert_eq!(
                live.lists.values().next().unwrap().layout,
                ListLayout::Grid(3),
                "a live text break must not dissolve the focused measured grid"
            );
            let mut slots = live
                .slots
                .values()
                .map(|slot| (slot.item, slot.columns))
                .collect::<Vec<_>>();
            slots.sort();
            assert_eq!(slots, (0..6).map(|item| (item, 3)).collect::<Vec<_>>());
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn retained_segment_geometry_matches_uncached_renderer_across_inputs(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "# A title\n\nBefore.\n\nText with **weight**, café and $x^2 + 1$.\n\n> A quoted paragraph.\n\n1. **First:** One action.\n2. Next action.\n\n| Key | Value |\n| --- | --- |\n| Name | café |\n\n![Diagram](diagram.png)\n\n<div><p>HTML with <strong>weight</strong>.</p></div>\n\n$$\nx^2 + y^2\n$$\n";
            let mut document = Document::from_markdown(source).unwrap();
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let check = |snapshot: &DocumentSnapshot, width: f32, image_size, math_edit: bool, measurement: &FontMeasurement| {
                let mut projection = TextProjection::from_snapshot(snapshot);
                measurement.measure_tables(&mut projection);
                if math_edit {
                    projection.math_edit_node = projection.segments().iter().find(|s| inline_math::has_math(&projection,s.node_id)).map(|s| s.node_id);
                }
                let images = projection.image_segments().map(|s| (s.node_id,(s.context.image_source.clone().unwrap(),image_size))).collect();
                for segment in projection.segments() {
                    let breaks = card_presentation_breaks(&projection,segment,true);
                    for font in [None,Some(20.)] {
                        let actual = build_visual_lines_for_segment(&projection,segment,&images,width,&breaks,Some(measurement),font);
                        let reference = build_visual_lines_for_segment_uncached(&projection,segment,&images,width,&breaks,Some(measurement),font);
                        assert_same_geometry(&actual,&reference);
                    }
                    // An unchanged leaf can acquire a different container,
                    // indentation, or first/last-quote role after a structural edit.
                    let mut nested = segment.clone();
                    nested.context.quote_depth += 1;
                    nested.context.quote_first = true;
                    nested.context.quote_last = true;
                    let actual = build_visual_lines_for_segment(&projection,&nested,&images,width,&[],Some(measurement),None);
                    let reference = build_visual_lines_for_segment_uncached(&projection,&nested,&images,width,&[],Some(measurement),None);
                    assert_same_geometry(&actual,&reference);
                }
            };
            for _ in 0..2 { check(&document.snapshot(),760.,(400,240),false,&measurement); }
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let before = projection.segments().iter().find(|s| &projection.text()[s.projection_range.clone()] == "Before.").unwrap().node_id;
            document.apply(EditCommand::ReplaceText { node_id: before, range: 0..0, text: "東京 😀 added before retained math and HTML ".into(), selection_after: None, typing: false }).unwrap();
            check(&document.snapshot(),760.,(400,240),false,&measurement);
            check(&document.snapshot(),759.75,(400,240),false,&measurement);
            check(&document.snapshot(),760.,(400,900),true,&measurement);
            check(&document.snapshot(),760.,(400,900),false,&measurement);
            let zoomed = FontMeasurement::new(cx.text_system().clone(),"Spline Sans Mineral".into(),2.);
            check(&document.snapshot(),380.,(400,900),false,&zoomed);
            document.undo().unwrap();
            check(&document.snapshot(),760.,(400,240),false,&measurement);
            assert_eq!(document.snapshot().serialize().unwrap(),source);
        });
    }

    #[gpui::test]
    fn warm_long_document_geometry_does_not_rewrap_unchanged_segments(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/17-long-layout-spec.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            measurement.measure_tables(&mut projection);
            let plan =
                build_measured_adaptive_plan(&projection, 1100., 1000., None, false, &measurement);
            let first = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1100.,
                &plan,
                Some(&measurement),
            );
            let scope = diagnostics::MeasurementScope::new();
            let second = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1100.,
                &plan,
                Some(&measurement),
            );
            let counts = scope.take_stage();
            assert_eq!(
                counts.wrap_requests, 0,
                "unchanged final geometry must not revisit native wraps: {counts:?}"
            );
            assert_eq!(counts.shaping_calls, 0);
            assert_same_geometry(&first, &second);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn scoped_reflow_measures_only_requested_chapters_and_retains_completed_rows(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = (0..20).map(|chapter| format!(
                "## Chapter {chapter}\n\n### Properties\n\n| Key | Value |\n| --- | --- |\n| Timeout | 30 s |\n| Workers | 4 |\n\n### Comparison\n\n| Environment | Endpoint | Retry |\n| --- | --- | ---: |\n| Local | localhost | 2 |\n| Staging | staging.example.test | 3 |\n\n### Features\n\n- **First:** Clear.\n- **Second:** Stable.\n- **Third:** Local.\n- **Fourth:** Safe.\n- **Fifth:** Readable.\n- **Sixth:** Complete.\n\n"
            )).collect::<String>();
            let document = Document::from_markdown(source.as_str()).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let windows = initial.adaptive.windows.clone();
            assert_eq!(windows.len(), 20);
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let prepare = |scope, previous: &AdaptivePlan, width, measurement: &FontMeasurement| {
                PreparedDocumentView::prepare_snapshot_with_images(&document.snapshot(), &HashMap::new(), None,
                    ReflowViewport { published_geometry: None, width, height: 1000., zoom: 1., math_edit_node: None, editing_node: None, table_layout_lock: None, html_disclosures: Arc::default(), html_loaded_images: Arc::default(), trace_mode: LayoutTraceMode::Off, visible_roots: scope, resource_generation: 0 }, previous, measurement).0
            };
            let counts = diagnostics::MeasurementScope::new();
            let mut prepared = prepare(Some(windows[0].clone()), &initial.adaptive, 1100., &measurement);
            let measured = counts.take_stage();
            assert_eq!(measured.tables_measured, 2, "offscreen tables must not be measured");
            assert!(measured.shaping_calls < 200,
                "one requested chapter must not shape text throughout twenty chapters: {measured:?}");
            assert!(measured.item_requests > 0 && measured.item_requests <= 30, "{measured:?}");
            assert_eq!(prepared.adaptive.measured_rows.windows, 1);
            assert_eq!(prepared.adaptive.measured_rows.deferred_windows, 19);
            assert!(prepared.adaptive.covers(&windows[0]));
            assert!(!prepared.adaptive.covers(&windows[1]));
            assert!(prepared.adaptive.measured_rows.chosen.iter().filter(|row| row.roots.start >= windows[0].end).all(|row| row.kind == RowKind::Stack && row.height_estimated));
            let first_slots = prepared.adaptive.slots.clone();
            prepared = prepare(Some(windows[1].clone()), &prepared.adaptive, 1100., &measurement);
            assert_eq!(counts.take_stage().tables_measured, 2);
            assert!(prepared.adaptive.covers(&windows[0]) && prepared.adaptive.covers(&windows[1]));
            for (node, slot) in first_slots { assert_eq!(prepared.adaptive.slots.get(&node), Some(&slot)); }
            for window in &windows[2..] {
                prepared = prepare(Some(window.clone()), &prepared.adaptive, 1100., &measurement);
                assert_eq!(prepared.adaptive.measured_rows.windows, 1);
                assert!(prepared.adaptive.measured_rows.chosen.iter().flat_map(|row| row.roots.clone()).eq(0..prepared.projection.roots().count()));
            }
            assert!(windows.iter().all(|window| prepared.adaptive.covers(window)));
            let fresh = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let oracle = prepare(None, &initial.adaptive, 1100., &fresh);
            assert_eq!(prepared.adaptive.slots, oracle.adaptive.slots);
            let geometry = |view: &PreparedDocumentView| view.visual_lines.iter().map(|line| (line.range.clone(), line.y, line.inset, line.x_fraction, line.width_fraction, line.style.line_height)).collect::<Vec<_>>();
            assert_eq!(geometry(&prepared), geometry(&oracle));
            let resized = prepare(Some(windows[19].clone()), &prepared.adaptive, 700., &measurement);
            assert!(!resized.adaptive.covers(&windows[0]));
            assert!(resized.adaptive.measured_rows.chosen.iter().filter(|row| row.roots.start < windows[19].start).all(|row| row.kind == RowKind::Stack));
            let font_changed = prepare(Some(windows[19].clone()), &prepared.adaptive, 1100., &fresh);
            assert!(!font_changed.adaptive.covers(&windows[0]));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn rich_code_cells_reserve_the_header_and_panel_padding_inside_the_row(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/36-rich-cell-panels.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            measurement.measure_tables(&mut projection);
            let plan = AdaptivePlan::build(&projection, 1000., None, false);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1000.,
                &plan,
                Some(&measurement),
            );
            for segment in projection.segments().iter().filter(|segment| {
                matches!(
                    projection.block(segment.node_id),
                    Some(BlockNode::CodeBlock(_))
                )
            }) {
                let first = lines
                    .iter()
                    .find(|line| line.range.start == segment.projection_range.start)
                    .unwrap();
                let last = lines
                    .iter()
                    .find(|line| line.range.end == segment.projection_range.end)
                    .unwrap();
                assert_eq!(first.inset, 12. + CODE_BLOCK_PADDING);
                assert!(
                    (first.y - first.table_row_y - (10. + CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING))
                        .abs()
                        < 0.01,
                    "cell code header must stay inside row, not paint over its predecessor"
                );
                assert!(
                    last.y + last.style.line_height + CODE_BLOCK_PADDING + 10.
                        <= last.table_row_y + last.table_row_height + 0.01
                );
                assert!(
                    projection
                        .table_measurements(segment.context.table_cell.unwrap().0)
                        .is_some(),
                    "code cells need measured column constraints"
                );
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn rich_html_cells_keep_their_table_geometry_and_disclosure_state(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/36-rich-cell-panels.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let plan = AdaptivePlan::build(&projection, 1000., None, false);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1000.,
                &plan,
                Some(&measurement),
            );
            let previews = lines
                .iter()
                .filter(|line| line.html_preview.is_some())
                .collect::<Vec<_>>();
            assert_eq!(previews.len(), 2);
            for (index, line) in previews.into_iter().enumerate() {
                assert!(
                    line.table_cell.is_some(),
                    "HTML preview must not escape the table row"
                );
                assert_eq!(line.inset, 12.);
                assert!((line.y - line.table_row_y - 10.).abs() < 0.01);
                let preview = line.html_preview.as_ref().unwrap();
                assert_eq!(preview.disclosures[0].open, index == 0);
                assert!(preview.width + 24. <= line.width_fraction * 1000. + 0.01);
                assert!(
                    line.y + line.style.line_height + 10.
                        <= line.table_row_y + line.table_row_height + 0.01
                );
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn table_ancestor_edges_are_distinct_from_cell_container_edges(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/35-rich-table-cells.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            measurement.measure_tables(&mut projection);
            let plan = AdaptivePlan::build(&projection, 1000., None, false);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1000.,
                &plan,
                Some(&measurement),
            );
            let segment = projection
                .segments()
                .iter()
                .find(|segment| {
                    &projection.text()[segment.projection_range.clone()]
                        == "Nested quote in a quoted table."
                })
                .unwrap();
            assert_eq!(table_insets(segment, &projection), (24., 24.));
            let edges = projection.container_edges(segment.node_id).unwrap();
            assert_eq!(edges.starts.len(), 1);
            assert_eq!(
                edges.ends.len(),
                2,
                "both quote endings must remain distinct"
            );
            let line = lines
                .iter()
                .find(|line| line.range == segment.projection_range)
                .unwrap();
            assert_eq!(line.inset, 36.);
            assert_eq!(table_outer_spacing(line, &projection), (0., 16.));
            assert_eq!(
                line.style.space_below, 42.,
                "cell owns 10px plus two distinct quote endings"
            );
            assert!((line.y - line.table_row_y - 26.).abs() < 0.01);
            let cell = segment.context.table_cell.unwrap();
            let sibling = lines
                .iter()
                .find(|line| {
                    line.table_cell.is_some_and(|(table, row, column, _)| {
                        table == cell.0 && row == cell.1 && column == 0
                    })
                })
                .unwrap();
            assert!((sibling.y - sibling.table_row_y - 10.).abs() < 0.01);
            assert!((sibling.x_fraction * 1000. - 24.).abs() < 0.01);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn ordered_cell_and_table_ancestors_keep_the_number_rail_gap(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/35-rich-table-cells.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            measurement.measure_tables(&mut projection);
            let plan = AdaptivePlan::build(&projection, 1000., None, false);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1000.,
                &plan,
                Some(&measurement),
            );
            for (text, outer) in [
                ("First numbered cell item", 0.),
                ("Numbered inside and outside.", 32.),
            ] {
                let segment = projection
                    .segments()
                    .iter()
                    .find(|segment| &projection.text()[segment.projection_range.clone()] == text)
                    .unwrap();
                assert_eq!(
                    table_insets(segment, &projection),
                    (outer, 32.),
                    "{text}: ordered rails need their extra gap"
                );
                let line = lines
                    .iter()
                    .find(|line| line.range == segment.projection_range)
                    .unwrap();
                // The outer ordered list also has its explicit Steps padding;
                // this is separate from the number rail's intrinsic spacing.
                let steps_padding = if plan
                    .lists
                    .get(&segment.top_level_node_id)
                    .is_some_and(|list| list.layout == ListLayout::Steps)
                {
                    8.
                } else {
                    0.
                };
                assert_eq!(line.inset, 44. + steps_padding);
                assert!((line.x_fraction * 1000. - outer).abs() < 0.01);
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn rich_cell_containers_do_not_indent_or_pad_the_whole_table(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/35-rich-table-cells.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            measurement.measure_tables(&mut projection);
            let width = 1000.;
            let plan = AdaptivePlan::build(&projection, width, None, false);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                width,
                &plan,
                Some(&measurement),
            );
            let table = projection
                .roots()
                .find(|root| matches!(root, BlockNode::Table(_)))
                .expect("HTML imports a typed rich table")
                .id();
            for (text, extra) in [
                ("Quoted cell text.", 24.),
                ("First cell item", 24.),
                ("Cell notice text.", 24.),
            ] {
                let segment = projection
                    .segments()
                    .iter()
                    .find(|segment| &projection.text()[segment.projection_range.clone()] == text)
                    .unwrap();
                assert_eq!(segment.context.table_cell.unwrap().0, table);
                let line = lines
                    .iter()
                    .find(|line| line.range == segment.projection_range)
                    .unwrap();
                assert_eq!(
                    line.inset,
                    12. + extra,
                    "{text}: container indent stays inside the cell"
                );
                assert_eq!(
                    line.x_fraction, 0.,
                    "{text}: the first column must not shift"
                );
                let row = line.table_cell.unwrap().1;
                let neighbor = lines
                    .iter()
                    .find(|line| {
                        line.table_cell.is_some_and(|(id, r, column, _)| {
                            id == table && r == row && column == 1
                        })
                    })
                    .unwrap();
                assert!(
                    (neighbor.y - neighbor.table_row_y - 10.).abs() < 0.01,
                    "a sibling cell must not inherit its neighbor's quote/notice padding"
                );
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn container_spacing_stays_outside_table_rows(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!("../../../../performance/layout-fixtures/34-container-boundaries.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            measurement.measure_tables(&mut projection);
            let width = 1000.;
            let plan = AdaptivePlan::build(&projection, width, None, false);
            let mut lines = build_measured_visual_lines(&projection, &HashMap::new(), width, &plan, Some(&measurement));
            position_visual_lines(&mut lines, &projection, width, 0.);
            let mut tables = HashSet::new();
            for line in &lines {
                let Some((table, _, _, _)) = line.table_cell else { continue; };
                tables.insert(table);
                assert!((line.y - line.table_row_y - 10.).abs() < 0.01,
                    "every short cell starts at the same 10px inset, including the first: y={}, row={}", line.y, line.table_row_y);
                assert!((line.table_row_height - line.style.line_height - 20.).abs() < 0.01,
                    "container bottom/header spacing must not inflate one table row");
            }
            assert_eq!(tables.len(), 4);
            let components = component_geometry(&projection, &lines, width, &visual_line_paint_order(&lines));
            for table in tables {
                let segment = projection.segments().iter()
                    .find(|segment| segment.context.table_cell.is_some_and(|(id, _, _)| id == table)).unwrap();
                let owner = &components.bounds[&segment.top_level_node_id];
                let table_bounds = &components.bounds[&table];
                let (header, trailing) = if segment.context.alert.is_some() { (ALERT_HEADER_HEIGHT, 8.) } else { (16., 0.) };
                let preceding = &lines[owner.first_line - 1];
                assert!(owner.top - header >= preceding.y + preceding.style.line_height + 8.,
                    "the container must not grow backward over its preceding heading");
                assert!((owner.right_fraction - table_bounds.right_fraction) * width - trailing >= 12.,
                    "a compact container must also enclose the trailing table border");
            }
            let once = lines.clone();
            position_visual_lines(&mut lines, &projection, width, 0.);
            assert_same_geometry(&lines, &once);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn nested_tables_apply_container_inset_once_outside_the_cells(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/33-nested-tables.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            measurement.measure_tables(&mut projection);
            for width in [304., 1000.] {
                let plan = AdaptivePlan::build(&projection, width, None, false);
                let mut lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&measurement),
                );
                position_visual_lines(&mut lines, &projection, width, 0.);
                let mut checked = HashSet::new();
                for line in &lines {
                    let Some((table, _, column, _)) = line.table_cell else {
                        continue;
                    };
                    let segment = segment_for_line(&projection, &line.range).unwrap();
                    if table == segment.top_level_node_id {
                        continue;
                    }
                    checked.insert(table);
                    let inset = segment.context.list_depth as f32 * 24.
                        + segment.context.quote_depth as f32 * 24.
                        + f32::from(segment.context.alert.is_some()) * ALERT_CONTENT_INSET;
                    assert_eq!(
                        line.inset, 12.,
                        "ancestor indentation must not repeat inside cells"
                    );
                    if column == 0 {
                        assert!(
                            (line.x_fraction * width - inset).abs() < 0.01,
                            "table border must start inside its authored container"
                        );
                    }
                    let text_width = segment_text_width(segment, &projection, width);
                    assert!(
                        (text_width - (line.width_fraction * width - 24.).max(1.)).abs() < 0.01,
                        "measurement and painted cell must reserve the same padding"
                    );
                    let painted = measurement
                        .line_width(&projection, line.range.clone(), line.style.font_size)
                        .unwrap();
                    assert!(
                        painted <= text_width + 0.5,
                        "nested cell text must fit its own column"
                    );
                }
                assert_eq!(checked.len(), 3);
                let components = component_geometry(
                    &projection,
                    &lines,
                    width,
                    &visual_line_paint_order(&lines),
                );
                for line in &lines {
                    let Some((table, _, _, _)) = line.table_cell else {
                        continue;
                    };
                    let segment = segment_for_line(&projection, &line.range).unwrap();
                    if table == segment.top_level_node_id {
                        continue;
                    }
                    let owner = &components.bounds[&segment.top_level_node_id];
                    assert!(
                        owner.bottom >= line.table_row_y + line.table_row_height,
                        "container must enclose the bottom table border, not stop at cell text"
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn scoped_reflow_measures_tables_owned_by_nested_containers(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let original =
                include_str!("../../../../performance/layout-fixtures/33-nested-tables.md");
            let document = Document::from_markdown(original).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let nested: HashMap<_, _> = initial
                .projection
                .segments()
                .iter()
                .filter_map(|segment| {
                    let (table, _, _) = segment.context.table_cell?;
                    (table != segment.top_level_node_id)
                        .then_some((table, segment.top_level_node_id))
                })
                .collect();
            assert_eq!(
                nested.len(),
                3,
                "quote, list item and alert tables must be canonical tables"
            );
            for (table, owner) in nested {
                let ordinal = initial.adaptive.root_ordinal(owner).unwrap();
                let scope = initial.adaptive.windows_for(&(ordinal..ordinal + 1))[0].clone();
                let measurement = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Mineral".into(),
                    1.,
                );
                let counts = diagnostics::MeasurementScope::new();
                let prepared = PreparedDocumentView::prepare_snapshot_with_images(
                    &document.snapshot(),
                    &HashMap::new(),
                    None,
                    ReflowViewport {
                        published_geometry: None,
                        width: 1000.,
                        height: 800.,
                        zoom: 1.,
                        math_edit_node: None,
                        editing_node: None,
                        table_layout_lock: None,
                        html_disclosures: Arc::default(),
                        html_loaded_images: Arc::default(),
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: Some(scope.clone()),
                        resource_generation: 0,
                    },
                    &initial.adaptive,
                    &measurement,
                )
                .0;
                let measured = counts.take_stage();
                assert_eq!(
                    measured.tables_measured, 1,
                    "only the scoped nested table is measured: {measured:?}"
                );
                let actual = prepared
                    .projection
                    .table_measurements(table)
                    .expect("active descendant table must have exact native widths");
                let mut oracle = TextProjection::from_snapshot(&document.snapshot());
                measurement.measure_tables(&mut oracle);
                assert_eq!(actual, oracle.table_measurements(table).unwrap());
                assert!(prepared.adaptive.covers(&scope));
                assert_eq!(document.snapshot().serialize().unwrap(), original);
            }
        });
    }

    #[gpui::test]
    fn scoped_reflow_invalidates_offscreen_edits_and_includes_the_focused_window(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/12-measured-tables.md");
            let mut document = Document::from_markdown(source).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let prepare = |snapshot: &DocumentSnapshot,
                           previous: &AdaptivePlan,
                           scope,
                           focus,
                           lock,
                           generation| {
                PreparedDocumentView::prepare_snapshot_with_images(
                    snapshot,
                    &HashMap::new(),
                    None,
                    ReflowViewport {
                        published_geometry: None,
                        width: 1100.,
                        height: 1000.,
                        zoom: 1.,
                        math_edit_node: None,
                        editing_node: focus,
                        table_layout_lock: lock,
                        html_disclosures: Arc::default(),
                        html_loaded_images: Arc::default(),
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: scope,
                        resource_generation: generation,
                    },
                    previous,
                    &measurement,
                )
                .0
            };
            let mut before = prepare(&document.snapshot(), &initial.adaptive, None, None, None, 0);
            let cell = before
                .projection
                .segments()
                .iter()
                .find(|s| &before.projection.text()[s.projection_range.clone()] == "64 MiB")
                .unwrap();
            let node = cell.node_id;
            let root = cell.top_level_node_id;
            let ordinal = before.adaptive.root_ordinal(root).unwrap();
            let changed_window = before.adaptive.windows_for(&(ordinal..ordinal + 1))[0].clone();
            let last = before.adaptive.windows.last().unwrap().clone();
            assert_ne!(changed_window, last);
            let row = before
                .adaptive
                .measured_rows
                .chosen
                .iter()
                .find(|row| row.roots.contains(&ordinal))
                .unwrap()
                .clone();
            assert_ne!(row.kind, RowKind::Stack);
            before.projection.lock_table_for_node(Some(node));
            let lock = before.projection.table_layout_lock.clone();
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: 0..0,
                    text: "Longer 東京 value ".into(),
                    selection_after: None,
                    typing: false,
                })
                .unwrap();
            let counts = diagnostics::MeasurementScope::new();
            let deferred = prepare(
                &document.snapshot(),
                &before.adaptive,
                Some(last.clone()),
                None,
                None,
                0,
            );
            assert_eq!(counts.take_stage().tables_measured, 0);
            assert!(!deferred.adaptive.covers(&changed_window));
            assert_eq!(
                deferred
                    .adaptive
                    .measured_rows
                    .chosen
                    .iter()
                    .find(|row| row.roots.contains(&ordinal))
                    .unwrap()
                    .kind,
                RowKind::Stack
            );
            let focused = prepare(
                &document.snapshot(),
                &before.adaptive,
                Some(last.clone()),
                Some(node),
                lock,
                0,
            );
            assert!(focused.adaptive.covers(&changed_window));
            let retained = focused
                .adaptive
                .measured_rows
                .chosen
                .iter()
                .find(|row| row.roots.contains(&ordinal))
                .unwrap();
            assert!(retained.edit_locked);
            assert_eq!(retained.widths, row.widths);
            assert_eq!(retained.kind, row.kind);
            let changed_resources = prepare(
                &document.snapshot(),
                &before.adaptive,
                Some(last),
                None,
                None,
                1,
            );
            assert!(!changed_resources.adaptive.covers(&changed_window));
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn unchanged_candidate_groups_reuse_complete_measurements(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/12-measured-tables.md");
            let document = Document::from_markdown(source).unwrap();
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let build = || {
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                measurement.measure_tables(&mut projection);
                build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement)
            };
            let counts = diagnostics::MeasurementScope::new();
            let first = build();
            let cold = counts.take_stage();
            assert!(cold.wrap_requests > 0 && cold.intrinsic_requests > 0);
            let second = build();
            let warm = counts.take_stage();
            assert_eq!(
                warm.wrap_requests, 0,
                "unchanged group wraps are still revisited: {warm:?}"
            );
            assert_eq!(
                warm.intrinsic_requests, 0,
                "unchanged table/group widths are still revisited: {warm:?}"
            );
            let placements = |plan: &AdaptivePlan| {
                plan.measured_rows
                    .chosen
                    .iter()
                    .map(|row| {
                        (
                            row.roots.clone(),
                            row.template,
                            row.widths.clone(),
                            row.heights.clone(),
                            row.cost(),
                        )
                    })
                    .collect::<Vec<_>>()
            };
            assert_eq!(placements(&first), placements(&second));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn cached_groups_invalidate_only_changed_content_and_match_fresh_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/12-measured-tables.md");
            let mut document = Document::from_markdown(source).unwrap();
            let make_measurement =
                || FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let measurement = make_measurement();
            let build = |snapshot: &DocumentSnapshot, measurement: &FontMeasurement, width: f32| {
                let mut projection = TextProjection::from_snapshot(snapshot);
                measurement.measure_tables(&mut projection);
                let plan = build_measured_adaptive_plan(
                    &projection,
                    width,
                    800.,
                    None,
                    false,
                    measurement,
                );
                (projection, plan)
            };
            let counts = diagnostics::MeasurementScope::new();
            let (initial, _) = build(&document.snapshot(), &measurement, 1100.);
            let cold = counts.take_stage();
            let cell = initial
                .segments()
                .iter()
                .find(|s| s.context.table_cell.is_some())
                .unwrap()
                .node_id;
            document
                .apply(EditCommand::ReplaceText {
                    node_id: cell,
                    range: 0..0,
                    text: "長い😀 value ".into(),
                    selection_after: None,
                    typing: false,
                })
                .unwrap();
            let (cached_projection, cached_plan) = build(&document.snapshot(), &measurement, 1100.);
            let affected = counts.take_stage();
            assert_eq!(affected.tables_measured, 1, "{affected:?}");
            assert_eq!(affected.table_cache_hits, cold.tables_measured - 1);
            assert!(
                affected.groups_measured > 0 && affected.groups_measured < cold.groups_measured,
                "{cold:?} -> {affected:?}"
            );
            assert!(affected.group_cache_hits > 0);
            let fresh = make_measurement();
            let (fresh_projection, fresh_plan) = build(&document.snapshot(), &fresh, 1100.);
            let geometry = |projection: &TextProjection,
                            plan: &AdaptivePlan,
                            measurement: &FontMeasurement| {
                build_measured_visual_lines(
                    projection,
                    &HashMap::new(),
                    1100.,
                    plan,
                    Some(measurement),
                )
                .into_iter()
                .map(|line| {
                    (
                        line.range,
                        line.y,
                        line.inset,
                        line.width_fraction,
                        line.style.line_height,
                        line.slot,
                    )
                })
                .collect::<Vec<_>>()
            };
            assert_eq!(
                geometry(&cached_projection, &cached_plan, &measurement),
                geometry(&fresh_projection, &fresh_plan, &fresh)
            );
            counts.take_stage();
            build(&document.snapshot(), &measurement, 1099.75);
            assert!(
                counts.take_stage().groups_measured > 0,
                "exact width must invalidate footprints"
            );
            build(&document.snapshot(), &measurement, 1100.);
            assert_eq!(
                counts.take_stage().groups_measured,
                0,
                "the original width should still be cached"
            );
            document.undo().unwrap();
            build(&document.snapshot(), &measurement, 1100.);
            let restored = counts.take_stage();
            assert_eq!(
                restored.tables_measured, 0,
                "undo restores immutable content identity"
            );
            assert_eq!(restored.groups_measured, 0);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            // A different document may reuse IDs/revision numbers, never its allocation identity.
            let other =
                Document::from_markdown(source.replace("Configuration", "Different configuration"))
                    .unwrap();
            build(&other.snapshot(), &measurement, 1100.);
            assert!(counts.take_stage().groups_measured > 0);
        });
    }

    #[gpui::test]
    fn group_cache_keys_include_loaded_image_dimensions_and_text_environment(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "## Figures\n\nA short explanation.\n\n![Diagram](diagram.png)\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let image = projection.image_segments().next().unwrap().node_id;
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let build = |measurement: &FontMeasurement, dimensions: &NodeImageDimensions| {
                build_edit_locked_adaptive_plan(
                    &projection,
                    1280.,
                    1000.,
                    None,
                    false,
                    LayoutMeasurement {
                        text: measurement,
                        images: Some(dimensions),
                        scope: None,
                        resource_generation: 0,
                    },
                    None,
                )
            };
            let mut dimensions = HashMap::from([(image, ("diagram.png".into(), (400, 240)))]);
            let counts = diagnostics::MeasurementScope::new();
            let first = build(&measurement, &dimensions);
            counts.take_stage();
            build(&measurement, &dimensions);
            assert_eq!(counts.take_stage().groups_measured, 0);
            dimensions.get_mut(&image).unwrap().1 = (400, 900);
            let changed = build(&measurement, &dimensions);
            assert!(counts.take_stage().groups_measured > 0);
            let fresh =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let oracle = build(&fresh, &dimensions);
            let footprints = |plan: &AdaptivePlan| {
                plan.measured_rows
                    .chosen
                    .iter()
                    .map(|row| (row.template, row.heights.clone(), row.cost()))
                    .collect::<Vec<_>>()
            };
            assert_ne!(footprints(&first), footprints(&changed));
            assert_eq!(footprints(&changed), footprints(&oracle));
            for (family, zoom) in [
                ("Spline Sans Mineral", 2.),
                ("Spline Sans Mono Mineral", 1.),
            ] {
                let different = FontMeasurement::new(cx.text_system().clone(), family.into(), zoom);
                counts.take_stage();
                build(&different, &dimensions);
                assert!(
                    counts.take_stage().groups_measured > 0,
                    "typography environments cannot share footprints"
                );
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn flat_list_candidates_reuse_item_footprints(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown("# Items\n\n- **One:** first\n- **Two:** second\n- **Three:** third\n- **Four:** fourth\n- **Five:** fifth\n- **Six:** sixth\n").unwrap();
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let counts = diagnostics::MeasurementScope::new();
            let first = build_measured_adaptive_plan(&projection, 1280., 1000., None, false, &measurement);
            let cold = counts.take_stage();
            assert!(!first.measured_lists.is_empty());
            let second = build_measured_adaptive_plan(&projection, 1280., 1000., None, false, &measurement);
            let warm = counts.take_stage();
            assert!(cold.wrap_requests > 0);
            assert_eq!(warm.wrap_requests, 0, "list candidates were remeasured: {warm:?}");
            assert_eq!(warm.intrinsic_requests, 0);
            for (id, first) in &first.measured_lists {
                assert_eq!(first.layout, second.measured_lists[id].layout);
            }
        });
    }

    #[gpui::test]
    fn measured_tables_share_unequal_rows_with_intact_cell_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            use crate::adaptive::rows::RowKind;
            let source =
                include_str!("../../../../performance/layout-fixtures/12-measured-tables.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            measurement.measure_tables(&mut projection);
            let plan =
                build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
            let paired = plan
                .measured_rows
                .chosen
                .iter()
                .find(|row| row.kind == RowKind::Tables)
                .unwrap_or_else(|| {
                    panic!(
                        "expected paired measured tables: {:?}",
                        plan.measured_rows.candidates
                    )
                });
            assert!(
                paired.widths[0] < paired.widths[1],
                "the compact property table must not take the wider span: {:?}",
                paired.widths
            );
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1100.,
                &plan,
                Some(&measurement),
            );
            for (column, part) in paired.parts.iter().enumerate() {
                let roots = projection.roots().collect::<Vec<_>>();
                let ids = roots[part.clone()]
                    .iter()
                    .map(|root| root.id())
                    .collect::<Vec<_>>();
                let column_lines = lines
                    .iter()
                    .filter(|line| {
                        projection
                            .segment_for_range(&line.range)
                            .is_some_and(|segment| ids.contains(&segment.top_level_node_id))
                    })
                    .collect::<Vec<_>>();
                let top = column_lines
                    .iter()
                    .map(|l| l.y - l.style.space_above)
                    .fold(f32::INFINITY, f32::min);
                let bottom = column_lines
                    .iter()
                    .map(|l| l.y + l.style.line_height + l.style.space_below)
                    .fold(0., f32::max);
                assert!(
                    (bottom - top - paired.heights[column]).abs() < 0.01,
                    "table height must match actual row/cell positioning"
                );
                let mut cells = HashMap::new();
                for line in column_lines {
                    if let Some((id, row, col, _)) = line.table_cell {
                        assert!(line.table_cell_first || cells.contains_key(&(id, row, col)));
                        cells
                            .entry((id, row, col))
                            .or_insert((line.x_fraction, line.table_row_y));
                        assert!(line.x_fraction + line.width_fraction <= 1.001);
                    }
                }
                assert!(!cells.is_empty());
                for ((id, row, col), (_, y)) in &cells {
                    if *col > 0 {
                        assert_eq!(*y, cells[&(*id, *row, 0)].1);
                    }
                }
            }
            let narrow = build_measured_adaptive_plan(
                &projection,
                360.,
                800.,
                Some(&plan),
                false,
                &measurement,
            );
            assert!(
                narrow
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.kind != RowKind::Tables)
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn measured_row_windows_partition_all_canonical_roots_at_the_work_bound(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "```text\nkeep exact source\n```\n\n".repeat(123);
            let document = Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let plan =
                build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
            assert_eq!(plan.measured_rows.windows, 4);
            assert_eq!(
                plan.measured_rows
                    .chosen
                    .iter()
                    .flat_map(|row| row.roots.clone())
                    .collect::<Vec<_>>(),
                (0..123).collect::<Vec<_>>()
            );
            assert!(
                plan.measured_rows
                    .candidates
                    .iter()
                    .all(|row| row.groups.end <= crate::adaptive::rows::WINDOW_GROUPS)
            );
            assert!(plan.measured_rows.chosen.iter().all(|row| row.legal()));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn galleries_wait_for_dimensions_and_use_measured_source_order_rows(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = format!(
                "## Gallery\n\n{}After all six figures.\n",
                (0..6)
                    .map(|i| if i % 2 == 0 {
                        format!("[![Figure {i}](figure-{i}.png)](full-{i}.png)\n\n")
                    } else {
                        format!("![Figure {i}](figure-{i}.png)\n\n")
                    })
                    .collect::<String>()
            );
            let document = Document::from_markdown(source.as_str()).unwrap();
            let snapshot = document.snapshot();
            let initial = PreparedDocumentView::prepare(&document);
            assert!(
                initial.adaptive.slots.is_empty(),
                "initial unknown-size figures must stay stacked"
            );
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let dimensions = (0..6)
                .map(|i| {
                    (
                        hash(&resolved_image_resource(&format!("figure-{i}.png"), None)),
                        (400, 240),
                    )
                })
                .collect::<SourceImageDimensions>();
            for (width, columns) in [(1280., 3), (900., 2), (420., 1)] {
                let (prepared, _, _) = PreparedDocumentView::prepare_snapshot_with_images(
                    &snapshot,
                    &dimensions,
                    None,
                    ReflowViewport {
                        published_geometry: None,
                        width,
                        height: 1000.,
                        zoom: 1.,
                        math_edit_node: None,
                        editing_node: None,
                        table_layout_lock: None,
                        trace_mode: LayoutTraceMode::Off,
                        html_disclosures: Arc::default(),
                        html_loaded_images: Arc::default(),
                        visible_roots: None,
                        resource_generation: 0,
                    },
                    &initial.adaptive,
                    &measurement,
                );
                let figures = prepared
                    .visual_lines
                    .iter()
                    .filter(|line| {
                        prepared
                            .projection
                            .segment_for_range(&line.range)
                            .is_some_and(|s| s.context.image_source.is_some())
                    })
                    .collect::<Vec<_>>();
                assert_eq!(figures.len(), 6);
                assert!(
                    prepared.visual_lines[0].slot.is_none(),
                    "gallery heading stays full width"
                );
                for (i, line) in figures.iter().enumerate() {
                    assert_eq!(
                        line.slot.map_or(1, |slot| slot.columns),
                        columns,
                        "wrong measured columns at {width}"
                    );
                    if i % columns > 0 {
                        assert!((line.y - figures[i - 1].y).abs() < 0.01);
                        assert!(line.x_fraction > figures[i - 1].x_fraction);
                    } else if i > 0 {
                        assert!(line.y >= figures[i - 1].y + figures[i - 1].style.line_height);
                    }
                    assert!(
                        line.style.line_height > 0.
                            && line.x_fraction + line.width_fraction <= 1.001
                    );
                }
                assert!(
                    prepared
                        .adaptive
                        .measured_rows
                        .chosen
                        .iter()
                        .flat_map(|row| row.roots.clone())
                        .eq(0..snapshot.blocks().len())
                );
                assert!(
                    prepared.visual_lines.last().unwrap().y
                        >= figures[5].y + figures[5].style.line_height + 24.
                );
                assert_eq!(snapshot.serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn large_consecutive_figures_pair_only_when_the_complete_row_fits(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "# Figures\n\nIntroduction.\n\nA second introduction.\n\n[![First](first.png)](full.png)\n\n![Second](second.png)\n\nFollowing prose.\n";
            let document = Document::from_markdown(source).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            for size in [(960, 360), (1920, 720)] {
                let dimensions = ["first.png", "second.png"].into_iter()
                    .map(|source| (hash(&resolved_image_resource(source, None)), size)).collect();
                for (width, height, expected_columns) in [(992., 928., 2), (1280., 928., 2), (420., 928., 1), (992., 150., 1)] {
                    let (prepared, _, _) = PreparedDocumentView::prepare_snapshot_with_images(
                        &document.snapshot(), &dimensions, None,
                        ReflowViewport { published_geometry: None, width, height, zoom: 1., math_edit_node: None,
                            editing_node: None, table_layout_lock: None, trace_mode: LayoutTraceMode::Off,
                            visible_roots: None, html_disclosures: Arc::default(), html_loaded_images: Arc::default(), resource_generation: 1 },
                        &initial.adaptive, &measurement);
                    let figures = prepared.visual_lines.iter().filter(|line| prepared.projection.segment_for_range(&line.range)
                        .is_some_and(|segment| segment.context.image_source.is_some())).collect::<Vec<_>>();
                    assert_eq!(figures.len(), 2);
                    for figure in &figures {
                        assert_eq!(figure.slot.map_or(1, |slot| slot.columns), expected_columns,
                            "figure size={size:?}, canvas={width}, viewport={height}");
                        let available = figure.width_fraction * width - 16.;
                        assert!((figure.style.line_height - available.min(size.0 as f32) * size.1 as f32 / size.0 as f32).abs() < 0.1,
                            "the full intrinsic aspect ratio must be preserved");
                    }
                    if expected_columns == 2 {
                        assert_eq!(figures[0].y, figures[1].y);
                        assert!(figures[0].x_fraction < figures[1].x_fraction);
                        assert_eq!(figures[0].width_fraction, figures[1].width_fraction,
                            "identically sized figures should not get artificial asymmetric emphasis");
                    } else {
                        assert!(figures[1].y >= figures[0].y + figures[0].style.line_height);
                    }
                    assert!(prepared.adaptive.measured_rows.chosen.iter().flat_map(|r| r.roots.clone()).eq(0..document.snapshot().blocks().len()));
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                }
            }
        });
    }

    #[gpui::test]
    fn a_loading_gallery_keeps_its_edit_lock_then_reconsiders_on_blur(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document = Document::from_markdown("![First](first.png)\n\n![Second](second.png)\n").unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let prepare = |dimensions: &SourceImageDimensions, previous: &AdaptivePlan, editing_node| {
                PreparedDocumentView::prepare_snapshot_with_images(&document.snapshot(), dimensions, None,
                    ReflowViewport { published_geometry: None, width: 992., height: 928., zoom: 1., math_edit_node: None,
                        editing_node, table_layout_lock: None, trace_mode: LayoutTraceMode::Off,
                        visible_roots: None, html_disclosures: Arc::default(), html_loaded_images: Arc::default(), resource_generation: 1 },
                    previous, &measurement).0
            };
            let pending = prepare(&HashMap::new(), &initial.adaptive, None);
            let first = pending.projection.image_segments().next().unwrap().node_id;
            let ready = ["first.png", "second.png"].into_iter()
                .map(|source| (hash(&resolved_image_resource(source, None)), (960, 360))).collect();
            let focused = prepare(&ready, &pending.adaptive, Some(first));
            assert!(focused.adaptive.measured_rows.edit_locked);
            assert!(focused.visual_lines.iter().all(|line| line.slot.is_none_or(|slot| slot.columns == 1)));
            let focused = prepare(&ready, &focused.adaptive, Some(first));
            assert!(focused.visual_lines.iter().all(|line| line.slot.is_none_or(|slot| slot.columns == 1)));
            assert!(focused.adaptive.measured_rows.chosen.iter().all(|row| row.decision_provisional));
            let blurred = prepare(&ready, &focused.adaptive, None);
            assert!(blurred.visual_lines.iter().all(|line| line.slot.is_some_and(|slot| slot.columns == 2)),
                "a provisional stack held for editing must not become a permanent gallery preference");
            assert!(blurred.adaptive.measured_rows.chosen.iter().all(|row| !row.decision_provisional));
            let stable = prepare(&ready, &blurred.adaptive, None);
            assert!(stable.adaptive.measured_rows.retained_previous);
            assert!(stable.visual_lines.iter().all(|line| line.slot.is_some_and(|slot| slot.columns == 2)));
            assert_eq!(document.snapshot().serialize().unwrap(), "![First](first.png)\n\n![Second](second.png)\n");
        });
    }

    #[gpui::test]
    fn gallery_resource_arrival_does_not_pin_a_partial_composition(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = format!(
                "## Gallery\n\nIntroduction.\n\n{}After the gallery.\n",
                (0..6)
                    .map(|i| format!("![Figure {i}](figure-{i}.png)\n\n"))
                    .collect::<String>()
            );
            let document = Document::from_markdown(source.as_str()).unwrap();
            let snapshot = document.snapshot();
            let mut previous = PreparedDocumentView::prepare(&document);
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let mut dimensions = SourceImageDimensions::default();
            for loaded in 0..=6 {
                if loaded > 0 {
                    dimensions.insert(
                        hash(&resolved_image_resource(&format!("figure-{}.png", loaded - 1), None)),
                        (400, 240),
                    );
                }
                let (prepared, _, _) = PreparedDocumentView::prepare_snapshot_with_images(
                    &snapshot,
                    &dimensions,
                    None,
                    ReflowViewport { published_geometry: None,
                        width: 1280.,
                        height: 1000.,
                        zoom: 1.,
                        math_edit_node: None,
                        editing_node: None,
                        table_layout_lock: None,
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: None,
                        html_disclosures: Arc::default(), html_loaded_images: Arc::default(),
                        resource_generation: 0,
                    },
                    &previous.adaptive,
                    &measurement,
                );
                let columns = prepared.visual_lines.iter().filter_map(|line| {
                    prepared.projection.segment_for_range(&line.range)
                        .filter(|s| s.context.image_source.is_some())
                        .map(|_| line.slot.map_or(1, |slot| slot.columns))
                }).collect::<Vec<_>>();
                assert_eq!(columns, vec![if loaded == 6 { 3 } else { 1 }; 6],
                    "the gallery must resolve together, not retain a partial row after {loaded} resources");
                assert_eq!(snapshot.serialize().unwrap(), source);
                previous = prepared;
            }
        });
    }

    #[gpui::test]
    fn wide_stack_measurement_excludes_intentional_prose_margins(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document =
                Document::from_markdown("A short explanation with a comfortable reading measure.")
                    .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let roots = projection.roots().collect::<Vec<_>>();
            let segments = HashMap::from([(roots[0].id(), vec![0])]);
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let resources = LayoutMeasurement::from(&measurement);
            let plan = AdaptivePlan::build(&projection, 1280., None, false);
            let measure = |width| {
                measure_row_group(
                    &projection,
                    &roots,
                    &segments,
                    width,
                    false,
                    &resources,
                    &plan,
                )
                .unwrap()
            };
            let prose = measure(PROSE_WIDTH);
            let wide = measure(1280.);
            assert!(
                (prose.preferred_width - wide.preferred_width).abs() < 0.01,
                "blank space outside the prose measure is not intrinsic content width"
            );
            assert_eq!(prose.height, wide.height);
        });
    }

    #[gpui::test]
    fn loaded_image_explanation_uses_measured_geometry_and_pending_images_stack(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            use crate::adaptive::rows::RowKind;
            let source = "## Diagram\n\nRead the complete diagram beside this explanation.\n\n![Three connected components](diagram.png)\n\nFollowing content stays below the whole figure.\n";
            let document = Document::from_markdown(source).unwrap();
            let snapshot = document.snapshot();
            let initial = PreparedDocumentView::prepare(&document);
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let prepare = |dimensions: &SourceImageDimensions, width, previous: &AdaptivePlan| {
                PreparedDocumentView::prepare_snapshot_with_images(
                    &snapshot, dimensions, None,
                    ReflowViewport { published_geometry: None, width, height: 1000., zoom: 1., math_edit_node: None, editing_node: None, table_layout_lock: None, html_disclosures: Arc::default(), html_loaded_images: Arc::default(), trace_mode: LayoutTraceMode::Off, visible_roots: None, resource_generation: 0 },
                    previous, &measurement,
                ).0
            };
            let pending = prepare(&HashMap::new(), 1100., &initial.adaptive);
            assert!(pending.adaptive.measured_rows.chosen.iter().all(|row| row.kind == RowKind::Stack));
            assert!(pending.adaptive.measured_rows.chosen[0].height_estimated);
            let key = hash(&resolved_image_resource("diagram.png", None));
            let dimensions = HashMap::from([(key, (600, 180))]);
            let loaded = prepare(&dimensions, 1100., &pending.adaptive);
            let row = loaded.adaptive.measured_rows.chosen.iter().find(|row| row.kind == RowKind::Explanation)
                .unwrap_or_else(|| panic!("loaded image dimensions should enable a measured explanation pair: {:?}", loaded.adaptive.measured_rows));
            let image = loaded.projection.image_segments().next().unwrap();
            let BlockNode::Image(image_node) = loaded.projection.block(image.node_id).unwrap() else { unreachable!() };
            assert!(resolved_image_dimensions(image_node, &HashMap::from([(image.node_id, ("other.png".into(), (600, 180)))] )).is_none(), "dimensions from an old image source must not be reused");
            let node_dimensions = HashMap::from([(image.node_id, ("diagram.png".into(), (600, 180)))]);
            let focused = build_edit_locked_adaptive_plan(&loaded.projection, 1100., 1000., Some(&pending.adaptive), false,
                LayoutMeasurement { text: &measurement, images: Some(&node_dimensions), scope: None, resource_generation: 0 }, Some(loaded.projection.segments()[1].node_id));
            assert!(focused.measured_rows.edit_locked);
            assert!(focused.measured_rows.chosen.iter().all(|row| row.kind == RowKind::Stack), "resource completion must not rearrange the focused explanation");
            let line = loaded.visual_lines.iter().find(|line| line.range == image.projection_range).unwrap();
            assert!(line.x_fraction > 0.);
            assert!((line.style.line_height - row.heights[1]).abs() < 0.01);
            assert!((line.style.line_height - (row.widths[1] - 16.).min(600.) * 0.3).abs() < 0.01);
            let following = loaded.visual_lines.last().unwrap();
            assert!(following.y >= line.y + line.style.line_height + 24.);
            for (sizes, width) in [(&dimensions, 420.), (&HashMap::from([(key, (1000, 4000))]), 1100.), (&HashMap::from([(key, (0, 180))]), 1100.)] {
                let fallback = prepare(sizes, width, &loaded.adaptive);
                assert!(fallback.adaptive.measured_rows.chosen.iter().all(|row| row.kind == RowKind::Stack));
                assert_eq!(fallback.projection.text(), loaded.projection.text());
                assert!(fallback.visual_lines.iter().find(|line| line.range == image.projection_range).unwrap().style.line_height > 0., "even invalid dimensions retain visible fallback content");
            }
            assert_eq!(snapshot.serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn measured_explanation_code_uses_unequal_tracks_and_stacks_when_cramped(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            use crate::adaptive::rows::{RowKind, RowRejection};
            let source = "## Example\n\nRead the saved configuration.\n\n```rust\nlet configuration = read_configuration_from_workspace(&workspace_directory)?;\nstart_worker(configuration);\n```\n\nFollowing content remains below the whole pair.\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
            let row = plan.measured_rows.chosen.iter().find(|r| r.kind == RowKind::Explanation)
                .unwrap_or_else(|| panic!("expected a readable explanation pair: {:?}", plan.measured_rows.candidates));
            assert_ne!(row.widths[0], row.widths[1]);
            let lines = build_measured_visual_lines(&projection, &HashMap::new(), 1100., &plan, Some(&measurement));
            let code = projection.segments().iter().find(|s| matches!(projection.block(s.node_id), Some(BlockNode::CodeBlock(_)))).unwrap();
            let example = lines.iter().find(|l| l.range.start == code.projection_range.start).unwrap();
            assert!(example.x_fraction > 0.);
            assert!(example.x_fraction + example.width_fraction <= 1.001);
            assert!(lines[0].slot.is_none(), "major heading stays full width");
            assert_eq!(lines.iter().map(|l| &projection.text()[l.range.clone()]).collect::<String>(),
                projection.segments().iter().map(|s| &projection.text()[s.projection_range.clone()]).collect::<String>().replace(['\r', '\n'], ""));
            for pair in lines.windows(2) {
                assert!(pair[0].range.end <= pair[1].range.start);
                let gap = &projection.text()[pair[0].range.end..pair[1].range.start];
                assert!(gap.chars().all(|c| c == '\n' || c == '\r'), "only authored line/paragraph separators can be outside glyph ranges");
            }
            for (column, part) in row.parts.iter().enumerate() {
                let roots = projection.roots().collect::<Vec<_>>();
                let ids = roots[part.clone()].iter().map(|root| root.id()).collect::<Vec<_>>();
                let group_lines = lines.iter().filter(|line| projection.segment_for_range(&line.range).is_some_and(|segment| ids.contains(&segment.top_level_node_id))).collect::<Vec<_>>();
                let top = group_lines.iter().map(|line| line.y - line.style.space_above).fold(f32::INFINITY, f32::min);
                let bottom = group_lines.iter().map(|line| line.y + line.style.line_height + line.style.space_below).fold(0., f32::max);
                assert!((bottom - top - row.heights[column]).abs() < 0.01, "measured and rendered column height must agree");
            }
            let narrow = build_measured_adaptive_plan(&projection, 360., 800., Some(&plan), false, &measurement);
            assert!(narrow.slots.is_empty());
            let short = build_measured_adaptive_plan(&projection, 1100., 100., Some(&plan), false, &measurement);
            assert!(short.slots.is_empty());
            assert!(short.measured_rows.candidates.iter().any(|r| r.rejected == Some(RowRejection::TooTall)));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn provisional_canvas_does_not_seed_measured_layout_hysteresis(cx: &mut gpui::TestAppContext) {
        cx.update(init);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("- North\n- South\n- East\n- West\n- Above\n- Below\n")
                    .unwrap(),
                window,
                cx,
            )
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.element_bounds = None;
                assert!(!editor.reflow.is_active());
                editor.measured_layout = false;
                editor.adaptive.measured_lists.clear();
                drop(gpui::Render::render(editor, window, cx));
                assert!(
                    !editor.reflow.is_active(),
                    "wait for actual first-paint bounds"
                );
                assert!(editor.adaptive.measured_lists.is_empty());
            })
        });
    }

    #[gpui::test]
    fn actual_list_measurements_drive_grid_selection_and_preserve_source(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "1. North\n2. South\n3. East\n4. West\n5. Above\n6. Below\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            for (width, expected) in [
                (1280., ListLayout::Grid(3)),
                (1100., ListLayout::Grid(3)),
                (620., ListLayout::Grid(2)),
                (360., ListLayout::List),
            ] {
                let plan = build_measured_adaptive_plan(
                    &projection,
                    width,
                    800.,
                    None,
                    false,
                    &measurement,
                );
                let decision = plan.measured_lists.values().next().unwrap();
                assert_eq!(decision.layout, expected, "{width}: {decision:?}");
                assert!(decision.is_valid(6, width, None));
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&measurement),
                );
                assert_eq!(
                    lines
                        .iter()
                        .map(|line| &projection.text()[line.range.clone()])
                        .collect::<String>(),
                    projection
                        .segments()
                        .iter()
                        .map(|s| &projection.text()[s.projection_range.clone()])
                        .collect::<String>()
                );
                if let ListLayout::Grid(columns) = expected {
                    let candidate = decision
                        .candidates
                        .iter()
                        .find(|c| c.columns == columns)
                        .unwrap();
                    for (index, segment) in projection.segments().iter().enumerate() {
                        let rendered = lines
                            .iter()
                            .filter(|line| segment.projection_range.contains(&line.range.start))
                            .collect::<Vec<_>>();
                        assert_eq!(rendered.len(), candidate.items[index].lines);
                        assert!(rendered.len() <= 5);
                        let height = rendered
                            .iter()
                            .map(|l| {
                                l.style.line_height + l.style.space_above + l.style.space_below
                            })
                            .sum::<f32>();
                        assert_eq!(height, candidate.items[index].height);
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn measured_lists_reject_uneven_many_and_cross_referenced_items(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            for source in [
                format!(
                    "- Short\n- Brief\n- {}\n- Fourth\n- Fifth\n- Sixth\n",
                    "A much longer explanatory paragraph. ".repeat(12)
                ),
                (0..10).map(|n| format!("- Label {n}\n")).collect(),
                "1. Alpha\n2. The result of step 1\n3. Gamma\n".to_owned(),
                "1. Alpha\n2. [Related](#alpha)\n3. Gamma\n".to_owned(),
                "1. Alpha\n   - Nested detail\n2. Beta\n3. Gamma\n".to_owned(),
            ] {
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let plan = build_measured_adaptive_plan(
                    &projection,
                    1280.,
                    800.,
                    None,
                    false,
                    &measurement,
                );
                assert!(plan.slots.is_empty(), "{source}");
                assert!(
                    plan.lists
                        .values()
                        .all(|a| !matches!(a.layout, ListLayout::Grid(_)))
                );
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[test]
    fn pairwise_gaps_attach_headings_and_keep_table_padding_inside_the_table() {
        let source = "# Title\n\nIntro.\n\nA second paragraph.\n\n## Section\n\nText.\n\n| A | B |\n| - | - |\n| C | D |\n\nAfter table.\n\n---\n\nAfter break.\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let plan = AdaptivePlan::build(&projection, 1000., None, false);
        let lines = build_arranged_visual_lines(&projection, &HashMap::new(), 1000., &plan);
        let roots = projection.roots().collect::<Vec<_>>();
        let first = |root: usize| {
            lines
                .iter()
                .find(|line| {
                    projection
                        .segment_for_range(&line.range)
                        .unwrap()
                        .top_level_node_id
                        == roots[root].id()
                })
                .unwrap()
        };
        let near = |actual: f32, expected: f32| {
            assert!((actual - expected).abs() < 0.001, "{actual} != {expected}")
        };
        near(first(1).y - first(0).y - first(0).style.line_height, 10.);
        near(first(2).y - first(1).y - first(1).style.line_height, 24.);
        near(first(3).y - first(2).y - first(2).style.line_height, 44.);
        near(first(4).y - first(3).y - first(3).style.line_height, 10.);
        let table = first(5);
        near(
            table.table_row_y - first(4).y - first(4).style.line_height,
            12.,
        );
        near(table.y - table.table_row_y, 10.);
        let table_last = lines
            .iter()
            .rev()
            .find(|line| line.table_cell.is_some())
            .unwrap();
        near(
            first(6).y - table_last.table_row_y - table_last.table_row_height,
            24.,
        );
        assert!(matches!(roots[7], BlockNode::ThematicBreak { .. }));
        assert_eq!(first(7).style.line_height, 1.);
        near(first(8).y - first(7).y - 1., 24.);
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[gpui::test]
    fn cards_do_not_shrink_the_design_system_fonts(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
        let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
        let source =
            "### Alpha\n\nFirst idea.\n\n### Beta\n\nSecond idea.\n\n### Gamma\n\nThird idea.\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let plan = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
        let lines = build_arranged_visual_lines(&projection, &HashMap::new(), 1100., &plan);
        assert!(!plan.slots.is_empty());
        for line in lines {
            let segment = projection.segment_for_range(&line.range).unwrap();
            let block = projection.block(segment.node_id).unwrap();
            let expected = visual_line_style_for(&projection, block, segment, &line.range, None);
            assert_eq!(line.style.font_size, expected.font_size);
            assert_eq!(line.style.line_height, expected.line_height);
        }        });
    }

    #[test]
    fn thematic_break_prevents_a_gallery_from_crossing_the_boundary() {
        let document = Document::from_markdown("![A](a.png)\n\n---\n\n![B](b.png)\n").unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let plan = AdaptivePlan::build(&projection, 1000., None, false);
        assert!(plan.slots.is_empty());
        let lines = build_arranged_visual_lines(&projection, &HashMap::new(), 1000., &plan);
        assert_eq!(lines.len(), 3);
        assert!(lines.windows(2).all(|pair| pair[0].y < pair[1].y));
    }

    #[gpui::test]
    fn sibling_sections_compose_without_changing_source_and_stack_when_narrow(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
        let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
        let source =
            include_str!("../../../../performance/layout-fixtures/05-product-specification.md");
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let wide = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
        let lines = build_arranged_visual_lines(&projection, &HashMap::new(), 1100., &wide);
        let headings = projection.segments().iter().filter(|s| {
            matches!(projection.block(s.node_id), Some(BlockNode::Heading(h)) if h.content.as_string().ends_with("journey"))
        }).collect::<Vec<_>>();
        assert_eq!(headings.len(), 3);
        let first = wide.slots[&headings[0].node_id];
        for (index, heading) in headings.iter().enumerate() {
            assert_eq!(wide.slots[&heading.node_id].group, first.group);
            assert_eq!(wide.slots[&heading.node_id].item, index);
            assert_eq!(wide.slots[&heading.node_id].columns, 3);
        }
        let starts = headings
            .iter()
            .map(|s| {
                lines
                    .iter()
                    .find(|l| l.range.start == s.projection_range.start)
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(starts[0].y, starts[2].y);
        assert_eq!(starts[0].table_row_height, starts[2].table_row_height);
        let narrow = build_measured_adaptive_plan(&projection, 500., 800., Some(&wide), false, &measurement);
        assert!(
            headings
                .iter()
                .all(|heading| !narrow.slots.contains_key(&heading.node_id))
        );
        assert_eq!(document.snapshot().serialize().unwrap(), source);        });
    }

    const SIX: &str = "# Principles\n\nSix independent ideas.\n\n1. **Clarity:** Find the main idea.\n2. **Continuity:** Keep the reader's place.\n3. **Balance:** Give ideas similar weight.\n4. **Restraint:** Decorate with purpose.\n5. **Fidelity:** Preserve the structure.\n6. **Comfort:** Leave room to think.\n";

    #[gpui::test]
    fn grids_keep_row_major_source_order_and_nonoverlapping_rows(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown(SIX).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let plan =
                build_measured_adaptive_plan(&projection, 1000., 1000., None, false, &measurement);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1000.,
                &plan,
                Some(&measurement),
            );
            let starts = lines
                .iter()
                .filter(|line| {
                    line.slot.is_some()
                        && projection
                            .segment_for_range(&line.range)
                            .unwrap()
                            .projection_range
                            .start
                            == line.range.start
                })
                .collect::<Vec<_>>();
            assert_eq!(starts.len(), 6);
            assert_eq!(starts[0].y, starts[1].y);
            assert_eq!(starts[1].y, starts[2].y);
            assert!(
                starts[0].x_fraction < starts[1].x_fraction
                    && starts[1].x_fraction < starts[2].x_fraction
            );
            assert_eq!(starts[3].x_fraction, 0.);
            assert!(starts[3].y >= starts[0].table_row_y + starts[0].table_row_height);
            assert!(
                lines
                    .windows(2)
                    .all(|pair| pair[0].range.start <= pair[1].range.start)
            );
            assert_eq!(document.snapshot().serialize().unwrap(), SIX);
        });
    }

    #[test]
    fn card_labels_wrap_before_descriptions_without_empty_lines_or_lost_text() {
        let document = Document::from_markdown(SIX).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        for width in [620., 696., 1000.] {
            let plan = AdaptivePlan::build(&projection, width, None, false);
            let lines = build_arranged_visual_lines(&projection, &HashMap::new(), width, &plan);
            for segment in projection.segments() {
                if !plan.slots.contains_key(&segment.node_id) {
                    continue;
                }
                let ranges = lines
                    .iter()
                    .filter(|line| segment.projection_range.contains(&line.range.start))
                    .map(|line| line.range.clone())
                    .collect::<Vec<_>>();
                assert!(
                    ranges
                        .iter()
                        .all(|range| !projection.text()[range.clone()].trim().is_empty())
                );
                let rendered = ranges
                    .iter()
                    .map(|range| &projection.text()[range.clone()])
                    .collect::<String>();
                assert_eq!(
                    rendered,
                    projection.text()[segment.projection_range.clone()]
                );
                assert!(
                    projection.text()[ranges[0].clone()]
                        .trim_end()
                        .ends_with(':')
                );
            }
        }
    }

    #[test]
    fn paired_images_keep_full_height_and_remain_visible_beside_shorter_images() {
        let document = Document::from_markdown(
            "![Tall](tall.png)\n\n![Wide](wide.png)\n\nA following paragraph.\n",
        )
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let images = projection.image_segments().collect::<Vec<_>>();
        let dimensions = HashMap::from([
            (images[0].node_id, ("tall.png".into(), (600, 1200))),
            (images[1].node_id, ("wide.png".into(), (1200, 600))),
        ]);
        let mut plan = AdaptivePlan::build(&projection, 1000., None, false);
        // Deliberately retained imbalanced geometry (e.g. media arrival while
        // editing) tests the renderer, independently of automatic eligibility.
        for (item, image) in images.iter().enumerate() {
            plan.slots.insert(
                image.node_id,
                LayoutSlot {
                    group: images[0].node_id,
                    item,
                    columns: 2,
                    cards: false,
                    track_start: (item * 6) as u8,
                    span: 6,
                    fixed_canvas: None,
                },
            );
        }
        let lines = build_arranged_visual_lines(&projection, &dimensions, 1000., &plan);
        assert_eq!(lines[0].y, lines[1].y);
        assert!(lines[0].style.line_height > lines[1].style.line_height * 3.);
        assert!(lines[2].y >= lines[0].y + lines[0].style.line_height);
        let geometry =
            component_geometry(&projection, &lines, 1000., &visual_line_paint_order(&lines));
        let order = visual_line_paint_order(&lines);
        assert!(
            geometry
                .visible_range(&lines, &order, 500., 700.)
                .map(|i| order[i])
                .any(|index| index == 0)
        );
        let narrow = AdaptivePlan::build(&projection, 420., Some(&plan), false);
        assert!(narrow.slots.is_empty());
    }

    #[gpui::test]
    fn arrow_cards_keep_every_source_byte_and_explicit_stage_order(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
        let source = "1. Request → promise → resolution.\n2. Invoice → payment → correction.\n3. Booking → reschedule → cancellation.\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
        let plan = build_measured_adaptive_plan(&projection, 1100., 1000., None, false, &measurement);
        let lines = build_measured_visual_lines(&projection, &HashMap::new(), 1100., &plan, Some(&measurement));
        for segment in projection.segments() {
            let fragments = lines
                .iter()
                .filter(|line| segment.projection_range.contains(&line.range.start))
                .collect::<Vec<_>>();
            assert_eq!(fragments.len(), 3);
            assert!(
                fragments[1..]
                    .iter()
                    .all(|line| projection.text()[line.range.clone()].starts_with('→'))
            );
            assert_eq!(
                fragments
                    .iter()
                    .map(|line| &projection.text()[line.range.clone()])
                    .collect::<String>(),
                projection.text()[segment.projection_range.clone()]
            );
        }
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn invalid_retained_row_falls_back_to_complete_rendered_source_order(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            use crate::adaptive::rows::RowKind;
            let source = "### Alpha\n\nA short explanation.\n\n### Beta\n\nA second explanation.\n\n### Gamma\n\nA third explanation.\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let mut previous = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
            let focused = projection.segments()[1].node_id;
            let retained = previous.measured_rows.chosen.iter_mut()
                .find(|row| row.kind == RowKind::Peer).unwrap();
            // Plausible geometry with an invalid canonical owner must not get
            // through the focus lock just because its tracks still fit.
            retained.ids[0] = NodeId::new_unchecked(u64::MAX);
            assert!(retained.legal());
            let recovered = build_edit_locked_adaptive_plan(
                &projection, 1100., 800., Some(&previous), false, &measurement, Some(focused),
            );
            assert_eq!(recovered.measured_rows.validation_fallbacks, 1);
            assert!(recovered.slots.is_empty());
            assert!(recovered.measured_rows.chosen.iter().all(|row| row.kind == RowKind::Stack && row.legal()));
            assert!(recovered.measured_rows.chosen.iter().flat_map(|row| row.roots.clone()).eq(0..projection.roots().count()));
            let lines = build_measured_visual_lines(&projection, &HashMap::new(), 1100., &recovered, Some(&measurement));
            assert!(!lines.is_empty());
            assert!(lines.windows(2).all(|pair| pair[0].y + pair[0].style.line_height <= pair[1].y));
            for segment in projection.segments() {
                let actual = lines.iter().filter(|line| segment.projection_range.contains(&line.range.start))
                    .map(|line| &projection.text()[line.range.clone()]).collect::<String>();
                assert_eq!(actual, projection.text()[segment.projection_range.clone()]);
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn focused_peer_row_keeps_widths_through_growth_and_reflows_only_after_blur(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let source = "### Alpha\n\nA short explanation.\n\n### Beta\n\nA second explanation.\n\n### Gamma\n\nA third explanation.\n";
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
            let node = projection.segments().iter().find(|s| projection.text()[s.projection_range.clone()] == *"A second explanation.").unwrap().node_id;
            let slot = plan.slots[&node];
            document.apply(EditCommand::ReplaceText {
                node_id: node, range: 0..0, text: "Extra detail. ".repeat(50),
                typing: true, selection_after: None,
            }).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let edited = document.snapshot().serialize().unwrap();
            let locked = build_edit_locked_adaptive_plan(&projection, 1280., 250., Some(&plan), false, &measurement, Some(node));
            assert!(locked.measured_rows.edit_locked);
            let retained = locked.slots[&node];
            assert_eq!(retained.item, slot.item);
            assert_eq!(retained.width(1280.), slot.width(1100.));
            assert_eq!(retained.left(1280.), slot.left(1100.));
            let lines = build_measured_visual_lines(&projection, &HashMap::new(), 1280., &locked, Some(&measurement));
            for segment in projection.segments() {
                let fragments = lines.iter().filter(|line| segment.projection_range.contains(&line.range.start));
                assert_eq!(fragments.map(|line| &projection.text()[line.range.clone()]).collect::<String>(), projection.text()[segment.projection_range.clone()]);
            }
            let released = build_edit_locked_adaptive_plan(&projection, 1280., 250., Some(&locked), false, &measurement, None);
            assert!(!released.slots.contains_key(&node));
            let narrow = build_edit_locked_adaptive_plan(&projection, 500., 800., Some(&locked), false, &measurement, Some(node));
            assert!(!narrow.slots.contains_key(&node));
            assert_eq!(document.snapshot().serialize().unwrap(), edited);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn locking_title_and_intro_preserves_their_vertical_gap(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let document = Document::from_markdown("# Document title\n\nA short introductory paragraph.\n\n## Next section\n\nMore content.\n").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
            let node = projection.segments()[0].node_id;
            let locked = build_edit_locked_adaptive_plan(&projection, 1100., 800., Some(&plan), false, &measurement, Some(node));
            let before = build_measured_visual_lines(&projection, &HashMap::new(), 1100., &plan, Some(&measurement));
            let after = build_measured_visual_lines(&projection, &HashMap::new(), 1100., &locked, Some(&measurement));
            assert_eq!(before.len(), after.len());
            for (before, after) in before.iter().zip(&after) {
                assert_eq!(before.range, after.range);
                assert!((before.y - after.y).abs() < 0.01, "gap moved for {:?}: {} -> {}", before.range, before.y, after.y);
            }
        });
    }

    #[gpui::test]
    fn focused_stack_keeps_its_prose_measure_when_the_window_grows(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let document = Document::from_markdown(
                "A paragraph that keeps its reading measure while editing.\n",
            )
            .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan =
                build_measured_adaptive_plan(&projection, 600., 800., None, false, &measurement);
            let node = projection.segments()[0].node_id;
            let locked = build_edit_locked_adaptive_plan(
                &projection,
                1280.,
                800.,
                Some(&plan),
                false,
                &measurement,
                Some(node),
            );
            assert_eq!(locked.slots[&node].width(1280.), 600.);
            let initial = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                600.,
                &plan,
                Some(&measurement),
            );
            let widened = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1280.,
                &locked,
                Some(&measurement),
            );
            assert_eq!(
                initial
                    .iter()
                    .map(|line| line.range.clone())
                    .collect::<Vec<_>>(),
                widened
                    .iter()
                    .map(|line| line.range.clone())
                    .collect::<Vec<_>>()
            );
            let released = build_edit_locked_adaptive_plan(
                &projection,
                1280.,
                800.,
                Some(&locked),
                false,
                &measurement,
                None,
            );
            assert!(!released.slots.contains_key(&node));
        });
    }

    #[gpui::test]
    fn focused_list_grid_retains_its_columns_at_a_wider_canvas(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let mut document =
                Document::from_markdown("- North\n- South\n- East\n- West\n- Above\n- Below\n")
                    .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan =
                build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
            let node = projection.segments()[0].node_id;
            let slot = plan.slots[&node];
            let locked = build_edit_locked_adaptive_plan(
                &projection,
                1280.,
                800.,
                Some(&plan),
                false,
                &measurement,
                Some(node),
            );
            assert_eq!(locked.slots[&node].columns, slot.columns);
            assert_eq!(locked.slots[&node].width(1280.), slot.width(1100.));
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: 0..0,
                    text: "More detail ".repeat(400),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let typing = build_measured_adaptive_plan(
                &projection,
                1280.,
                800.,
                Some(&locked),
                true,
                &measurement,
            );
            assert_eq!(typing.slots[&node].width(1280.), slot.width(1100.));
            let background = build_edit_locked_adaptive_plan(
                &projection,
                1280.,
                800.,
                Some(&typing),
                false,
                &measurement,
                Some(node),
            );
            assert_eq!(background.slots[&node].width(1280.), slot.width(1100.));
            let narrow = build_edit_locked_adaptive_plan(
                &projection,
                500.,
                800.,
                Some(&locked),
                false,
                &measurement,
                Some(node),
            );
            assert!(!narrow.slots.contains_key(&node));
        });
    }

    #[gpui::test]
    fn growing_a_sibling_card_keeps_its_column_until_reconsideration(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
        let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
        let source = "### Alpha\n\nA short explanation.\n\n### Beta\n\nA second explanation.\n\n### Gamma\n\nA third explanation.\n";
        let mut document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let plan = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
        let node = projection
            .segments()
            .iter()
            .find(|s| projection.text()[s.projection_range.clone()] == *"A second explanation.")
            .unwrap()
            .node_id;
        let slot = plan.slots[&node];
        assert_eq!(slot.item, 1);
        document
            .apply(EditCommand::ReplaceText {
                node_id: node,
                range: 0..0,
                text: "Extra detail. ".repeat(50),
                typing: true,
                selection_after: None,
            })
            .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let typing = build_measured_adaptive_plan(&projection, 1100., 800., Some(&plan), true, &measurement);
        assert_eq!(typing.slots[&node], slot);
        let reconsidered = build_measured_adaptive_plan(&projection, 1100., 800., Some(&typing), false, &measurement);
        assert!(!reconsidered.slots.contains_key(&node));
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn typing_in_an_automatic_grid_preserves_columns_selection_and_undo(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(
                    "1. North\n2. South\n3. East\n4. West\n5. Above\n6. Below\n",
                )
                .unwrap(),
                window,
                cx,
            )
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                // A new document intentionally starts stacked. Install the
                // same native-measured preparation produced by its worker
                // before exercising typing inside the automatically chosen grid.
                let (prepared, _, _) = PreparedDocumentView::prepare_snapshot_with_images(
                    &editor.document.snapshot(),
                    &HashMap::new(),
                    None,
                    ReflowViewport {
                        published_geometry: None,
                        width: 760.,
                        height: 800.,
                        zoom: 1.,
                        math_edit_node: None,
                        editing_node: None,
                        table_layout_lock: None,
                        html_disclosures: Arc::default(),
                        html_loaded_images: Arc::default(),
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: None,
                        resource_generation: 0,
                    },
                    &editor.adaptive,
                    &editor.measurement,
                );
                editor.install_prepared(prepared);
                editor.measured_layout = true;
                let id = *editor.adaptive.lists.keys().next().unwrap();
                let before = editor.document.snapshot().serialize().unwrap();
                let node = editor.adaptive.lists[&id].first_node;
                let offset = editor
                    .projection
                    .segment_for_node(node)
                    .unwrap()
                    .projection_range
                    .end;
                let slot = editor.adaptive.slots[&node];
                editor.set_selection(offset..offset, false, window, cx);
                let revision = editor.document.snapshot().revision();
                assert_eq!(editor.selected_byte_range().0, offset..offset);
                assert_eq!(editor.document.snapshot().revision(), revision);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), before);
                let added = " More detail that makes this item considerably longer.".repeat(6);
                assert!(editor.replace_range(offset..offset, &added, true, window, cx));
                assert_eq!(
                    editor.adaptive.slots[&node], slot,
                    "typing never changes the arrangement"
                );
                assert_eq!(
                    editor.selected_byte_range().0,
                    offset + added.len()..offset + added.len()
                );
                editor.perform_undo(window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), before);
            })
        });
    }
}
