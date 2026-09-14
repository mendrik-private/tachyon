use super::*;
use crate::adaptive::rows::RowKind;

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
    narrative: bool,
    bibliography: Vec<(NodeId, NodeId)>,
    figure_text: Option<(NodeId, crate::FigureTextRole)>,
    quote_roles: Vec<(NodeId, crate::quotes::TextRole)>,
    metadata: bool,
    editorial: Option<crate::adaptive::editorial::Member>,
    lead: bool,
    steps: bool,
    starts_document: bool,
    math_edit: Option<NodeId>,
    expanded_code_tail: Option<NodeId>,
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
    records: bool,
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
    footnotes: crate::footnotes::Numbering,
    command_strip_lock: Option<(NodeId, Option<u32>)>,
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
                bytes = bytes.checked_add(projection.segments()[index].projection_len())?;
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
                        records: projection.uses_record_layout(root.id(), width),
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
                editorial: presentation.editorials.get(&root.id()).copied(),
                content: measurement::ContentIdentity(projection.block_handle(root.id())?.clone()),
                narrative: indexes
                    .first()
                    .is_some_and(|&i| projection.segments()[i].context.narrative),
                bibliography: indexes
                    .iter()
                    .filter_map(|&i| {
                        let segment = &projection.segments()[i];
                        segment
                            .context
                            .bibliography
                            .map(|owner| (segment.node_id, owner))
                    })
                    .collect(),
                figure_text: indexes
                    .first()
                    .and_then(|&i| projection.segments()[i].context.figure_text),
                quote_roles: indexes
                    .iter()
                    .filter_map(|&i| {
                        let segment = &projection.segments()[i];
                        crate::quotes::TextRole::of(&segment.context)
                            .map(|role| (segment.node_id, role))
                    })
                    .collect(),
                metadata: indexes
                    .first()
                    .is_some_and(|&i| projection.segments()[i].context.metadata),
                lead: presentation.lead == Some(root.id()),
                steps: presentation
                    .lists
                    .get(&root.id())
                    .is_some_and(|list| list.layout == ListLayout::Steps),
                starts_document: indexes
                    .first()
                    .is_some_and(|&index| projection.segments()[index].projection_start() == 0),
                math_edit: projection.preview_edit_node.filter(|node| {
                    indexes
                        .iter()
                        .any(|&index| projection.segments()[index].node_id == *node)
                }),
                expanded_code_tail: projection.expanded_code_tail.filter(|node| {
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
            footnotes: projection.footnotes.numbering.clone(),
            command_strip_lock: projection.command_strip_lock,
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

#[derive(Clone)]
pub(super) struct ComponentIndex {
    bounds: HashMap<NodeId, ComponentGeometry>,
    headings: Vec<(f32, NodeId)>,
    paint_bottoms: Vec<f32>,
}

pub(super) struct SimpleComponentRebase {
    pub id: NodeId,
    pub geometry: ComponentGeometry,
    pub heading_y: Option<f32>,
    pub old_y_after: f32,
    pub y_delta: f32,
    pub paint_start: usize,
    pub paint_end: usize,
    pub line_bottoms: Vec<f32>,
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
            .map(|(y, _)| {
                // A row can contain several source-ordered headings. Scrolling
                // identifies that row, not its last column; use its first entry.
                self.headings[self.headings.partition_point(|(top, _)| top < y)].1
            })
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

    /// Rebase a suffix after a localized edit whose component-bearing lines
    /// are byte-for-byte identical in geometry and whose following lines only
    /// move upward. The compact component map and prefix-max array avoid
    /// revisiting every large visual-line record.
    pub fn shift_suffix_up(&mut self, old_y_after: f32, y_delta: f32, paint_end: usize) {
        debug_assert!(y_delta < 0.);
        for geometry in self.bounds.values_mut() {
            if geometry.top >= old_y_after {
                geometry.top += y_delta;
                geometry.bottom += y_delta;
            } else if geometry.bottom > old_y_after {
                geometry.bottom += y_delta;
            }
        }
        for (y, _) in &mut self.headings {
            if *y >= old_y_after {
                *y += y_delta;
            }
        }
        let floor = paint_end
            .checked_sub(1)
            .and_then(|index| self.paint_bottoms.get(index))
            .copied()
            .unwrap_or_default();
        for bottom in self.paint_bottoms.iter_mut().skip(paint_end) {
            if *bottom > floor {
                *bottom = (*bottom + y_delta).max(floor);
            }
        }
    }

    pub fn replace_simple_and_shift_up(&mut self, rebase: SimpleComponentRebase) {
        let SimpleComponentRebase {
            id,
            geometry,
            heading_y,
            old_y_after,
            y_delta,
            paint_start,
            paint_end,
            line_bottoms,
        } = rebase;
        debug_assert!(y_delta < 0.);
        let index_delta = line_bottoms.len() as isize - (paint_end - paint_start) as isize;
        for (candidate, bounds) in &mut self.bounds {
            if *candidate == id {
                continue;
            }
            if bounds.top >= old_y_after {
                bounds.top += y_delta;
                bounds.bottom += y_delta;
            } else if bounds.bottom > old_y_after {
                bounds.bottom += y_delta;
            }
            if bounds.first_line >= paint_end {
                bounds.first_line = bounds
                    .first_line
                    .checked_add_signed(index_delta)
                    .expect("localized component line delta must remain in bounds");
            }
        }
        self.bounds.insert(id, geometry);
        for (y, candidate) in &mut self.headings {
            if *candidate == id {
                if let Some(heading_y) = heading_y {
                    *y = heading_y;
                }
            } else if *y >= old_y_after {
                *y += y_delta;
            }
        }
        self.headings.sort_by(|a, b| a.0.total_cmp(&b.0));

        let before = paint_start
            .checked_sub(1)
            .and_then(|index| self.paint_bottoms.get(index))
            .copied()
            .unwrap_or_default();
        let old_after = paint_end
            .checked_sub(1)
            .and_then(|index| self.paint_bottoms.get(index))
            .copied()
            .unwrap_or(before);
        let mut running = before;
        let replacement = line_bottoms
            .iter()
            .map(|line_bottom| {
                running = running.max(*line_bottom);
                running
            })
            .collect::<Vec<_>>();
        self.paint_bottoms
            .splice(paint_start..paint_end, replacement);
        for bottom in self
            .paint_bottoms
            .iter_mut()
            .skip(paint_start + line_bottoms.len())
        {
            *bottom = if *bottom > old_after {
                (*bottom + y_delta).max(running)
            } else {
                running
            };
        }
    }
}

/// One linear pass at geometry publication, replacing full-document scans for
/// every visible code header, quote, callout and list during scroll frames.
pub(super) fn component_geometry(
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    width: f32,
    zoom: f32,
    paint_order: &[usize],
) -> ComponentIndex {
    let mut components = HashMap::<NodeId, ComponentGeometry>::new();
    let mut headings = Vec::new();
    let mut row_headings = HashMap::<(NodeId, usize), (f32, usize)>::new();
    let segments = projection.segments();
    let mut segment_index = 0usize;
    let mut previous_line_start = None;
    for (index, line) in lines.iter().enumerate() {
        let monotonic = previous_line_start.is_none_or(|start| start <= line.projected_start());
        previous_line_start = Some(line.projected_start());
        let segment = if monotonic {
            while segment_index + 1 < segments.len()
                && segments[segment_index + 1].projection_start() <= line.projected_start()
            {
                segment_index += 1;
            }
            segments.get(segment_index).filter(|candidate| {
                candidate
                    .projection_range()
                    .contains(&line.projected_start())
                    || candidate.projection_range() == line.projected_range()
                    || candidate.projection_end() == line.projected_end()
            })
        } else {
            projection.segment_for_range(&line.projected_range())
        };
        let Some(segment) = segment else {
            continue;
        };
        let geometry = ComponentGeometry {
            left_fraction: line.x_fraction + line.inset / width.max(1.),
            right_fraction: line.x_fraction + line.width_fraction,
            top: line.y,
            bottom: line.y + line.style.line_height,
            first_line: index,
        };
        if line.projected_start() == segment.projection_start()
            && matches!(
                projection.block(segment.node_id),
                Some(BlockNode::Heading(_))
            )
        {
            let y = if let Some(slot) = line.slot.filter(|slot| slot.columns > 1) {
                // Table/code chrome can offset peer glyphs slightly. Their
                // first headings still identify one visual row. Only normalize
                // the first heading in each column; nested headings keep their
                // own positions, and subsequent grid rows are distinct.
                let row = row_headings
                    .entry((slot.group, slot.row))
                    .or_insert((line.y, 0));
                let column = 1 << slot.track_start;
                if row.1 & column == 0 {
                    row.1 |= column;
                    row.0
                } else {
                    line.y
                }
            } else {
                line.y
            };
            headings.push((y, segment.node_id));
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
        .chain(segment.context.list_ancestors.iter().copied())
        .chain(segment.context.quote_ancestors.iter().copied())
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
            let mut geometry = geometry;
            if line.table_cell.is_none()
                && let Some(level) = segment
                    .context
                    .quote_ancestors
                    .iter()
                    .position(|ancestor| *ancestor == id)
            {
                // A quote consisting only of another quote still owns its
                // own rail and padding. Aggregate each ancestor's geometry,
                // not the innermost paragraph edge for every rail.
                let descendants = &segment.context.quote_ancestors[level + 1..];
                geometry.left_fraction -=
                    descendants.len() as f32 * DocumentStyle::QUOTE_INSET * zoom / width.max(1.);
                geometry.right_fraction -=
                    level as f32 * DocumentStyle::QUOTE_INSET * zoom / width.max(1.);
                if let Some(edges) = projection.container_edges(segment.node_id) {
                    if line.projected_start() == segment.projection_start() {
                        geometry.top -= descendants
                            .iter()
                            .filter(|id| edges.starts.contains(id))
                            .count() as f32
                            * DocumentStyle::QUOTE_PADDING
                            * zoom;
                    }
                    if line.projected_end() == segment.projection_end() {
                        geometry.bottom += descendants
                            .iter()
                            .filter(|id| edges.ends.contains(id))
                            .count() as f32
                            * DocumentStyle::QUOTE_PADDING
                            * zoom;
                    }
                }
            }
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
    let label_break = if cards && let BlockNode::Paragraph(paragraph) = block {
        crate::adaptive::authored_label_end(paragraph).map(|end| segment.projection_start() + end)
    } else {
        None
    };
    let mut breaks = label_break.into_iter().collect::<Vec<_>>();
    if cards && segment.context.list_depth > 0 {
        let text = &projection.text()[segment.projection_range()];
        let stages = text.split('→').collect::<Vec<_>>();
        if (3..=6).contains(&stages.len())
            && stages
                .iter()
                .all(|stage| !stage.trim().is_empty() && stage.len() <= 48)
        {
            breaks.extend(
                text.match_indices('→')
                    .map(|(offset, _)| segment.projection_start() + offset),
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
    plan.prose_measures = measurement.prose_measures();
    plan.editing_node = editing_node;
    plan.retain_reference_rhythm(projection, previous, keep_arrangements, editing_node);
    plan.retain_editorials(projection, previous, keep_arrangements);
    plan.windows = crate::adaptive::rows::planning_windows(&plan, projection);
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
    let mut wrap_cache = HashMap::new();
    let presentation = plan.clone();
    crate::adaptive::rows::measure_rows(
        &mut plan,
        projection,
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
        |range| {
            *wrap_cache
                .entry((range.start, range.end))
                .or_insert_with(|| {
                    figure_flow::prefers_wrap(
                        projection,
                        &presentation,
                        &roots[range],
                        viewport,
                        &resources,
                    )
                })
        },
    );
    // A retained typing layout must not require reading down one screen and
    // scrolling back up for the next prose column. Recheck current content,
    // including synchronous text refreshes whose retained heights are stale.
    let mut oversized_prose = Vec::new();
    for row in &mut plan.measured_rows.chosen {
        if row.widths.len() != 2
            || !matches!(
                row.kind,
                RowKind::Opening | RowKind::Peer | RowKind::Explanation
            )
        {
            continue;
        }
        let heights = row
            .parts
            .iter()
            .zip(&row.widths)
            .map(|(part, width)| {
                let cards = row.kind == RowKind::Peer;
                cache
                    .entry((part.start, part.end, width.to_bits(), cards))
                    .or_insert_with(|| {
                        measure_row_group(
                            projection,
                            &roots[part.clone()],
                            &segments,
                            *width,
                            cards,
                            &resources,
                            &presentation,
                        )
                    })
                    .map(|m| m.height)
            })
            .collect::<Option<Vec<_>>>();
        if let Some(heights) = heights.filter(|heights| {
            viewport.is_finite()
                && viewport > 0.
                && heights
                    .iter()
                    .all(|height| height.is_finite() && *height > 0. && *height <= viewport)
        }) {
            row.heights = heights;
            row.height_estimated = false;
        } else {
            oversized_prose.push(row.ids[0]);
        }
    }
    plan.measured_rows
        .chosen
        .retain(|row| !oversized_prose.contains(&row.ids[0]));
    plan.slots
        .retain(|_, slot| !oversized_prose.contains(&slot.group));
    plan.measure_lists(
        projection,
        width,
        previous,
        keep_arrangements,
        |node, width, cards| {
            let segment = projection.segment_for_node(node)?;
            let block = projection.block(node)?;
            measurement.cached_list_item(projection, node, width, cards, || {
                if let Some(resource) = presentation.resources.get(&node) {
                    return resource::measure(projection, segment, *resource, width, measurement);
                }
                let markerless = cards
                    && (segment.context.ordered_list_depth > 0
                        || matches!(projection.block(segment.top_level_node_id), Some(BlockNode::List(list)) if crate::adaptive::list_has_authored_labels(list)));
                let padding = if cards && segment.context.ordered_list_depth > 0 {
                    CARD_PADDING
                } else {
                    0.
                };
                let available = segment_text_width(
                    segment,
                    projection,
                    if cards {
                        width - padding * 2. + 8.
                    } else {
                        width.min(presentation.prose_measures.reference)
                    },
                ) + if markerless { container_inset(segment) } else { 0. };
                let breaks = card_presentation_breaks(projection, segment, cards);
                let mut ranges = Vec::new();
                for_each_display_line_range(
                    &projection.text()[segment.projection_range()],
                    |local| {
                        let mut start = segment.projection_start() + local.start;
                        let end = segment.projection_start() + local.end;
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
                    let mut style = visual_line_style_for(projection, block, segment, &range, None);
                    if cards && breaks.first().is_some_and(|end| range.end <= *end) {
                        style.font_size = DocumentStyle::FEATURE_TITLE_SIZE;
                    }
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
                    height: height
                        + if cards {
                            padding * 2.
                                + if segment.context.ordered_list_depth > 0 {
                                    40.
                                } else {
                                    0.
                                }
                        } else {
                            0.
                        },
                    preferred_width: preferred + (width - available),
                    overflow,
                })
            })
        },
    );
    if !keep_arrangements {
        let mut nearby_gap_lines = HashMap::new();
        let mut after_horizontal_list = false;
        let mut accumulated_lines = 0_usize;
        for root in &roots {
            if let BlockNode::List(list) = root {
                after_horizontal_list = plan
                    .measured_lists
                    .get(&list.id)
                    .is_some_and(|decision| matches!(decision.layout, ListLayout::Grid(_)));
                accumulated_lines = 0;
                continue;
            }
            if !after_horizontal_list {
                continue;
            }
            if !matches!(root, BlockNode::Paragraph(_) | BlockNode::Heading(_)) {
                after_horizontal_list = false;
                continue;
            }
            let reused = previous
                .filter(|old| {
                    plan.compatible_environment(old) && plan.unchanged_root(old, root.id())
                })
                .and_then(|old| old.nearby_list_gap_lines.get(&root.id()))
                .copied();
            let lines = if let Some(lines) = reused {
                lines
            } else {
                if !plan.measures_root(root.id()) {
                    after_horizontal_list = false;
                    continue;
                }
                let Some(indexes) = segments.get(&root.id()) else {
                    after_horizontal_list = false;
                    continue;
                };
                let Some((&start, &end)) = indexes.first().zip(indexes.last()) else {
                    after_horizontal_list = false;
                    continue;
                };
                build_measured_visual_lines_for_segments(
                    projection,
                    resources.images.unwrap_or(&HashMap::new()),
                    width,
                    &plan,
                    Some(measurement),
                    start..end + 1,
                )
                .len()
            };
            nearby_gap_lines.insert(root.id(), lines);
            accumulated_lines = accumulated_lines.saturating_add(lines);
            if accumulated_lines > crate::adaptive::NEARBY_LIST_GRID_GAP_LINES {
                after_horizontal_list = false;
            }
        }
        plan.align_nearby_list_grids(projection, &nearby_gap_lines);
        plan.nearby_list_gap_lines = nearby_gap_lines;
    }
    plan.place_resources(projection, previous, keep_arrangements);
    plan.place_editorials(projection);
    plan.measure_label_rows(projection, previous, |list, width, horizontal, metadata| {
        if metadata {
            super::metadata::measure(projection, list, width, horizontal, measurement)
        } else {
            label_rows::measure(projection, list, width, horizontal, measurement)
        }
    });
    plan.place_editorials(projection);
    plan.place_definitions(projection, previous, |segment| {
        // Strong RTL labels use the source-order stacked form until leading
        // rails support mirrored bidi placement. Never reverse canonical text.
        if measurement::contains_strong_rtl(&projection.text()[segment.projection_range()]) {
            return None;
        }
        measurement.line_width(
            projection,
            segment.projection_range(),
            DocumentStyle::REFERENCE_SIZE,
        )
    });
    inline_lists::measure(
        &mut plan,
        projection,
        previous,
        keep_arrangements,
        measurement,
    );
    figure_flow::measure(
        &mut plan,
        projection,
        viewport,
        previous,
        keep_arrangements,
        &resources,
    );
    prose_flow::measure(
        &mut plan,
        projection,
        width,
        viewport,
        previous,
        keep_arrangements,
        measurement,
    );
    for segment in projection.segments() {
        if !plan.has_measured_geometry(segment.top_level_node_id) {
            continue;
        }
        let available = plan
            .slots
            .get(&segment.node_id)
            .map_or(width, |slot| slot.width(width) - 2. * slot.inset());
        if let Some(leading) =
            code_panel::strip_leading(projection, segment, available, Some(measurement))
        {
            plan.command_strips
                .insert(segment.node_id, leading.to_bits());
        }
    }
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
    if roots
        .first()
        .is_some_and(|root| presentation.editorials.contains_key(&root.id()))
    {
        return editorial::measure(
            projection,
            roots,
            segments,
            width,
            presentation,
            measurement,
        );
    }
    // Peer sections start with headings and remain open. The other enclosed
    // row form is an introduction's companion list, which owns a real panel.
    let padding = if cards && !matches!(roots.first(), Some(BlockNode::Heading(_))) {
        CARD_PADDING
    } else {
        0.
    };
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
                    | BlockNode::Alert { .. }
                    | BlockNode::BlockQuote { .. }
            )
                || matches!(root, BlockNode::Alert { blocks, .. }
                    if cards || blocks.iter().any(|block| !matches!(block.as_ref(), BlockNode::Paragraph(_))))
                || matches!(root, BlockNode::BlockQuote { blocks, .. }
                    if cards || blocks.iter().any(|block| !matches!(block.as_ref(), BlockNode::Paragraph(_)))
                        || segments.get(&root.id()).and_then(|indexes| indexes.first())
                            .is_none_or(|index| projection.segments()[*index].context.margin_note_anchor.is_none()))
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
        if !cards && matches!(root, BlockNode::Table(_) | BlockNode::CodeBlock(_)) {
            result.component_top.get_or_insert(result.height);
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
            result.height += image_reserved_height(projection, root, segment, dimensions, width)?
                + map_preview::footer_height(image);
            // Maps carry evidence labels inside their pixels. At increased
            // text zoom, prefer a full-width stack over a cramped paired map.
            // A smaller authored image is still never enlarged to meet this floor.
            result.overflow |= map_preview::label(image).is_some()
                && width + 0.5 < (intrinsic_width as f32).min(720.);
            result.preferred_width = result.preferred_width.max(intrinsic_width as f32);
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
        if let BlockNode::List(list) = root {
            if crate::adaptive::timeline::is_timeline(list)
                && crate::adaptive::timeline::has_supporting_blocks(list)
            {
                // A rich timeline owns a complete vertical sequence. The
                // compact label-only measurement below must not omit its
                // code, tables or nested supporting blocks to fit a peer row.
                return None;
            }
            if presentation
                .lists
                .get(&list.id)
                .is_some_and(|list| list.layout == ListLayout::Checklist)
            {
                // Count/progress chrome occupies real space above the first
                // task in both open sibling rows and stacked checklists.
                result.height += LAYOUT_HEADER;
            }
            let inner = if cards {
                width - 2. * padding + 8.
            } else {
                width.min(presentation.prose_measures.reference)
            };
            let rows = if presentation.metadata_lists.contains(&list.id) {
                super::metadata::measure(projection, list, inner, false, measurement)
            } else {
                label_rows::measure(projection, list, inner, false, measurement)
            };
            if let Some(rows) = rows {
                if cards && segment_count > 0 {
                    result.height += 4.;
                }
                for (row, (node, columns)) in rows.iter().enumerate() {
                    let segment = projection.segment_for_node(*node)?;
                    let lines =
                        label_rows::build(projection, segment, *columns, Some(measurement))?;
                    result.height += label_rows::height(&lines) + if row > 0 { 12. } else { 0. };
                    result.preferred_width = result.preferred_width.max(if columns.stacked() {
                        columns.label_width + container_inset(segment)
                    } else {
                        columns.label_width
                            + LAYOUT_GAP
                            + columns.body_width
                            + container_inset(segment)
                    });
                }
                segment_count += rows.len();
                continue;
            }
        }
        if indexes.len() > 12 {
            return None;
        }
        for index in indexes {
            let prepared = compact_tree::prepare(&projection.segments()[*index], width);
            let segment = prepared.as_ref();
            let block = projection.block(segment.node_id)?;
            if let Some(resource) = presentation.resources.get(&segment.node_id) {
                let outer = width.min(
                    presentation.prose_measures.reference
                        + resource.inset() * 2.
                        + resource::ICON_GUTTER,
                );
                let item = resource::measure(projection, segment, *resource, outer, measurement)?;
                result.height += item.height
                    + if segment_count > 0 && matches!(root, BlockNode::List(_)) {
                        LAYOUT_GAP
                    } else {
                        0.
                    };
                result.preferred_width = result.preferred_width.max(item.preferred_width);
                result.overflow |= item.overflow;
                segment_count += 1;
                continue;
            }
            let text = block.text()?;
            if text.len() > 4096 || crate::math::is_math(block) {
                return None;
            }
            let figure_width = segment.context.figure_text.and_then(|(id, role)| {
                if role.gallery_start().is_some() {
                    return Some(width);
                }
                let BlockNode::Image(image) = projection.block(id)? else {
                    return None;
                };
                let empty = HashMap::new();
                resolved_image_dimensions(image, resources.images.unwrap_or(&empty))
                    .map(|(w, _)| width.min(w as f32))
            });
            let outer = if let Some(figure_width) = figure_width {
                figure_width
            } else if !cards
                && matches!(block, BlockNode::Paragraph(_))
                && segment.context.table_cell.is_none()
                && segment.context.alert.is_none()
            {
                nested_measures::reading_width(presentation, projection, segment, width)
            } else {
                width
            };
            let inner = if cards {
                outer - 2. * padding + 8.
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
                lead.then_some(DocumentStyle::LEAD_SIZE),
            );
            if let Some(first) = lines.first()
                && let Some(diagram) = &first.diagram
            {
                let inset = first.inset
                    + if diagram.source_visible {
                        CODE_BLOCK_PADDING
                    } else {
                        8.
                    };
                let preferred = diagram.light.width + inset;
                result.preferred_width = result.preferred_width.max(preferred);
                result.overflow |= preferred > inner + 0.5;
                if !diagram.source_visible {
                    // The source range identifies an atomic figure, not a
                    // line of Mermaid text. Its native figure and scroll rail
                    // already own the realized height; outer gaps belong to
                    // the shared group-spacing pass.
                    if cards {
                        return None;
                    }
                    result.height += first.style.line_height;
                    segment_count += 1;
                    continue;
                }
            }
            for line in &lines {
                let shaped = measurement.line_width(
                    projection,
                    line.projected_range(),
                    if lead {
                        DocumentStyle::LEAD_SIZE
                    } else {
                        line.style.font_size
                    },
                )?;
                result.overflow |= shaped > available + 0.5;
                result.height += if lead {
                    DocumentStyle::LEAD_LEADING
                } else {
                    line.style.line_height
                };
                if !cards {
                    result.height += line.style.space_above + line.style.space_below;
                }
            }
            let mut preferred = 0_f32;
            let mut valid = true;
            for_each_display_line_range(&projection.text()[segment.projection_range()], |range| {
                let range = segment.projection_start() + range.start
                    ..segment.projection_start() + range.end;
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
                    if lead {
                        DocumentStyle::LEAD_SIZE
                    } else {
                        style.font_size
                    },
                ) {
                    preferred = preferred.max(w);
                } else {
                    valid = false;
                }
            });
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
                        _ if lines.first()?.rendered_code_preview() => DocumentStyle::EQUATION_GAP,
                        BlockNode::Heading(_) => lines.first()?.style.space_above,
                        BlockNode::CodeBlock(_) => 8.,
                        _ => 0.,
                    };
                }
                if Some(index) == indexes.last() {
                    let external = if lines.last()?.rendered_code_preview() {
                        DocumentStyle::EQUATION_GAP
                    } else if segment.context.figure_text.is_some()
                        || super::quotes::has_no_outer_paragraph_margin(segment, block)
                    {
                        0.
                    } else if segment.context.bibliography.is_some() {
                        DocumentStyle::BIBLIOGRAPHY_GAP
                    } else if matches!(block, BlockNode::Heading(_)) {
                        7.
                    } else {
                        16.
                    };
                    // A list item has only 8px of trailing space. Match the
                    // renderer's saturating removal; never subtract a margin
                    // that was not present in the measured component.
                    result.height -= external.min(lines.last()?.style.space_below);
                }
            }
            segment_count += 1;
        }
    }
    if cards {
        result.height += 2. * padding;
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
        let available = line.width_fraction * width - line.inset - line.code_gutter() - 12.;
        overflow |=
            measurement.line_width(projection, line.projected_range(), line.style.font_size)?
                > available + 0.5;
    }
    Some(crate::adaptive::rows::GroupMeasurement {
        height,
        preferred_width: measured.preferred.iter().sum(),
        overflow,
        component_top: Some(0.),
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
        let prepared = compact_tree::prepare(segment, width);
        let segment = prepared.as_ref();
        let segment_index = segments.start + local;
        let Some(block) = projection.block(segment.node_id) else {
            continue;
        };
        let slot = plan.slots.get(&segment.node_id).copied();
        if let Some(list) = plan.inline_lists.get(&segment.node_id) {
            lines.extend(inline_lists::build(
                projection,
                segment,
                list,
                width,
                measurement,
            ));
            continue;
        }
        if let Some(flow) = plan.figure_flows.get(&segment.node_id) {
            lines.extend(figure_flow::build(
                projection,
                segment,
                flow,
                width,
                measurement,
            ));
            continue;
        }
        if let Some(flow) = plan.prose_flows.get(&segment.node_id) {
            lines.extend(prose_flow::build(
                projection,
                segment,
                flow,
                width,
                measurement,
            ));
            continue;
        }
        if slot.is_some_and(|s| matches!(s.card_accent, crate::adaptive::CardAccent::Editorial(_)))
        {
            let slot = slot.unwrap();
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
            let mut node_lines = editorial::build_segment(
                projection,
                segment,
                if matches!(block, BlockNode::CodeBlock(_)) {
                    slot.width(width)
                } else {
                    slot.width(width)
                        .min(plan.prose_measures.reference + 2. * CARD_PADDING)
                },
                first,
                last,
                measurement.filter(|_| plan.has_measured_geometry(segment.top_level_node_id)),
            );
            for line in &mut node_lines {
                line.set_slot(Some(slot));
                line.x_fraction = slot.left(width) / width;
                line.width_fraction = slot.width(width) / width;
            }
            lines.extend(node_lines);
            continue;
        }
        if let Some(crate::adaptive::CardAccent::Resource(resource)) =
            slot.map(|slot| slot.card_accent)
        {
            let slot = slot.unwrap();
            let mut node_lines = resource::build(
                projection,
                segment,
                resource,
                slot.width(width),
                measurement,
            );
            for line in &mut node_lines {
                line.set_slot(Some(slot));
                line.x_fraction = slot.left(width) / width;
                line.width_fraction = slot.width(width) / width;
            }
            lines.extend(node_lines);
            continue;
        }
        let wide = matches!(
            block,
            BlockNode::CodeBlock(_) | BlockNode::Image(_) | BlockNode::Heading(_)
        ) || segment.context.table_cell.is_some()
            || segment
                .context
                .figure_text
                .is_some_and(|(_, role)| role.gallery_start().is_some());
        let measure = nested_measures::reading_width(plan, projection, segment, width);
        let segment_width = slot.map_or_else(
            || if wide { width } else { width.min(measure) },
            |slot| slot.width(width),
        );
        // The visible image and its authored labels share one leading edge
        // and intrinsic, non-upscaled measure, even inside a wider module.
        let figure = segment
            .context
            .figure_text
            .filter(|(_, role)| role.gallery_start().is_none())
            .map(|(id, _)| id)
            .or_else(|| matches!(block, BlockNode::Image(_)).then_some(segment.node_id))
            // Table measurement must retain the table's available width.
            // The image is fitted inside its resolved cell, not treated as an
            // independent intrinsic-width parent for the entire table.
            .filter(|_| segment.context.table_cell.is_none());
        let segment_width = figure
            .and_then(|id| {
                let BlockNode::Image(image) = projection.block(id)? else {
                    return None;
                };
                let (intrinsic, _) = resolved_image_dimensions(image, image_dimensions)?;
                let ancestry = if segment.context.list_depth > 0 {
                    container_inset(segment)
                } else {
                    0.
                };
                Some(
                    slot.map_or(width, |s| s.width(width))
                        .min(intrinsic as f32 + ancestry),
                )
            })
            .unwrap_or(segment_width);
        let steps = plan
            .lists
            .get(&segment.top_level_node_id)
            .is_some_and(|list| list.layout == ListLayout::Steps);
        let markerless_card = slot.is_some_and(|slot| {
            slot.cards
                && (slot.card_accent == crate::adaptive::CardAccent::Numbered
                    || (slot.group == segment.top_level_node_id
                        && slot.card_accent == crate::adaptive::CardAccent::OpenLabeled
                        && segment.context.list_depth == 1))
        });
        let content_width = segment_width
            + if markerless_card {
                container_inset(segment)
            } else {
                0.
            }
            - if slot.is_some_and(|slot| slot.cards) {
                // The segment helper already subtracts the ordinary 8 px
                // trailing inset. Cards replace it with symmetric padding.
                slot.unwrap().inset() * 2. - 8.
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
        let font = (plan.lead == Some(segment.node_id)).then_some(DocumentStyle::LEAD_SIZE);
        let label_lines = plan.label_rows.get(&segment.node_id).and_then(|columns| {
            label_rows::build(projection, segment, *columns, segment_measurement)
        });
        let mut node_lines = if let Some(lines) = label_lines {
            lines
        } else if let Some(reuse) = extensions.as_deref_mut()
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
        // Keep discretionary suffixes with the geometry that chose the breaks.
        // Painting may retain these lines while a typography reflow is pending.
        let hyphens = segment_measurement
            .map(|fonts| fonts.hyphen_breaks(projection, segment))
            .unwrap_or_default();
        for line in &mut node_lines {
            let hyphenated = hyphens.binary_search(&line.projected_range().end).is_ok();
            if line.hyphenated != hyphenated {
                line.hyphenated = hyphenated;
            }
        }
        // Compact equations belong to the surrounding reading column. A long
        // formula may negotiate the whole canvas before needing its own pan.
        // Use the already prepared glyph width, not TeX character count.
        let segment_width = if slot.is_none()
            && segment.context.table_cell.is_none()
            && node_lines.first().is_some_and(|line| {
                line.rendered_code_preview()
                    && line
                        .preview_image(false)
                        .is_some_and(|image| image.width <= measure - 8.)
            }) {
            segment_width.min(measure)
        } else {
            segment_width
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
            line.set_slot(slot);
            if segment.context.table_cell.is_none() {
                line.width_fraction = segment_width / width;
            }
            if let Some(slot) = slot {
                line.x_fraction = slot.left(width) / width;
                if matches!(
                    projection.block(slot.group),
                    Some(BlockNode::Definition { .. })
                ) {
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
                    // Remove outside paragraph margins only at the column
                    // edges. Rich descriptions retain internal block spacing,
                    // code headers, quote padding and semantic table geometry.
                    if line.table_cell.is_none() {
                        if index == 0 && first {
                            let external = match block {
                                _ if line.rendered_code_preview() => DocumentStyle::EQUATION_GAP,
                                BlockNode::Heading(_) => 20.,
                                BlockNode::CodeBlock(_) | BlockNode::Image(_) => 8.,
                                _ => 0.,
                            };
                            line.style.space_above = (line.style.space_above - external).max(0.);
                        }
                        if index + 1 == count && last {
                            let external = if line.rendered_code_preview() {
                                DocumentStyle::EQUATION_GAP
                            } else if matches!(block, BlockNode::Heading(_)) {
                                7.
                            } else {
                                16.
                            };
                            line.style.space_below = (line.style.space_below - external).max(0.);
                        }
                    }
                }
                if slot.cards {
                    line.inset += slot.inset();
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
                        slot.inset()
                            + if slot.card_accent == crate::adaptive::CardAccent::Numbered {
                                40.
                            } else {
                                0.
                            }
                    } else {
                        0.
                    };
                    if slot.card_accent == crate::adaptive::CardAccent::Numbered
                        || (slot.group == segment.top_level_node_id
                            && slot.card_accent == crate::adaptive::CardAccent::OpenLabeled
                            && segment.context.list_depth == 1)
                    {
                        line.inset -= container_inset(segment);
                    }
                    line.style.space_below = if index + 1 == count {
                        if last {
                            slot.inset()
                        } else if line.label_row.is_some() {
                            12.
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
            if segment.context.quote_pull && slot.is_none_or(|s| s.columns == 1 && !s.cards) {
                // Quote and attribution share the panel's leading inset,
                // including while the source-order stack is being edited.
                line.x_fraction = 0.;
            }
            if plan.lead == Some(segment.node_id) {
                line.style.font_size = DocumentStyle::LEAD_SIZE;
                line.style.line_height = line.style.line_height.max(DocumentStyle::LEAD_LEADING);
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
    for line in &mut lines {
        if line.table_record.is_some()
            && let Some(segment) = projection.segment_for_range(&line.projected_range())
            && segment
                .context
                .table_cell
                .is_some_and(|(_, _, column)| column == 0)
            && line.projected_start() == segment.projection_start()
        {
            line.gap_before = table_records::RECORD_GAP;
        }
    }
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
        let Some(segment) = projection.segment_for_range(&lines[start].projected_range()) else {
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
                    .segment_for_range(&line.projected_range())
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
                let first_segment = projection
                    .segment_for_range(&first.projected_range())
                    .unwrap();
                let first_block = projection.block(first_segment.node_id).unwrap();
                let external = match first_block {
                    _ if first.rendered_code_preview() => DocumentStyle::EQUATION_GAP,
                    BlockNode::Heading(_) => {
                        if first.projected_start() == 0 {
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
                let last_segment = projection
                    .segment_for_range(&last.projected_range())
                    .unwrap();
                let last_block = projection.block(last_segment.node_id).unwrap();
                let external = if last.rendered_code_preview() {
                    DocumentStyle::EQUATION_GAP
                } else if super::quotes::has_no_outer_paragraph_margin(last_segment, last_block) {
                    0.
                } else if last_segment.context.bibliography.is_some() {
                    DocumentStyle::BIBLIOGRAPHY_GAP
                } else if matches!(last_block, BlockNode::Heading(_)) {
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
        previous_slot = lines[end - 1].slot;
        start = end;
    }
}

impl RichDocumentEditor {
    pub(super) fn render_task_summaries(
        &self,
        visible: &[usize],
        palette: TachyonPalette,
    ) -> Vec<AnyElement> {
        let mut seen = HashSet::new();
        visible
            .iter()
            .filter_map(|index| {
                let line = &self.visual_lines[*index];
                let segment = self.projection.segment_for_range(&line.projected_range())?;
                let id = segment.top_level_node_id;
                let list = self.adaptive.lists.get(&id)?;
                if list.layout != ListLayout::Checklist || !seen.insert(id) {
                    return None;
                }
                let first = &self.visual_lines[self.components.get(&id)?.first_line];
                Some(
                    div()
                        .absolute()
                        .top(px(first.y - (LAYOUT_HEADER - 4.) * self.zoom_factor))
                        .left(px(first.x_fraction * self.layout_width
                            + first.slot.map_or(0., |slot| slot.inset())
                                * self.zoom_factor))
                        .w(px((first
                            .slot
                            .filter(|s| s.group == id && s.columns > 1)
                            .map_or(self.layout_width * first.width_fraction, |s| {
                                s.fixed_canvas
                                    .unwrap_or(self.layout_width / self.zoom_factor)
                                    * self.zoom_factor
                            })
                            - 24. * self.zoom_factor)
                            .max(1.)))
                        .text_size(px(DocumentStyle::CAPTION_SIZE * self.zoom_factor))
                        .line_height(px(DocumentStyle::CAPTION_LEADING * self.zoom_factor))
                        .text_color(rgb(palette.secondary))
                        .child(
                            div()
                                .text_right()
                                .child(format!("{} of {} complete", list.completed, list.count)),
                        )
                        .child(
                            div()
                                .mt(px(8. * self.zoom_factor))
                                .h(px(8. * self.zoom_factor))
                                .w_full()
                                .rounded(px(4. * self.zoom_factor))
                                .bg(rgb(palette.surface_quiet))
                                .child(
                                    div()
                                        .h_full()
                                        .w(relative(
                                            list.completed as f32 / list.count.max(1) as f32,
                                        ))
                                        .rounded(px(4. * self.zoom_factor))
                                        .bg(rgb(palette.accent)),
                                ),
                        )
                        .into_any_element(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::adaptive::PROSE_WIDTH;
    use crate::adaptive::rows::RowKind;

    #[test]
    fn compact_component_rebase_matches_removed_wrap_geometry() {
        let heading = NodeId::new_unchecked(1);
        let body = NodeId::new_unchecked(2);
        let mut index = ComponentIndex {
            bounds: HashMap::from([
                (
                    heading,
                    ComponentGeometry {
                        left_fraction: 0.,
                        right_fraction: 1.,
                        top: 0.,
                        bottom: 40.,
                        first_line: 0,
                    },
                ),
                (
                    body,
                    ComponentGeometry {
                        left_fraction: 0.,
                        right_fraction: 1.,
                        top: 100.,
                        bottom: 120.,
                        first_line: 2,
                    },
                ),
            ]),
            headings: vec![(0., heading), (100., body)],
            paint_bottoms: vec![20., 40., 120.],
        };

        index.replace_simple_and_shift_up(SimpleComponentRebase {
            id: heading,
            geometry: ComponentGeometry {
                left_fraction: 0.,
                right_fraction: 1.,
                top: 0.,
                bottom: 20.,
                first_line: 0,
            },
            heading_y: Some(0.),
            old_y_after: 100.,
            y_delta: -20.,
            paint_start: 0,
            paint_end: 2,
            line_bottoms: vec![20.],
        });

        assert_eq!(index.bounds[&heading].bottom, 20.);
        assert_eq!(index.bounds[&body].top, 80.);
        assert_eq!(index.bounds[&body].bottom, 100.);
        assert_eq!(index.bounds[&body].first_line, 1);
        assert_eq!(index.headings, [(0., heading), (80., body)]);
        assert_eq!(index.paint_bottoms, [20., 100.]);
    }

    #[gpui::test]
    fn adjacent_optional_callouts_share_normal_reading_width(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!("../../../../performance/layout-fixtures/01-field-notes.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let measure = fonts
                .prose_measures()
                .fit_width(f32::INFINITY, false, false);
            let canvas = measure * 2. + LAYOUT_GAP;
            let plan =
                build_measured_adaptive_plan(&projection, canvas, 1500., None, false, &fonts);
            let alerts = projection
                .roots()
                .filter(|root| matches!(root, BlockNode::Alert { .. }))
                .map(BlockNode::id)
                .collect::<Vec<_>>();
            assert_eq!(alerts.len(), 2);
            assert!(
                plan.measured_rows
                    .chosen
                    .iter()
                    .any(|row| row.kind == RowKind::Guidance),
                "guidance candidates: {:?}",
                plan.measured_rows
                    .candidates
                    .iter()
                    .filter(|row| row.kind == RowKind::Guidance)
                    .collect::<Vec<_>>()
            );
            let alerts = alerts
                .iter()
                .map(|id| {
                    projection
                        .segments()
                        .iter()
                        .find(|s| s.top_level_node_id == *id)
                        .unwrap()
                        .node_id
                })
                .collect::<Vec<_>>();
            let left = plan
                .slots
                .get(&alerts[0])
                .expect("optional note shares a guidance row");
            let right = plan
                .slots
                .get(&alerts[1])
                .expect("adjacent tip shares the row");
            assert_eq!(left.group, right.group);
            assert_eq!((left.item, right.item), (0, 1));
            assert!(
                !left.cards && !right.cards,
                "callouts own their existing panels"
            );
            assert!((left.width(canvas) - measure).abs() < 0.01);
            assert_eq!(plan.measured_rows.validation_fallbacks, 0);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn optional_callout_rows_preserve_boundaries_resize_and_focused_growth(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let note =
                "> [!NOTE]\n> Keep the complete source and its original context available.\n\n";
            let tip =
                "> [!TIP]\n> Read the associated guidance before choosing a presentation.\n\n";
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                let canvas = fonts
                    .prose_measures()
                    .fit_width(f32::INFINITY, false, false)
                    * 2.
                    + LAYOUT_GAP;
                for (body, expected) in [
                    (format!("{note}{tip}"), true),
                    (format!("{tip}{note}"), true),
                    (
                        format!("{note}{tip}").replace("[!NOTE]", "[!WARNING]"),
                        false,
                    ),
                    (
                        format!("{note}{tip}").replace("[!TIP]", "[!CAUTION]"),
                        false,
                    ),
                    (
                        format!("{note}{tip}").replace("[!TIP]", "[!IMPORTANT]"),
                        false,
                    ),
                    (format!("{note}## Another section\n\n{tip}"), false),
                    (
                        format!("{note}An intervening explanation stays here.\n\n{tip}"),
                        false,
                    ),
                    (
                        format!(
                            "{note}> [!TIP]\n> - A nested list remains a structured notice.\n\n"
                        ),
                        false,
                    ),
                    (
                        format!(
                            "{note}> [!TIP]\n> {}\n\n",
                            "A much longer explanation. ".repeat(30)
                        ),
                        false,
                    ),
                    (
                        format!(
                            "{note}> [!TIP]\n> {}\n\n",
                            "A much longer explanation. ".repeat(12)
                        ),
                        false,
                    ),
                ] {
                    let source = format!("# Guidance\n\n{body}");
                    let mut document = Document::from_markdown(source.as_str()).unwrap();
                    let projection = TextProjection::from_snapshot(&document.snapshot());
                    let ready = build_measured_adaptive_plan(
                        &projection,
                        canvas,
                        1400.,
                        None,
                        false,
                        &fonts,
                    );
                    assert_eq!(
                        ready
                            .measured_rows
                            .chosen
                            .iter()
                            .any(|row| row.kind == RowKind::Guidance),
                        expected,
                        "{source}"
                    );
                    assert_eq!(ready.measured_rows.validation_fallbacks, 0);
                    if expected {
                        let title = projection.roots().next().unwrap().id();
                        assert!(
                            !ready.slots.contains_key(&title),
                            "shared heading stays above both notices"
                        );
                        for (width, height) in [(650., 1400.), (canvas, 60.)] {
                            let stack = build_measured_adaptive_plan(
                                &projection,
                                width,
                                height,
                                Some(&ready),
                                false,
                                &fonts,
                            );
                            assert!(
                                !stack
                                    .measured_rows
                                    .chosen
                                    .iter()
                                    .any(|row| row.kind == RowKind::Guidance)
                            );
                            let restored = build_measured_adaptive_plan(
                                &projection,
                                canvas,
                                1400.,
                                Some(&stack),
                                false,
                                &fonts,
                            );
                            assert!(ready.geometry_key().matches(&restored));
                        }
                        let cell = projection
                            .segments()
                            .iter()
                            .find(|s| s.context.alert.is_some())
                            .unwrap()
                            .node_id;
                        let slot = ready.slots[&cell];
                        document
                            .apply(EditCommand::ReplaceText {
                                node_id: cell,
                                range: 0..0,
                                text: "Expanded guidance ".repeat(60),
                                typing: true,
                                selection_after: None,
                            })
                            .unwrap();
                        let edited = TextProjection::from_snapshot(&document.snapshot());
                        let held = build_edit_locked_adaptive_plan(
                            &edited,
                            canvas,
                            1400.,
                            Some(&ready),
                            false,
                            &fonts,
                            Some(cell),
                        );
                        assert_eq!(held.slots[&cell], slot);
                        let lines = build_measured_visual_lines(
                            &edited,
                            &HashMap::new(),
                            canvas,
                            &held,
                            Some(&fonts),
                        );
                        let segment = edited.segment_for_node(cell).unwrap();
                        let ranges = lines
                            .iter()
                            .filter(|line| {
                                segment.projection_range().contains(&line.projected_start())
                            })
                            .map(|line| line.projected_range())
                            .collect::<Vec<_>>();
                        assert_eq!(ranges.first().unwrap().start, segment.projection_start());
                        assert_eq!(ranges.last().unwrap().end, segment.projection_end());
                        assert!(ranges.windows(2).all(|pair| {
                            edited.text()[pair[0].end..pair[1].start].trim().is_empty()
                        }));
                        let released = build_measured_adaptive_plan(
                            &edited,
                            canvas,
                            1400.,
                            Some(&held),
                            false,
                            &fonts,
                        );
                        assert!(
                            !released
                                .measured_rows
                                .chosen
                                .iter()
                                .any(|row| row.kind == RowKind::Guidance)
                        );
                        document.undo().unwrap();
                    }
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                }
            }
        });
    }

    #[gpui::test]
    fn compact_table_leaves_more_tracks_for_its_wide_comparison(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/03-technical-reference.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let plan = build_measured_adaptive_plan(&projection, 1314., 1500., None, false, &fonts);
            let heading =
                |name| {
                    projection.roots().find(|root|
                matches!(root, BlockNode::Heading(h) if h.content.as_cow() == name)).unwrap().id()
                };
            let compact = plan.slots[&heading("Configuration")];
            let comparison = plan.slots[&heading("Comparison across environments")];
            assert_eq!(compact.group, comparison.group);
            assert_eq!(
                (compact.span, comparison.span),
                (4, 8),
                "a compact table should leave more tracks for its wider comparison"
            );
            assert_eq!(plan.measured_rows.validation_fallbacks, 0);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn narrow_table_tracks_require_natural_fit_and_preserve_resize_and_editing(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!("../../../../performance/layout-fixtures/03-technical-reference.md");
            let start = source.find("## Configuration").unwrap();
            let split = source.find("## Comparison across environments").unwrap();
            let end = source.find("## Exact code").unwrap();
            let compact = &source[start..split];
            let wide = &source[split..end];
            for (body, expected) in [
                (format!("{compact}{wide}"), Some((3, 9))),
                (format!("{wide}{compact}"), Some((9, 3))),
                (format!("{compact}{wide}").replace("light |", "a substantially longer setting value that needs a full reading measure |"), None),
                (format!("{compact}{wide}").replace("## Configuration", "## Configuration with a deliberately long descriptive title"), None),
                (format!("{compact}{wide}").replace("## Configuration\n\n", "## Configuration\n\nAn authored explanation stays with these settings.\n\n"), None),
            ] {
                let source = format!("# Reference\n\n{body}");
                let mut document = Document::from_markdown(source.as_str()).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
                fonts.measure_tables(&mut projection);
                let ready = build_measured_adaptive_plan(&projection, 1600., 1500., None, false, &fonts);
                let rail = ready.measured_rows.chosen.iter().find(|row|
                    row.widths.len() == 2 && crate::adaptive::rows::TEMPLATES[row.template].contains(&3));
                assert_eq!(rail.map(|row| {
                    let spans = crate::adaptive::rows::TEMPLATES[row.template];
                    (spans[0], spans[1])
                }), expected);
                if expected.is_some() {
                    for (width, height) in [(650., 1500.), (1600., 180.)] {
                        let stacked = build_measured_adaptive_plan(&projection, width, height, Some(&ready), false, &fonts);
                        assert!(stacked.measured_rows.chosen.iter().all(|row| row.kind == RowKind::Stack));
                        let restored = build_measured_adaptive_plan(&projection, 1600., 1500., Some(&stacked), false, &fonts);
                        assert!(ready.geometry_key().matches(&restored));
                    }
                    let cell = projection.segments().iter().find(|s| projection.text()[s.projection_range()].trim() == "light").unwrap().node_id;
                    let slot = ready.slots[&cell];
                    document.apply(EditCommand::ReplaceText {
                        node_id: cell, range: 0..0, text: "Expanded setting ".repeat(50), typing: true, selection_after: None,
                    }).unwrap();
                    let mut edited = TextProjection::from_snapshot(&document.snapshot());
                    fonts.measure_tables(&mut edited);
                    let held = build_edit_locked_adaptive_plan(&edited, 1600., 1500., Some(&ready), false, &fonts, Some(cell));
                    assert_eq!(held.slots[&cell], slot);
                    document.undo().unwrap();
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                assert_eq!(ready.measured_rows.validation_fallbacks, 0);
            }
        });
    }

    #[gpui::test]
    fn specification_and_matching_authored_example_negotiate_one_technical_row(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/03-technical-reference.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let plan = build_measured_adaptive_plan(&projection, 1314., 1500., None, false, &fonts);
            let heading =
                |name| {
                    projection.roots().find(|root|
                matches!(root, BlockNode::Heading(h) if h.content.as_cow() == name)).unwrap().id()
                };
            let specification = heading("Parameters");
            let example = heading("Request");
            let left = plan
                .slots
                .get(&specification)
                .expect("specification shares the technical row");
            let right = plan
                .slots
                .get(&example)
                .expect("authored example keeps its enclosure in the row");
            assert_eq!(left.group, right.group);
            assert_eq!((left.item, right.item), (0, 1));
            assert!(!left.cards && right.cards);
            assert_eq!(
                plan.editorials[&example].kind,
                crate::adaptive::editorial::Kind::Request
            );
            assert_eq!(plan.measured_rows.validation_fallbacks, 0);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn specification_example_resize_and_editing_preserve_complete_units(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/03-technical-reference.md");
            for zoom in [1., 1.5, 2.] {
                let mut document = Document::from_markdown(source).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                fonts.measure_tables(&mut projection);
                let ready =
                    build_measured_adaptive_plan(&projection, 1314., 1500., None, false, &fonts);
                let example = projection
                    .roots()
                    .find(|root| {
                        matches!(root,
                    BlockNode::Heading(h) if h.content.as_cow() == "Request")
                    })
                    .unwrap()
                    .id();
                let pair = ready.slots[&example];
                assert_eq!(pair.columns, 2);
                for (width, height) in [(650., 1500.), (1314., 180.)] {
                    let stack = build_measured_adaptive_plan(
                        &projection,
                        width,
                        height,
                        Some(&ready),
                        false,
                        &fonts,
                    );
                    assert_eq!(stack.slots[&example].columns, 1);
                    let restored = build_measured_adaptive_plan(
                        &projection,
                        1314.,
                        1500.,
                        Some(&stack),
                        false,
                        &fonts,
                    );
                    assert!(
                        ready.geometry_key().matches(&restored),
                        "same space restores the complete specification/example"
                    );
                }
                let code = projection
                    .roots()
                    .find(|root| {
                        matches!(root,
                    BlockNode::CodeBlock(c) if c.language.as_deref() == Some("json"))
                    })
                    .unwrap()
                    .id();
                document
                    .apply(EditCommand::ReplaceText {
                        node_id: code,
                        range: 0..0,
                        text: "Additional authored content.\n".repeat(100),
                        typing: true,
                        selection_after: None,
                    })
                    .unwrap();
                let mut edited = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut edited);
                let held = build_edit_locked_adaptive_plan(
                    &edited,
                    1314.,
                    1500.,
                    Some(&ready),
                    false,
                    &fonts,
                    Some(code),
                );
                assert_eq!(
                    held.slots[&example], pair,
                    "typing growth retains the row and editorial enclosure"
                );
                let lines = build_measured_visual_lines(
                    &edited,
                    &HashMap::new(),
                    1314.,
                    &held,
                    Some(&fonts),
                );
                let segment = edited.segment_for_node(code).unwrap();
                let rendered = lines
                    .iter()
                    .filter(|line| segment.projection_range().contains(&line.projected_start()))
                    .map(|line| &edited.text()[line.projected_range()])
                    .collect::<String>();
                // Code hard breaks are source separators, not painted glyphs.
                assert_eq!(
                    rendered,
                    edited.text()[segment.projection_range()].replace('\n', "")
                );
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn prose_columns_stack_when_the_current_viewport_cannot_hold_the_row(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!("../../../../performance/layout-fixtures/129-small-fish.md");
            for zoom in [1., 1.5, 2.] {
                let mut document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                let ready =
                    build_measured_adaptive_plan(&projection, 1725., 1500., None, false, &fonts);
                let row = ready
                    .measured_rows
                    .chosen
                    .iter()
                    .find(|row| row.kind == RowKind::Opening)
                    .unwrap();
                let height = row.heights.iter().copied().fold(0., f32::max);
                let body = projection.roots().nth(1).unwrap().id();
                for keep in [false, true] {
                    let short = build_edit_locked_adaptive_plan(
                        &projection,
                        1725.,
                        height - 1.,
                        Some(&ready),
                        keep,
                        &fonts,
                        Some(body),
                    );
                    assert!(!short.slots.contains_key(&body), "zoom={zoom}, keep={keep}");
                }
                document
                    .apply(EditCommand::ReplaceText {
                        node_id: body,
                        range: 0..0,
                        text: "A longer argument continues here. ".repeat(500),
                        typing: true,
                        selection_after: None,
                    })
                    .unwrap();
                let edited = TextProjection::from_snapshot(&document.snapshot());
                for keep in [false, true] {
                    let grown = build_edit_locked_adaptive_plan(
                        &edited,
                        1725.,
                        600.,
                        Some(&ready),
                        keep,
                        &fonts,
                        Some(body),
                    );
                    assert!(
                        !grown.slots.contains_key(&body),
                        "grown prose must stack: zoom={zoom}, keep={keep}"
                    );
                }
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                let restored =
                    build_measured_adaptive_plan(&projection, 1725., 1500., None, false, &fonts);
                assert!(restored.slots.contains_key(&body));
            }
        });
    }

    #[gpui::test]
    fn opening_lead_and_overview_share_width_below_the_authored_title(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            // Mock shaping checks topology/source. Native README captures
            // independently verify the loaded-font measures and appearance.
            let source = include_str!("../../../../performance/layout-fixtures/129-small-fish.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let roots = projection.roots().map(BlockNode::id).collect::<Vec<_>>();
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 1725., 1500., None, false, &fonts);
            assert_eq!(plan.lead, Some(roots[1]));
            assert!(
                !plan.slots.contains_key(&roots[0]),
                "the title spans the opening"
            );
            let lead = plan
                .slots
                .get(&roots[1])
                .expect("measured equal-width opening");
            let overview = plan
                .slots
                .get(&roots[2])
                .expect("complete adjacent overview");
            assert_eq!(lead.group, overview.group);
            assert_eq!(lead.span, overview.span);
            assert_eq!(lead.width(1725.), overview.width(1725.));
            assert_eq!((lead.item, overview.item), (0, 1));
            assert!(!lead.cards && !overview.cards);
            assert!(
                !plan.slots.contains_key(&roots[3]),
                "the next heading stays outside"
            );
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1725.,
                &plan,
                Some(&fonts),
            );
            for segment in projection
                .segments()
                .iter()
                .filter(|s| roots[..4].contains(&s.node_id))
            {
                let actual = lines
                    .iter()
                    .filter(|line| segment.projection_range().contains(&line.projected_start()))
                    .map(|line| &projection.text()[line.projected_range()])
                    .collect::<String>();
                assert_eq!(actual, projection.text()[segment.projection_range()]);
            }
            let first = |id| {
                lines
                    .iter()
                    .find(|line| {
                        segment_for_line(&projection, &line.projected_range())
                            .unwrap()
                            .node_id
                            == id
                    })
                    .unwrap()
            };
            assert_eq!(first(roots[1]).y, first(roots[2]).y);
            assert_eq!(first(roots[1]).style.font_size, DocumentStyle::LEAD_SIZE);
            assert_eq!(first(roots[2]).style.font_size, DocumentStyle::READING_SIZE);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn opening_resize_recovery_keeps_focused_source_and_typography(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            use crate::adaptive::rows::RowKind;
            let source =
                include_str!("../../../../performance/layout-fixtures/129-small-fish.md");
            for zoom in [1., 1.5, 2.] {
                let mut document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let roots = projection.roots().map(BlockNode::id).collect::<Vec<_>>();
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                let ready =
                    build_measured_adaptive_plan(&projection, 1725., 1500., None, false, &fonts);
                assert!(ready.measured_rows.chosen.iter().any(|row| row.kind == RowKind::Opening));
                for (width, height) in [(650., 1500.), (1725., 304.)] {
                    let stack = build_measured_adaptive_plan(
                        &projection,
                        width,
                        height,
                        Some(&ready),
                        false,
                        &fonts,
                    );
                    assert!(
                        !stack
                            .measured_rows
                            .chosen
                            .iter()
                            .any(|r| r.kind == RowKind::Opening)
                    );
                    let restored = build_measured_adaptive_plan(
                        &projection,
                        1725.,
                        1500.,
                        Some(&stack),
                        false,
                        &fonts,
                    );
                    assert!(ready.geometry_key().matches(&restored));
                    let title_focus = build_edit_locked_adaptive_plan(
                        &projection,
                        1725.,
                        1500.,
                        Some(&stack),
                        false,
                        &fonts,
                        Some(roots[0]),
                    );
                    assert!(ready.geometry_key().matches(&title_focus));
                    for body in &roots[1..3] {
                        let held = build_edit_locked_adaptive_plan(
                            &projection,
                            1725.,
                            1500.,
                            Some(&stack),
                            false,
                            &fonts,
                            Some(*body),
                        );
                        assert!(
                            !held
                                .measured_rows
                                .chosen
                                .iter()
                                .any(|r| r.kind == RowKind::Opening)
                        );
                        let released = build_measured_adaptive_plan(
                            &projection,
                            1725.,
                            1500.,
                            Some(&held),
                            false,
                            &fonts,
                        );
                        assert!(ready.geometry_key().matches(&released),
                            "opening ownership recovers after blur: zoom={zoom}, from={width}x{height}, body={body:?}");
                    }
                }
                for body in &roots[1..3] {
                    document
                        .apply(EditCommand::ReplaceText {
                            node_id: *body,
                            range: 0..0,
                            text: "More detail. ".repeat(250),
                            typing: true,
                            selection_after: None,
                        })
                        .unwrap();
                    let edited = TextProjection::from_snapshot(&document.snapshot());
                    let focused = build_edit_locked_adaptive_plan(
                        &edited,
                        1725.,
                        1500.,
                        Some(&ready),
                        false,
                        &fonts,
                        Some(*body),
                    );
                    assert!(focused.measured_rows.chosen.iter().all(|r| r.kind != RowKind::Opening),
                        "grown prose must fit the viewport before retaining columns");
                    assert!(!focused.slots.contains_key(body));
                    let lines = build_measured_visual_lines(
                        &edited,
                        &HashMap::new(),
                        1725.,
                        &focused,
                        Some(&fonts),
                    );
                    let range = edited
                        .segment_for_node(*body)
                        .unwrap()
                        .projection_range();
                    assert_eq!(
                        lines
                            .iter()
                            .filter(|line| range.contains(&line.projected_start()))
                            .map(|line| &edited.text()[line.projected_range()])
                            .collect::<String>(),
                        edited.text()[range]
                    );
                    let short = build_edit_locked_adaptive_plan(
                        &edited,
                        1725.,
                        304.,
                        Some(&focused),
                        false,
                        &fonts,
                        Some(*body),
                    );
                    assert!(
                        !short
                            .measured_rows
                            .chosen
                            .iter()
                            .any(|r| r.kind == RowKind::Opening)
                    );
                    document.undo().unwrap();
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                }
            }
        });
    }

    #[gpui::test]
    fn opening_measured_balance_rejects_short_or_disproportionate_prose(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            use crate::adaptive::rows::RowKind;
            let source =
                include_str!("../../../../performance/layout-fixtures/118-opening-overview.md");
            let paragraphs = source.split("\n\n").collect::<Vec<_>>();
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for (lead, overview) in [
                ("A short introduction.".to_owned(), paragraphs[2].to_owned()),
                (paragraphs[1].to_owned(), "A short overview.".to_owned()),
                (paragraphs[1].to_owned(), paragraphs[2].repeat(8)),
                (paragraphs[1].repeat(8), paragraphs[2].to_owned()),
            ] {
                let source =
                    format!("# Tachyon\n\n{lead}\n\n{overview}\n\n## Next\n\nNext section.\n");
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let plan =
                    build_measured_adaptive_plan(&projection, 1314., 1500., None, false, &fonts);
                assert!(
                    !plan
                        .measured_rows
                        .chosen
                        .iter()
                        .any(|r| r.kind == RowKind::Opening)
                );
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn outline_uses_the_first_source_heading_in_a_shared_row(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown(include_str!(
                "../../../../performance/layout-fixtures/79-technical-sections.md"
            ))
            .unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let plan = build_measured_adaptive_plan(&projection, 1040., 926., None, false, &fonts);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1040.,
                &plan,
                Some(&fonts),
            );
            let order = (0..lines.len()).collect::<Vec<_>>();
            let index = component_geometry(&projection, &lines, 1040., 1., &order);
            let heading = |name: &str| {
                projection
                    .segments()
                    .iter()
                    .find(|s| projection.text()[s.projection_range()] == *name)
                    .unwrap()
                    .node_id
            };
            let first = heading("Configuration options");
            let second = heading("Configuration file");
            let y = index.get(&first).unwrap().top;
            assert_eq!(y, index.get(&second).unwrap().top);
            assert_eq!(
                index.active_heading(y, 400.),
                Some(first),
                "a shared row must not make the last sibling the active outline entry"
            );
            assert_eq!(index.active_heading(y + 60., 400.), Some(first));
            let mut offset_lines = lines.clone();
            let second_start = projection
                .segment_for_node(second)
                .unwrap()
                .projection_start();
            offset_lines
                .iter_mut()
                .find(|line| line.projected_start() == second_start)
                .unwrap()
                .y -= 0.5;
            let offset_index = component_geometry(&projection, &offset_lines, 1040., 1., &order);
            assert_eq!(
                offset_index.active_heading(y, 400.),
                Some(first),
                "peer chrome offsets must not change the active source heading"
            );
            let next = heading("Notes for the team");
            let next_start = projection
                .segment_for_node(next)
                .unwrap()
                .projection_start();
            let first_start = projection
                .segment_for_node(first)
                .unwrap()
                .projection_start();
            let mut slot = lines
                .iter()
                .find(|line| line.projected_start() == first_start)
                .unwrap()
                .slot
                .unwrap();
            // A later heading inside the same column, or at the start of a
            // subsequent grid row, must not inherit the earlier row's anchor.
            for item in [0, slot.columns] {
                let mut later_lines = lines.clone();
                slot.item = item;
                slot.row = item / slot.columns;
                later_lines
                    .iter_mut()
                    .find(|line| line.projected_start() == next_start)
                    .unwrap()
                    .set_slot(Some(slot));
                let later = component_geometry(&projection, &later_lines, 1040., 1., &order);
                assert_eq!(
                    later.active_heading(index.get(&next).unwrap().top, 400.),
                    Some(next)
                );
            }
            assert_eq!(
                index.active_heading(index.get(&next).unwrap().top, 400.),
                Some(next)
            );
        });
    }

    #[gpui::test]
    fn technical_sibling_rhythm_preserves_chapter_separation(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/79-technical-sections.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for width in [360., 480.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    1.,
                );
                let plan = AdaptivePlan::build(&projection, width, None, false);
                let mut lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                for zoom in [1., 2.] {
                    if zoom == 2. {
                        scale_visual_lines(&mut lines, 2.);
                    }
                    for (label, gap) in [
                        ("Configuration file", 32.),
                        ("Notes for the team", 64.),
                        ("A separate chapter", 64.),
                        ("Environment file", 32.),
                    ] {
                        let index = lines
                            .iter()
                            .position(|line| projection.text()[line.projected_range()] == *label)
                            .unwrap();
                        let previous_bottom = lines[..index]
                            .iter()
                            .map(|line| line.y + line.style.line_height + line.style.space_below)
                            .fold(0.0_f32, f32::max);
                        let actual = lines[index].y - previous_bottom;
                        assert!(
                            (actual - gap * zoom).abs() < 0.01,
                            "{label}: {actual}, expected {}",
                            gap * zoom
                        );
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn opening_rhythm_is_tighter_than_later_sections_in_measured_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            for (source, expected) in [
                ("# Title\n\nIntroductory prose.\n\n## First section\n\nBody.\n\n## Later section\n\nMore.\n", 48.),
                ("# Title\n\nIntroductory prose.\n\nA second opening paragraph.\n\n## First section\n\nBody.\n\n## Later section\n\nMore.\n", 48.),
                ("# Title\n\n## First section\n\nBody.\n\n## Later section\n\nMore.\n", 28.),
                ("# Title\n\nIntroductory prose.\n\n- **Status:** Draft\n- **Owner:** Team\n\n## First section\n\nBody.\n\n## Later section\n\nMore.\n", 48.),
                ("Preamble.\n\n# Not the opening title\n\nProse.\n\n## First section\n\nBody.\n\n## Later section\n\nMore.\n", 64.),
                ("# Title\n\n### Earlier subsection\n\nProse.\n\n## First section\n\nBody.\n\n## Later section\n\nMore.\n", 64.),
                ("# Title\n\n- First item\n- Second item\n\nProse after content.\n\n## First section\n\nBody.\n\n## Later section\n\nMore.\n", 64.),
                ("# Title\n\n```sh\ncommand\n```\n\nProse after code.\n\n## First section\n\nBody.\n\n## Later section\n\nMore.\n", 64.),
            ] {
                let document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                for width in [360., 1040.] {
                    let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
                    let plan = AdaptivePlan::build(&projection, width, None, false);
                    let mut lines = build_measured_visual_lines(&projection, &HashMap::new(), width, &plan, Some(&fonts));
                    for zoom in [1., 2.] {
                        if zoom == 2. { scale_visual_lines(&mut lines, 2.); }
                        let line_index = |name: &str| lines.iter().position(|line| {
                            projection.segment_for_range(&line.projected_range()).is_some_and(|segment| {
                                projection.text()[segment.projection_range()] == *name
                            })
                        }).unwrap();
                        let first = line_index("First section");
                        let before = &lines[first - 1];
                        let gap = lines[first].y - (before.y + before.style.line_height + before.style.space_below);
                        assert!((gap - expected * zoom).abs() < 0.01,
                            "opening gap {gap} != {} at width {width}, zoom {zoom}", expected * zoom);
                        let later = &lines[line_index("Later section")];
                        assert_eq!(later.gap_before, 64. * zoom,
                            "later section retains chapter separation");
                    }
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn rich_cell_image_height_uses_the_fitted_inner_column_width(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for metadata in ["[240,360]", "[null,null]", "[240,null]"] {
            let source = include_str!("../../../../performance/layout-fixtures/83-rich-cell-images.md")
                .replace("[240,360]", metadata);
            let document = Document::from_markdown(source.as_str()).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let images = projection.image_segments().map(|s| s.node_id).collect::<Vec<_>>();
            assert_eq!(images.len(), 2, "HTML cells must reach standalone image layout");
            let dimensions = images.iter().map(|id| {
                let BlockNode::Image(image) = projection.block(*id).unwrap() else { panic!("standalone image"); };
                (*id, (image.source.clone(), (400, 240)))
            }).collect::<HashMap<_, _>>();
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            for width in [240., 400., 760., 1280.] {
                let plan = AdaptivePlan::build(&projection, width, None, false);
                let lines = build_measured_visual_lines(&projection, &dimensions, width, &plan, Some(&fonts));
                for id in &images {
                    let segment = projection.segment_for_node(*id).unwrap();
                    let line = lines.iter().find(|line| line.projected_start() == segment.projection_start()).unwrap();
                    let trailing = if line.table_record.is_some() { table_records::INSET } else { 12. };
                    let expected_width = (line.width_fraction * width - line.inset - trailing).clamp(1., 400.);
                    let expected_height = expected_width * 240. / 400.;
                    assert!((line.style.line_height - expected_height).abs() < 0.01,
                        "image height {} != {expected_height} for inner width {expected_width} at canvas {width}", line.style.line_height);
                    assert!(line.y + line.style.line_height <= line.table_row_y + line.table_row_height);
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn rich_cell_image_paint_matches_insets_zoom_and_intrinsic_size(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(include_str!(
                    "../../../../performance/layout-fixtures/83-rich-cell-images.md"
                ))
                .unwrap(),
                window,
                cx,
            )
        });
        cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    1.,
                );
                fonts.measure_tables(&mut editor.projection);
                for (natural_width, natural_height) in [(400, 240), (80, 48)] {
                    editor.image_layout_dimensions = editor
                        .projection
                        .image_segments()
                        .map(|segment| {
                            (
                                segment.node_id,
                                (
                                    segment.context.image_source.clone().unwrap(),
                                    (natural_width, natural_height),
                                ),
                            )
                        })
                        .collect();
                    let plan = AdaptivePlan::build(&editor.projection, 760., None, false);
                    let mut lines = build_measured_visual_lines(
                        &editor.projection,
                        &editor.image_layout_dimensions,
                        760.,
                        &plan,
                        Some(&fonts),
                    );
                    for zoom in [1., 2.] {
                        editor.zoom_factor = zoom;
                        if zoom == 2. {
                            scale_visual_lines(&mut lines, 2.);
                        }
                        let container =
                            Bounds::new(point(px(50.), px(90.)), size(px(760. * zoom), px(2000.)));
                        for segment in editor.projection.image_segments() {
                            let line = lines
                                .iter()
                                .find(|line| line.projected_start() == segment.projection_start())
                                .unwrap();
                            for pan in [0., 80.] {
                                let bounds = image_element_bounds(editor, line, container, pan);
                                let expected_width = (240. * zoom - line.inset - 12. * zoom)
                                    .min(natural_width as f32 * zoom);
                                assert_eq!(bounds.left(), container.left() + px(line.inset - pan));
                                assert!(
                                    (f32::from(bounds.size.width) - expected_width).abs() < 0.01
                                );
                                assert!(
                                    (f32::from(bounds.size.height) - expected_width * 0.6).abs()
                                        < 0.01
                                );
                                assert_eq!(bounds.top(), container.top() + px(line.y));
                            }
                        }
                    }
                }
            })
        });
    }

    #[gpui::test]
    fn explicit_margin_notes_anchor_to_only_the_immediately_preceding_paragraph(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/101-margin-notes.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 1314., 1366., None, false, &fonts);
            let rows = plan
                .measured_rows
                .chosen
                .iter()
                .filter(|row| row.kind == RowKind::Aside)
                .collect::<Vec<_>>();
            assert_eq!(
                rows.len(),
                2,
                "each explicitly anchored note should use the available rail: {:?}",
                plan.measured_rows
                    .candidates
                    .iter()
                    .filter(|row| row.kind == RowKind::Aside)
                    .collect::<Vec<_>>()
            );
            let roots = projection.roots().collect::<Vec<_>>();
            for row in rows {
                assert_eq!(
                    row.parts[0].len(),
                    1,
                    "earlier paragraphs must stay above the note's anchor"
                );
                assert_eq!(row.parts[1].len(), 1);
                assert_eq!(row.parts[0].end, row.parts[1].start);
                assert!(matches!(roots[row.parts[0].start], BlockNode::Paragraph(_)));
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn margin_note_cluster_uses_one_complete_rail(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/110-margin-note-cluster.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                let plan =
                    build_measured_adaptive_plan(&projection, 1314., 1366., None, false, &fonts);
                let rail = plan
                    .measured_rows
                    .chosen
                    .iter()
                    .find(|row| row.kind == RowKind::Aside)
                    .unwrap_or_else(|| {
                        panic!("complete note cluster should share the available rail at {zoom}")
                    });
                assert_eq!(rail.parts[0].len(), 1);
                assert_eq!(rail.parts[1].len(), 3);
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    1314.,
                    &plan,
                    Some(&fonts),
                );
                let notes = projection
                    .segments()
                    .iter()
                    .filter(|s| s.context.margin_note_anchor.is_some())
                    .collect::<Vec<_>>();
                assert_eq!(notes.len(), 3);
                let mut bottom = 0.;
                for note in notes {
                    let own = lines
                        .iter()
                        .filter(|line| {
                            projection
                                .segment_for_range(&line.projected_range())
                                .unwrap()
                                .node_id
                                == note.node_id
                        })
                        .collect::<Vec<_>>();
                    assert!(
                        own[0].y >= bottom,
                        "notes cannot collide in their shared rail"
                    );
                    assert!(own.iter().all(|line| line.slot.is_some_and(|s| s.item == 1)
                        && line.style.font_size == DocumentStyle::METADATA_SIZE));
                    bottom = own.last().unwrap().y + own.last().unwrap().style.line_height;
                }
                for (width, height) in [(520., 1366.), (657., 683.), (1314., 240.)] {
                    let stack = build_measured_adaptive_plan(
                        &projection,
                        width,
                        height,
                        Some(&plan),
                        false,
                        &fonts,
                    );
                    assert!(
                        stack
                            .measured_rows
                            .chosen
                            .iter()
                            .all(|row| row.kind != RowKind::Aside),
                        "the complete cluster must fall back together"
                    );
                    let restored = build_measured_adaptive_plan(
                        &projection,
                        1314.,
                        1366.,
                        Some(&stack),
                        false,
                        &fonts,
                    );
                    assert_eq!(restored.measured_rows.validation_fallbacks, 0);
                    assert!(
                        restored
                            .measured_rows
                            .chosen
                            .iter()
                            .any(|row| row.kind == RowKind::Aside && row.parts == rail.parts),
                        "the complete rail must return when space is restored"
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn margin_notes_reflow_inline_and_keep_measured_source_anchor_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/101-margin-notes.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for (width, height, zoom, rails) in [
                (1314., 1366., 1., true),
                (520., 1366., 1., false),
                (657., 683., 2., false),
                (1314., 240., 1., false),
            ] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                let plan =
                    build_measured_adaptive_plan(&projection, width, height, None, false, &fonts);
                assert_eq!(plan.measured_rows.validation_fallbacks, 0);
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                for note in projection
                    .segments()
                    .iter()
                    .filter(|s| s.context.margin_note_anchor.is_some() && s.context.quote_first)
                {
                    let anchor = projection
                        .segment_for_node(note.context.margin_note_anchor.unwrap())
                        .unwrap();
                    let first = |node: &crate::ProjectionSegment| {
                        lines
                            .iter()
                            .find(|line| line.projected_start() == node.projection_start())
                            .unwrap()
                    };
                    let note_line = first(note);
                    let anchor_line = first(anchor);
                    assert_eq!(note_line.style.font_size, DocumentStyle::METADATA_SIZE);
                    assert_eq!(note_line.style.line_height, DocumentStyle::METADATA_LEADING);
                    if rails {
                        assert!(note_line.slot.is_some());
                        assert!(anchor_line.slot.unwrap().same_row(note_line.slot.unwrap()));
                        assert!(note_line.x_fraction > anchor_line.x_fraction);
                        assert!(
                            (note_line.y - anchor_line.y).abs()
                                <= DocumentStyle::QUOTE_PADDING + 0.01
                        );
                    } else {
                        assert!(note_line.slot.is_none());
                        let bottom = lines
                            .iter()
                            .filter(|line| {
                                line.projected_start() >= anchor.projection_start()
                                    && line.projected_start() < anchor.projection_end()
                            })
                            .map(|line| line.y + line.style.line_height)
                            .fold(0_f32, f32::max);
                        assert!(
                            note_line.y >= bottom,
                            "inline note follows its complete anchor"
                        );
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn optional_notes_share_space_with_their_prose_only_when_measured_fit_allows(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/80-supporting-notes.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let segment = |prefix: &str| {
                projection
                    .segments()
                    .iter()
                    .find(|segment| {
                        projection.text()[segment.projection_range()].starts_with(prefix)
                    })
                    .unwrap()
            };
            for width in [1400., 1632.] {
                let plan =
                    build_measured_adaptive_plan(&projection, width, 926., None, false, &fonts);
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                for row in plan
                    .measured_rows
                    .chosen
                    .iter()
                    .filter(|row| row.kind == RowKind::Aside)
                {
                    for (item, expected) in row.heights.iter().enumerate() {
                        let members = lines
                            .iter()
                            .filter(|line| {
                                line.slot.is_some_and(|slot| {
                                    slot.group == row.ids[0] && slot.item == item
                                })
                            })
                            .collect::<Vec<_>>();
                        let top = members
                            .iter()
                            .map(|line| line.y - line.style.space_above)
                            .fold(f32::INFINITY, f32::min);
                        let bottom = members
                            .iter()
                            .map(|line| line.y + line.style.line_height + line.style.space_below)
                            .fold(0_f32, f32::max);
                        assert!(
                            (bottom - top - expected).abs() < 0.01,
                            "measured aside footprint must match final geometry: {} != {expected}",
                            bottom - top
                        );
                    }
                }
                for (prose, note) in [
                    ("Start with", "The document remains"),
                    ("Read the explanation", "Review one"),
                ] {
                    let left = plan
                        .slots
                        .get(&segment(prose).node_id)
                        .expect("prose should use a measured main column");
                    let right = plan
                        .slots
                        .get(&segment(note).node_id)
                        .expect("optional note should use a supporting column");
                    assert_eq!(left.group, right.group);
                    assert!(left.width(width) > right.width(width));
                    assert!(left.width(width) <= plan.prose_measures.reference + 0.01);
                    assert!((right.left(width) - left.width(width) - LAYOUT_GAP).abs() < 0.01);
                }
                assert!(
                    !plan
                        .slots
                        .contains_key(&segment("Replacing a file").node_id)
                );
                assert!(
                    !plan
                        .slots
                        .contains_key(&segment("Prepare the workspace").node_id)
                );
            }
            for (width, height) in [(576., 926.), (816., 463.), (1400., 200.)] {
                let plan =
                    build_measured_adaptive_plan(&projection, width, height, None, false, &fonts);
                assert!(
                    !plan
                        .slots
                        .contains_key(&segment("The document remains").node_id)
                );
                let restored = build_measured_adaptive_plan(
                    &projection,
                    1400.,
                    926.,
                    Some(&plan),
                    false,
                    &fonts,
                );
                assert!(
                    restored
                        .slots
                        .contains_key(&segment("The document remains").node_id),
                    "available space must restore the optional aside"
                );
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn table_painted_columns_match_resolved_measurement_widths(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for (widths, nested, zoom) in ["100,300", "100,null", "null,null"]
                .into_iter()
                .flat_map(|widths| [false, true].into_iter().flat_map(move |nested| [1., 2.].into_iter().map(move |zoom| (widths, nested, zoom))))
            {
                let table_source = format!("<!-- tachyon-table:v1 {{\"border\":\"Dotted\",\"widths\":[{widths}]}} -->\n| Label | Description |\n| --- | --- |\n| narrow | A longer explanation with words that need to wrap in the resolved column. |\n");
                let source = if nested {
                    table_source.lines().map(|line| format!("> {line}\n")).collect::<String>()
                } else { table_source };
                let document = Document::from_markdown(source.as_str()).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), zoom);
                fonts.measure_tables(&mut projection);
                let table = projection.segments().iter().find_map(|segment| segment.context.table_cell.map(|(id, _, _)| id)).unwrap();
                for width in [240., 400., 1040.] {
                    let plan = build_measured_adaptive_plan(&projection, width, 900., None, false, &fonts);
                    let mut lines = build_measured_visual_lines(&projection, &HashMap::new(), width, &plan, Some(&fonts));
                    assert!(lines.iter().any(|line| line.table_cell.is_some()));
                    let outer = if nested { DocumentStyle::QUOTE_INSET } else { 0. };
                    let available = projection.table_available_width(table, table_container_width(width, outer));
                    let expected = projection.fitted_table_widths(table, available).unwrap();
                    scale_visual_lines(&mut lines, zoom);
                    for line in lines.iter().filter(|line| line.table_cell.is_some()) {
                        let (_, _, column, _) = line.table_cell.unwrap();
                        assert!((line.width_fraction * width * zoom - expected[column] * zoom).abs() < 0.01,
                            "painted column must equal measured width: {} != {}, canvas={width}, authored={widths}", line.width_fraction * width, expected[column]);
                        let segment = segment_for_line(&projection, &line.projected_range()).unwrap();
                        assert!((segment_text_width(segment, &projection, width) - (expected[column] - 24.).max(1.)).abs() < 0.01);
                        assert!((line.x_fraction * width - outer - expected[..column].iter().sum::<f32>()).abs() < 0.01);
                    }
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn short_code_labels_stay_above_examples_while_explanations_can_pair(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!("../../../../README.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let find = |text: &str| {
                projection
                    .segments()
                    .iter()
                    .find(|segment| projection.text()[segment.projection_range()].starts_with(text))
                    .unwrap()
                    .node_id
            };
            for width in [1040., 1320., 1640.] {
                let plan =
                    build_measured_adaptive_plan(&projection, width, 1100., None, false, &fonts);
                for label in [
                    "Build and open a document:",
                    "Run the repository checks with:",
                ] {
                    assert!(
                        !plan.slots.contains_key(&find(label)),
                        "a short code label belongs above its example: {label}, width={width}"
                    );
                }
                assert_eq!(
                    plan.slots.contains_key(&find("Tachyon requires Linux")),
                    width >= 1320.,
                    "the wider column gap stacks a cramped explanation and retains a measured pair when it fits"
                );
            }
        });
    }

    #[gpui::test]
    fn adjacent_optional_notes_keep_one_supporting_column(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/81-supporting-note-pair.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 1160., 1026., None, false, &fonts);
            let row = plan
                .measured_rows
                .chosen
                .iter()
                .find(|row| row.kind == RowKind::Aside)
                .unwrap();
            assert_eq!(
                row.parts[1].len(),
                2,
                "adjacent supporting notes belong in the same column"
            );
            assert_eq!(row.ids.len(), 3);
            assert!(row.legal());
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1160.,
                &plan,
                Some(&fonts),
            );
            for (item, expected) in row.heights.iter().enumerate() {
                let members = lines
                    .iter()
                    .filter(|line| {
                        line.slot
                            .is_some_and(|slot| slot.group == row.ids[0] && slot.item == item)
                    })
                    .collect::<Vec<_>>();
                let top = members
                    .iter()
                    .map(|line| line.y - line.style.space_above)
                    .fold(f32::INFINITY, f32::min);
                let bottom = members
                    .iter()
                    .map(|line| line.y + line.style.line_height + line.style.space_below)
                    .fold(0_f32, f32::max);
                assert!(
                    (bottom - top - expected).abs() < 0.01,
                    "measured rail height disagrees with final geometry: {} != {expected}",
                    bottom - top
                );
            }
            for (width, height) in [(576., 1026.), (816., 513.), (1160., 200.)] {
                let narrow = build_measured_adaptive_plan(
                    &projection,
                    width,
                    height,
                    Some(&plan),
                    false,
                    &fonts,
                );
                assert!(
                    narrow
                        .measured_rows
                        .chosen
                        .iter()
                        .all(|row| row.kind != RowKind::Aside)
                );
                let restored = build_measured_adaptive_plan(
                    &projection,
                    1160.,
                    1026.,
                    Some(&narrow),
                    false,
                    &fonts,
                );
                assert!(
                    restored
                        .measured_rows
                        .chosen
                        .iter()
                        .any(|row| row.kind == RowKind::Aside && row.parts[1].len() == 2)
                );
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn supporting_note_clusters_do_not_partially_relocate(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/81-supporting-note-pair.md");
            let prefix = source.split("> [!TIP]").next().unwrap();
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for ending in [
                "> [!WARNING]\n> Keep this warning in sequence.\n".to_owned(),
                "> [!TIP]\n> First tip.\n\n> [!NOTE]\n> Third note.\n".into(),
                "> [!TIP]\n> ```rust\n> example();\n> ```\n".into(),
                format!(
                    "> [!TIP]\n> {}\n",
                    "A long supporting explanation. ".repeat(40)
                ),
                "> An ordinary quotation, not optional guidance.\n".into(),
            ] {
                let source = format!("{prefix}{ending}");
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let plan =
                    build_measured_adaptive_plan(&projection, 1160., 1026., None, false, &fonts);
                assert!(
                    plan.measured_rows
                        .chosen
                        .iter()
                        .all(|row| row.kind != RowKind::Aside),
                    "cluster must stay whole: {ending}"
                );
            }
        });
    }

    #[gpui::test]
    fn optional_asides_reject_critical_long_and_cross_section_content(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!("../../../../performance/layout-fixtures/80-supporting-notes.md");
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for replacement in [
                "> [!WARNING]\n> A critical instruction.",
                "> [!CAUTION]\n> A destructive action.",
                "> [!IMPORTANT]\n> A required instruction.",
                "> An ordinary quotation.",
                "## Another topic\n\n> [!NOTE]\n> A separate section.",
                "> [!NOTE]\n> ```rust\n> do_work();\n> ```",
            ] {
                let prefix = source.split("> [!NOTE]").next().unwrap();
                let document = Document::from_markdown(format!("{prefix}{replacement}\n")).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let plan = build_measured_adaptive_plan(&projection, 1160., 926., None, false, &fonts);
                assert!(plan.measured_rows.chosen.iter().all(|row| row.kind != RowKind::Aside), "{replacement}");
            }
            let prefix = source.split("> [!NOTE]").next().unwrap();
            for source in [format!("{prefix}> [!NOTE]\n> {}\n", "Long explanation. ".repeat(100)), "## Brief\n\nA short introduction.\n\n> [!TIP]\n> Keep this below its introduction.\n".into()] {
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let plan = build_measured_adaptive_plan(&projection, 1160., 926., None, false, &fonts);
                assert!(plan.measured_rows.chosen.iter().all(|row| row.kind != RowKind::Aside));
            }
        });
    }

    #[gpui::test]
    fn figure_led_explanation_uses_one_complete_source_order_pair(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/102-figure-led-explanations.md"
            );
            let document = Document::from_markdown(source).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let dimensions = [
                ("layout-candidates.svg", (840, 360)),
                ("measured-components.svg", (600, 180)),
            ]
            .into_iter()
            .map(|(source, size)| (hash(&resolved_image_resource(source, None)), size))
            .collect();
            let prepared = PreparedDocumentView::prepare_snapshot_with_images(
                &document.snapshot(),
                &dimensions,
                None,
                ReflowViewport {
                    published_geometry: None,
                    width: 1725.,
                    height: 1800.,
                    zoom: 1.,
                    preview_edit_node: None,
                    expanded_code_tail: None,
                    editing_node: None,
                    table_layout_lock: None,
                    trace_mode: LayoutTraceMode::Off,
                    visible_roots: None,
                    html_disclosures: Arc::default(),
                    html_loaded_images: Arc::default(),
                    resource_generation: 1,
                },
                &initial.adaptive,
                &fonts,
            )
            .0;
            let roots = prepared.projection.roots().collect::<Vec<_>>();
            let first = roots
                .iter()
                .position(|root| matches!(root, BlockNode::Image(_)))
                .unwrap();
            let row = prepared
                .adaptive
                .measured_rows
                .chosen
                .iter()
                .find(|row| {
                    row.parts.first().is_some_and(|part| part.start == first)
                        && row.parts.len() == 2
                })
                .expect("complete figure and following explanation should share a measured row");
            assert_eq!(row.parts, vec![first..first + 3, first + 3..first + 6]);
            assert_eq!(prepared.adaptive.measured_rows.validation_fallbacks, 0);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn figure_led_pairs_reflow_without_cropping_or_losing_text(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/102-figure-led-explanations.md"
            );
            let document = Document::from_markdown(source).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let dimensions = [
                ("layout-candidates.svg", (840, 360)),
                ("measured-components.svg", (600, 180)),
            ]
            .into_iter()
            .map(|(source, size)| (hash(&resolved_image_resource(source, None)), size))
            .collect();
            for (width, height, zoom, paired) in [
                (1725., 1800., 1., true),
                (1500., 1000., 1., true),
                (2200., 1400., 1., true),
                (520., 1800., 1., false),
                (657., 683., 2., false),
                (1725., 220., 1., false),
            ] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                let prepared = PreparedDocumentView::prepare_snapshot_with_images(
                    &document.snapshot(),
                    &dimensions,
                    None,
                    ReflowViewport {
                        published_geometry: None,
                        width,
                        height,
                        zoom,
                        preview_edit_node: None,
                        expanded_code_tail: None,
                        editing_node: None,
                        table_layout_lock: None,
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: None,
                        html_disclosures: Arc::default(),
                        html_loaded_images: Arc::default(),
                        resource_generation: 1,
                    },
                    &initial.adaptive,
                    &fonts,
                )
                .0;
                let plan = &prepared.adaptive;
                let projection = &prepared.projection;
                let lines = &prepared.visual_lines;
                let roots = projection.roots().collect::<Vec<_>>();
                let rows = plan
                    .measured_rows
                    .chosen
                    .iter()
                    .filter(|row| row.kind == RowKind::FigureExplanation)
                    .collect::<Vec<_>>();
                assert_eq!(
                    rows.len(),
                    usize::from(paired),
                    "{width} × {height}, zoom {zoom}; candidates={:?}",
                    plan.measured_rows
                        .candidates
                        .iter()
                        .filter(|row| row.kind == RowKind::FigureExplanation)
                        .collect::<Vec<_>>()
                );
                assert_eq!(plan.measured_rows.validation_fallbacks, 0);
                for row in rows {
                    let first = projection
                        .segment_for_node(roots[row.parts[0].start].id())
                        .unwrap();
                    let image = lines
                        .iter()
                        .find(|line| line.projected_start() == first.projection_start())
                        .unwrap();
                    assert!(
                        row.widths[0] <= 840. + 0.01,
                        "intrinsic image width caps the track, not only its paint"
                    );
                    assert!((image.style.line_height - row.widths[0] * 360. / 840.).abs() < 0.1);
                    assert!(
                        row.widths[1] <= plan.prose_measures.fit_width(width, false, false) + 0.01
                    );
                    for (part, expected_height) in row.parts.iter().zip(&row.heights) {
                        let part_lines = lines
                            .iter()
                            .filter(|line| {
                                projection
                                    .segment_for_range(&line.projected_range())
                                    .is_some_and(|segment| {
                                        roots[part.clone()]
                                            .iter()
                                            .any(|root| root.id() == segment.top_level_node_id)
                                    })
                            })
                            .collect::<Vec<_>>();
                        assert!(
                            (part_lines.first().unwrap().y - image.y).abs() < 0.1,
                            "figure and explanation share their first baseline band"
                        );
                        let rendered_height = part_lines
                            .iter()
                            .map(|line| line.y + line.style.line_height)
                            .fold(0., f32::max)
                            - part_lines[0].y;
                        assert!(
                            (rendered_height - expected_height).abs() < 0.1,
                            "measured {expected_height}, rendered {rendered_height}"
                        );
                        for line in part_lines {
                            assert!(!line.slot.unwrap().cards);
                        }
                    }
                }
                for segment in projection.segments().iter().filter(|segment| {
                    matches!(
                        projection.block(segment.node_id),
                        Some(BlockNode::Paragraph(_))
                    )
                }) {
                    assert_eq!(
                        lines
                            .iter()
                            .filter(|line| segment
                                .projection_range()
                                .contains(&line.projected_start()))
                            .map(|line| &projection.text()[line.projected_range()])
                            .collect::<String>(),
                        &projection.text()[segment.projection_range()]
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn figure_led_pairs_retain_focus_through_resource_and_text_changes(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/102-figure-led-explanations.md"
            );
            let source = source.split("\n## Section boundaries").next().unwrap();
            let mut document = Document::from_markdown(source).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let dimensions = [
                ("layout-candidates.svg", (840, 360)),
                ("measured-components.svg", (600, 180)),
            ]
            .into_iter()
            .map(|(source, size)| (hash(&resolved_image_resource(source, None)), size))
            .collect();
            let prepare = |document: &Document,
                           previous: &AdaptivePlan,
                           dimensions: &SourceImageDimensions,
                           width,
                           editing_node| {
                PreparedDocumentView::prepare_snapshot_with_images(
                    &document.snapshot(),
                    dimensions,
                    None,
                    ReflowViewport {
                        published_geometry: None,
                        width,
                        height: 1800.,
                        zoom: 1.,
                        preview_edit_node: None,
                        expanded_code_tail: None,
                        editing_node,
                        table_layout_lock: None,
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: None,
                        html_disclosures: Arc::default(),
                        html_loaded_images: Arc::default(),
                        resource_generation: 1,
                    },
                    previous,
                    &fonts,
                )
                .0
            };
            let pending = prepare(&document, &initial.adaptive, &HashMap::new(), 1725., None);
            let body = pending
                .projection
                .segments()
                .iter()
                .find(|s| {
                    pending.projection.text()[s.projection_range()]
                        .starts_with("The first arrangement")
                })
                .unwrap()
                .node_id;
            let caption = pending
                .projection
                .segments()
                .iter()
                .find(|s| {
                    pending.projection.text()[s.projection_range()].starts_with("Caption: Three")
                })
                .unwrap()
                .node_id;
            assert!(
                pending
                    .adaptive
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.kind != RowKind::FigureExplanation)
            );
            let held = prepare(&document, &pending.adaptive, &dimensions, 1725., Some(body));
            assert!(
                held.adaptive
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.kind != RowKind::FigureExplanation),
                "resource arrival cannot move the focused explanation"
            );
            let ready = prepare(&document, &held.adaptive, &dimensions, 1725., None);
            let row = ready
                .adaptive
                .measured_rows
                .chosen
                .iter()
                .find(|row| row.kind == RowKind::FigureExplanation)
                .expect("loaded evidence can compose after blur");
            for node in [caption, body] {
                let end = ready.projection.block(node).unwrap().text().unwrap().len();
                document
                    .apply(EditCommand::ReplaceText {
                        node_id: node,
                        range: end..end,
                        text: " More authored detail.".repeat(120),
                        typing: true,
                        selection_after: None,
                    })
                    .unwrap();
                let edited = prepare(&document, &ready.adaptive, &dimensions, 2200., Some(node));
                let retained = edited
                    .adaptive
                    .measured_rows
                    .chosen
                    .iter()
                    .find(|row| row.kind == RowKind::FigureExplanation)
                    .expect("focused growth retains the pair");
                assert!(retained.edit_locked);
                assert_eq!(retained.widths, row.widths);
                assert_eq!(edited.adaptive.measured_rows.validation_fallbacks, 0);
                let narrow = prepare(&document, &edited.adaptive, &dimensions, 520., Some(node));
                assert!(
                    narrow
                        .adaptive
                        .measured_rows
                        .chosen
                        .iter()
                        .all(|row| row.kind != RowKind::FigureExplanation),
                    "unfittable frozen widths release to source-order stacks"
                );
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn rendered_diagram_rows_use_figure_dimensions_not_source_text(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/84-diagram-composition.md");
            for source in [
                source.to_owned(),
                source.replace(
                    "flowchart LR",
                    &format!(
                        "flowchart LR\n%% {}",
                        "An invisible source comment. ".repeat(30)
                    ),
                ),
            ] {
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                for zoom in [1., 1.5, 2.] {
                    let fonts = FontMeasurement::new(
                        cx.text_system().clone(),
                        "Public Sans Tachyon".into(),
                        zoom,
                    );
                    for width in [1400., 1725., 2142.] {
                        let plan = build_measured_adaptive_plan(
                            &projection,
                            width,
                            1000.,
                            None,
                            false,
                            &fonts,
                        );
                        let row = plan
                            .measured_rows
                            .chosen
                            .iter()
                            .find(|row| row.kind == RowKind::Explanation)
                            .expect("the visible explanation and diagram fit a measured pair");
                        let lines = build_measured_visual_lines(
                            &projection,
                            &HashMap::new(),
                            width,
                            &plan,
                            Some(&fonts),
                        );
                        let diagram = lines.iter().find(|line| line.diagram.is_some()).unwrap();
                        let figure = &diagram.diagram.as_ref().unwrap().light;
                        assert!(figure.width <= row.widths[1]);
                        assert!((row.heights[1] - diagram.style.line_height).abs() < 0.01);
                        assert!(
                            plan.measured_rows
                                .candidates
                                .iter()
                                .filter(|candidate| candidate.kind == RowKind::Explanation
                                    && candidate.widths[1] < figure.width)
                                .all(|candidate| candidate.rejected.is_some())
                        );
                    }
                    for (width, height) in [(600., 1000.), (800., 1000.), (1312., 200.)] {
                        let plan = build_measured_adaptive_plan(
                            &projection,
                            width,
                            height,
                            None,
                            false,
                            &fonts,
                        );
                        assert!(
                            plan.measured_rows
                                .chosen
                                .iter()
                                .all(|row| row.kind != RowKind::Explanation)
                        );
                    }
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn major_checklist_and_properties_share_a_measured_reference_row(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/53-document-grammar.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let roots = projection.roots().collect::<Vec<_>>();
            let first = roots
                .iter()
                .position(|root| root.plain_text() == "Properties and evidence")
                .unwrap();
            for width in [1040., 1312., 1632.] {
                let plan =
                    build_measured_adaptive_plan(&projection, width, 1100., None, false, &fonts);
                let row = plan
                    .measured_rows
                    .chosen
                    .iter()
                    .find(|row| row.kind == RowKind::Technical && row.roots.start == first)
                    .expect("properties and checklist should share a reference row");
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                for (part, expected) in row.parts.iter().zip(&row.heights) {
                    let part_lines = lines
                        .iter()
                        .filter(|line| {
                            projection
                                .segment_for_range(&line.projected_range())
                                .is_some_and(|s| {
                                    roots[part.clone()]
                                        .iter()
                                        .any(|root| root.id() == s.top_level_node_id)
                                })
                        })
                        .collect::<Vec<_>>();
                    let height = part_lines
                        .iter()
                        .map(|line| line.y + line.style.line_height + line.style.space_below)
                        .fold(0., f32::max)
                        - part_lines[0].y;
                    assert!(
                        (height - expected).abs() < 0.01,
                        "predicted {expected} vs realized {height}"
                    );
                }
            }
            for (width, height) in [(600., 1100.), (1312., 200.)] {
                let plan =
                    build_measured_adaptive_plan(&projection, width, height, None, false, &fonts);
                assert!(
                    plan.measured_rows
                        .chosen
                        .iter()
                        .all(|row| { row.kind != RowKind::Technical || row.roots.start != first }),
                    "reference sections must stack at {width}×{height}"
                );
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn technical_trio_can_keep_wide_evidence_above_two_complete_compact_sections(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/76-entity-records.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let width = 833.;
            let plan = build_measured_adaptive_plan(&projection, width, 1500., None, false, &fonts);
            assert!(
                plan.measured_rows
                    .candidates
                    .iter()
                    .any(|row| row.kind == RowKind::Technical && row.roots == (4..8)),
                "the complete pair must reach measurement before fit and scoring decide"
            );
            let pair = plan
                .measured_rows
                .chosen
                .iter()
                .find(|row| row.kind == RowKind::Technical && row.roots == (4..8))
                .expect(
                    "compact technical peers must be measured even beside a larger third sibling",
                );
            assert_eq!(pair.parts, [4..6, 6..8]);
            assert_eq!(plan.measured_rows.validation_fallbacks, 0);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                width,
                &plan,
                Some(&fonts),
            );
            let roots = projection.roots().collect::<Vec<_>>();
            let first = |root: usize| {
                lines
                    .iter()
                    .find(|line| {
                        projection
                            .segment_for_range(&line.projected_range())
                            .is_some_and(|s| s.top_level_node_id == roots[root].id())
                    })
                    .unwrap()
            };
            assert_eq!(first(4).y, first(6).y);
            assert!(
                first(2).slot.is_none(),
                "larger directory retains full width"
            );
            assert!(first(8).y > first(4).y + pair.heights.iter().copied().fold(0., f32::max));
            assert!(
                (first(6).x_fraction * width
                    - (first(4).x_fraction * width + pair.widths[0])
                    - LAYOUT_GAP)
                    .abs()
                    < 0.01
            );
            for (part, expected) in pair.parts.iter().zip(&pair.heights) {
                let bottom = lines
                    .iter()
                    .filter(|line| {
                        projection
                            .segment_for_range(&line.projected_range())
                            .is_some_and(|s| {
                                roots[part.clone()]
                                    .iter()
                                    .any(|r| r.id() == s.top_level_node_id)
                            })
                    })
                    .map(|line| line.y + line.style.line_height + line.style.space_below)
                    .fold(0., f32::max);
                assert!((bottom - first(part.start).y - expected).abs() < 0.01);
            }
            for segment in projection.segments() {
                let actual = lines
                    .iter()
                    .filter(|line| {
                        projection
                            .segment_for_range(&line.projected_range())
                            .is_some_and(|s| s.node_id == segment.node_id)
                    })
                    .map(|line| &projection.text()[line.projected_range()])
                    .collect::<String>();
                assert_eq!(actual, projection.text()[segment.projection_range()]);
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn technical_partition_recovers_after_narrow_and_short_fallback_at_each_zoom(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/76-entity-records.md");
            let document = Document::from_markdown(source).unwrap();
            for zoom in [1., 1.5, 2.] {
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                fonts.measure_tables(&mut projection);
                let mut previous = None;
                for (width, height, paired) in [
                    (833., 1500., true),
                    (520., 1500., false),
                    (833., 1500., true),
                    (833., 180., false),
                    (833., 1500., true),
                ] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        height,
                        previous.as_ref(),
                        false,
                        &fonts,
                    );
                    assert_eq!(plan.measured_rows.validation_fallbacks, 0);
                    assert_eq!(
                        plan.measured_rows
                            .chosen
                            .iter()
                            .any(|row| row.kind == RowKind::Technical && row.roots == (4..8)),
                        paired,
                        "width {width}, height {height}, zoom {zoom}"
                    );
                    let mut lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    scale_visual_lines(&mut lines, zoom);
                    let heading = |name: &str| {
                        let segment = projection
                            .segments()
                            .iter()
                            .find(|s| projection.text()[s.projection_range()] == *name)
                            .unwrap();
                        lines
                            .iter()
                            .find(|line| line.projected_start() == segment.projection_start())
                            .unwrap()
                    };
                    if paired {
                        assert_eq!(
                            heading("Comparable capacity").y,
                            heading("Compact directory").y
                        );
                    } else {
                        assert!(heading("Comparable capacity").y < heading("Compact directory").y);
                    }
                    for line in &lines {
                        if let Some(slot) = line.slot {
                            assert!(slot.left(width) + slot.width(width) <= width + 0.01);
                        }
                    }
                    assert!(heading("Review notes").y > heading("Compact directory").y);
                    previous = Some(plan);
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn technical_continuations_stay_inside_complete_measured_peers(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/112-technical-continuations.md"
            );
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let roots = projection.roots().collect::<Vec<_>>();
            let plan = build_measured_adaptive_plan(&projection, 1314., 1300., None, false, &fonts);
            let row = plan
                .measured_rows
                .chosen
                .iter()
                .find(|r| r.kind == RowKind::Technical && r.roots.start == 2)
                .expect("complete technical sections should share a measured row");
            assert_eq!(row.parts, vec![2..6, 6..10]);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1314.,
                &plan,
                Some(&fonts),
            );
            for (part, height) in row.parts.iter().zip(&row.heights) {
                let own = lines
                    .iter()
                    .filter(|l| {
                        projection
                            .segment_for_range(&l.projected_range())
                            .is_some_and(|s| {
                                roots[part.clone()]
                                    .iter()
                                    .any(|r| r.id() == s.top_level_node_id)
                            })
                    })
                    .collect::<Vec<_>>();
                let last = own.last().unwrap();
                assert!(
                    (last.y + last.style.line_height + last.style.space_below - own[0].y - height)
                        .abs()
                        < 0.01
                );
                let tail = roots[part.end - 1].id();
                let tail_first = own
                    .iter()
                    .find(|l| {
                        projection
                            .segment_for_range(&l.projected_range())
                            .unwrap()
                            .node_id
                            == tail
                    })
                    .unwrap();
                assert!(
                    tail_first.y
                        > own
                            .iter()
                            .filter(|l| projection
                                .segment_for_range(&l.projected_range())
                                .unwrap()
                                .top_level_node_id
                                == roots[part.end - 2].id())
                            .map(|l| l.y)
                            .fold(0., f32::max)
                );
                assert_eq!(tail_first.slot.unwrap().group, own[0].slot.unwrap().group);
            }
            for segment in projection.segments() {
                let mut cursor = segment.projection_start();
                let code = matches!(
                    projection.block(segment.node_id),
                    Some(BlockNode::CodeBlock(_))
                );
                for line in lines.iter().filter(|l| {
                    projection
                        .segment_for_range(&l.projected_range())
                        .unwrap()
                        .node_id
                        == segment.node_id
                }) {
                    assert!(
                        line.projected_start() >= cursor
                            && line.projected_end() <= segment.projection_end()
                    );
                    let gap = &projection.text()[cursor..line.projected_start()];
                    assert!(
                        gap.is_empty() || (code && gap.chars().all(|c| matches!(c, '\n' | '\r')))
                    );
                    cursor = line.projected_end();
                }
                let gap = &projection.text()[cursor..segment.projection_end()];
                assert!(gap.is_empty() || (code && gap.chars().all(|c| matches!(c, '\n' | '\r'))));
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn technical_continuations_recover_from_narrow_and_short_stacks_at_each_zoom(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/112-technical-continuations.md"
            );
            for zoom in [1., 1.5, 2.] {
                let document = Document::from_markdown(source).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                fonts.measure_tables(&mut projection);
                let mut previous = None;
                for (width, height, paired) in [
                    (1314., 1300., true),
                    (520., 1300., false),
                    (1314., 1300., true),
                    (1314., 250., false),
                    (1314., 1300., true),
                ] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        height,
                        previous.as_ref(),
                        false,
                        &fonts,
                    );
                    assert_eq!(
                        plan.measured_rows
                            .chosen
                            .iter()
                            .any(|r| r.kind == RowKind::Technical && r.roots == (2..10)),
                        paired,
                        "{width}/{height}/{zoom}"
                    );
                    assert_eq!(plan.measured_rows.validation_fallbacks, 0);
                    let lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    let first = |text: &str| {
                        lines
                            .iter()
                            .find(|l| projection.text()[l.projected_range()].starts_with(text))
                            .unwrap()
                    };
                    let left = first("Configuration options");
                    let right = first("Configuration file");
                    if paired {
                        assert_eq!(left.y, right.y);
                        let a = left.slot.unwrap();
                        let b = right.slot.unwrap();
                        assert!(
                            (b.left(width) - a.left(width) - a.width(width) - LAYOUT_GAP).abs()
                                < 0.01
                        );
                    } else {
                        assert!(right.y > first("The history setting").y);
                    }
                    let next = first("Review the result");
                    assert!(next.y > first("The autosave option").y);
                    assert!(next.y > first("The history setting").y);
                    previous = Some(plan);
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn technical_sibling_components_share_a_measured_anchor(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/79-technical-sections.md");
            let variants = [
                source.to_owned(),
                source.replace("Configuration options", "Configuration options for local documents"),
                source.replace(
                    "Save these values in your project configuration before opening the document.",
                    concat!(
                        "Save these values before opening the document. Keep the local options ",
                        "together so everyone can review the exact settings and use the same configuration.",
                    ),
                ),
            ];
            for source in variants {
                let document = Document::from_markdown(source.as_str()).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts =
                    FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
                fonts.measure_tables(&mut projection);
                let roots = projection.roots().collect::<Vec<_>>();
                for width in [1040., 1312., 1632.] {
                    let plan =
                        build_measured_adaptive_plan(&projection, width, 1040., None, false, &fonts);
                    let focused = build_edit_locked_adaptive_plan(
                        &projection, width, 1040., Some(&plan), false, &fonts, Some(roots[3].id()),
                    );
                    for plan in [plan, focused] {
                        let row = plan.measured_rows.chosen.iter()
                            .find(|row| row.kind == RowKind::Technical && row.roots.start == 2)
                            .expect("technical pair");
                        let lines = build_measured_visual_lines(
                            &projection, &HashMap::new(), width, &plan, Some(&fonts),
                        );
                        let belongs = |line: &VisualLineSpec, part: &Range<usize>| {
                            projection.segment_for_range(&line.projected_range()).is_some_and(|segment| {
                                roots[part.clone()].iter()
                                    .any(|root| root.id() == segment.top_level_node_id)
                            })
                        };
                        let top = |part: &Range<usize>| {
                            let component = roots[part.clone()].iter()
                                .find(|root| matches!(root, BlockNode::Table(_) | BlockNode::CodeBlock(_)))
                                .unwrap();
                            let line = lines.iter().find(|line| {
                                projection.segment_for_range(&line.projected_range()).is_some_and(|segment| {
                                    segment.top_level_node_id == component.id()
                                })
                            }).unwrap();
                            if line.table_cell.is_some() {
                                line.table_row_y
                            } else {
                                line.y - CODE_HEADER_HEIGHT - CODE_BLOCK_PADDING
                            }
                        };
                        assert!(
                            (top(&row.parts[0]) - top(&row.parts[1])).abs() < 0.01,
                            "component tops differ at {width}: {} / {}",
                            top(&row.parts[0]), top(&row.parts[1]),
                        );
                        for (part, measured_height) in row.parts.iter().zip(&row.heights) {
                            let part_lines = lines.iter()
                                .filter(|line| belongs(line, part)).collect::<Vec<_>>();
                            let bottom = part_lines.iter()
                                .map(|line| line.y + line.style.line_height + line.style.space_below)
                                .fold(0., f32::max);
                            let height = bottom - part_lines[0].y;
                            assert!(
                                (height - measured_height).abs() < 0.01,
                                "candidate height {measured_height} differs from realized {height} at {width}",
                            );
                        }
                        // Local edits position their rebuilt row again; alignment
                        // must not accumulate or change the source ranges.
                        let mut positioned_again = lines.clone();
                        position_visual_lines(&mut positioned_again, &projection, width, 0.);
                        for (before, after) in lines.iter().zip(&positioned_again) {
                            assert_eq!(before.projected_range(), after.projected_range());
                            assert_eq!(before.y, after.y);
                            assert_eq!(before.table_row_y, after.table_row_y);
                            assert_eq!(before.table_row_height, after.table_row_height);
                        }
                    }
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn compact_technical_siblings_share_a_measured_row(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/79-technical-sections.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let provisional =
                build_measured_adaptive_plan(&projection, 1632., 926., None, false, &fonts);
            assert!(
                provisional
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.kind != RowKind::Technical)
            );
            fonts.measure_tables(&mut projection);
            let narrow = build_measured_adaptive_plan(&projection, 650., 580., None, false, &fonts);
            let expanded = build_measured_adaptive_plan(
                &projection,
                1040.,
                580.,
                Some(&narrow),
                false,
                &fonts,
            );
            assert!(
                expanded
                    .measured_rows
                    .chosen
                    .iter()
                    .any(|row| row.kind == RowKind::Technical && row.roots.start == 2),
                "widening must recover compact technical rows from a mandatory narrow stack"
            );
            let short = build_measured_adaptive_plan(
                &projection,
                1040.,
                250.,
                Some(&expanded),
                false,
                &fonts,
            );
            assert!(
                !short
                    .measured_rows
                    .chosen
                    .iter()
                    .any(|row| row.kind == RowKind::Technical && row.roots.start == 2)
            );
            let tall =
                build_measured_adaptive_plan(&projection, 1040., 926., Some(&short), false, &fonts);
            assert!(
                tall.measured_rows
                    .chosen
                    .iter()
                    .any(|row| row.kind == RowKind::Technical && row.roots.start == 2),
                "gaining viewport height must restore the technical row at unchanged width"
            );
            let editing_node = projection
                .segments()
                .iter()
                .find(|segment| {
                    projection.text()[segment.projection_range()].starts_with("Save these values")
                })
                .unwrap()
                .node_id;
            let focused_short = build_edit_locked_adaptive_plan(
                &projection,
                1040.,
                250.,
                Some(&expanded),
                false,
                &fonts,
                Some(editing_node),
            );
            assert!(
                !focused_short
                    .measured_rows
                    .chosen
                    .iter()
                    .any(|row| row.kind == RowKind::Technical && row.roots.start == 2),
                "an editing lock must not retain a row too tall for the resized viewport"
            );
            for width in [1100., 1632.] {
                let plan =
                    build_measured_adaptive_plan(&projection, width, 926., None, false, &fonts);
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                let heading = |title: &str| {
                    lines
                        .iter()
                        .find(|line| &projection.text()[line.projected_range()] == title)
                        .unwrap()
                };
                let options = heading("Configuration options");
                let file = heading("Configuration file");
                assert_eq!(
                    options.y, file.y,
                    "compact table/code siblings should share a row"
                );
                let left = options.slot.expect("left section");
                let right = file.slot.expect("right section");
                assert_eq!(left.group, right.group);
                assert!(
                    (right.left(width) - left.left(width) - left.width(width) - LAYOUT_GAP).abs()
                        < 0.01
                );
                assert!(heading("Notes for the team").y > options.y + 100.);
                for segment in projection.segments() {
                    let mut cursor = segment.projection_start();
                    for line in lines
                        .iter()
                        .filter(|line| segment.projection_range().contains(&line.projected_start()))
                    {
                        assert!(line.projected_start() >= cursor);
                        assert!(line.projected_end() <= segment.projection_end());
                        // Code newlines separate visual rows without becoming
                        // glyphs. No other source bytes may be omitted.
                        assert!(
                            projection.text()[cursor..line.projected_start()]
                                .chars()
                                .all(|c| matches!(c, '\n' | '\r'))
                        );
                        cursor = line.projected_end();
                    }
                    assert!(
                        projection.text()[cursor..segment.projection_end()]
                            .chars()
                            .all(|c| matches!(c, '\n' | '\r'))
                    );
                }
            }
            for (width, height) in [(576., 926.), (816., 463.), (1632., 300.)] {
                let plan =
                    build_measured_adaptive_plan(&projection, width, height, None, false, &fonts);
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                let position = |title: &str| {
                    lines
                        .iter()
                        .find(|line| &projection.text()[line.projected_range()] == title)
                        .unwrap()
                        .y
                };
                assert!(position("Configuration file") > position("Configuration options"));
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[test]
    fn nested_guides_follow_mixed_numbering_and_survive_viewport_clipping() {
        let document = Document::from_markdown(
            "- Parent\n\n  4. First child\n  5. Second child\n     - Grandchild\n- Next parent\n",
        )
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let plan = AdaptivePlan::build(&projection, 760., None, false);
        let original = build_arranged_visual_lines(&projection, &HashMap::new(), 760., &plan);
        let child = &projection.segments()[1];
        let grandchild = &projection.segments()[3];
        for zoom in [0.75, 1., 2.] {
            let mut lines = original.clone();
            scale_visual_lines(&mut lines, zoom);
            let order = visual_line_paint_order(&lines);
            let index = component_geometry(&projection, &lines, 760. * zoom, zoom, &order);
            let origin = point(px(100.), px(80.));
            let guide = outline_guide_bounds(
                index.get(&child.context.list_ancestors[1]).unwrap(),
                origin,
                760. * zoom,
                zoom,
                true,
            );
            assert_eq!(guide.left(), px(100. + 24. * zoom));
            assert_eq!(guide.size.width, px(zoom));
            let nested = outline_guide_bounds(
                index.get(&grandchild.context.list_ancestors[2]).unwrap(),
                origin,
                760. * zoom,
                zoom,
                false,
            );
            assert_eq!(nested.left(), px(100. + 56. * zoom));
            assert!(nested.top() >= guide.top() && nested.bottom() <= guide.bottom());
            let viewport = Bounds::new(
                point(px(0.), guide.top() + px(10. * zoom)),
                size(px(760. * zoom), px(20. * zoom)),
            );
            let clipped = intersect_bounds(guide, viewport).unwrap();
            assert_eq!(clipped.top(), viewport.top());
            assert!(clipped.bottom() <= viewport.bottom());
        }
    }

    pub(in crate::editor) fn assert_same_geometry(a: &[VisualLineSpec], b: &[VisualLineSpec]) {
        assert_eq!(a.len(), b.len());
        for (a, b) in a.iter().zip(b) {
            assert_eq!(a.projected_range(), b.projected_range());
            assert_eq!(a.label_row, b.label_row);
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
            assert_eq!(metrics(a), metrics(b), "range {:?}", a.projected_range());
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
                    assert_eq!(a.x, b.x);
                    match (&a.content, &b.content) {
                        (
                            inline_math::Content::Formula {
                                light: al,
                                dark: ad,
                            },
                            inline_math::Content::Formula {
                                light: bl,
                                dark: bd,
                            },
                        ) => {
                            assert_eq!(
                                [al.width, al.height, ad.width, ad.height],
                                [bl.width, bl.height, bd.width, bd.height]
                            );
                        }
                        (
                            inline_math::Content::Reference {
                                number: an,
                                label: al,
                                advance_em: aw,
                            },
                            inline_math::Content::Reference {
                                number: bn,
                                label: bl,
                                advance_em: bw,
                            },
                        ) => {
                            assert_eq!((an, al, aw), (bn, bl, bw));
                        }
                        _ => panic!("attachment kind changed"),
                    }
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let check = |snapshot: &DocumentSnapshot, width: f32, image_size, math_edit: bool, measurement: &FontMeasurement| {
                let mut projection = TextProjection::from_snapshot(snapshot);
                measurement.measure_tables(&mut projection);
                if math_edit {
                    projection.preview_edit_node = projection.segments().iter().find(|s| inline_math::has_math(&projection,s.node_id)).map(|s| s.node_id);
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
            let before = projection.segments().iter().find(|s| &projection.text()[s.projection_range()] == "Before.").unwrap().node_id;
            document.apply(EditCommand::ReplaceText { node_id: before, range: 0..0, text: "東京 😀 added before retained math and HTML ".into(), selection_after: None, typing: false }).unwrap();
            check(&document.snapshot(),760.,(400,240),false,&measurement);
            check(&document.snapshot(),759.75,(400,240),false,&measurement);
            check(&document.snapshot(),760.,(400,900),true,&measurement);
            check(&document.snapshot(),760.,(400,900),false,&measurement);
            let zoomed = FontMeasurement::new(cx.text_system().clone(),"Public Sans Tachyon".into(),2.);
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let prepare = |scope, previous: &AdaptivePlan, width, measurement: &FontMeasurement| {
                PreparedDocumentView::prepare_snapshot_with_images(&document.snapshot(), &HashMap::new(), None,
                    ReflowViewport { published_geometry: None, width, height: 1000., zoom: 1., preview_edit_node: None, expanded_code_tail: None, editing_node: None, table_layout_lock: None, html_disclosures: Arc::default(), html_loaded_images: Arc::default(), trace_mode: LayoutTraceMode::Off, visible_roots: scope, resource_generation: 0 }, previous, measurement).0
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
            let fresh = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let oracle = prepare(None, &initial.adaptive, 1100., &fresh);
            assert_eq!(prepared.adaptive.slots, oracle.adaptive.slots);
            let geometry = |view: &PreparedDocumentView| view.visual_lines.iter().map(|line| (line.projected_range(), line.y, line.inset, line.x_fraction, line.width_fraction, line.style.line_height)).collect::<Vec<_>>();
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                    .find(|line| line.projected_start() == segment.projection_start())
                    .unwrap();
                let last = lines
                    .iter()
                    .rev()
                    .find(|line| {
                        projection
                            .segment_for_range(&line.projected_range())
                            .is_some_and(|owner| owner.node_id == segment.node_id)
                    })
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                    &projection.text()[segment.projection_range()]
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
                .find(|line| line.projected_range() == segment.projection_range())
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
    fn ordinary_and_card_bodies_match_the_reference_opening_size(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/127-default-body-size.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                for width in [400., 1000., 1800.] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1400.,
                        None,
                        false,
                        &fonts,
                    );
                    let lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    for segment in projection.segments() {
                        let Some(BlockNode::Paragraph(_)) = projection.block(segment.node_id)
                        else {
                            continue;
                        };
                        let own = lines
                            .iter()
                            .filter(|line| {
                                projection
                                    .segment_for_range(&line.projected_range())
                                    .is_some_and(|s| s.node_id == segment.node_id)
                            })
                            .collect::<Vec<_>>();
                        assert!(!own.is_empty());
                        assert_eq!(
                            own.iter()
                                .map(|line| &projection.text()[line.projected_range()])
                                .collect::<String>(),
                            &projection.text()[segment.projection_range()]
                        );
                        let body = own.last().unwrap();
                        assert_eq!(
                            body.style.font_size,
                            DocumentStyle::BODY_SIZE,
                            "ordinary and card bodies match the measured native opening reference"
                        );
                        assert_eq!(body.style.line_height, DocumentStyle::BODY_LEADING);
                    }
                    assert!(
                        lines
                            .iter()
                            .filter(|line| projection
                                .segment_for_range(&line.projected_range())
                                .is_some_and(|segment| matches!(
                                    projection.block(segment.node_id),
                                    Some(BlockNode::CodeBlock(_))
                                )))
                            .all(|line| line.style.font_size == DocumentStyle::CODE_SIZE)
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn automatic_table_edges_follow_loaded_font_measure(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/126-automatic-table-edges.md"
            );
            let source = source.replace(
                "when opened again.",
                "when opened again with the original settings.",
            );
            let document = Document::from_markdown(source.as_str()).unwrap();
            for zoom in [1., 1.5, 2.] {
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                fonts.measure_tables(&mut projection);
                let table = projection
                    .segments()
                    .iter()
                    .find(|segment| &projection.text()[segment.projection_range()] == "Open")
                    .unwrap()
                    .context
                    .table_cell
                    .unwrap()
                    .0;
                let reading = fonts
                    .prose_measures()
                    .fit_width(f32::INFINITY, false, false);
                for canvas in [400., 1000., 1600.] {
                    let widths = projection.fitted_table_widths(table, canvas).unwrap();
                    assert!((widths.iter().sum::<f32>() - canvas.min(reading)).abs() < 0.01);
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        canvas,
                        1000.,
                        None,
                        false,
                        &fonts,
                    );
                    let lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        canvas,
                        &plan,
                        Some(&fonts),
                    );
                    for segment in projection
                        .segments()
                        .iter()
                        .filter(|s| s.context.table_cell.is_some())
                    {
                        let text = lines
                            .iter()
                            .filter(|line| {
                                line.projected_start() >= segment.projection_start()
                                    && line.projected_end() <= segment.projection_end()
                            })
                            .map(|line| &projection.text()[line.projected_range()])
                            .collect::<String>();
                        assert_eq!(text, &projection.text()[segment.projection_range()]);
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn wide_table_prose_keeps_a_reading_measure_inside_authored_columns(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/124-table-reading-measure.md"
            );
            let document = Document::from_markdown(source).unwrap();
            for zoom in [1., 1.5, 2.] {
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                fonts.measure_tables(&mut projection);
                for width in [600., 1280., 1800.] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1200.,
                        None,
                        false,
                        &fonts,
                    );
                    let lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    for segment in projection
                        .segments()
                        .iter()
                        .filter(|s| s.context.table_cell.is_some())
                    {
                        let (table, _, _) = segment.context.table_cell.unwrap();
                        assert_eq!(
                            projection.fitted_table_widths(table, width).unwrap(),
                            [240., 1200.]
                        );
                        let cell_lines = lines
                            .iter()
                            .filter(|l| {
                                l.projected_start() >= segment.projection_start()
                                    && l.projected_end() <= segment.projection_end()
                            })
                            .collect::<Vec<_>>();
                        assert_eq!(
                            cell_lines
                                .iter()
                                .map(|l| &projection.text()[l.projected_range()])
                                .collect::<String>(),
                            &projection.text()[segment.projection_range()]
                        );
                        for line in cell_lines {
                            let measured = fonts
                                .line_width(
                                    &projection,
                                    line.projected_range(),
                                    line.style.font_size,
                                )
                                .unwrap();
                            let reading = fonts.prose_width(false, line.style.font_size);
                            assert!(
                                measured <= reading + 0.5,
                                "{width}/{zoom}: {measured} exceeds {reading}"
                            );
                        }
                    }
                }
            }
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                    .find(|segment| &projection.text()[segment.projection_range()] == text)
                    .unwrap();
                assert_eq!(
                    table_insets(segment, &projection),
                    (outer, 32.),
                    "{text}: ordered rails need their extra gap"
                );
                let line = lines
                    .iter()
                    .find(|line| line.projected_range() == segment.projection_range())
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                    .find(|segment| &projection.text()[segment.projection_range()] == text)
                    .unwrap();
                assert_eq!(segment.context.table_cell.unwrap().0, table);
                let line = lines
                    .iter()
                    .find(|line| line.projected_range() == segment.projection_range())
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
            let components = component_geometry(&projection, &lines, width, 1., &visual_line_paint_order(&lines));
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                    let segment = segment_for_line(&projection, &line.projected_range()).unwrap();
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
                        .line_width(&projection, line.projected_range(), line.style.font_size)
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
                    1.,
                    &visual_line_paint_order(&lines),
                );
                for line in &lines {
                    let Some((table, _, _, _)) = line.table_cell else {
                        continue;
                    };
                    let segment = segment_for_line(&projection, &line.projected_range()).unwrap();
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
                    "Public Sans Tachyon".into(),
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
                        preview_edit_node: None,
                        expanded_code_tail: None,
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
            // The compact technical entries can legitimately share a planning
            // window, including their trailing explanations. Give this scoped
            // invalidation test an explicit, unrelated document boundary;
            // do not depend on an optional composition rejecting a sibling.
            let source = concat!(
                include_str!("../../../../performance/layout-fixtures/12-measured-tables.md"),
                "\n# Separate appendix\n\nThis unrelated reading section supplies the offscreen planning scope.\n",
            );
            let mut document = Document::from_markdown(source).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                        preview_edit_node: None,
                        expanded_code_tail: None,
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
                .find(|s| &before.projection.text()[s.projection_range()] == "64 MiB")
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                warm.intrinsic_cache_misses, 0,
                "unchanged table/group widths must reuse cached measurements: {warm:?}"
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
                || FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                    let slot = line.slot;
                    (
                        line.projected_range(),
                        line.y,
                        line.inset,
                        line.width_fraction,
                        line.style.line_height,
                        slot,
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
    fn map_candidate_measures_its_control_and_rejects_cramped_evidence(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            for intrinsic in [400, 1000] {
                let source = "[![Map: Source labels](map.svg)](#description)\n";
                let document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let roots = projection.roots().collect::<Vec<_>>();
                let segments = HashMap::from([(roots[0].id(), vec![0])]);
                let dimensions =
                    HashMap::from([(roots[0].id(), ("map.svg".into(), (intrinsic, 400)))]);
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    1.,
                );
                let resources = LayoutMeasurement {
                    text: &fonts,
                    images: Some(&dimensions),
                    scope: None,
                    resource_generation: 0,
                };
                for width in [260., 400., 719., 720., 1000.] {
                    let plan = AdaptivePlan::build(&projection, width, None, false);
                    let measured = measure_row_group(
                        &projection,
                        &roots,
                        &segments,
                        width,
                        false,
                        &resources,
                        &plan,
                    )
                    .unwrap();
                    assert_eq!(measured.overflow, width < (intrinsic as f32).min(720.));
                    let rendered = build_measured_visual_lines(
                        &projection,
                        &dimensions,
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    assert_eq!(
                        measured.height, rendered[0].style.line_height,
                        "candidate and painted strip agree"
                    );
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                ("Public Sans Tachyon", 2.),
                ("Spline Sans Mono Tachyon", 1.),
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                            .segment_for_range(&line.projected_range())
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
    fn critical_instructions_keep_the_warning_above_the_action_at_every_measure(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/99-warning-actions.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                for width in [360., 760., 1280.] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1000.,
                        None,
                        false,
                        &fonts,
                    );
                    let lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    let root_for = |prefix: &str| {
                        projection
                            .segments()
                            .iter()
                            .find(|s| projection.text()[s.projection_range()].starts_with(prefix))
                            .unwrap()
                            .top_level_node_id
                    };
                    for (before, after) in [
                        ("Verify the current folder", "Run this read-only"),
                        ("Run this read-only", "pwd"),
                        ("Keep the original document", "Save a separate"),
                        ("Review these example", "[document]"),
                    ] {
                        let next = lines
                            .iter()
                            .find(|line| {
                                projection
                                    .segment_for_range(&line.projected_range())
                                    .is_some_and(|s| s.top_level_node_id == root_for(after))
                            })
                            .unwrap();
                        let bottom = lines
                            .iter()
                            .filter(|line| {
                                projection
                                    .segment_for_range(&line.projected_range())
                                    .is_some_and(|s| s.top_level_node_id == root_for(before))
                            })
                            .map(|l| l.y + l.style.line_height + l.style.space_below)
                            .fold(0., f32::max);
                        assert_eq!(next.gap_before, DocumentStyle::INSTRUCTION_GAP);
                        assert!(
                            next.y - next.style.space_above
                                >= bottom + DocumentStyle::INSTRUCTION_GAP - 0.02
                        );
                        assert_eq!(
                            next.x_fraction, 0.,
                            "action stays below its warning, not beside it"
                        );
                    }
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                }
            }
        });
    }

    #[gpui::test]
    fn shared_gallery_end_uses_the_visible_caption_line(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let source = "![A](a.png)\n\n![B](b.png)\n\nGallery: Two botanical studies shown together, with every branch visible and the original image order retained.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                editor.focus_handle.focus(window, cx);
                editor.layout_width = 1314.;
                editor.refresh_projection();
                let segment = editor.projection.segments().last().unwrap().clone();
                let caption_lines = editor
                    .visual_lines
                    .iter()
                    .filter(|l| l.projected_start() >= segment.projection_start())
                    .collect::<Vec<_>>();
                assert_eq!(caption_lines.len(), 1);
                assert_eq!(caption_lines[0].projected_end(), segment.projection_end());
                editor.set_selection(
                    segment.projection_start() + 20..segment.projection_start() + 20,
                    false,
                    window,
                    cx,
                );
                editor.refresh_projection();
                editor.end(&End, window, cx);
                assert_eq!(
                    editor.selected_byte_range().0,
                    segment.projection_end()..segment.projection_end()
                );
            });
        });
    }

    #[gpui::test]
    fn shared_gallery_captions_follow_every_image_row_at_the_available_measure(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/98-shared-gallery-captions.md"
            );
            let document = Document::from_markdown(source).unwrap();
            let snapshot = document.snapshot();
            let initial = PreparedDocumentView::prepare(&document);
            let dimensions = [(
                hash(&resolved_image_resource("supporting-botanical.svg", None)),
                (600, 600),
            )]
            .into_iter()
            .collect();
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                for width in [360., 900., 1280.] {
                    let prepared = PreparedDocumentView::prepare_snapshot_with_images(
                        &snapshot,
                        &dimensions,
                        None,
                        ReflowViewport {
                            published_geometry: None,
                            width,
                            height: 1400.,
                            zoom,
                            preview_edit_node: None,
                            expanded_code_tail: None,
                            editing_node: None,
                            table_layout_lock: None,
                            trace_mode: LayoutTraceMode::Off,
                            html_disclosures: Arc::default(),
                            html_loaded_images: Arc::default(),
                            visible_roots: None,
                            resource_generation: 0,
                        },
                        &initial.adaptive,
                        &fonts,
                    )
                    .0;
                    let projection = &prepared.projection;
                    let lines = &prepared.visual_lines;
                    let labels = projection
                        .segments()
                        .iter()
                        .filter(|s| {
                            s.context
                                .figure_text
                                .is_some_and(|(_, role)| role.gallery_start().is_some())
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(labels.len(), 4);
                    for segment in labels {
                        let row = lines
                            .iter()
                            .find(|l| l.projected_start() == segment.projection_start())
                            .unwrap();
                        let role = segment.context.figure_text.unwrap().1;
                        let focused = PreparedDocumentView::prepare_snapshot_with_images(
                            &snapshot,
                            &dimensions,
                            None,
                            ReflowViewport {
                                published_geometry: prepared.published_geometry.clone(),
                                width,
                                height: 1400.,
                                zoom,
                                preview_edit_node: None,
                                expanded_code_tail: None,
                                editing_node: Some(segment.node_id),
                                table_layout_lock: None,
                                trace_mode: LayoutTraceMode::Off,
                                html_disclosures: Arc::default(),
                                html_loaded_images: Arc::default(),
                                visible_roots: None,
                                resource_generation: 0,
                            },
                            &prepared.adaptive,
                            &fonts,
                        )
                        .0;
                        let focused_row = focused
                            .visual_lines
                            .iter()
                            .find(|l| l.projected_start() == segment.projection_start())
                            .unwrap();
                        assert_eq!(
                            focused_row.width_fraction, row.width_fraction,
                            "focused worker must preserve the shared caption measure"
                        );
                        assert_eq!(
                            focused_row.projected_range(),
                            row.projected_range(),
                            "focus must not rewrap the visible caption"
                        );
                        assert_eq!(row.style.font_size, DocumentStyle::CAPTION_SIZE * zoom);
                        assert!(row.slot.is_none(), "shared caption is not a gallery tile");
                        assert_eq!(row.x_fraction, 0.);
                        assert!((row.width_fraction - 1.).abs() < 0.01);
                        let first = projection
                            .segment_for_node(role.gallery_start().unwrap())
                            .unwrap()
                            .projection_start();
                        let bottom = lines
                            .iter()
                            .filter(|l| {
                                l.projected_start() >= first
                                    && l.projected_start() < segment.projection_start()
                            })
                            .map(|l| l.y + l.style.line_height + l.style.space_below)
                            .fold(0., f32::max);
                        assert!(
                            (row.y - bottom - role.gap() * zoom).abs() < 0.02,
                            "shared label gap at width {width}, zoom {zoom}: {}",
                            row.y - bottom
                        );
                    }
                    let actual = lines
                        .iter()
                        .map(|l| &projection.text()[l.projected_range()])
                        .collect::<String>();
                    let expected = projection
                        .segments()
                        .iter()
                        .map(|s| &projection.text()[s.projection_range()])
                        .collect::<String>();
                    assert_eq!(actual, expected, "canonical text once, in source order");
                    assert_eq!(snapshot.serialize().unwrap(), source);
                }
            }
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                        preview_edit_node: None,
                        expanded_code_tail: None,
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
                            .segment_for_range(&line.projected_range())
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for size in [(960, 360), (1920, 720)] {
                let dimensions = ["first.png", "second.png"].into_iter()
                    .map(|source| (hash(&resolved_image_resource(source, None)), size)).collect();
                for (width, height, expected_columns) in [(992., 928., 2), (1280., 928., 2), (420., 928., 1), (992., 150., 1)] {
                    let (prepared, _, _) = PreparedDocumentView::prepare_snapshot_with_images(
                        &document.snapshot(), &dimensions, None,
                        ReflowViewport { published_geometry: None, width, height, zoom: 1., preview_edit_node: None, expanded_code_tail: None,
                            editing_node: None, table_layout_lock: None, trace_mode: LayoutTraceMode::Off,
                            visible_roots: None, html_disclosures: Arc::default(), html_loaded_images: Arc::default(), resource_generation: 1 },
                        &initial.adaptive, &measurement);
                    let figures = prepared.visual_lines.iter().filter(|line| prepared.projection.segment_for_range(&line.projected_range())
                        .is_some_and(|segment| segment.context.image_source.is_some())).collect::<Vec<_>>();
                    assert_eq!(figures.len(), 2);
                    for figure in &figures {
                        assert_eq!(figure.slot.map_or(1, |slot| slot.columns), expected_columns,
                            "figure size={size:?}, canvas={width}, viewport={height}");
                        let available = figure.width_fraction * width;
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let prepare = |dimensions: &SourceImageDimensions, previous: &AdaptivePlan, editing_node| {
                PreparedDocumentView::prepare_snapshot_with_images(&document.snapshot(), dimensions, None,
                    ReflowViewport { published_geometry: None, width: 992., height: 928., zoom: 1., preview_edit_node: None, expanded_code_tail: None,
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                        preview_edit_node: None, expanded_code_tail: None,
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
                    prepared.projection.segment_for_range(&line.projected_range())
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let prepare = |dimensions: &SourceImageDimensions, width, previous: &AdaptivePlan| {
                PreparedDocumentView::prepare_snapshot_with_images(
                    &snapshot, dimensions, None,
                    ReflowViewport { published_geometry: None, width, height: 1000., zoom: 1., preview_edit_node: None, expanded_code_tail: None, editing_node: None, table_layout_lock: None, html_disclosures: Arc::default(), html_loaded_images: Arc::default(), trace_mode: LayoutTraceMode::Off, visible_roots: None, resource_generation: 0 },
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
            let line = loaded.visual_lines.iter().find(|line| line.projected_range() == image.projection_range()).unwrap();
            assert!(line.x_fraction > 0.);
            assert!((line.style.line_height - row.heights[1]).abs() < 0.01);
            assert!((line.style.line_height - row.widths[1].min(600.) * 0.3).abs() < 0.01, "height={} width={} inset={}", line.style.line_height, row.widths[1], line.inset);
            let following = loaded.visual_lines.last().unwrap();
            assert!(following.y >= line.y + line.style.line_height + 24.);
            for (sizes, width) in [(&dimensions, 420.), (&HashMap::from([(key, (1000, 4000))]), 1100.), (&HashMap::from([(key, (0, 180))]), 1100.)] {
                let fallback = prepare(sizes, width, &loaded.adaptive);
                assert!(fallback.adaptive.measured_rows.chosen.iter().all(|row| row.kind == RowKind::Stack));
                assert_eq!(fallback.projection.text(), loaded.projection.text());
                assert!(fallback.visual_lines.iter().find(|line| line.projected_range() == image.projection_range()).unwrap().style.line_height > 0., "even invalid dimensions retain visible fallback content");
            }
            assert_eq!(snapshot.serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn extended_explanations_use_measured_pairs(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/105-extended-explanations.md"
            );
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let plan = build_measured_adaptive_plan(&projection, 1314., 1600., None, false, &fonts);
            let pairs = plan
                .measured_rows
                .candidates
                .iter()
                .filter(|row| row.kind == crate::adaptive::rows::RowKind::Explanation)
                .collect::<Vec<_>>();
            for start in [2, 6] {
                assert!(
                    pairs
                        .iter()
                        .any(|row| row.roots.start == start && row.legal()),
                    "both explanations must have a legal measured alternative"
                );
            }
            assert!(
                plan.measured_rows
                    .chosen
                    .iter()
                    .any(|row| row.kind == crate::adaptive::rows::RowKind::Technical),
                "a better complete technical peer row must remain available"
            );
            for source in [
                include_str!("../../../../performance/layout-fixtures/106-extended-table.md"),
                include_str!("../../../../performance/layout-fixtures/107-extended-code.md"),
            ] {
                let mut document = Document::from_markdown(source).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut projection);
                let plan =
                    build_measured_adaptive_plan(&projection, 1314., 1600., None, false, &fonts);
                let row = plan
                    .measured_rows
                    .chosen
                    .iter()
                    .find(|row| row.kind == crate::adaptive::rows::RowKind::Explanation)
                    .expect("an isolated extended explanation should use a measured pair");
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    1314.,
                    &plan,
                    Some(&fonts),
                );
                assert!(lines[0].slot.is_none(), "section heading stays full-width");
                assert_eq!(
                    lines
                        .iter()
                        .map(|line| &projection.text()[line.projected_range()])
                        .collect::<String>(),
                    projection
                        .segments()
                        .iter()
                        .map(|segment| &projection.text()[segment.projection_range()])
                        .collect::<String>()
                        .replace(['\r', '\n'], "")
                );
                for (item, expected) in row.heights.iter().enumerate() {
                    let members = lines
                        .iter()
                        .filter(|line| {
                            line.slot
                                .is_some_and(|slot| slot.group == row.ids[0] && slot.item == item)
                        })
                        .collect::<Vec<_>>();
                    let top = members
                        .iter()
                        .map(|line| line.y - line.style.space_above)
                        .fold(f32::INFINITY, f32::min);
                    let bottom = members
                        .iter()
                        .map(|line| line.y + line.style.line_height + line.style.space_below)
                        .fold(0_f32, f32::max);
                    assert!(
                        (bottom - top - expected).abs() < 0.01,
                        "measured and realized column heights agree"
                    );
                }
                for (width, height) in [(480., 1600.), (1314., 180.), (657., 800.)] {
                    let fallback = build_measured_adaptive_plan(
                        &projection,
                        width,
                        height,
                        Some(&plan),
                        false,
                        &fonts,
                    );
                    assert!(
                        fallback
                            .measured_rows
                            .chosen
                            .iter()
                            .all(|row| row.kind == crate::adaptive::rows::RowKind::Stack),
                        "cramped width/height must stack: {width}×{height}"
                    );
                    let restored = build_measured_adaptive_plan(
                        &projection,
                        1314.,
                        1600.,
                        Some(&fallback),
                        false,
                        &fonts,
                    );
                    assert!(
                        restored
                            .measured_rows
                            .chosen
                            .iter()
                            .any(|candidate| candidate.kind
                                == crate::adaptive::rows::RowKind::Explanation
                                && candidate.ids == row.ids),
                        "space returning after {width}×{height} must recover the explanation pair"
                    );
                    let focused = build_edit_locked_adaptive_plan(
                        &projection,
                        1314.,
                        1600.,
                        Some(&fallback),
                        false,
                        LayoutMeasurement {
                            text: &fonts,
                            images: None,
                            scope: None,
                            resource_generation: 0,
                        },
                        Some(
                            projection
                                .roots()
                                .find(|root| matches!(root, BlockNode::Paragraph(_)))
                                .unwrap()
                                .id(),
                        ),
                    );
                    assert!(
                        focused.measured_rows.chosen.iter().all(
                            |candidate| candidate.kind == crate::adaptive::rows::RowKind::Stack
                        ),
                        "restored space must not override the focused stack"
                    );
                    let released = build_measured_adaptive_plan(
                        &projection,
                        1314.,
                        1600.,
                        Some(&focused),
                        false,
                        &fonts,
                    );
                    assert!(
                        released.geometry_key().matches(&restored),
                        "blur must release the temporary stack"
                    );
                    let heading_focus = build_edit_locked_adaptive_plan(
                        &projection,
                        1314.,
                        1600.,
                        Some(&fallback),
                        false,
                        LayoutMeasurement {
                            text: &fonts,
                            images: None,
                            scope: None,
                            resource_generation: 0,
                        },
                        Some(projection.roots().next().unwrap().id()),
                    );
                    assert!(
                        heading_focus.geometry_key().matches(&restored),
                        "full-width heading caret must not lock columns below it"
                    );
                    let repeated = build_measured_adaptive_plan(
                        &projection,
                        1314.,
                        1600.,
                        Some(&restored),
                        false,
                        &fonts,
                    );
                    assert!(
                        repeated.geometry_key().matches(&restored),
                        "duplicate size must be stable"
                    );
                }
                let paragraph = projection
                    .roots()
                    .find(|root| matches!(root, BlockNode::Paragraph(_)))
                    .unwrap()
                    .id();
                document
                    .apply(document_core::EditCommand::ReplaceText {
                        node_id: paragraph,
                        range: 0..0,
                        text: "Continued explanation. ".repeat(240),
                        selection_after: None,
                        typing: false,
                    })
                    .unwrap();
                let mut edited = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut edited);
                let focused = build_edit_locked_adaptive_plan(
                    &edited,
                    1314.,
                    1600.,
                    Some(&plan),
                    false,
                    LayoutMeasurement {
                        text: &fonts,
                        images: None,
                        scope: None,
                        resource_generation: 0,
                    },
                    Some(paragraph),
                );
                assert!(
                    focused
                        .measured_rows
                        .chosen
                        .iter()
                        .all(|candidate| candidate.ids != row.ids),
                    "unmeasurable prose cannot prove that two columns fit the viewport"
                );
                assert!(!focused.slots.contains_key(&paragraph));
                let released = build_measured_adaptive_plan(
                    &edited,
                    1314.,
                    1600.,
                    Some(&focused),
                    false,
                    &fonts,
                );
                assert!(
                    released
                        .measured_rows
                        .chosen
                        .iter()
                        .all(|row| row.kind == crate::adaptive::rows::RowKind::Stack),
                    "oversized prose releases to complete flow on blur"
                );
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn explanation_resize_recovery_is_stable_at_each_text_scale(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            use crate::adaptive::rows::RowKind;
            for source in [
                include_str!("../../../../performance/layout-fixtures/106-extended-table.md"),
                include_str!("../../../../performance/layout-fixtures/107-extended-code.md"),
            ] {
                let document = Document::from_markdown(source).unwrap();
                for zoom in [1., 1.5, 2.] {
                    let fonts = FontMeasurement::new(
                        cx.text_system().clone(),
                        "Public Sans Tachyon".into(),
                        zoom,
                    );
                    let mut projection = TextProjection::from_snapshot(&document.snapshot());
                    fonts.measure_tables(&mut projection);
                    let mut previous = build_measured_adaptive_plan(
                        &projection,
                        1314.,
                        1600.,
                        None,
                        false,
                        &fonts,
                    );
                    let original = previous.geometry_key();
                    for (width, height) in [(480., 1600.), (1314., 180.), (480., 1600.)] {
                        let stack = build_measured_adaptive_plan(
                            &projection,
                            width,
                            height,
                            Some(&previous),
                            false,
                            &fonts,
                        );
                        assert!(
                            stack
                                .measured_rows
                                .chosen
                                .iter()
                                .all(|row| row.kind == RowKind::Stack)
                        );
                        let restored = build_measured_adaptive_plan(
                            &projection,
                            1314.,
                            1600.,
                            Some(&stack),
                            false,
                            &fonts,
                        );
                        assert!(
                            restored
                                .measured_rows
                                .chosen
                                .iter()
                                .any(|row| row.kind == RowKind::Explanation)
                        );
                        assert!(
                            original.matches(&restored),
                            "same available space must recover original geometry at zoom {zoom}"
                        );
                        previous = restored;
                    }
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                }
            }
        });
    }

    #[gpui::test]
    fn content_led_reference_retains_source_geometry_and_recovers_after_resize(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            use crate::adaptive::rows::RowKind;
            let source = include_str!(
                "../../../../performance/layout-fixtures/115-content-led-explanations.md"
            );
            let document = Document::from_markdown(source).unwrap();
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut projection);
                let initial =
                    build_measured_adaptive_plan(&projection, 1314., 1666., None, false, &fonts);
                let row = initial
                    .measured_rows
                    .chosen
                    .iter()
                    .find(|row| row.kind == RowKind::ContentExplanation)
                    .unwrap_or_else(|| {
                        panic!(
                            "complete table/explanation should pair: {:?}",
                            initial.measured_rows.candidates
                        )
                    });
                assert_eq!(row.parts, [1..2, 2..5]);
                assert_eq!(initial.measured_rows.validation_fallbacks, 0);
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    1314.,
                    &initial,
                    Some(&fonts),
                );
                assert!(lines[0].slot.is_none(), "section heading spans both tracks");
                for pair in lines.windows(2) {
                    assert!(pair[0].projected_end() <= pair[1].projected_start());
                    assert!(
                        projection.text()[pair[0].projected_end()..pair[1].projected_start()]
                            .chars()
                            .all(|c| c == '\n' || c == '\r')
                    );
                }
                let root_ids = projection.roots().map(BlockNode::id).collect::<Vec<_>>();
                let body = lines
                    .iter()
                    .find(|line| {
                        projection
                            .segment_for_range(&line.projected_range())
                            .is_some_and(|s| s.node_id == root_ids[2])
                    })
                    .unwrap();
                assert!(body.x_fraction > 0.);
                let next = lines
                    .iter()
                    .find(|line| {
                        projection
                            .segment_for_range(&line.projected_range())
                            .is_some_and(|s| s.node_id == root_ids[5])
                    })
                    .unwrap();
                assert!(next.y > body.y + row.heights.iter().copied().fold(0., f32::max));
                let key = initial.geometry_key();
                let mut previous = initial;
                for (width, height) in [(480., 1666.), (1314., 180.), (480., 1666.)] {
                    let stack = build_measured_adaptive_plan(
                        &projection,
                        width,
                        height,
                        Some(&previous),
                        false,
                        &fonts,
                    );
                    assert!(
                        stack
                            .measured_rows
                            .chosen
                            .iter()
                            .all(|row| row.kind == RowKind::Stack)
                    );
                    let restored = build_measured_adaptive_plan(
                        &projection,
                        1314.,
                        1666.,
                        Some(&stack),
                        false,
                        &fonts,
                    );
                    assert!(
                        restored
                            .measured_rows
                            .chosen
                            .iter()
                            .any(|row| row.kind == RowKind::ContentExplanation)
                    );
                    assert!(
                        key.matches(&restored),
                        "available space restores the same geometry at {zoom}"
                    );
                    previous = restored;
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn content_led_focus_keeps_columns_and_defers_resize_recovery_until_blur(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            use crate::adaptive::rows::RowKind;
            for source in [
                include_str!(
                    "../../../../performance/layout-fixtures/115-content-led-explanations.md"
                ),
                include_str!(
                    "../../../../performance/layout-fixtures/116-code-led-explanations.md"
                ),
            ] {
                let mut document = Document::from_markdown(source).unwrap();
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    1.,
                );
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut projection);
                let ready =
                    build_measured_adaptive_plan(&projection, 1314., 1666., None, false, &fonts);
                let row = ready
                    .measured_rows
                    .chosen
                    .iter()
                    .find(|row| row.kind == RowKind::ContentExplanation)
                    .expect("table and code variants can both pair");
                let body = projection
                    .roots()
                    .find(|root| matches!(root, BlockNode::Paragraph(_)))
                    .unwrap()
                    .id();
                let narrow = build_measured_adaptive_plan(
                    &projection,
                    480.,
                    1666.,
                    Some(&ready),
                    false,
                    &fonts,
                );
                let held = build_edit_locked_adaptive_plan(
                    &projection,
                    1314.,
                    1666.,
                    Some(&narrow),
                    false,
                    &fonts,
                    Some(body),
                );
                let heading_focus = build_edit_locked_adaptive_plan(
                    &projection,
                    1314.,
                    1666.,
                    Some(&narrow),
                    false,
                    &fonts,
                    Some(projection.roots().next().unwrap().id()),
                );
                assert!(
                    heading_focus.geometry_key().matches(&ready),
                    "the unchanged full-width heading must not freeze independent columns below it"
                );
                assert!(
                    held.measured_rows
                        .chosen
                        .iter()
                        .all(|row| row.kind == RowKind::Stack)
                );
                let released = build_measured_adaptive_plan(
                    &projection,
                    1314.,
                    1666.,
                    Some(&held),
                    false,
                    &fonts,
                );
                assert!(
                    released.geometry_key().matches(&ready),
                    "blur releases a provisional stack"
                );
                document
                    .apply(EditCommand::ReplaceText {
                        node_id: body,
                        range: 0..0,
                        text: "More detail. ".repeat(250),
                        typing: true,
                        selection_after: None,
                    })
                    .unwrap();
                let mut edited = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut edited);
                let focused = build_edit_locked_adaptive_plan(
                    &edited,
                    1314.,
                    1666.,
                    Some(&ready),
                    false,
                    &fonts,
                    Some(body),
                );
                let retained = focused
                    .measured_rows
                    .chosen
                    .iter()
                    .find(|row| row.kind == RowKind::ContentExplanation)
                    .expect("growth beyond the nomination budget keeps the focused pair");
                assert_eq!(retained.parts, row.parts);
                assert_eq!(retained.widths, row.widths);
                assert!(retained.edit_locked);
                let lines = build_measured_visual_lines(
                    &edited,
                    &HashMap::new(),
                    1314.,
                    &focused,
                    Some(&fonts),
                );
                let body_range = edited.segment_for_node(body).unwrap().projection_range();
                assert_eq!(
                    lines
                        .iter()
                        .filter(|line| line.projected_start() >= body_range.start
                            && line.projected_end() <= body_range.end)
                        .map(|line| &edited.text()[line.projected_range()])
                        .collect::<String>(),
                    &edited.text()[body_range]
                );
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn measured_explanation_code_uses_unequal_tracks_and_stacks_when_cramped(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            use crate::adaptive::rows::{RowKind, RowRejection};
            // A generic explanation remains open; the explicit heading
            // "Example" now denotes the separately tested editorial object.
            let source = "## Configuration\n\nRead the saved configuration.\n\n```rust\nlet configuration = read_configuration_from_workspace(&workspace_directory)?;\nstart_worker(configuration);\n```\n\nFollowing content remains below the whole pair.\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
            let row = plan.measured_rows.chosen.iter().find(|r| r.kind == RowKind::Explanation)
                .unwrap_or_else(|| panic!("expected a readable explanation pair: {:?}", plan.measured_rows.candidates));
            assert_ne!(row.widths[0], row.widths[1]);
            let lines = build_measured_visual_lines(&projection, &HashMap::new(), 1100., &plan, Some(&measurement));
            let code = projection.segments().iter().find(|s| matches!(projection.block(s.node_id), Some(BlockNode::CodeBlock(_)))).unwrap();
            let example = lines.iter().find(|l| l.projected_start() == code.projection_start()).unwrap();
            assert!(example.x_fraction > 0.);
            assert!(example.x_fraction + example.width_fraction <= 1.001);
            assert!(lines[0].slot.is_none(), "major heading stays full width");
            assert_eq!(lines.iter().map(|l| &projection.text()[l.projected_range()]).collect::<String>(),
                projection.segments().iter().map(|s| &projection.text()[s.projection_range()]).collect::<String>().replace(['\r', '\n'], ""));
            for pair in lines.windows(2) {
                assert!(pair[0].projected_end() <= pair[1].projected_start());
                let gap = &projection.text()[pair[0].projected_end()..pair[1].projected_start()];
                assert!(gap.chars().all(|c| c == '\n' || c == '\r'), "only authored line/paragraph separators can be outside glyph ranges");
            }
            for (column, part) in row.parts.iter().enumerate() {
                let roots = projection.roots().collect::<Vec<_>>();
                let ids = roots[part.clone()].iter().map(|root| root.id()).collect::<Vec<_>>();
                let group_lines = lines.iter().filter(|line| projection.segment_for_range(&line.projected_range()).is_some_and(|segment| ids.contains(&segment.top_level_node_id))).collect::<Vec<_>>();
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                        .map(|line| &projection.text()[line.projected_range()])
                        .collect::<String>(),
                    projection
                        .segments()
                        .iter()
                        .map(|s| &projection.text()[s.projection_range()])
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
                            .filter(|line| {
                                segment.projection_range().contains(&line.projected_start())
                            })
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
    fn nearby_horizontal_lists_share_the_densest_common_grid(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = concat!(
                "Examples:\n\n",
                "- Reproduce the bug with a minimal failing case\n",
                "- Temporarily instrument the suspicious code path\n",
                "- Prototype a minimal patch and verify behavior\n",
                "- Add or run a focused test\n",
                "- Benchmark the hot path before optimizing\n",
                "- Compare output before and after the change\n\n",
                "The agent should use these steps to confirm that the planned fix addresses the actual issue.\n\n",
                "# 5. Communicate Clearly\n\n",
                "The agent should report its intentions, approach, and findings clearly to the user.\n\n",
                "Before making substantial changes, explain:\n\n",
                "- What appears to be wrong\n",
                "- Why that change is narrow and appropriate\n",
                "- What evidence supports that conclusion\n",
                "- How the fix will be validated\n",
                "- What change is planned\n\n",
                "After making changes, summarize:\n\n",
                "- What was changed\n",
                "- Why it was changed\n",
                "- What was tested\n",
                "- What risks remain, if any\n",
            );
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan =
                build_measured_adaptive_plan(&projection, 1314., 3000., None, false, &measurement);
            let lists = projection
                .roots()
                .filter_map(|root| match root {
                    BlockNode::List(list) => Some(list),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(lists.len(), 3);
            assert!(
                lists.iter().all(|list| {
                    plan.measured_lists
                        .get(&list.id)
                        .is_some_and(|decision| decision.layout == ListLayout::Grid(3))
                        && plan
                            .slots
                            .values()
                            .filter(|slot| slot.group == list.id)
                            .all(|slot| slot.columns == 3 && slot.span == 4)
                }),
                "decisions={:#?}",
                plan.measured_lists,
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn distant_horizontal_lists_keep_independent_grids(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = concat!(
                "- Reproduce the bug with a minimal failing case\n",
                "- Temporarily instrument the suspicious code path\n",
                "- Prototype a minimal patch and verify behavior\n",
                "- Add or run a focused test\n",
                "- Benchmark the hot path before optimizing\n",
                "- Compare output before and after the change\n\n",
                "First intervening line.\n\n",
                "Second intervening line.\n\n",
                "Third intervening line.\n\n",
                "Fourth intervening line.\n\n",
                "Fifth intervening line.\n\n",
                "Sixth intervening line.\n\n",
                "- What was changed\n",
                "- Why it was changed\n",
                "- What was tested\n",
                "- What risks remain, if any\n",
            );
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan =
                build_measured_adaptive_plan(&projection, 1500., 3000., None, false, &measurement);
            let columns = projection
                .roots()
                .filter_map(|root| match root {
                    BlockNode::List(list) => Some(plan.measured_lists[&list.id].layout),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(columns, [ListLayout::Grid(2), ListLayout::Grid(3)]);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn labeled_features_use_open_measured_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for source in [
                "- **Markdown support:** Write in plain text with rich results.\n- **A quieter workspace:** Keep attention on the document.\n- **Adaptive layouts:** Let content find a comfortable shape.\n",
                include_str!("../../../../performance/layout-fixtures/53-document-grammar.md"),
            ] {
                let document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let plan =
                    build_measured_adaptive_plan(&projection, 1314., 1366., None, false, &fonts);
                let list = projection.roots().find_map(|root| match root {
                    BlockNode::List(list) if matches!(list.kind, document_core::ListKind::Unordered) => Some(list),
                    _ => None,
                }).unwrap();
                let slots = plan.slots.values().filter(|slot| slot.group == list.id).collect::<Vec<_>>();
                assert_eq!(slots.len(), list.items.len(), "the actual planner must compose every feature");
                assert!(slots.iter().all(|slot| slot.inset() == 0.), "labels must not introduce enclosure padding: {slots:?}");
                let lines = build_measured_visual_lines(&projection, &HashMap::new(), 1314., &plan, Some(&fonts));
                let decision = &plan.measured_lists[&list.id];
                let candidate = decision.candidates.iter().find(|candidate| candidate.row_columns == decision.row_columns).unwrap();
                for slot in slots {
                    let rendered = lines.iter().filter(|line| line.slot.is_some_and(|s| s.group == slot.group && s.item == slot.item)).collect::<Vec<_>>();
                    let height = rendered.iter().map(|line| line.style.line_height + line.style.space_above + line.style.space_below).sum::<f32>();
                    assert_eq!(height, candidate.items[slot.item].height, "candidate and rendering must use the same open geometry");
                    assert_eq!(rendered.len(), candidate.items[slot.item].lines);
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn open_labeled_features_preserve_width_zoom_and_focused_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/53-document-grammar.md");
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for (width, zoom) in [
                (520., 1.),
                (657., 2.),
                (1040., 1.),
                (1314., 1.),
                (1700., 1.),
            ] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                let plan = build_measured_adaptive_plan(
                    &projection,
                    width,
                    1366. / zoom,
                    None,
                    false,
                    &fonts,
                );
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                for root in projection.roots() {
                    let BlockNode::List(list) = root else {
                        continue;
                    };
                    let Some(decision) = plan.measured_lists.get(&list.id) else {
                        continue;
                    };
                    assert!(
                        decision
                            .candidates
                            .iter()
                            .all(|candidate| candidate.columns <= 3)
                    );
                    if !matches!(decision.layout, ListLayout::Grid(_)) {
                        continue;
                    }
                    let numbered = matches!(list.kind, document_core::ListKind::Ordered { .. });
                    let candidate = decision
                        .candidates
                        .iter()
                        .find(|candidate| candidate.row_columns == decision.row_columns)
                        .unwrap();
                    for (index, item) in list.items.iter().enumerate() {
                        let node = item.blocks.get(0).unwrap().id();
                        let slot = plan.slots[&node];
                        assert_eq!(
                            slot.card_accent,
                            if numbered {
                                crate::adaptive::CardAccent::Numbered
                            } else {
                                crate::adaptive::CardAccent::OpenLabeled
                            }
                        );
                        assert_eq!(slot.inset(), if numbered { CARD_PADDING } else { 0. });
                        assert!(slot.left(width) + slot.width(width) <= width + 0.01);
                        let rendered = lines
                            .iter()
                            .filter(|line| {
                                line.slot
                                    .is_some_and(|s| s.group == list.id && s.item == index)
                            })
                            .collect::<Vec<_>>();
                        assert_eq!(rendered.len(), candidate.items[index].lines);
                        assert_eq!(
                            rendered
                                .iter()
                                .map(|line| line.style.line_height
                                    + line.style.space_above
                                    + line.style.space_below)
                                .sum::<f32>(),
                            candidate.items[index].height
                        );
                        let segment = projection.segment_for_node(node).unwrap();
                        assert_eq!(
                            rendered
                                .iter()
                                .map(|line| &projection.text()[line.projected_range()])
                                .collect::<String>(),
                            &projection.text()[segment.projection_range()]
                        );
                        for line in rendered {
                            assert!(
                                fonts
                                    .line_width(
                                        &projection,
                                        line.projected_range(),
                                        line.style.font_size
                                    )
                                    .unwrap()
                                    <= slot.width(width) - 2. * slot.inset() + 0.5
                            );
                        }
                    }
                }
            }
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 1314., 1366., None, false, &fonts);
            let segment = projection
                .segments()
                .iter()
                .find(|s| projection.text()[s.projection_range()].starts_with("Markdown support:"))
                .unwrap();
            let node = segment.node_id;
            let slot = plan.slots[&node];
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: 18..18,
                    text: "Additional context ".repeat(80),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let edited = TextProjection::from_snapshot(&document.snapshot());
            let focused = build_edit_locked_adaptive_plan(
                &edited,
                1700.,
                1366.,
                Some(&plan),
                false,
                &fonts,
                Some(node),
            );
            assert_eq!(
                focused.slots[&node].card_accent,
                crate::adaptive::CardAccent::OpenLabeled
            );
            assert_eq!(focused.slots[&node].width(1700.), slot.width(1314.));
            assert_eq!(focused.slots[&node].inset(), 0.);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn very_short_open_features_use_four_columns(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for count in [4, 8, 12] {
                let source = (0..count)
                    .map(|n| format!("- Short entry {n}\n"))
                    .collect::<String>();
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                for width in [620., 1111., 1112., 1280., 1700.] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1000.,
                        None,
                        false,
                        &fonts,
                    );
                    let decision = plan.measured_lists.values().next().unwrap();
                    assert!(decision.is_valid(count, width, None));
                    if width < 1184. {
                        assert!(!decision.row_columns.contains(&4));
                        continue;
                    }
                    assert_eq!(decision.row_columns, vec![4; count / 4]);
                    let lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    assert_eq!(
                        lines.len(),
                        count,
                        "four-column entries must stay on one line"
                    );
                    for (index, line) in lines.iter().enumerate() {
                        let slot = line.slot.unwrap();
                        assert_eq!(
                            (
                                slot.item,
                                slot.row,
                                slot.columns,
                                slot.track_start,
                                slot.span
                            ),
                            (index, index / 4, 4, ((index % 4) * 3) as u8, 3)
                        );
                        assert!(slot.left(width) + slot.width(width) <= width + 0.01);
                        if index % 4 != 0 {
                            let previous = &lines[index - 1];
                            assert_eq!(line.y, previous.y);
                            let before = previous.slot.unwrap();
                            assert!(
                                (slot.left(width)
                                    - before.left(width)
                                    - before.width(width)
                                    - LAYOUT_GAP)
                                    .abs()
                                    < 0.01
                            );
                        } else if index > 0 {
                            assert!(
                                line.y >= lines[index - 1].y + lines[index - 1].style.line_height
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
                            .map(|segment| &projection.text()[segment.projection_range()])
                            .collect::<String>()
                    );
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn numbered_and_labeled_features_do_not_nominate_four_columns(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for ordered in [true, false] {
                let source = (1..=8)
                    .map(|n| {
                        if ordered {
                            format!("{n}. Short point {n}\n")
                        } else {
                            format!("- **Topic {n}:** Brief explanation.\n")
                        }
                    })
                    .collect::<String>();
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let plan =
                    build_measured_adaptive_plan(&projection, 1700., 1000., None, false, &fonts);
                assert!(!plan.measured_lists.is_empty());
                assert!(
                    plan.measured_lists
                        .values()
                        .all(|d| d.candidates.iter().all(|c| c.columns <= 3))
                );
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn six_short_interfaces_use_three_shared_columns(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/123-aligned-card-grids.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 1725., 1366., None, false, &fonts);
            let decision = plan
                .measured_lists
                .values()
                .find(|d| d.candidates.iter().any(|c| c.items.len() == 6))
                .unwrap();
            assert_eq!(decision.layout, ListLayout::Grid(3), "{decision:#?}");
        });
    }

    #[gpui::test]
    fn measured_short_lists_consider_two_through_twelve_items(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for count in 1..=13 {
                for labeled in [false, true] {
                    let source = (0..count)
                        .map(|n| {
                            if labeled {
                                format!("- **Topic {n}:** A compact independent explanation.\n")
                            } else {
                                format!("- A compact independent point numbered {n}.\n")
                            }
                        })
                        .collect::<String>();
                    let document = Document::from_markdown(source.as_str()).unwrap();
                    let projection = TextProjection::from_snapshot(&document.snapshot());
                    for width in [360., 1280.] {
                        let plan = build_measured_adaptive_plan(
                            &projection,
                            width,
                            1000.,
                            None,
                            false,
                            &fonts,
                        );
                        let grid = plan
                            .lists
                            .values()
                            .any(|list| matches!(list.layout, ListLayout::Grid(_)));
                        if width == 360. || !(2..=12).contains(&count) || (labeled && count == 2) {
                            assert!(!grid);
                        } else if [2, 10, 12].contains(&count) {
                            assert!(grid, "count={count}, labeled={labeled}, width={width}");
                        }
                        if labeled && count == 2 {
                            assert_eq!(
                                plan.label_rows.len(),
                                count,
                                "authored term pairs retain aligned rows"
                            );
                        } else if (2..=12).contains(&count) {
                            let decision = plan
                                .measured_lists
                                .values()
                                .next()
                                .expect("every eligible count needs a measured decision");
                            assert!(decision.is_valid(count, width, None));
                            if width == 1280. {
                                assert!(
                                    decision
                                        .candidates
                                        .iter()
                                        .any(|candidate| candidate.columns > 1
                                            && candidate.rejected.is_none())
                                );
                            }
                        }
                        let lines = build_measured_visual_lines(
                            &projection,
                            &HashMap::new(),
                            width,
                            &plan,
                            Some(&fonts),
                        );
                        if grid {
                            let starts = projection
                                .segments()
                                .iter()
                                .map(|segment| {
                                    lines
                                        .iter()
                                        .find(|line| {
                                            line.projected_start() == segment.projection_start()
                                        })
                                        .unwrap()
                                })
                                .collect::<Vec<_>>();
                            let rows = starts
                                .chunk_by(|a, b| a.slot.unwrap().same_row(b.slot.unwrap()))
                                .collect::<Vec<_>>();
                            for row in &rows {
                                assert!(row.windows(2).all(|pair| pair[0].y == pair[1].y
                                    && pair[0].x_fraction < pair[1].x_fraction));
                                for line in *row {
                                    let slot = line.slot.unwrap();
                                    assert!(slot.left(width) + slot.width(width) <= width + 0.01);
                                }
                            }
                            for pair in rows.windows(2) {
                                let row = pair[0][0].slot.unwrap();
                                let bottom = lines
                                    .iter()
                                    .filter(|line| line.slot.is_some_and(|slot| slot.same_row(row)))
                                    .map(|line| line.y + line.style.line_height)
                                    .fold(0., f32::max);
                                assert!(pair[1][0].y >= bottom);
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
                                .map(|s| &projection.text()[s.projection_range()])
                                .collect::<String>()
                        );
                    }
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                }
            }
        });
    }

    #[gpui::test]
    fn measured_lists_reject_uneven_many_and_cross_referenced_items(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for source in [
                format!(
                    "- Short\n- Brief\n- {}\n- Fourth\n- Fifth\n- Sixth\n",
                    "A much longer explanatory paragraph. ".repeat(12)
                ),
                (0..13).map(|n| format!("- Label {n}\n")).collect(),
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

    #[gpui::test]
    fn expanded_list_counts_keep_semantic_and_fit_guards(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for count in [2, 4, 8, 10, 12] {
                for case in ["tasks", "nested", "dependent", "long", "uneven"] {
                    let source = (0..count)
                        .map(|n| match case {
                            "tasks" => {
                                format!("- [{}] Task {n}\n", if n % 2 == 0 { 'x' } else { ' ' })
                            }
                            "nested" => format!("- Item {n}\n  - Nested detail\n"),
                            "dependent" => {
                                format!("{}. Continue using the result of step 1.\n", n + 1)
                            }
                            "long" => {
                                format!("- Item {n}: {}\n", "An extended explanation. ".repeat(40))
                            }
                            "uneven" if n == 0 => format!(
                                "- {}\n",
                                "A much longer explanatory paragraph. ".repeat(12)
                            ),
                            _ => format!("- Item {n}\n"),
                        })
                        .collect::<String>();
                    let document = Document::from_markdown(source.as_str()).unwrap();
                    let projection = TextProjection::from_snapshot(&document.snapshot());
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        1280.,
                        1000.,
                        None,
                        false,
                        &fonts,
                    );
                    assert!(
                        plan.lists
                            .values()
                            .all(|list| !matches!(list.layout, ListLayout::Grid(_))),
                        "count={count}, case={case}"
                    );
                    assert_eq!(document.snapshot().serialize().unwrap(), source);
                }
            }
        });
    }

    #[test]
    fn pairwise_gaps_attach_headings_and_separate_tables_from_preceding_content() {
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
                        .segment_for_range(&line.projected_range())
                        .unwrap()
                        .top_level_node_id
                        == roots[root].id()
                })
                .unwrap()
        };
        let near = |actual: f32, expected: f32| {
            assert!((actual - expected).abs() < 0.001, "{actual} != {expected}")
        };
        near(first(1).y - first(0).y - first(0).style.line_height, 24.);
        near(first(2).y - first(1).y - first(1).style.line_height, 24.);
        near(first(3).y - first(2).y - first(2).style.line_height, 48.);
        near(first(4).y - first(3).y - first(3).style.line_height, 24.);
        let table = first(5);
        near(
            table.table_row_y - first(4).y - first(4).style.line_height,
            DocumentStyle::TABLE_TOP_GAP,
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

    #[test]
    fn table_at_document_start_has_no_extra_top_gap() {
        let source = "| A | B |\n| - | - |\n| C | D |\n\nAfter table.\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let plan = AdaptivePlan::build(&projection, 1000., None, false);
        let lines = build_arranged_visual_lines(&projection, &HashMap::new(), 1000., &plan);
        let first = lines.first().unwrap();

        assert_eq!(first.gap_before, 0.);
        assert_eq!(first.table_row_y, 0.);
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[gpui::test]
    fn cards_do_not_shrink_the_design_system_fonts(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
        let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
        let source =
            "### Alpha\n\nFirst idea.\n\n### Beta\n\nSecond idea.\n\n### Gamma\n\nThird idea.\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let plan = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
        let lines = build_arranged_visual_lines(&projection, &HashMap::new(), 1100., &plan);
        assert!(!plan.slots.is_empty());
        for line in lines {
            let segment = projection.segment_for_range(&line.projected_range()).unwrap();
            let block = projection.block(segment.node_id).unwrap();
            let expected = visual_line_style_for(&projection, block, segment, &line.projected_range(), None);
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
        let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
        let source =
            include_str!("../../../../performance/layout-fixtures/05-product-specification.md");
        // Keep peer heights comparable at the larger default body size.
        let source = source.replace("3. Review a correction.", "3. Review a correction.\n4. Verify the total.\n5. Close the invoice.")
            .replace("3. Confirm cancellation.", "3. Confirm cancellation.\n4. Check the refund.\n5. Close the booking.");
        let document = Document::from_markdown(source.as_str()).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let wide = build_measured_adaptive_plan(&projection, 1500., 800., None, false, &measurement);
        let lines = build_arranged_visual_lines(&projection, &HashMap::new(), 1500., &wide);
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
                    .find(|l| l.projected_start() == s.projection_start())
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                            .segment_for_range(&line.projected_range())
                            .unwrap()
                            .projection_start()
                            == line.projected_start()
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
                    .all(|pair| pair[0].projected_start() <= pair[1].projected_start())
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
                    .filter(|line| segment.projection_range().contains(&line.projected_start()))
                    .map(|line| line.projected_range())
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
                assert_eq!(rendered, projection.text()[segment.projection_range()]);
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
                    align_components: false,
                    group: images[0].node_id,
                    item,
                    row: item / 2,
                    columns: 2,
                    cards: false,
                    card_accent: crate::adaptive::CardAccent::None,
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
        let geometry = component_geometry(
            &projection,
            &lines,
            1000.,
            1.,
            &visual_line_paint_order(&lines),
        );
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
        let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
        let plan = build_measured_adaptive_plan(&projection, 1100., 1000., None, false, &measurement);
        let lines = build_measured_visual_lines(&projection, &HashMap::new(), 1100., &plan, Some(&measurement));
        for segment in projection.segments() {
            let fragments = lines
                .iter()
                .filter(|line| segment.projection_range().contains(&line.projected_start()))
                .collect::<Vec<_>>();
            assert_eq!(fragments.len(), 3);
            assert!(
                fragments[1..]
                    .iter()
                    .all(|line| projection.text()[line.projected_range()].starts_with('→'))
            );
            assert_eq!(
                fragments
                    .iter()
                    .map(|line| &projection.text()[line.projected_range()])
                    .collect::<String>(),
                projection.text()[segment.projection_range()]
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                let actual = lines.iter().filter(|line| segment.projection_range().contains(&line.projected_start()))
                    .map(|line| &projection.text()[line.projected_range()]).collect::<String>();
                assert_eq!(actual, projection.text()[segment.projection_range()]);
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn section_intro_and_short_independent_list_share_a_measured_row(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            use crate::adaptive::rows::RowKind;

            let source = concat!(
                "# Native document workspace\n\n",
                "A calm editing surface.\n\n",
                "## 1. Product contract\n\n",
                "The editing surface is the document.\n\n",
                "The planner keeps source order stable.\n\n",
                "Confirmed decisions:\n\n",
                "- [x] Rich content inside table cells\n",
                "- [x] Byte-preserving autosave\n",
                "- [x] Files and Outline; no minimap\n",
                "- [x] Automatic layout\n",
                "- [x] Light theme\n",
                "- [x] Native controls\n",
                "- [x] Stable selection and scroll anchors\n",
                "- [x] Accessible source-order navigation\n\n",
                "## 2. Following section\n\n",
                "This remains below the complete arrangement.\n",
            );
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let wide =
                build_measured_adaptive_plan(&projection, 992., 1000., None, false, &measurement);
            let row = wide
                .measured_rows
                .chosen
                .iter()
                .find(|row| row.kind == RowKind::IntroList)
                .unwrap_or_else(|| {
                    panic!(
                        "measured section introduction/list row; chosen={:?}; candidates={:?}",
                        wide.measured_rows.chosen, wide.measured_rows.candidates
                    )
                });
            assert_eq!(row.parts.len(), 2);
            assert_eq!(row.widths.len(), 2);

            let segment = |text: &str| {
                projection
                    .segments()
                    .iter()
                    .find(|segment| &projection.text()[segment.projection_range()] == text)
                    .unwrap()
            };
            assert!(
                !wide
                    .slots
                    .contains_key(&segment("1. Product contract").node_id),
                "the authored heading remains full width"
            );
            for text in [
                "The editing surface is the document.",
                "The planner keeps source order stable.",
            ] {
                let slot = wide.slots[&segment(text).node_id];
                assert_eq!(slot.item, 0);
                assert!(!slot.cards);
            }
            let label = wide.slots[&segment("Confirmed decisions:").node_id];
            assert_eq!(label.item, 1);
            assert!(label.cards);
            let checklist = wide.slots[&segment("Rich content inside table cells").node_id];
            assert_eq!(checklist.item, 1);
            assert!(checklist.cards);

            let narrow =
                build_measured_adaptive_plan(&projection, 520., 1000., None, false, &measurement);
            assert!(
                narrow
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.kind != RowKind::IntroList),
                "narrow layouts retain the source-order stack"
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn product_plan_opens_with_a_labelled_editorial_split(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!("../../../../plan.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan =
                build_measured_adaptive_plan(&projection, 1600., 1400., None, false, &measurement);
            let row = plan
                .measured_rows
                .chosen
                .iter()
                .find(|row| row.kind == RowKind::IntroList)
                .unwrap_or_else(|| {
                    panic!(
                        "plan.md must compose its product contract; candidates={:?}",
                        plan.measured_rows.candidates
                    )
                });
            assert_eq!(row.parts.len(), 2);
            assert!(
                plan.slots
                    .values()
                    .any(|slot| slot.card_accent == crate::adaptive::CardAccent::Leading)
            );
            let labelled_groups = plan
                .slots
                .values()
                .filter(|slot| slot.card_accent == crate::adaptive::CardAccent::OpenLabeled)
                .map(|slot| slot.group)
                .collect::<HashSet<_>>();
            assert!(
                !labelled_groups.is_empty(),
                "plan.md should compose repeated labelled lists throughout the document; groups={labelled_groups:?}; decisions={:?}",
                plan.measured_lists
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn labelled_lists_compose_into_open_features_across_the_document(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = include_str!(
                "../../../../performance/layout-fixtures/52-labelled-list-compositions.md"
            );
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan =
                build_measured_adaptive_plan(&projection, 1200., 4000., None, false, &measurement);
            let accented = plan
                .slots
                .values()
                .filter(|slot| slot.card_accent == crate::adaptive::CardAccent::OpenLabeled)
                .count();
            let grids = plan
                .lists
                .values()
                .filter(|list| matches!(list.layout, ListLayout::Grid(_)))
                .count();
            // Both independent entity groups fit the title + four-body-line
            // budget. The uneven constraints and instruction sequence do not.
            assert_eq!(grids, 2, "decisions={:?}", plan.measured_lists);
            assert_eq!(accented, 9);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn three_matched_sibling_sections_prefer_one_complete_card_row(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            use crate::adaptive::rows::RowKind;

            let source = concat!(
                "## Architecture\n\n",
                "Three bounded components.\n\n",
                "### Core\n\nStable nodes, source-preserving import, transactions, selection, undo, and serialization.\n\n",
                "### View\n\nMeasured shaping, automatic arrangements, hit testing, virtualization, tables, HTML, and formulas.\n\n",
                "### App\n\nNative shell, file navigation, autosave, recovery, image loading, and performance instrumentation.\n",
            );
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let wide =
                build_measured_adaptive_plan(&projection, 992., 1000., None, false, &measurement);
            let row = wide
                .measured_rows
                .chosen
                .iter()
                .find(|row| row.kind == RowKind::Peer)
                .expect("complete sibling row");
            assert_eq!(
                row.ids.len(),
                3,
                "planner fragmented matched siblings: chosen={:?}; complete={:?}",
                wide.measured_rows.chosen,
                wide.measured_rows
                    .candidates
                    .iter()
                    .filter(|row| row.kind == RowKind::Peer && row.ids.len() == 3)
                    .collect::<Vec<_>>()
            );
            assert_eq!(row.widths.len(), 3);
            assert!(row.parts.iter().all(|part| {
                projection.roots().collect::<Vec<_>>()[part.clone()]
                    .first()
                    .is_some_and(|block| matches!(block, BlockNode::Heading(_)))
            }));

            let narrow =
                build_measured_adaptive_plan(&projection, 700., 1000., None, false, &measurement);
            assert!(
                narrow
                    .measured_rows
                    .chosen
                    .iter()
                    .all(|row| row.kind != RowKind::Peer),
                "a matched trio must not collapse into an orphan plus pair: {:?}",
                narrow.measured_rows.chosen,
            );
            let restored = build_measured_adaptive_plan(
                &projection,
                1030.,
                1000.,
                Some(&narrow),
                false,
                &measurement,
            );
            assert!(
                restored
                    .measured_rows
                    .chosen
                    .iter()
                    .any(|row| row.kind == RowKind::Peer && row.ids.len() == 3),
                "the complete row must reform after width returns: {:?}",
                restored.measured_rows.chosen,
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn focused_peer_row_keeps_widths_through_growth_and_reflows_only_after_blur(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let source = "### Alpha\n\nA short explanation.\n\n### Beta\n\nA second explanation.\n\n### Gamma\n\nA third explanation.\n";
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
            let node = projection.segments().iter().find(|s| projection.text()[s.projection_range()] == *"A second explanation.").unwrap().node_id;
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
                let fragments = lines.iter().filter(|line| segment.projection_range().contains(&line.projected_start()));
                assert_eq!(fragments.map(|line| &projection.text()[line.projected_range()]).collect::<String>(), projection.text()[segment.projection_range()]);
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let document = Document::from_markdown("# Document title\n\nA short introductory paragraph.\n\n## Next section\n\nMore content.\n").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
            let node = projection.segments()[0].node_id;
            let locked = build_edit_locked_adaptive_plan(&projection, 1100., 800., Some(&plan), false, &measurement, Some(node));
            let before = build_measured_visual_lines(&projection, &HashMap::new(), 1100., &plan, Some(&measurement));
            let after = build_measured_visual_lines(&projection, &HashMap::new(), 1100., &locked, Some(&measurement));
            assert_eq!(before.len(), after.len());
            for (before, after) in before.iter().zip(&after) {
                assert_eq!(before.projected_range(), after.projected_range());
                assert!((before.y - after.y).abs() < 0.01, "gap moved for {:?}: {} -> {}", before.projected_range(), before.y, after.y);
            }
        });
    }

    #[gpui::test]
    fn focused_stack_keeps_its_prose_measure_when_the_window_grows(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
                    .map(|line| line.projected_range())
                    .collect::<Vec<_>>(),
                widened
                    .iter()
                    .map(|line| line.projected_range())
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
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
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
        let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
        let source = "### Alpha\n\nA short explanation.\n\n### Beta\n\nA second explanation.\n\n### Gamma\n\nA third explanation.\n";
        let mut document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let plan = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
        let node = projection
            .segments()
            .iter()
            .find(|s| projection.text()[s.projection_range()] == *"A second explanation.")
            .unwrap()
            .node_id;
        let slot = plan.slots[&node];
        assert_eq!(slot.item, 1);
        document
            .apply(EditCommand::ReplaceText {
                node_id: node,
                range: 0..0,
                text: "Extra detail. " .repeat(10),
                typing: true,
                selection_after: None,
            })
            .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let typing = build_measured_adaptive_plan(&projection, 1100., 800., Some(&plan), true, &measurement);
        assert_eq!(typing.slots[&node], slot);
        let reconsidered = build_measured_adaptive_plan(&projection, 1100., 800., Some(&typing), false, &measurement);
        let fresh = build_measured_adaptive_plan(&projection, 1100., 800., None, false, &measurement);
        assert!(!fresh.slots.contains_key(&node));
        assert!(reconsidered.measured_rows.candidates.iter().any(|row|
            row.rejected == Some(crate::adaptive::rows::RowRejection::UnevenHeights)));
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
                        preview_edit_node: None,
                        expanded_code_tail: None,
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
                    .projection_end();
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
