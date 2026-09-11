//! Bounded, deterministic source-order row search. No editor or renderer access.
use std::ops::Range;

use document_core::NodeId;

use super::candidates::Penalties;
use super::groups::{GroupKind, RelationshipKind};
use super::{AdaptivePlan, LayoutSlot, compact_section};
use crate::TextProjection;
use document_core::BlockNode;

pub(crate) const WINDOW_GROUPS: usize = 40;
// Soft logical display width for a readable figure, distinct from raster
// resolution and from the 260px hard candidate floor. Smaller authored images
// keep their intrinsic size and are never enlarged to meet this preference.
const GALLERY_COMFORT_WIDTH: f32 = 360.;
pub(crate) const TEMPLATES: &[&[u8]] = &[
    &[12],
    &[6, 6],
    &[4, 8],
    &[8, 4],
    &[5, 7],
    &[7, 5],
    &[4, 4, 4],
    // Narrow reference rails are measured table-only candidates, not prose.
    &[3, 9],
    &[9, 3],
];

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GroupMeasurement {
    pub height: f32,
    pub preferred_width: f32,
    pub overflow: bool,
    /// First root table/code border, relative to this group's start.
    pub component_top: Option<f32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowKind {
    Stack,
    /// Authored introductory lead beside its complete overview, below the title.
    Opening,
    Peer,
    /// Compact reference siblings retain block spacing, code/table chrome,
    /// and explicit checklist summaries. Ordinary H3 peers remain open modules.
    Technical,
    IntroList,
    Explanation,
    /// Complete evidence figure followed by its source-adjacent explanation.
    FigureExplanation,
    /// A table/code block followed by its complete same-section explanation.
    ContentExplanation,
    /// Source-adjacent optional guidance beside its main prose, never a warning.
    Aside,
    /// Adjacent optional callouts retain their independent semantic panels.
    Guidance,
    Tables,
    Gallery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowRejection {
    TooNarrow,
    Unmeasured,
    Overflow,
    TooTall,
    UnevenHeights,
    EditLock,
}

#[derive(Clone, Debug)]
pub(crate) struct RowCandidate {
    pub canvas: f32,
    pub roots: Range<usize>,
    /// Half-open interval in the bounded unit window (groups, or a gallery's
    /// consecutive internal figures).
    pub groups: Range<usize>,
    pub ids: Vec<NodeId>,
    /// Canonical root ranges, one per column (a heading prefix may stay above).
    pub parts: Vec<Range<usize>>,
    pub kind: RowKind,
    pub template: usize,
    pub widths: Vec<f32>,
    pub heights: Vec<f32>,
    pub penalties: Penalties,
    pub rejected: Option<RowRejection>,
    pub previous: bool,
    /// A stacked explanation/example has two internal flow rows; scoring a
    /// compound stack as one rectangle otherwise hides its unused row widths.
    pub flow_rows: u8,
    pub edit_locked: bool,
    /// The retained editing topology is valid even when bounded candidate
    /// measurement cannot revisit a grown node; final renderer geometry still
    /// lays out all canonical text. Do not report this height as newly measured.
    pub height_estimated: bool,
    /// A resource fallback held by an edit lock is not a settled presentation
    /// preference, even after its current footprint can be measured exactly.
    pub decision_provisional: bool,
}

impl RowCandidate {
    pub fn cost(&self) -> f32 {
        self.penalties.weighted() * f32::from(self.flow_rows)
    }
    pub fn legal(&self) -> bool {
        self.rejected.is_none()
            && (1..=if self.kind == RowKind::Aside {
                WINDOW_GROUPS
            } else {
                3
            })
                .contains(&self.groups.len())
            && !self.roots.is_empty()
            && self.ids.len() == self.groups.len()
            && self
                .ids
                .iter()
                .enumerate()
                .all(|(index, id)| !self.ids[..index].contains(id))
            && self.parts.len() == self.widths.len()
            && self.parts.iter().all(|part| {
                !part.is_empty() && part.start >= self.roots.start && part.end <= self.roots.end
            })
            && self
                .parts
                .windows(2)
                .all(|parts| parts[0].end <= parts[1].start)
            && (self.kind != RowKind::Stack
                || (self.template == 0 && self.parts.first() == Some(&self.roots)))
            && (1..=2).contains(&self.flow_rows)
            && self.cost().is_finite()
            && self.template < TEMPLATES.len()
            && self.widths.len() == TEMPLATES[self.template].len()
            && self
                .widths
                .iter()
                .zip(TEMPLATES[self.template])
                .all(|(width, span)| {
                    super::candidates::span_width(self.canvas, *span)
                        .is_some_and(|expected| (expected - width).abs() < 0.01)
                })
            && self
                .widths
                .iter()
                .all(|width| width.is_finite() && *width > 0.)
            && (self.kind == RowKind::Stack
                || (self.heights.len() == self.widths.len()
                    && self
                        .heights
                        .iter()
                        .all(|height| height.is_finite() && *height > 0.)))
    }
    pub fn same_placement(&self, other: &Self) -> bool {
        self.ids == other.ids && self.template == other.template && self.kind == other.kind
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct RowSelection {
    pub viewport: f32,
    pub chosen: Vec<RowCandidate>,
    pub candidates: Vec<RowCandidate>,
    pub windows: usize,
    pub retained_previous: bool,
    pub edit_locked: bool,
    pub covered: Vec<Range<usize>>,
    pub deferred_windows: usize,
    pub validation_fallbacks: usize,
}

struct Unit {
    /// Beginning of the object, excluding a heading/explanation but including
    /// its trailing authored caption and credit in roots.end.
    object_start: usize,
    id: NodeId,
    roots: Range<usize>,
    kind: GroupKind,
    section: Option<NodeId>,
    peer: Option<(Option<NodeId>, u8, u8)>,
    /// Adjacent specification/example sharing literal field identifiers.
    specification_pair: Option<NodeId>,
    /// First prose root of a complete source-adjacent explanation, if any.
    explanation: Option<usize>,
    table_parent: Option<Option<NodeId>>,
    gallery: Option<NodeId>,
    pending_gallery: bool,
    boundary: bool,
}

fn contiguous_peer_count(units: &[Unit], index: usize) -> usize {
    let peer = units[index].peer;
    if peer.is_none() {
        return 0;
    }
    let start = units[..index]
        .iter()
        .rposition(|unit| unit.peer != peer)
        .map_or(0, |position| position + 1);
    let end = units[index + 1..]
        .iter()
        .position(|unit| unit.peer != peer)
        .map_or(units.len(), |position| index + 1 + position);
    end - start
}

fn remap_roots(range: &Range<usize>, old: &[NodeId], new: &[NodeId]) -> Option<Range<usize>> {
    let ids = old.get(range.clone())?;
    let first = *ids.first()?;
    let start = new.iter().position(|id| *id == first)?;
    let end = start.checked_add(ids.len())?;
    (new.get(start..end)? == ids).then_some(start..end)
}

/// Prefix roots paint above the row's columns. A caret in that shared heading
/// does not own the independent column arrangement below it. Use topology,
/// not a family allowlist: a peer heading inside a column must stay locked.
fn keeps_heading_outside_columns(row: &RowCandidate, heading: usize) -> bool {
    row.roots.start == heading
        && (row.kind == RowKind::Stack
            || (row
                .parts
                .first()
                .is_some_and(|part| part.start == heading + 1)
                && row.parts.iter().all(|part| part.start > heading)))
}

/// Focus is a hard constraint, not a score bonus. A grown paragraph may no
/// longer nominate a compact peer unit, so retain the canonical row explicitly
/// rather than relying on the old template being enumerated again.
fn constrain_editing_row(
    candidates: &mut Vec<RowCandidate>,
    window: &[Unit],
    plan: &AdaptivePlan,
    projection: &TextProjection,
    previous: Option<&AdaptivePlan>,
    canvas: f32,
    measure: &mut impl FnMut(Range<usize>, f32, bool) -> Option<GroupMeasurement>,
) -> bool {
    let Some((node, old)) = plan.editing_node.zip(previous) else {
        return false;
    };
    let Some(root) = projection
        .segment_for_node(node)
        .map(|s| s.top_level_node_id)
    else {
        return false;
    };
    let Some(ordinal) = old.root_ids.iter().position(|id| *id == root) else {
        return false;
    };
    let Some(old_row) = old
        .measured_rows
        .chosen
        .iter()
        .find(|row| row.roots.contains(&ordinal))
    else {
        return false;
    };
    let Some(roots) = remap_roots(&old_row.roots, &old.root_ids, &plan.root_ids) else {
        return false;
    };
    let overlaps = |range: &Range<usize>| range.start < roots.end && roots.start < range.end;
    if !window.iter().any(|unit| overlaps(&unit.roots)) {
        return false;
    }
    // The section heading precedes both explanation columns at full width.
    // Its caret must not freeze the independent arrangement below it. Other
    // row kinds can move that heading into a track and remain locked out.
    if matches!(projection.block(root), Some(BlockNode::Heading(_)))
        && keeps_heading_outside_columns(old_row, ordinal)
    {
        let mut locked = false;
        for candidate in candidates
            .iter_mut()
            .filter(|row| row.roots.contains(&roots.start))
        {
            if !keeps_heading_outside_columns(candidate, roots.start) {
                candidate.rejected = Some(RowRejection::EditLock);
                locked = true;
            }
        }
        return locked;
    }
    let parts = old_row
        .parts
        .iter()
        .map(|part| remap_roots(part, &old.root_ids, &plan.root_ids))
        .collect::<Option<Vec<_>>>();
    let bounds = window
        .iter()
        .position(|unit| unit.roots.start == roots.start)
        .zip(window.iter().position(|unit| unit.roots.end == roots.end));
    // Keep natural typing growth stable, but a deliberate height reduction
    // must release columns that no longer fit the same viewport-height gate
    // used by candidate selection. Unknown retained heights cannot prove fit.
    let fits_shorter_viewport = (old_row.kind != RowKind::Opening
        || plan.measured_rows.viewport >= 480.)
        && (plan.measured_rows.viewport + 0.5 >= old.measured_rows.viewport
            || old_row.kind == RowKind::Stack
            || (!old_row.height_estimated
                && old_row
                    .heights
                    .iter()
                    .all(|height| *height <= plan.measured_rows.viewport * 0.7)));
    let retained = if old_row.canvas <= canvas + 0.01 && fits_shorter_viewport {
        bounds
            .zip(parts)
            .map(|((start, end), parts)| {
                let mut row = old_row.clone();
                row.roots = roots.clone();
                row.groups = start..end + 1;
                row.parts = parts;
                // Natural height growth must not release the lock. Bounded
                // measurement may decline a large edited node; retain its topology
                // and mark the cached height explicitly until the renderer lays it out.
                let measurements = row
                    .parts
                    .iter()
                    .zip(&row.widths)
                    .enumerate()
                    .map(|(item, (part, width))| {
                        let cards = row.kind == RowKind::Peer
                            || (row.kind == RowKind::IntroList && item == 1);
                        measure(part.clone(), *width, cards)
                    })
                    .collect::<Option<Vec<_>>>();
                if let Some(mut measurements) = measurements
                    .filter(|values| values.iter().all(|m| m.height.is_finite() && m.height > 0.))
                {
                    if row.kind == RowKind::Technical {
                        align_component_measurements(&mut measurements);
                    }
                    row.heights = measurements.iter().map(|m| m.height).collect();
                    row.height_estimated = false;
                } else {
                    row.height_estimated = true;
                }
                row.edit_locked = true;
                row.previous = true;
                row.rejected = None;
                row
            })
            .filter(RowCandidate::legal)
    } else {
        None
    };
    // A newly usable explanation pair may be deferred only because its prose
    // has focus (including after resize/resource arrival). A forced stack is not a
    // settled layout preference. Keep the lock now, but allow normal measured
    // competition after blur instead of charging a permanent change penalty.
    let deferred_pair = candidates.iter().find_map(|candidate| {
        (old_row.kind == RowKind::Stack
            && matches!(
                candidate.kind,
                RowKind::FigureExplanation
                    | RowKind::ContentExplanation
                    | RowKind::Explanation
                    | RowKind::Opening
                    | RowKind::Guidance
            )
            && candidate.legal()
            && overlaps(&candidate.roots))
        .then(|| candidate.roots.clone())
    });
    if let Some(deferred) = &deferred_pair {
        for candidate in candidates.iter_mut().filter(|candidate| {
            candidate.kind == RowKind::Stack
                && candidate.roots.start < deferred.end
                && deferred.start < candidate.roots.end
        }) {
            candidate.decision_provisional = true;
        }
    }
    for candidate in candidates.iter_mut().filter(|row| overlaps(&row.roots)) {
        // If the old widths cannot fit, use the nearest source-order stack;
        // do not jump the caret into a different two/three-column template.
        if retained.is_some() || candidate.kind != RowKind::Stack {
            candidate.rejected = Some(RowRejection::EditLock);
        }
    }
    if let Some(mut row) = retained {
        row.decision_provisional |= deferred_pair.is_some();
        if row.decision_provisional
            && let Some(gallery) = window
                .iter()
                .find(|unit| overlaps(&unit.roots))
                .and_then(|unit| unit.gallery)
        {
            // The focused figure can prevent its neighbors from sharing a
            // row. Do not turn those forced neighboring stacks into permanent
            // preferences either. This applies only to the same source group.
            for candidate in candidates.iter_mut() {
                if window[candidate.groups.clone()]
                    .iter()
                    .any(|unit| unit.gallery == Some(gallery))
                {
                    candidate.decision_provisional = true;
                }
            }
        }
        candidates.push(row);
    }
    true
}

/// Canonical content stays whole. Galleries may arrange consecutive internal
/// figures; bounded complete subsections may combine their own child groups.
/// Neither can absorb a following unrelated sibling or chapter.
pub(crate) fn measure_rows(
    plan: &mut AdaptivePlan,
    projection: &TextProjection,
    viewport: f32,
    previous: Option<&AdaptivePlan>,
    keep: bool,
    mut measure: impl FnMut(Range<usize>, f32, bool) -> Option<GroupMeasurement>,
    mut prefers_figure_wrap: impl FnMut(Range<usize>) -> bool,
) {
    let canvas = plan.canvas;
    plan.measured_rows.viewport = viewport;
    if keep && let Some(old) = previous {
        plan.measured_rows = old.measured_rows.clone();
        plan.measured_rows.viewport = viewport;
        plan.measured_rows.chosen = old
            .measured_rows
            .chosen
            .iter()
            .filter_map(|row| {
                if !row.legal() {
                    return None;
                }
                let mut row = row.clone();
                row.roots = remap_roots(&row.roots, &old.root_ids, &plan.root_ids)?;
                row.parts = row
                    .parts
                    .iter()
                    .map(|part| remap_roots(part, &old.root_ids, &plan.root_ids))
                    .collect::<Option<Vec<_>>>()?;
                Some(row)
            })
            .collect();
        // Candidate ranges/scores belong to the old revision; only the
        // identity-remapped placements are meaningful during a text refresh.
        plan.measured_rows.candidates.clear();
        plan.measured_rows.covered = old
            .measured_rows
            .covered
            .iter()
            .filter_map(|range| {
                let current = remap_roots(range, &old.root_ids, &plan.root_ids)?;
                current
                    .clone()
                    .all(|i| plan.unchanged_root(old, plan.root_ids[i]))
                    .then_some(current)
            })
            .collect();
        // Reapply retained canonical parts to the current descendants. Copying
        // only old leaf IDs leaves inserted table cells at the document origin
        // until the asynchronous optimizer gets a turn.
        let roots = projection.roots().collect::<Vec<_>>();
        let chosen = plan.measured_rows.chosen.clone();
        apply_rows(plan, projection, &roots, canvas, &chosen);
        return;
    }
    let roots = projection.roots().collect::<Vec<_>>();
    let units = build_units(plan, &roots);
    let previous_rows = previous
        .map(|old| {
            old.measured_rows
                .chosen
                .iter()
                .filter(|row| row.legal())
                .filter_map(|row| Some((*old.root_ids.get(row.roots.start)?, row)))
                .collect::<std::collections::HashMap<_, _>>()
        })
        .unwrap_or_default();
    let previous_covered = previous
        .map(|old| {
            old.measured_rows
                .covered
                .iter()
                .filter_map(|range| {
                    let start = plan.root_ordinal(*old.root_ids.get(range.start)?)?;
                    let end = start.checked_add(range.len())?;
                    (old.root_ids.get(range.clone())? == plan.root_ids.get(start..end)?)
                        .then_some((start, end))
                })
                .collect::<std::collections::HashSet<_>>()
        })
        .unwrap_or_default();
    for unit_range in unit_windows(&units) {
        let window = &units[unit_range];
        let root_range = window[0].roots.start..window.last().unwrap().roots.end;
        let active = plan.measurement_ranges.as_ref().is_none_or(|ranges| {
            ranges
                .iter()
                .any(|range| range.start < root_range.end && root_range.start < range.end)
        });
        let mut candidates = Vec::new();
        if !active {
            let chosen = offscreen_rows(plan, projection, previous, window, &previous_rows, canvas);
            let covered = previous.is_some_and(|old| {
                plan.compatible_environment(old)
                    && root_range
                        .clone()
                        .all(|ordinal| plan.unchanged_root(old, plan.root_ids[ordinal]))
                    && previous_covered.contains(&(root_range.start, root_range.end))
                    && (old.editing_node == plan.editing_node
                        || old
                            .editing_node
                            .and_then(|node| projection.segment_for_node(node))
                            .and_then(|s| plan.root_ordinal(s.top_level_node_id))
                            .is_none_or(|ordinal| !root_range.contains(&ordinal)))
                    && chosen.iter().all(|row| {
                        row.previous && (!row.edit_locked || old.editing_node == plan.editing_node)
                    })
            });
            if covered {
                plan.measured_rows.covered.push(root_range);
            }
            apply_rows(plan, projection, &roots, canvas, &chosen);
            plan.measured_rows.chosen.extend(chosen);
            plan.measured_rows.deferred_windows += 1;
            continue;
        }
        plan.measured_rows.covered.push(root_range);
        for (i, unit) in window.iter().enumerate() {
            let measured = measure(unit.roots.clone(), canvas, false);
            candidates.push(stack_candidate(unit, i, canvas, measured));
            for (template, spans, explanation) in
                TEMPLATES
                    .iter()
                    .enumerate()
                    .skip(1)
                    .flat_map(|(template, spans)| {
                        std::iter::once((template, spans, None)).chain(
                            unit.explanation
                                .filter(|_| spans.len() == 2)
                                .map(|start| (template, spans, Some(start))),
                        )
                    })
            {
                let (kind, consume, parts) = if let Some(start) = explanation {
                    let split = unit.object_start;
                    // Prefix headings stay full-width. The complete adjacent
                    // introduction is measured as one source-order prose slot.
                    (
                        RowKind::Explanation,
                        1,
                        vec![start..split, split..unit.roots.end],
                    )
                } else if spans.len() == 2
                    && let Some(parts) = opening_parts(window, i, &roots, plan, projection)
                {
                    (RowKind::Opening, 2, parts)
                } else if *spans == [6, 6]
                    && let Some(parts) = adjacent_guidance_parts(window, i, &roots)
                {
                    (RowKind::Guidance, 2, parts)
                } else if spans.len() == 2
                    && let Some(parts) = following_figure_parts(window, i, &roots)
                    && !prefers_figure_wrap(parts[0].start..parts[1].end)
                {
                    (RowKind::FigureExplanation, 2, parts)
                } else if spans.len() == 2
                    && let Some(parts) = following_content_parts(window, i, &roots)
                {
                    (RowKind::ContentExplanation, 2, parts)
                } else if matches!(*spans, [8, 4] | [7, 5])
                    && let Some((consume, parts)) = adjacent_aside_parts(window, i, &roots)
                {
                    (RowKind::Aside, consume, parts)
                } else if spans.len() == 2
                    && let Some((consume, parts)) = intro_list_parts(window, i, &roots)
                {
                    (RowKind::IntroList, consume, parts)
                } else if unit.gallery.is_some()
                    && !unit.pending_gallery
                    && i + spans.len() <= window.len()
                    && window[i..i + spans.len()]
                        .iter()
                        .all(|other| other.gallery == unit.gallery)
                {
                    (
                        RowKind::Gallery,
                        spans.len(),
                        window[i..i + spans.len()]
                            .iter()
                            .map(|unit| unit.object_start..unit.roots.end)
                            .collect(),
                    )
                } else if unit.table_parent.is_some()
                    && spans.len() == 2
                    && i + 2 <= window.len()
                    && window[i + 1].table_parent == unit.table_parent
                {
                    (
                        RowKind::Tables,
                        2,
                        window[i..i + 2]
                            .iter()
                            .map(|unit| unit.roots.clone())
                            .collect(),
                    )
                } else if spans.len() == 2
                    && unit.specification_pair.is_some()
                    && window
                        .get(i + 1)
                        .is_some_and(|next| next.specification_pair == unit.specification_pair)
                {
                    (
                        RowKind::Technical,
                        2,
                        window[i..i + 2]
                            .iter()
                            .map(|unit| unit.roots.clone())
                            .collect(),
                    )
                } else if unit.peer.is_some()
                    && plan.editorials.get(&unit.id).filter(|m| m.kind.counterpart().is_some()).is_none_or(|member| {
                        *spans == [6, 6] && window.get(i + 1)
                            .and_then(|next| plan.editorials.get(&next.id))
                            .is_some_and(|m| member.kind.accepts_counterpart(m.kind))
                    })
                    && i + spans.len() <= window.len()
                    && window[i..i + spans.len()]
                        .iter()
                        .all(|u| u.peer == unit.peer)
                    // A matched card trio is one visual idea. Technical
                    // sections merely share a broad reference role: a wide
                    // comparison may precede two compact tables. Let measured
                    // source-order partitions compete for those complete
                    // sections instead of treating them as three equal cards.
                    // Named counterparts are pairs, not a three-item family.
                    && !(spans.len() == 2
                        && unit.peer.is_some_and(|(_, _, shape)| shape != super::TECHNICAL_SECTION_SHAPE)
                        && contiguous_peer_count(window, i) == 3
                        && plan.editorials.get(&unit.id)
                            .and_then(|m| m.kind.counterpart()).is_none())
                {
                    (
                        if unit
                            .peer
                            .is_some_and(|(_, _, shape)| shape == super::TECHNICAL_SECTION_SHAPE)
                        {
                            RowKind::Technical
                        } else {
                            RowKind::Peer
                        },
                        spans.len(),
                        window[i..i + spans.len()]
                            .iter()
                            .map(|u| u.roots.clone())
                            .collect(),
                    )
                } else {
                    continue;
                };
                // Sibling text columns share a measure. Content length may
                // select a stack, but must not give one sibling a wider track.
                if matches!(kind, RowKind::Opening | RowKind::Peer)
                    && spans.windows(2).any(|pair| pair[0] != pair[1])
                {
                    continue;
                }
                let narrow_table = spans.iter().position(|span| *span == 3);
                if narrow_table.is_some()
                    && (!matches!(kind, RowKind::Technical | RowKind::Tables)
                        || !parts.iter().all(|part| table_only_part(&roots, part)))
                {
                    continue;
                }
                // Keep the main explanation at its loaded-font reading
                // measure. The pair may use a bounded part of an ultrawide
                // canvas; unused outer space is preferable to very long lines.
                let canvas = if kind == RowKind::Guidance {
                    canvas.min(
                        plan.prose_measures.fit_width(f32::INFINITY, false, false) * 2.
                            + super::LAYOUT_GAP,
                    )
                } else if kind == RowKind::Opening {
                    opening_measures(plan, projection, &roots, &parts)
                        .into_iter()
                        .zip(spans.iter())
                        .map(|(measure, span)| {
                            let maximum = measure
                                * crate::theme::DocumentStyle::MAX_PROSE_CHARACTERS
                                / crate::theme::DocumentStyle::PROSE_CHARACTERS;
                            (maximum + super::LAYOUT_GAP) * 12. / f32::from(*span)
                                - super::LAYOUT_GAP
                        })
                        .fold(canvas, f32::min)
                } else if kind == RowKind::Aside {
                    canvas.min(
                        (plan.prose_measures.reference + super::LAYOUT_GAP) * 12.
                            / f32::from(spans[0])
                            - super::LAYOUT_GAP,
                    )
                } else if matches!(
                    kind,
                    RowKind::FigureExplanation | RowKind::ContentExplanation
                ) {
                    let narrative = projection
                        .segment_for_node(roots[parts[1].start].id())
                        .is_some_and(|segment| segment.context.narrative);
                    let prose_width = plan.prose_measures.fit_width(canvas, narrative, false);
                    let object_width = if matches!(roots[parts[0].start], BlockNode::Table(_)) {
                        let id = roots[parts[0].start].id();
                        projection
                            .fitted_table_widths(id, projection.table_available_width(id, canvas))
                            .map_or(canvas, |widths| widths.iter().sum())
                    } else if kind == RowKind::FigureExplanation {
                        measure(parts[0].start..parts[0].start + 1, canvas, false)
                            .map_or(canvas, |image| image.preferred_width)
                    } else {
                        canvas
                    };
                    [(prose_width, spans[1]), (object_width, spans[0])]
                        .into_iter()
                        .map(|(width, span)| {
                            (width + super::LAYOUT_GAP) * 12. / f32::from(span) - super::LAYOUT_GAP
                        })
                        .fold(canvas, f32::min)
                } else {
                    canvas
                };
                let mut widths = spans
                    .iter()
                    .map(|s| super::candidates::span_width(canvas, *s).unwrap_or(0.))
                    .collect::<Vec<_>>();
                let mut rejection = (canvas < crate::theme::DocumentStyle::SINGLE_COLUMN_WIDTH
                    || (matches!(
                        kind,
                        RowKind::Aside
                            | RowKind::FigureExplanation
                            | RowKind::ContentExplanation
                            | RowKind::Opening
                    ) && canvas < 900.)
                    || (matches!(
                        kind,
                        RowKind::FigureExplanation | RowKind::ContentExplanation
                    ) && widths[1] < plan.prose_measures.reference * 0.6)
                    || (kind == RowKind::Opening
                        && widths
                            .iter()
                            .zip(opening_measures(plan, projection, &roots, &parts))
                            .any(|(width, measure)| {
                                *width
                                    < measure * 40. / crate::theme::DocumentStyle::PROSE_CHARACTERS
                            }))
                    || (kind == RowKind::Guidance
                        && widths.iter().any(|width| {
                            *width + 0.01
                                < plan.prose_measures.fit_width(f32::INFINITY, false, false)
                        }))
                    || widths.iter().any(|w| *w < 260.))
                .then_some(RowRejection::TooNarrow);
                if kind == RowKind::Opening && viewport < 480. {
                    rejection = Some(RowRejection::TooTall);
                }
                let mut measurements = Vec::new();
                if rejection.is_none() {
                    for (item, (part, width)) in parts.iter().zip(&widths).enumerate() {
                        let cards =
                            kind == RowKind::Peer || (kind == RowKind::IntroList && item == 1);
                        match measure(part.clone(), *width, cards) {
                            Some(m)
                                if m.height.is_finite()
                                    && m.height > 0.
                                    && m.preferred_width.is_finite() =>
                            {
                                measurements.push(m)
                            }
                            _ => {
                                rejection = Some(RowRejection::Unmeasured);
                                break;
                            }
                        }
                    }
                }
                // Intrinsically small figures must not be upscaled. Fit a
                // complete gallery to its actual footprints so unused track
                // space does not inflate the visible image gutter. Re-measure
                // it; labels still negotiate width and all fit gates apply.
                let available_canvas = canvas;
                let canvas = if kind == RowKind::Gallery
                    && rejection.is_none()
                    && measurements
                        .iter()
                        .zip(&widths)
                        .all(|(m, w)| m.preferred_width <= *w)
                {
                    let fitted = measurements
                        .iter()
                        .zip(spans.iter())
                        .map(|(m, span)| {
                            (m.preferred_width + super::LAYOUT_GAP) * 12. / f32::from(*span)
                                - super::LAYOUT_GAP
                        })
                        .fold(0_f32, f32::max)
                        .min(canvas);
                    let fitted_widths = spans
                        .iter()
                        .map(|span| super::candidates::span_width(fitted, *span).unwrap_or(0.))
                        .collect::<Vec<_>>();
                    if fitted >= crate::theme::DocumentStyle::SINGLE_COLUMN_WIDTH
                        && fitted_widths.iter().all(|w| *w >= 260.)
                    {
                        if let Some(next) = parts
                            .iter()
                            .zip(&fitted_widths)
                            .map(|(part, w)| {
                                measure(part.clone(), *w, false).filter(|m| {
                                    m.height.is_finite()
                                        && m.height > 0.
                                        && m.preferred_width.is_finite()
                                })
                            })
                            .collect::<Option<Vec<_>>>()
                        {
                            measurements = next;
                            widths = fitted_widths;
                            fitted
                        } else {
                            canvas
                        }
                    } else {
                        canvas
                    }
                } else {
                    canvas
                };
                let short_component_label = rejection.is_none()
                    && kind == RowKind::Explanation
                    && matches!(
                        roots[parts[1].start],
                        BlockNode::CodeBlock(_) | BlockNode::Table(_)
                    )
                    && matches!(roots[parts[0].end - 1], BlockNode::Paragraph(p)
                        if p.content.as_string().trim_end().ends_with(':'))
                    && measure(parts[0].end - 1..parts[0].end, widths[0], false).is_none_or(
                        |label| label.height < 3. * crate::theme::DocumentStyle::REFERENCE_LEADING,
                    );
                if rejection.is_none() {
                    // A quarter-width rail must fit the complete natural
                    // table and heading. Never gain space by wrapping its
                    // values or squeezing ordinary prose into this track.
                    if narrow_table
                        .is_some_and(|item| measurements[item].preferred_width > widths[item])
                    {
                        rejection = Some(RowRejection::TooNarrow);
                    }
                }
                if rejection.is_none() {
                    if kind == RowKind::Technical {
                        align_component_measurements(&mut measurements);
                    }
                    if measurements.iter().any(|m| m.overflow) {
                        rejection = Some(RowRejection::Overflow);
                    } else if viewport <= 0.
                        || measurements.iter().any(|m| m.height > viewport * 0.7)
                    {
                        rejection = Some(RowRejection::TooTall);
                    } else if (matches!(
                        kind,
                        RowKind::FigureExplanation | RowKind::ContentExplanation
                    ) && (measurements[1].height
                        < 4. * crate::theme::DocumentStyle::REFERENCE_LEADING
                        || measurements[0].height.max(measurements[1].height)
                            > measurements[0].height.min(measurements[1].height) * 2.))
                        || (kind == RowKind::Aside
                            && (measurements[0].height
                                < 4. * crate::theme::DocumentStyle::REFERENCE_LEADING
                                || measurements[1].height > measurements[0].height * 1.5))
                    {
                        rejection = Some(RowRejection::UnevenHeights);
                    } else if kind == RowKind::Explanation
                        && matches!(roots[parts[1].start], BlockNode::Table(_))
                        && (measurements[0].height
                            < 4. * crate::theme::DocumentStyle::REFERENCE_LEADING
                            || measurements[0].height < measurements[1].height * 0.6)
                    {
                        // A caption/brief lead-in belongs above its table, not
                        // alone in a tall neighboring column. Tables stay block
                        // level; this is a paired explanation, never a float.
                        rejection = Some(RowRejection::UnevenHeights);
                    } else if kind == RowKind::Opening
                        && (measurements[0].height < 3. * crate::theme::DocumentStyle::LEAD_LEADING
                            || measurements[1].height
                                < 4. * if projection
                                    .segment_for_node(roots[parts[1].start].id())
                                    .is_some_and(|s| s.context.narrative)
                                {
                                    crate::theme::DocumentStyle::READING_LEADING
                                } else {
                                    crate::theme::DocumentStyle::REFERENCE_LEADING
                                }
                            || measurements[0].height.max(measurements[1].height)
                                > measurements[0].height.min(measurements[1].height) * 1.5)
                    {
                        rejection = Some(RowRejection::UnevenHeights);
                    } else if short_component_label {
                        // An authored short lead-in labels the example below;
                        // it does not justify a mostly empty explanation column.
                        // Longer explanations ending in ':' still negotiate a
                        // pair using their real measured footprint.
                        rejection = Some(RowRejection::UnevenHeights);
                    } else if kind == RowKind::Explanation
                        && unit.peer.is_some()
                        && measurements[0].height
                            < 4. * crate::theme::DocumentStyle::REFERENCE_LEADING
                    {
                        // A compact sibling's brief description stays over its
                        // component; it is not a separate reading column. Rich
                        // introductions may still compete with the peer row.
                        rejection = Some(RowRejection::UnevenHeights);
                    } else if matches!(kind, RowKind::Peer | RowKind::Technical | RowKind::Guidance)
                    {
                        let shortest = measurements
                            .iter()
                            .map(|m| m.height)
                            .fold(f32::INFINITY, f32::min);
                        let tallest = measurements.iter().map(|m| m.height).fold(0., f32::max);
                        // A compact sibling row must not strand short units
                        // beside a sustained argument. Natural-height open
                        // modules use the same balance limit as feature grids;
                        // supported explanation/figure pairs are a distinct role.
                        if tallest > shortest * 1.5 + 0.001 {
                            rejection = Some(RowRejection::UnevenHeights);
                        }
                    }
                }
                let mut penalties = score(kind, &measurements, &widths, viewport, true, false);
                if kind == RowKind::Gallery {
                    // Fitting moves unused space outside the gallery; it does
                    // not make that space disappear from the page budget.
                    penalties.unused_width = penalties
                        .unused_width
                        .max(1. - canvas / available_canvas.max(1.));
                }
                candidates.push(RowCandidate {
                    canvas,
                    roots: unit.roots.start..window[i + consume - 1].roots.end,
                    groups: i..i + consume,
                    ids: window[i..i + consume].iter().map(|u| u.id).collect(),
                    parts,
                    kind,
                    template,
                    widths: widths.clone(),
                    heights: measurements.iter().map(|m| m.height).collect(),
                    penalties,
                    rejected: rejection,
                    previous: false,
                    flow_rows: 1,
                    edit_locked: false,
                    height_estimated: false,
                    decision_provisional: false,
                });
            }
        }
        for i in 0..candidates.len() {
            if candidates[i].kind == RowKind::Stack {
                let index = candidates[i].groups.start;
                let readable = candidates.iter().any(|row| {
                    row.kind != RowKind::Stack
                        && row.rejected.is_none()
                        && row.groups.contains(&index)
                });
                if readable {
                    let m = measure(window[index].roots.clone(), canvas, false).unwrap_or_default();
                    candidates[i].penalties = score(
                        RowKind::Stack,
                        &[m],
                        &[canvas.min(plan.prose_measures.reference)],
                        viewport,
                        true,
                        false,
                    );
                    // If three matched siblings fit as one measured row, a
                    // stack necessarily leaves one member visually orphaned
                    // above or below a two-card row. Treat that split as a
                    // stronger relationship break than an ordinary readable
                    // pair. This remains a soft score: hard width, height, and
                    // overflow gates still force the canonical stack.
                    let complete_three_peer_row = candidates.iter().any(|row| {
                        row.kind == RowKind::Peer
                            && row.rejected.is_none()
                            && row.widths.len() == 3
                            && row.groups.contains(&index)
                    });
                    if complete_three_peer_row {
                        candidates[i].penalties.separation =
                            candidates[i].penalties.separation.max(0.45);
                    }
                    // Splitting a source-adjacent gallery adds vertical travel
                    // that the old constant separation penalty did not see.
                    // Compare complete measured footprints, not image count or
                    // source pixels. Only a legal gallery alternative can add
                    // this normalized cost; narrow/tall fallbacks stay free.
                    let compact_height = candidates
                        .iter()
                        .filter(|row| {
                            row.kind == RowKind::Gallery
                                && row.rejected.is_none()
                                && row.groups.contains(&index)
                        })
                        .map(|row| row.heights.iter().copied().fold(0., f32::max))
                        .min_by(f32::total_cmp);
                    if let Some(compact_height) = compact_height {
                        candidates[i].penalties.separation = (candidates[i].penalties.separation
                            + (m.height - compact_height).max(0.) / viewport.max(1.))
                        .clamp(0., 1.);
                    }
                }
            }
            // Resource-pending/unsupported stacks are usable provisional
            // geometry, not measured decisions that should win hysteresis.
            // Explicit editing locks below still preserve focused topology.
            candidates[i].previous = previous.is_some_and(|old| {
                old.measured_rows.chosen.iter().any(|row| {
                    !row.height_estimated
                        && !row.decision_provisional
                        && row.same_placement(&candidates[i])
                })
            });
            let had_previous = previous.is_some_and(|old| {
                old.measured_rows.chosen.iter().any(|row| {
                    !row.height_estimated
                        && !row.decision_provisional
                        && row.ids.iter().any(|id| candidates[i].ids.contains(id))
                })
            });
            // A narrow or short viewport makes matched trios, technical
            // sections and explanation pairs stack. Once space returns, allow
            // their row to compete without a presentation-change penalty. Width,
            // height, overflow and editing-lock gates still apply.
            let responsive_peer_expansion = (candidates[i].kind == RowKind::Explanation
                || (candidates[i].kind == RowKind::Peer && candidates[i].ids.len() == 3)
                || (matches!(
                    candidates[i].kind,
                    RowKind::Technical
                        | RowKind::Aside
                        | RowKind::FigureExplanation
                        | RowKind::ContentExplanation
                        | RowKind::Opening
                        | RowKind::Guidance
                ) && candidates[i].ids.len() >= 2))
                && previous.is_some_and(|old| {
                    candidates[i].ids.iter().all(|id| {
                        old.measured_rows.chosen.iter().any(|row| {
                            row.kind == RowKind::Stack
                                && row.ids.as_slice() == [*id]
                                && (row.canvas + 0.5 < candidates[i].canvas
                                    || old.measured_rows.viewport + 0.5 < viewport)
                        })
                    })
                });
            // A wrapping figure overrides its backing stack during realization.
            // If that wrap no longer fits, its stack was never a settled visual
            // preference. Let the complete pair compete; editing locks still
            // preserve the active geometry below and in figure-flow retention.
            let previous_figure_wrap = candidates[i].kind == RowKind::FigureExplanation
                && previous.is_some_and(|old| {
                    old.figure_flows
                        .contains_key(&roots[candidates[i].parts[0].start].id())
                });
            candidates[i].penalties.change = if had_previous
                && !candidates[i].previous
                && !responsive_peer_expansion
                && !previous_figure_wrap
            {
                1.
            } else {
                0.
            };
        }
        plan.measured_rows.edit_locked |= constrain_editing_row(
            &mut candidates,
            window,
            plan,
            projection,
            previous,
            canvas,
            &mut measure,
        );
        let mut chosen = choose_window(window.len(), &candidates);
        // Exact ordered partition is checked before any native placement.
        if !chosen.iter().all(|row| valid_for_window(row, window))
            || !chosen
                .iter()
                .flat_map(|row| row.roots.clone())
                .eq(window[0].roots.start..window.last().unwrap().roots.end)
            || !chosen
                .iter()
                .flat_map(|row| row.groups.clone())
                .eq(0..window.len())
        {
            // Rebuild from canonical units, never from failed candidate data.
            // Missing measurements affect optimization, not content coverage.
            plan.measured_rows.validation_fallbacks += 1;
            chosen = window
                .iter()
                .enumerate()
                .map(|(index, unit)| stack_candidate(unit, index, canvas, None))
                .collect();
        }
        apply_rows(plan, projection, &roots, canvas, &chosen);
        plan.measured_rows.retained_previous |= chosen.iter().any(|row| row.previous);
        plan.measured_rows.chosen.extend(chosen);
        plan.measured_rows.candidates.extend(candidates);
        plan.measured_rows.windows += 1;
    }
}

fn table_only_part(roots: &[&BlockNode], part: &Range<usize>) -> bool {
    matches!(
        &roots[part.clone()],
        [BlockNode::Table(_)] | [BlockNode::Heading(_), BlockNode::Table(_)]
    )
}

fn valid_for_window(row: &RowCandidate, window: &[Unit]) -> bool {
    row.legal()
        && window.get(row.groups.clone()).is_some_and(|units| {
            units
                .first()
                .is_some_and(|unit| unit.roots.start == row.roots.start)
                && units
                    .last()
                    .is_some_and(|unit| unit.roots.end == row.roots.end)
                && units.iter().map(|unit| unit.id).eq(row.ids.iter().copied())
                && match row.kind {
                    RowKind::Stack => row.parts.as_slice() == [row.roots.clone()],
                    RowKind::Opening => {
                        units.len() == 2
                            && units[0].kind == GroupKind::Heading
                            && units[0].roots == (0..2)
                            && units[1].kind == GroupKind::Prose
                            && units[0].section == units[1].section
                            && row.parts.as_slice() == [1..2, units[1].roots.clone()]
                    }
                    RowKind::Guidance => {
                        units.len() == 2
                            && units[0].section == units[1].section
                            && row.parts[0] == (units[0].roots.end - 1..units[0].roots.end)
                            && row.parts[1] == units[1].roots
                    }
                    RowKind::Peer | RowKind::Technical | RowKind::Tables => {
                        row.parts.iter().eq(units.iter().map(|unit| &unit.roots))
                    }
                    RowKind::IntroList => {
                        let list = &units.last().unwrap().roots;
                        (2..=3).contains(&units.len())
                            && units.first().is_some_and(|unit| {
                                unit.kind == GroupKind::Heading && unit.boundary
                            })
                            && units
                                .last()
                                .is_some_and(|unit| unit.kind == GroupKind::List)
                            && units[1..units.len() - 1]
                                .iter()
                                .all(|unit| unit.kind == GroupKind::Prose)
                            && row.parts.len() == 2
                            && row.parts[0].start == row.roots.start + 1
                            && row.parts[0].end == row.parts[1].start
                            && matches!(row.parts[1].start, start if start == list.start || start + 1 == list.start)
                            && row.parts[1].end == list.end
                    }
                    RowKind::Explanation => {
                        units.len() == 1
                            && row.roots.len() >= 2
                            && units[0].explanation.is_some_and(|start| row.parts.as_slice()
                                == [
                                    start..units[0].object_start,
                                    units[0].object_start..row.roots.end,
                                ])
                    }
                    RowKind::Aside => {
                        (2..=WINDOW_GROUPS).contains(&units.len())
                            && units[1..].iter().all(|unit| unit.kind == GroupKind::Quote)
                            && row.parts.len() == 2
                            && row.parts[1] == (units[1].roots.start..row.roots.end)
                            && row.parts[0].end == units[0].roots.end
                            && row.parts[0].start == units[0].roots.start
                                + usize::from(units[0].kind == GroupKind::Heading)
                    }
                    RowKind::FigureExplanation | RowKind::ContentExplanation => {
                        units.len() == 2
                            && (units[0].kind == GroupKind::Heading
                                || (row.kind == RowKind::FigureExplanation && units[0].kind == GroupKind::Figure)
                                || (row.kind == RowKind::ContentExplanation && matches!(units[0].kind, GroupKind::Table | GroupKind::Code)))
                            && units[1].kind == GroupKind::Prose
                            && units[0].section == units[1].section
                            && row.parts.as_slice() == [
                                units[0].roots.start + usize::from(units[0].kind == GroupKind::Heading)..units[0].roots.end,
                                units[1].roots.clone(),
                            ]
                    }
                    RowKind::Gallery => row
                        .parts
                        .iter()
                        .cloned()
                        .eq(units.iter().map(|unit| unit.object_start..unit.roots.end)),
                }
        })
}

fn stack_candidate(
    unit: &Unit,
    index: usize,
    canvas: f32,
    measured: Option<GroupMeasurement>,
) -> RowCandidate {
    RowCandidate {
        canvas,
        roots: unit.roots.clone(),
        groups: index..index + 1,
        ids: vec![unit.id],
        parts: vec![unit.roots.clone()],
        kind: RowKind::Stack,
        template: 0,
        widths: vec![canvas],
        heights: vec![measured.unwrap_or_default().height],
        penalties: Penalties::default(),
        rejected: None,
        previous: false,
        flow_rows: if unit.explanation.is_some() { 2 } else { 1 },
        edit_locked: false,
        height_estimated: measured.is_none() || unit.pending_gallery,
        decision_provisional: measured.is_none() || unit.pending_gallery,
    }
}

fn offscreen_rows(
    plan: &AdaptivePlan,
    projection: &TextProjection,
    previous: Option<&AdaptivePlan>,
    window: &[Unit],
    old_rows: &std::collections::HashMap<NodeId, &RowCandidate>,
    canvas: f32,
) -> Vec<RowCandidate> {
    let mut chosen = Vec::new();
    let mut index = 0;
    while index < window.len() {
        let retained = previous
            .filter(|old| plan.compatible_environment(old))
            .and_then(|old| {
                let unit = &window[index];
                let before = *old_rows.get(&plan.root_ids[unit.roots.start])?;
                if !before.legal() {
                    return None;
                }
                let count = before.ids.len();
                let end = index.checked_add(count)?;
                let units = window.get(index..end)?;
                if !units
                    .iter()
                    .map(|unit| unit.id)
                    .eq(before.ids.iter().copied())
                {
                    return None;
                }
                let range = unit.roots.start..units.last()?.roots.end;
                if old.root_ids.get(before.roots.clone())? != plan.root_ids.get(range.clone())?
                    || !range
                        .clone()
                        .all(|i| plan.unchanged_root(old, plan.root_ids[i]))
                    || range.clone().any(|i| {
                        matches!(plan.root_content[i].as_ref(), BlockNode::Table(_))
                            && projection.table_measurements(plan.root_ids[i]).is_none()
                    })
                {
                    return None;
                }
                let mut row = before.clone();
                row.parts = before
                    .parts
                    .iter()
                    .map(|part| {
                        let start = plan.root_ordinal(*old.root_ids.get(part.start)?)?;
                        Some(start..start + part.len())
                    })
                    .collect::<Option<Vec<_>>>()?;
                row.roots = range;
                row.groups = index..end;
                row.previous = true;
                valid_for_window(&row, window).then_some(row)
            });
        if let Some(row) = retained {
            index = row.groups.end;
            chosen.push(row);
        } else {
            chosen.push(stack_candidate(&window[index], index, canvas, None));
            index += 1;
        }
    }
    chosen
}

fn build_units(plan: &AdaptivePlan, roots: &[&BlockNode]) -> Vec<Unit> {
    let specification_pairs = specification_example_pairs(plan, roots);
    let mut units = Vec::new();
    let mut g = 0;
    while g < plan.groups.groups.len() {
        let group = &plan.groups.groups[g];
        if group.kind == GroupKind::Gallery {
            // A gallery is one canonical group with source-order internal
            // figures. These bounded units let row search choose 2/3-column
            // rows without column-major flow, masonry, or duplicate content.
            let first = group.roots.start
                + usize::from(matches!(roots[group.roots.start], BlockNode::Heading(_)));
            // Resource arrivals resolve one canonical gallery together. A
            // measured first image is not yet a measured gallery decision:
            // otherwise each provisional stack acquires a change penalty and
            // prevents the complete gallery from ever adopting its best rows.
            let pending_gallery = (first..group.roots.end)
                .any(|ordinal| plan.pending_images.contains(&roots[ordinal].id()));
            let mut ordinal = first;
            while ordinal < group.roots.end {
                let end = crate::figures::end(roots, ordinal).min(group.roots.end);
                units.push(Unit {
                    object_start: ordinal,
                    id: roots[ordinal].id(),
                    roots: if ordinal == first { group.roots.start } else { ordinal }..end,
                    kind: group.kind,
                    section: group.section,
                    peer: None,
                    specification_pair: None,
                    explanation: None,
                    table_parent: None,
                    gallery: Some(group.id),
                    pending_gallery,
                    boundary: ordinal == first
                        && matches!(roots[group.roots.start], BlockNode::Heading(h) if h.level <= 2),
                });
                ordinal = end;
            }
            g += 1;
            continue;
        }
        let mut unit = Unit {
            object_start: group
                .relationships
                .iter()
                .find(|r| r.kind == RelationshipKind::AdjacentExplanation)
                .and_then(|r| plan.root_ordinal(r.basis[1]))
                .unwrap_or(group.roots.end - 1),
            id: group.id,
            roots: group.roots.clone(),
            kind: group.kind,
            section: group.section,
            peer: None,
            specification_pair: specification_pairs.get(&group.id).copied(),
            gallery: None,
            pending_gallery: false,
            table_parent: match &roots[group.roots.clone()] {
                [BlockNode::Table(_)] => Some(group.section),
                [BlockNode::Heading(heading), BlockNode::Table(_)] if heading.level >= 3 => {
                    Some(plan.groups.sections[&heading.id].parent)
                }
                _ => None,
            },
            explanation: (group.kind == GroupKind::ExplanationContent
                && group
                    .relationships
                    .iter()
                    .any(|r| r.kind == RelationshipKind::AdjacentExplanation))
            .then_some(
                group.roots.start
                    + usize::from(matches!(roots[group.roots.start], BlockNode::Heading(_))),
            ),
            boundary: group.kind == GroupKind::Barrier
                || matches!(roots[group.roots.start], BlockNode::Heading(h) if h.level <= 2),
        };
        // Reference entries belong to one ordered citation flow. Do not turn
        // adjacent bibliographic sections into feature cards or paired prose.
        if roots[group.roots.clone()]
            .iter()
            .any(|root| plan.bibliography.contains_key(&root.id()))
        {
            unit.kind = GroupKind::Opaque;
            unit.boundary = true;
            unit.explanation = None;
            units.push(unit);
            g += 1;
            continue;
        }
        let editorial = plan
            .editorials
            .get(&group.id)
            .filter(|m| m.owner == group.id)
            .and_then(|member| {
                let BlockNode::Heading(heading) = roots[group.roots.start] else {
                    return None;
                };
                let end = (group.roots.start + 1..roots.len())
                    .find(|&i| plan.editorials.get(&roots[i].id()) != Some(member))
                    .unwrap_or(roots.len());
                Some((heading.level, end, member.kind.peer_shape()))
            });
        let compact = compact_section(roots, group.roots.start).filter(|&(level, end, shape)| {
            if shape != super::TECHNICAL_SECTION_SHAPE {
                return true;
            }
            let parent = plan.groups.sections[&group.id].parent;
            // Isolated examples retain their existing paragraph/object flow
            // and chapter measurement window. Merge only when a neighboring
            // reference sibling can actually participate in the same family.
            unit.specification_pair.is_some()
                || units
                    .last()
                    .is_some_and(|unit| unit.peer == Some((parent, level, shape)))
                || (roots
                    .get(end)
                    .is_some_and(|next| !plan.editorials.contains_key(&next.id()))
                    && compact_section(roots, end).is_some_and(|(next_level, _, next_shape)| {
                        next_level == level
                            && next_shape == shape
                            && plan.groups.sections[&roots[end].id()].parent == parent
                    }))
        });
        if let Some((level, end, shape)) = editorial.or(compact) {
            let last = plan.groups.groups[g..]
                .iter()
                .position(|group| group.roots.end >= end)
                .map(|offset| g + offset);
            if let Some(last) = last
                && plan.groups.groups[last].roots.end == end
            {
                if unit.roots.end != end {
                    unit.explanation = None;
                }
                unit.roots.end = end;
                unit.peer = Some((plan.groups.sections[&group.id].parent, level, shape));
                // Bounded, explicitly named objects may be H2 as well as H3.
                // Their semantic parent/level still prevents crossing a real
                // chapter or nesting boundary; H2 alone need not force a stack.
                if editorial.is_some() || shape == super::TECHNICAL_SECTION_SHAPE {
                    unit.boundary = false;
                }
                g = last;
            }
        }
        units.push(unit);
        g += 1;
    }
    units
}

/// A schema and an authored payload/example may share a technical band, but
/// adjacency alone does not prove that relationship. Preserve explicit
/// exchanges first, require a shared section parent/level and at least two
/// complete literal field identifiers. No title/filename-specific layout rule.
fn specification_example_pairs(
    plan: &AdaptivePlan,
    roots: &[&BlockNode],
) -> std::collections::HashMap<NodeId, NodeId> {
    use super::editorial::Kind;
    use std::collections::{HashMap, HashSet};
    let mut pairs = HashMap::new();
    for start in 0..roots.len() {
        let Some((level, middle, super::TECHNICAL_SECTION_SHAPE)) = compact_section(roots, start)
        else {
            continue;
        };
        let Some((next_level, end, super::TECHNICAL_SECTION_SHAPE)) =
            compact_section(roots, middle)
        else {
            continue;
        };
        let left = roots[start].id();
        let right = roots[middle].id();
        if level != next_level
            || pairs.contains_key(&left)
            || pairs.contains_key(&right)
            || plan.groups.sections[&left].parent != plan.groups.sections[&right].parent
        {
            continue;
        }
        let (specification, example, example_start, example_end) =
            match (plan.editorials.get(&left), plan.editorials.get(&right)) {
                (None, Some(member)) => (start..middle, member, middle, end),
                (Some(member), None) => (middle..end, member, start, middle),
                _ => continue,
            };
        if !matches!(example.kind, Kind::Example | Kind::Request | Kind::Response) {
            continue;
        }
        let previous_heading = (0..example_start)
            .rev()
            .find(|&i| matches!(roots[i], BlockNode::Heading(_)));
        let has_counterpart = [
            previous_heading,
            (example_end < roots.len()).then_some(example_end),
        ]
        .into_iter()
        .flatten()
        .any(|i| {
            let Some(member) = plan.editorials.get(&roots[i].id()) else {
                return false;
            };
            matches!(roots[i], BlockNode::Heading(h) if h.level == level)
                && plan.groups.sections[&member.owner].parent
                    == plan.groups.sections[&example.owner].parent
                && if i < example_start {
                    member.kind.accepts_counterpart(example.kind)
                } else {
                    example.kind.accepts_counterpart(member.kind)
                }
        });
        if has_counterpart {
            continue;
        }
        let mut table = None;
        let valid_specification = roots[specification.start + 1..specification.end]
            .iter()
            .all(|root| match root {
                BlockNode::Table(value) if table.is_none() => {
                    table = Some(value);
                    true
                }
                BlockNode::Paragraph(_) => true,
                _ => false,
            });
        let mut code = None;
        let valid_example = roots[example_start + 1..example_end]
            .iter()
            .all(|root| match root {
                BlockNode::CodeBlock(value) if code.is_none() && !crate::math::is_math(root) => {
                    code = Some(value);
                    true
                }
                BlockNode::Paragraph(_) => true,
                _ => false,
            });
        let (Some(table), Some(code)) = (
            table.filter(|_| valid_specification),
            code.filter(|_| valid_example),
        ) else {
            continue;
        };
        let code_text = code.content.as_cow();
        let identifiers = code_text
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .filter(|word| !word.is_empty())
            .collect::<HashSet<_>>();
        let fields = table
            .rows
            .iter()
            .skip(1)
            .filter_map(|row| {
                let cell = row.cells.first()?;
                let BlockNode::Paragraph(value) = cell.blocks.get(0)?.as_ref() else {
                    return None;
                };
                let field = value.content.as_string();
                let identifier = field
                    .as_bytes()
                    .first()
                    .is_some_and(|c| c.is_ascii_alphabetic() || *c == b'_')
                    && field
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_');
                (identifier && identifiers.contains(field.as_str())).then_some(field)
            })
            .collect::<HashSet<_>>();
        if fields.len() >= 2 {
            pairs.insert(left, left);
            pairs.insert(right, left);
        }
    }
    pairs
}

/// Only the real document opening can nominate this relationship. Do not
/// borrow a section's first paragraph to manufacture a lead or an overview.
fn opening_parts(
    window: &[Unit],
    index: usize,
    roots: &[&BlockNode],
    plan: &AdaptivePlan,
    projection: &TextProjection,
) -> Option<Vec<Range<usize>>> {
    let first = window.get(index)?;
    let overview = window.get(index + 1)?;
    if first.kind != GroupKind::Heading
        || first.roots != (0..2)
        || !matches!(roots[0], BlockNode::Heading(h) if h.level == 1)
        || plan.lead != Some(roots[1].id())
        || overview.kind != GroupKind::Prose
        || overview.roots.start != 2
        || !(1..=4).contains(&overview.roots.len())
        || overview.section != first.section
        || roots
            .get(overview.roots.end)
            .is_some_and(|root| !matches!(root, BlockNode::Heading(_)))
    {
        return None;
    }
    for root in &roots[1..overview.roots.end] {
        let BlockNode::Paragraph(paragraph) = root else {
            return None;
        };
        let segment = projection.segment_for_node(root.id())?;
        let context = &segment.context;
        if context.metadata
            || context.figure_text.is_some()
            || context.bibliography.is_some()
            || context.resource_title_end.is_some()
            || context.margin_note_anchor.is_some()
            || context.metric.is_some()
            || context.color_role.is_some()
            || context.badge.is_some()
            || plan.editorials.contains_key(&root.id())
            || plan.resources.contains_key(&root.id())
            || projection.text()[segment.projection_range()].contains(['\n', '\r'])
            || projection.text()[segment.projection_range()]
                .chars()
                .any(|character| {
                    matches!(
                        unicode_bidi::bidi_class(character),
                        unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL
                    )
                })
            || paragraph.content.runs().iter().any(|run| {
                run.styles.iter().any(|style| {
                    matches!(
                        style,
                        document_core::InlineStyle::Image { .. }
                            | document_core::InlineStyle::Math { .. }
                            | document_core::InlineStyle::PreservedHtml(_)
                    )
                })
            })
        {
            return None;
        }
    }
    Some(vec![1..2, overview.roots.clone()])
}

fn opening_measures(
    plan: &AdaptivePlan,
    projection: &TextProjection,
    roots: &[&BlockNode],
    parts: &[Range<usize>],
) -> [f32; 2] {
    use crate::theme::DocumentStyle;
    std::array::from_fn(|column| {
        let narrative = projection
            .segment_for_node(roots[parts[column].start].id())
            .is_some_and(|s| s.context.narrative);
        if column == 0 && narrative {
            plan.prose_measures.narrative * DocumentStyle::LEAD_SIZE / DocumentStyle::READING_SIZE
        } else {
            plan.prose_measures.for_role(narrative, column == 0)
        }
    })
}

/// A bounded section introduction may sit beside a compact independent list.
/// The authored heading remains full-width; only its following prose and list
/// become the two source-ordered columns. A short label immediately before the
/// list moves with that list, producing a genuine labelled panel instead of
/// leaving its caption stranded at the bottom of the prose column. Ordered
/// lists are excluded because their sequence usually benefits from the full
/// reading measure.
fn intro_list_parts(
    window: &[Unit],
    start: usize,
    roots: &[&BlockNode],
) -> Option<(usize, Vec<Range<usize>>)> {
    let first = window.get(start)?;
    if first.kind != GroupKind::Heading || !first.boundary || first.roots.len() < 2 {
        return None;
    }
    if !matches!(roots[first.roots.start], BlockNode::Heading(heading) if heading.level == 2)
        || !roots[first.roots.start + 1..first.roots.end]
            .iter()
            .all(|block| matches!(block, BlockNode::Paragraph(_)))
    {
        return None;
    }
    let mut list_index = start + 1;
    while list_index < window.len()
        && list_index - start < 3
        && window[list_index].kind == GroupKind::Prose
        && window[list_index].section == first.section
    {
        list_index += 1;
    }
    let list_unit = window.get(list_index)?;
    let consume = list_index - start + 1;
    if !(2..=3).contains(&consume)
        || list_unit.kind != GroupKind::List
        || list_unit.section != first.section
        || list_unit.roots.len() != 1
    {
        return None;
    }
    let BlockNode::List(list) = roots[list_unit.roots.start] else {
        return None;
    };
    if !matches!(
        list.kind,
        document_core::ListKind::Unordered | document_core::ListKind::Task
    ) || super::list_has_authored_labels(list)
        || super::resource::is_resource_list(list)
        || !(3..=9).contains(&list.items.len())
        || !list.items.iter().all(|item| {
            item.blocks.len() == 1
                && matches!(
                    item.blocks.iter().next().map(AsRef::as_ref),
                    Some(BlockNode::Paragraph(paragraph))
                        if paragraph.content.len() <= 240
                            && !paragraph.content.as_string().contains('\n')
                )
        })
    {
        return None;
    }
    let prose_start = first.roots.start + 1;
    let panel_start = list_unit.roots.start;
    let Some(BlockNode::Paragraph(label)) = panel_start
        .checked_sub(1)
        .and_then(|index| roots.get(index).copied())
    else {
        return None;
    };
    let text = label.content.as_string();
    if text.len() > 72 || !text.trim_end().ends_with(':') {
        return None;
    }
    let panel_start = panel_start - 1;
    let prose = prose_start..panel_start;
    let panel = panel_start..list_unit.roots.end;
    (!prose.is_empty()).then_some((consume, vec![prose, panel]))
}

/// A figure's complete caption/credit unit precedes its explanation. Native
/// wrap measurement decides whether a supporting illustration needs this pair;
/// evidence always remains one block.
fn following_figure_parts(
    window: &[Unit],
    index: usize,
    roots: &[&BlockNode],
) -> Option<Vec<Range<usize>>> {
    let figure = window.get(index)?;
    let prose = window.get(index + 1)?;
    if figure.peer.is_some()
        || !matches!(figure.kind, GroupKind::Heading | GroupKind::Figure)
        || prose.kind != GroupKind::Prose
        || figure.section != prose.section
        || figure.roots.end != prose.roots.start
        || !(1..=4).contains(&prose.roots.len())
    {
        return None;
    }
    let start = figure.roots.start + usize::from(figure.kind == GroupKind::Heading);
    let BlockNode::Image(_) = roots[start] else {
        return None;
    };
    if crate::figures::end(roots, start) != figure.roots.end
        || roots[prose.roots.clone()].iter().any(|root| {
            !matches!(root, BlockNode::Paragraph(p) if p.content.len() <= 2400)
                || crate::figures::classify(root).is_some()
        })
        || matches!(roots.get(prose.roots.end), Some(BlockNode::BlockQuote { blocks, .. })
            if crate::quotes::margin_note_anchor(Some(roots[prose.roots.end - 1]), blocks).is_some())
    {
        return None;
    }
    Some(vec![start..figure.roots.end, prose.roots.clone()])
}

/// A complete reference component may lead its explanation. Never borrow a
/// caption, another example's introduction, or an explicitly anchored note's
/// prose just to occupy the unused half of a page.
fn following_content_parts(
    window: &[Unit],
    index: usize,
    roots: &[&BlockNode],
) -> Option<Vec<Range<usize>>> {
    let content = window.get(index)?;
    let prose = window.get(index + 1)?;
    if content.peer.is_some()
        || !matches!(
            content.kind,
            GroupKind::Heading | GroupKind::Table | GroupKind::Code
        )
        || prose.kind != GroupKind::Prose
        || content.section != prose.section
        || content.roots.end != prose.roots.start
        || !(1..=4).contains(&prose.roots.len())
        || matches!(roots[content.roots.start], BlockNode::Heading(h) if h.level == 1)
    {
        return None;
    }
    let start = content.roots.start + usize::from(content.kind == GroupKind::Heading);
    if content.roots.end != start + 1
        || !matches!(roots[start], BlockNode::Table(_) | BlockNode::CodeBlock(_))
        || roots[prose.roots.clone()].iter().any(|root| {
            !matches!(root, BlockNode::Paragraph(p) if p.content.len() <= 2400)
                || crate::figures::classify(root).is_some()
        })
        || matches!(roots.get(prose.roots.end), Some(BlockNode::Paragraph(_)))
        || matches!(roots.get(prose.roots.end), Some(BlockNode::BlockQuote { blocks, .. })
            if crate::quotes::margin_note_anchor(Some(roots[prose.roots.end - 1]), blocks).is_some())
    {
        return None;
    }
    Some(vec![start..content.roots.end, prose.roots.clone()])
}

/// Complete same-section optional callouts may sit beside one another, below
/// their shared heading. Critical conditions and their actions never enter.
fn adjacent_guidance_parts(
    window: &[Unit],
    index: usize,
    roots: &[&BlockNode],
) -> Option<Vec<Range<usize>>> {
    let first = window.get(index)?;
    let second = window.get(index + 1)?;
    let start =
        first.roots.start + usize::from(matches!(roots[first.roots.start], BlockNode::Heading(_)));
    if first.section != second.section
        || first.roots.end != second.roots.start
        || first.roots.end != start + 1
        || second.roots.len() != 1
        || ![start, second.roots.start].into_iter().all(|ordinal| {
            matches!(roots[ordinal], BlockNode::Alert { kind: document_core::AlertKind::Note | document_core::AlertKind::Tip, blocks, .. }
                if (1..=2).contains(&blocks.len()) && blocks.iter().all(|block|
                    matches!(block.as_ref(), BlockNode::Paragraph(p) if p.content.len() <= 480)))
        })
    {
        return None;
    }
    Some(vec![start..first.roots.end, second.roots.clone()])
}

/// The source itself places an optional note after this prose in the same
/// section. Preserve that adjacency and the full-width heading; this is a
/// paired module, not an invented footnote anchor or floating text corridor.
fn adjacent_aside_parts(
    window: &[Unit],
    index: usize,
    roots: &[&BlockNode],
) -> Option<(usize, Vec<Range<usize>>)> {
    let main = window.get(index)?;
    let aside = window.get(index + 1)?;
    if main.peer.is_some()
        || !matches!(main.kind, GroupKind::Heading | GroupKind::Prose)
        || aside.kind != GroupKind::Quote
        || aside.roots.len() != 1
        || main.section != aside.section
    {
        return None;
    }
    let start = main.roots.start + usize::from(main.kind == GroupKind::Heading);
    if main.kind == GroupKind::Heading
        && !matches!(roots[main.roots.start], BlockNode::Heading(h) if h.level >= 2)
    {
        return None;
    }
    let prose = &roots[start..main.roots.end];
    if !(1..=4).contains(&prose.len())
        || !prose
            .iter()
            .all(|root| matches!(root, BlockNode::Paragraph(p) if p.content.len() <= 2400))
    {
        return None;
    }
    // Keep a bounded adjacent note cluster together. Never move only its first
    // note beside the prose and strand another below, or relocate a warning.
    // Explicit margin notes can share their anchor throughout this bounded
    // search window. The existing two-note limit for general Note/Tip asides
    // is unchanged; their source does not declare one common margin anchor.
    let mut count = 0;
    let mut margin_only = true;
    let mut end = main.roots.end;
    for note in window[index + 1..]
        .iter()
        .take_while(|unit| unit.kind == GroupKind::Quote && unit.section == main.section)
    {
        if note.roots.len() != 1 || note.roots.start != end {
            return None;
        }
        let blocks = match roots[note.roots.start] {
            BlockNode::Alert {
                kind: document_core::AlertKind::Note | document_core::AlertKind::Tip,
                blocks,
                ..
            } => {
                if count >= 2 {
                    return None;
                }
                margin_only = false;
                blocks
            }
            BlockNode::BlockQuote { blocks, .. }
                if (count < 2 || margin_only)
                    && prose.len() == 1
                    && crate::quotes::margin_note_anchor(
                        Some(roots[main.roots.end - 1]),
                        blocks,
                    )
                    .is_some() =>
            {
                blocks
            }
            _ => return None,
        };
        if !(1..=2).contains(&blocks.len())
            || !blocks.iter().all(
                |block| matches!(block.as_ref(), BlockNode::Paragraph(p) if p.content.len() <= 480),
            )
        {
            return None;
        }
        count += 1;
        end = note.roots.end;
    }
    // A bounded search window may end inside the cluster. Its canonical next
    // root is still authoritative; defer the entire pair instead of truncating.
    if roots
        .get(end)
        .is_some_and(|root| matches!(root, BlockNode::Alert { .. } | BlockNode::BlockQuote { .. }))
    {
        return None;
    }
    Some((
        count + 1,
        vec![start..main.roots.end, aside.roots.start..end],
    ))
}

fn unit_windows(units: &[Unit]) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    let mut first = 0;
    while first < units.len() {
        let limit = (first + WINDOW_GROUPS).min(units.len());
        let end = (first + 1..limit)
            .find(|&i| units[i].boundary)
            .unwrap_or(limit);
        ranges.push(first..end);
        first = end;
    }
    ranges
}

pub(crate) fn planning_windows(
    plan: &AdaptivePlan,
    projection: &TextProjection,
) -> Vec<Range<usize>> {
    let roots = projection.roots().collect::<Vec<_>>();
    let units = build_units(plan, &roots);
    unit_windows(&units)
        .iter()
        .map(|range| units[range.start].roots.start..units[range.end - 1].roots.end)
        .collect()
}

fn apply_rows(
    plan: &mut AdaptivePlan,
    projection: &TextProjection,
    roots: &[&BlockNode],
    canvas: f32,
    chosen: &[RowCandidate],
) {
    for row in chosen {
        if row.kind == RowKind::Stack && !row.edit_locked {
            continue;
        }
        let spans = TEMPLATES[row.template];
        for (item, part) in row.parts.iter().enumerate() {
            let slot = LayoutSlot {
                align_components: row.kind == RowKind::Technical,
                group: row.ids[0],
                item,
                row: 0,
                columns: spans.len(),
                cards: row.kind == RowKind::Peer || (row.kind == RowKind::IntroList && item == 1),
                card_accent: if row.kind == RowKind::Peer {
                    super::CardAccent::Open
                } else if row.kind == RowKind::IntroList && item == 1 {
                    super::CardAccent::Leading
                } else {
                    super::CardAccent::None
                },
                track_start: spans[..item].iter().sum(),
                span: spans[item],
                fixed_canvas: (row.canvas != canvas).then_some(row.canvas),
            };
            for root in &roots[part.clone()] {
                // Internal list grids have their own focus lock and
                // widths; a containing stack must not overwrite them.
                if row.kind == RowKind::Stack && matches!(root, BlockNode::List(_)) {
                    continue;
                }
                for node in &plan.groups.group_for_root(root.id()).unwrap().nodes {
                    if let Some(segment) = projection
                        .segment_for_node(*node)
                        .filter(|s| s.top_level_node_id == root.id())
                    {
                        let mut node_slot = slot;
                        if row.kind == RowKind::Stack {
                            let wide = matches!(
                                root,
                                BlockNode::Heading(_)
                                    | BlockNode::CodeBlock(_)
                                    | BlockNode::Table(_)
                                    | BlockNode::Image(_)
                            ) || segment
                                .context
                                .figure_text
                                .is_some_and(|(_, role)| role.gallery_start().is_some());
                            node_slot.fixed_canvas = Some(if wide {
                                row.canvas
                            } else {
                                plan.prose_measures.fit_width(
                                    row.canvas,
                                    segment.context.narrative
                                        || segment.context.quote.is_some()
                                        || segment.context.bibliography.is_some(),
                                    plan.lead == Some(*node),
                                )
                            });
                        }
                        plan.slots.insert(*node, node_slot);
                    }
                }
            }
        }
    }
}

/// Include alignment padding before fit rejection and scoring. It belongs to
/// the section prefix, not to code padding or table-cell height.
fn align_component_measurements(measurements: &mut [GroupMeasurement]) {
    if measurements.iter().any(|m| m.component_top.is_none()) {
        return;
    }
    let anchor = measurements
        .iter()
        .filter_map(|m| m.component_top)
        .fold(0., f32::max);
    for measurement in measurements {
        measurement.height += anchor - measurement.component_top.unwrap();
        measurement.component_top = Some(anchor);
    }
}

/// Seven normalized terms, with intentional prose measure excluded from waste.
/// Row transition complexity is accounted for in the DP state, not guessed here.
pub(crate) fn score(
    kind: RowKind,
    measurements: &[GroupMeasurement],
    widths: &[f32],
    viewport: f32,
    has_readable_peer: bool,
    changed: bool,
) -> Penalties {
    let columns = widths.len();
    let height = measurements.iter().map(|m| m.height).fold(0., f32::max);
    let row_area = widths.iter().sum::<f32>() * height;
    let used = measurements
        .iter()
        .zip(widths)
        .map(|(m, w)| m.height * w)
        .sum::<f32>();
    Penalties {
        // Compact text groups avoid excessive wrapping. A figure scales as
        // one complete image: bitmap pixel density is not a prose measure.
        // Figures have a logical display-width preference instead. Column
        // width and rendered height still pass the same hard fit checks.
        discomfort: if columns > 1 {
            measurements
                .iter()
                .zip(widths)
                .map(|(m, w)| {
                    if kind == RowKind::Gallery {
                        (m.preferred_width.min(GALLERY_COMFORT_WIDTH) / w.max(1.) - 1.)
                            .clamp(0., 1.)
                    } else {
                        ((m.preferred_width / w.max(1.) - 1.) / 3.).clamp(0., 1.)
                    }
                })
                .sum::<f32>()
                / columns as f32
        } else {
            0.
        },
        separation: if kind == RowKind::Stack && has_readable_peer {
            // Eligibility already established an authored relationship and
            // rejected cramped or unbalanced alternatives. Keeping that unit
            // visibly together should outweigh a modest template transition.
            0.5
        } else {
            0.
        },
        reading_jump: if columns > 1 {
            (measurements
                .iter()
                .zip(widths)
                .take(columns - 1)
                // Approximate the return path from the trailing content edge
                // to the next group's leading edge. A compact group doesn't
                // fill its assigned span; that empty horizontal distance is
                // part of the reading jump, not productive occupied width.
                .map(|(m, width)| {
                    let horizontal = (width - m.preferred_width).max(0.) + super::LAYOUT_GAP;
                    if kind == RowKind::Gallery {
                        // A figure is a complete visual, not a text column
                        // whose last line forces a return to the next top.
                        // Charging its full height here rewards shrinking the
                        // first of two identical figures for no source reason.
                        // The hard height gate and imbalance term still apply.
                        horizontal
                    } else {
                        m.height.hypot(horizontal)
                    }
                })
                .sum::<f32>()
                / ((columns - 1) as f32 * viewport.max(1.)))
            .clamp(0., 1.)
        } else {
            0.
        },
        imbalance: if row_area > 0. {
            (1. - used / row_area).clamp(0., 1.)
        } else {
            0.
        },
        // Three repeated sibling cards remain one familiar visual unit. Do
        // not charge them like three unrelated prose columns, or the search
        // strands the first sibling above an otherwise valid two-card row.
        complexity: if kind == RowKind::Peer && columns == 3 {
            0.45
        } else {
            (columns.saturating_sub(1) as f32 / 3.).clamp(0., 1.)
        },
        unused_width: if has_readable_peer {
            measurements
                .iter()
                .zip(widths)
                .map(|(m, w)| (1. - m.preferred_width / w.max(1.)).clamp(0., 1.))
                .sum::<f32>()
                / columns.max(1) as f32
        } else {
            0.
        },
        change: if changed { 1. } else { 0. },
    }
}

fn transition_cost(previous: usize, next: usize) -> f32 {
    // One third of normalized complexity reserved for a template transition;
    // the other two thirds are at most two additional simultaneous columns.
    if previous < TEMPLATES.len() && previous != next {
        2. / 3.
    } else {
        0.
    }
}

/// n <= 40; rows consume contiguous groups within this bounded window. Previous template is
/// explicit state, so transition cost participates in the optimum correctly.
pub(crate) fn choose_window(n: usize, candidates: &[RowCandidate]) -> Vec<RowCandidate> {
    if n > WINDOW_GROUPS {
        return Vec::new();
    }
    let states = TEMPLATES.len() + 1;
    let mut cost = vec![vec![f32::INFINITY; states]; n + 1];
    let mut chosen = vec![vec![None; states]; n];
    cost[n].fill(0.);
    for i in (0..n).rev() {
        for prior in 0..states {
            for (index, row) in candidates
                .iter()
                .enumerate()
                .filter(|(_, row)| row.groups.start == i && row.groups.end <= n && row.legal())
            {
                let next = row.cost()
                    + transition_cost(prior, row.template)
                    + cost[row.groups.end][row.template];
                let better_tie = chosen[i][prior].is_none_or(|old: usize| {
                    let old = &candidates[old];
                    (!row.previous, row.widths.len(), row.template, &row.ids)
                        < (!old.previous, old.widths.len(), old.template, &old.ids)
                });
                if next < cost[i][prior] || (next == cost[i][prior] && better_tie) {
                    cost[i][prior] = next;
                    chosen[i][prior] = Some(index);
                }
            }
        }
    }
    let mut rows = Vec::new();
    let mut i = 0;
    let mut prior = states - 1;
    while i < n {
        let Some(index) = chosen[i][prior] else {
            return Vec::new();
        };
        let row = candidates[index].clone();
        i = row.groups.end;
        prior = row.template;
        rows.push(row);
    }
    // Retain only a complete, currently legal previous partition. A newly
    // invalid width/height escapes immediately; preferences cannot force fit.
    let mut previous = Vec::new();
    let mut end = 0;
    while end < n {
        let Some(row) = candidates.iter().find(|row| {
            row.previous && row.legal() && row.groups.start == end && row.groups.end <= n
        }) else {
            break;
        };
        end = row.groups.end;
        previous.push(row.clone());
    }
    let total = |rows: &[RowCandidate]| {
        rows.iter()
            .enumerate()
            .map(|(i, row)| {
                row.cost()
                    + i.checked_sub(1).map_or(0., |prior| {
                        transition_cost(rows[prior].template, row.template)
                    })
            })
            .sum::<f32>()
    };
    if end == n && total(&previous) - total(&rows) < total(&previous).abs() * 0.1 + 0.001 {
        previous
    } else {
        rows
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn specification_example_relationship_requires_shared_fields_and_preserves_exchanges() {
        let specification = "## Fields\n\n| Name | Type |\n| --- | --- |\n| `title` | string |\n| `content` | string |\n\n";
        let request = "## Request\n\n```json\n{\"title\": \"Notes\", \"content\": \"Complete text\"}\n```\n\n";
        let response = request.replace("## Request", "## Response");
        for (source, expected) in [
            (format!("# API\n\n{specification}{request}"), 2),
            (format!("# API\n\n{request}{specification}"), 2),
            (
                format!(
                    "{specification}{}",
                    request.replace("Request", "Example: Payload")
                ),
                2,
            ),
            (
                format!(
                    "{specification}{}",
                    request.replace("Request", "Unlabelled payload")
                ),
                0,
            ),
            (
                format!("{specification}{}", request.replace("content", "unrelated")),
                0,
            ),
            (
                format!(
                    "{specification}{}",
                    request
                        .replace("title", "title_suffix")
                        .replace("content", "contention")
                ),
                0,
            ),
            (
                format!("{}{request}", specification.replace("content", "title")),
                0,
            ),
            (format!("{specification}{request}{response}"), 0),
            (format!("{request}{response}{specification}"), 0),
            (
                format!(
                    "{specification}{}",
                    request.replace("## Request", "### Request")
                ),
                0,
            ),
            (format!("{specification}# Another chapter\n\n{request}"), 0),
            (
                format!(
                    "{specification}A warning boundary.\n\n> [!WARNING]\n> Stop here.\n\n{request}"
                ),
                0,
            ),
        ] {
            let document = document_core::Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let roots = projection.roots().collect::<Vec<_>>();
            let plan = AdaptivePlan::build(&projection, 1314., None, false);
            assert_eq!(
                specification_example_pairs(&plan, &roots).len(),
                expected,
                "{source}"
            );
            assert!(plan.groups.validate(&roots));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn heading_focus_exception_depends_on_prefix_geometry_not_row_family() {
        for kind in [
            RowKind::Opening,
            RowKind::Explanation,
            RowKind::ContentExplanation,
            RowKind::FigureExplanation,
            RowKind::Aside,
            RowKind::Gallery,
        ] {
            let mut row = candidate(3, 2, 1, 0.);
            row.kind = kind;
            row.roots = 3..6;
            row.parts = vec![4..5, 5..6];
            assert!(keeps_heading_outside_columns(&row, 3));
            assert!(
                !keeps_heading_outside_columns(&row, 4),
                "a heading inside a track remains owned by that row"
            );
            row.parts[0].start = 3;
            assert!(!keeps_heading_outside_columns(&row, 3));
        }
        let peer = candidate(3, 2, 1, 0.);
        assert!(!keeps_heading_outside_columns(&peer, 3));
        let stack = candidate(3, 1, 0, 0.);
        assert!(keeps_heading_outside_columns(&stack, 3));
        assert!(!keeps_heading_outside_columns(&stack, 2));
    }

    #[test]
    fn opening_nomination_requires_the_actual_complete_introductory_context() {
        let base = include_str!("../../../../performance/layout-fixtures/118-opening-overview.md");
        let opening = base.split("## Build and run").next().unwrap();
        for (source, expected) in [
            (base.to_owned(), true),
            (opening.to_owned(), true),
            (base.replacen("# Mineral", "## Mineral", 1), false),
            (format!("A preamble.\n\n{base}"), false),
            (
                base.replacen("The current", "A hard break.  \nThe current", 1),
                false,
            ),
            (base.replacen("The current", "שלום The current", 1), false),
            (
                format!("{opening}A third paragraph.\n\nA fourth.\n\nA fifth.\n\nA sixth.\n"),
                false,
            ),
            (format!("{opening}- A following instruction.\n"), false),
            (
                base.replacen("The current", "$$x^2$$\n\nThe current", 1),
                false,
            ),
        ] {
            let document = document_core::Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = AdaptivePlan::build(&projection, 1314., None, false);
            let roots = projection.roots().collect::<Vec<_>>();
            let units = build_units(&plan, &roots);
            let parts =
                (0..units.len()).find_map(|i| opening_parts(&units, i, &roots, &plan, &projection));
            assert_eq!(parts.is_some(), expected, "{source}");
            if let Some(parts) = parts {
                assert_eq!(parts[0], 1..2);
                assert_eq!(parts[1], 2..3);
            }
            assert!(plan.groups.validate(&roots));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn content_led_pairs_keep_complete_prose_and_semantic_boundaries() {
        let table = "| Key | Value |\n| --- | --- |\n| Mode | Local |";
        let code = "```rust\nrun();\n```";
        for (prefix, tail, expected) in [
            (
                "## Reference\n\n",
                "Following explanation.\n\nMore explanation.",
                true,
            ),
            ("", "Following explanation.", true),
            ("# Opening\n\n", "Following explanation.", false),
            ("", "## Boundary\n\nFollowing explanation.", false),
            ("", "Caption: Source description", false),
            ("", "Explanation.\n\n> Margin note: Keep my anchor.", false),
            (
                "",
                "Explanation of the next example:\n\n```sh\npwd\n```",
                false,
            ),
            ("", "One.\n\nTwo.\n\nThree.\n\nFour.\n\nFive.", false),
            (
                "",
                "One.\n\nTwo.\n\nThree.\n\nFour.\n\nFive.\n\nSix.\n\nSeven.\n\nEight.\n\nNine.",
                false,
            ),
        ] {
            for component in [table, code] {
                let source = format!("{prefix}{component}\n\n{tail}\n");
                let document = document_core::Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let plan = AdaptivePlan::build(&projection, 1314., None, false);
                let roots = projection.roots().collect::<Vec<_>>();
                let units = build_units(&plan, &roots);
                let pairs = (0..units.len())
                    .filter_map(|i| following_content_parts(&units, i, &roots))
                    .collect::<Vec<_>>();
                assert_eq!(!pairs.is_empty(), expected, "{source}");
                for parts in pairs {
                    assert_eq!(parts[0].len(), 1);
                    assert_eq!(parts[0].end, parts[1].start);
                    assert_eq!(parts[1].end, roots.len());
                }
                assert!(plan.groups.validate(&roots));
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        }
    }

    #[test]
    fn figure_led_pairs_respect_complete_roles_and_source_boundaries() {
        for (source, expected) in [
            (
                "## Evidence\n\n![Diagram](a.svg)\n\nCaption: Complete frame\n\nCredit: Source\n\nA following explanation.\n\nMore explanation.\n",
                true,
            ),
            ("![Diagram](a.svg)\n\nFollowing explanation.\n", true),
            (
                "![Botanical illustration](a.svg)\n\nFollowing explanation.\n",
                true,
            ),
            (
                "![Diagram](a.svg)\n\n## Boundary\n\nFollowing explanation.\n",
                false,
            ),
            (
                "![Diagram](a.svg)\n\n![Second diagram](b.svg)\n\nFollowing explanation.\n",
                false,
            ),
            (
                "![Diagram](a.svg)\n\nCaption: Complete frame\n\nCaption: Ambiguous second caption\n",
                false,
            ),
            (
                "![Diagram](a.svg)\n\nFollowing explanation.\n\n> Margin note: Attached to this explanation.\n",
                false,
            ),
            (
                "![Diagram](a.svg)\n\nExplanation of this code:\n\n```rust\nrun();\n```\n",
                false,
            ),
            (
                "![Diagram](a.svg)\n\nOne.\n\nTwo.\n\nThree.\n\nFour.\n\nFive.\n",
                false,
            ),
            ("> ![Diagram](a.svg)\n>\n> Following explanation.\n", false),
        ] {
            let document = document_core::Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = AdaptivePlan::build(&projection, 1314., None, false);
            let roots = projection.roots().collect::<Vec<_>>();
            let units = build_units(&plan, &roots);
            let pairs = (0..units.len())
                .filter_map(|i| following_figure_parts(&units, i, &roots))
                .collect::<Vec<_>>();
            assert_eq!(!pairs.is_empty(), expected, "{source}");
            for parts in pairs {
                assert_eq!(parts[0].end, parts[1].start);
                assert_eq!(crate::figures::end(&roots, parts[0].start), parts[0].end);
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn margin_note_clusters_are_complete_within_the_bounded_search_window() {
        for count in [3, WINDOW_GROUPS - 1, WINDOW_GROUPS, WINDOW_GROUPS + 1] {
            let source = format!("Anchor.\n\n{}", "> Margin note: Context.\n\n".repeat(count));
            let document = document_core::Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = AdaptivePlan::build(&projection, 1314., None, false);
            let roots = projection.roots().collect::<Vec<_>>();
            let units = build_units(&plan, &roots);
            let complete = adjacent_aside_parts(&units, 0, &roots).unwrap();
            assert_eq!(complete.0, count + 1);
            assert_eq!(complete.1[1].len(), count);
            let window = &units[..units.len().min(WINDOW_GROUPS)];
            assert_eq!(
                adjacent_aside_parts(window, 0, &roots).is_some(),
                count < WINDOW_GROUPS
            );
            assert!(adjacent_aside_parts(&units[..count], 0, &roots).is_none());
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn supporting_notes_are_not_split_at_the_search_window_boundary() {
        let document = document_core::Document::from_markdown(include_str!(
            "../../../../performance/layout-fixtures/81-supporting-note-pair.md"
        ))
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let plan = AdaptivePlan::build(&projection, 1160., None, false);
        let roots = projection.roots().collect::<Vec<_>>();
        let units = build_units(&plan, &roots);
        let index = (0..units.len())
            .find(|index| adjacent_aside_parts(&units, *index, &roots).is_some())
            .expect("fixture contains a complete supporting pair");
        assert_eq!(adjacent_aside_parts(&units, index, &roots).unwrap().0, 3);
        assert!(adjacent_aside_parts(&units[..index + 2], index, &roots).is_none());
        assert!(adjacent_aside_parts(&units[..index + 1], index, &roots).is_none());
    }

    #[test]
    fn gallery_scaling_is_not_prose_wrapping_discomfort() {
        let images = [GroupMeasurement {
            height: 177.,
            preferred_width: 976.,
            overflow: false,
            component_top: None,
        }; 2];
        let widths = [488., 488.];
        assert_eq!(
            score(RowKind::Gallery, &images, &widths, 928., true, false).discomfort,
            0.
        );
        assert!(score(RowKind::Peer, &images, &widths, 928., true, false).discomfort > 0.);
        let compact = score(RowKind::Gallery, &images, &[280., 280.], 928., true, false);
        assert!(
            compact.discomfort > 0.,
            "small logical figures retain a readability cost"
        );
        let high_dpi = [GroupMeasurement {
            preferred_width: 1936.,
            ..images[0]
        }; 2];
        assert_eq!(
            compact.discomfort,
            score(
                RowKind::Gallery,
                &high_dpi,
                &[280., 280.],
                928.,
                true,
                false
            )
            .discomfort,
            "more source pixels do not make an equally sized display less readable"
        );
    }

    #[test]
    fn table_pair_does_not_reward_a_large_gap_after_the_compact_group() {
        let measurements = [
            GroupMeasurement {
                height: 211.,
                preferred_width: 220.,
                overflow: false,
                component_top: None,
            },
            GroupMeasurement {
                height: 211.,
                preferred_width: 518.,
                overflow: false,
                component_top: None,
            },
        ];
        let cost = |spans: &[u8]| {
            let widths = spans
                .iter()
                .map(|span| super::super::candidates::span_width(1280., *span).unwrap())
                .collect::<Vec<_>>();
            score(RowKind::Tables, &measurements, &widths, 1100., true, false).weighted()
        };
        assert!(
            cost(&[4, 8]) < cost(&[7, 5]),
            "a shorter horizontal reading jump should win when both tables fit"
        );
    }

    fn candidate(start: usize, count: usize, template: usize, cost: f32) -> RowCandidate {
        RowCandidate {
            canvas: 200. * count as f32 + super::super::LAYOUT_GAP * count.saturating_sub(1) as f32,
            roots: start..start + count,
            groups: start..start + count,
            ids: (start..start + count)
                .map(|index| NodeId::new_unchecked(index as u64 + 1))
                .collect(),
            parts: (start..start + count)
                .map(|index| index..index + 1)
                .collect(),
            kind: if count == 1 {
                RowKind::Stack
            } else {
                RowKind::Peer
            },
            template,
            widths: vec![200.; count],
            heights: vec![100.; count],
            penalties: Penalties {
                discomfort: cost / 8.,
                ..Default::default()
            },
            rejected: None,
            previous: false,
            flow_rows: 1,
            edit_locked: false,
            height_estimated: false,
            decision_provisional: false,
        }
    }

    fn template_candidate(start: usize, template: usize, canvas: f32, cost: f32) -> RowCandidate {
        let spans = TEMPLATES[template];
        let count = spans.len();
        RowCandidate {
            canvas,
            roots: start..start + count,
            groups: start..start + count,
            ids: (start..start + count)
                .map(|index| NodeId::new_unchecked(index as u64 + 1))
                .collect(),
            parts: (start..start + count)
                .map(|index| index..index + 1)
                .collect(),
            kind: if count == 1 {
                RowKind::Stack
            } else {
                RowKind::Peer
            },
            template,
            widths: spans
                .iter()
                .map(|span| super::super::candidates::span_width(canvas, *span).unwrap())
                .collect(),
            heights: vec![100.; count],
            penalties: Penalties {
                discomfort: cost / 8.,
                ..Default::default()
            },
            rejected: None,
            previous: false,
            flow_rows: 1,
            edit_locked: false,
            height_estimated: false,
            decision_provisional: false,
        }
    }

    #[test]
    fn malformed_rows_are_rejected_before_search_or_hysteresis() {
        let good = candidate(0, 2, 1, 1.);
        assert!(good.legal());
        type RowMutation = fn(&mut RowCandidate);
        let mutations: &[(&str, RowMutation)] = &[
            ("non-progressing groups", |row| {
                row.groups.end = row.groups.start
            }),
            ("reversed roots", |row| row.roots.start = row.roots.end + 1),
            ("missing identity", |row| {
                row.ids.pop();
            }),
            ("duplicate identity", |row| row.ids[1] = row.ids[0]),
            ("missing column", |row| {
                row.parts.pop();
            }),
            ("overlapping columns", |row| {
                row.parts[1] = row.parts[0].clone()
            }),
            ("out of range column", |row| row.parts[1].end += 1),
            ("empty column", |row| row.parts[0].end = row.parts[0].start),
            ("nonfinite cost", |row| row.penalties.discomfort = f32::NAN),
            ("missing flow", |row| row.flow_rows = 0),
        ];
        for (name, mutate) in mutations {
            let mut malformed = good.clone();
            mutate(&mut malformed);
            malformed.previous = true;
            // Check before invoking the search: the old non-progressing
            // previous-row loop would hang instead of failing this assertion.
            assert!(!malformed.legal(), "{name}");
            let rows = choose_window(
                2,
                &[malformed, candidate(0, 1, 0, 1.), candidate(1, 1, 0, 1.)],
            );
            assert_eq!(rows.len(), 2, "{name}");
            assert!(
                rows.iter()
                    .all(|row| row.kind == RowKind::Stack && row.legal())
            );
            assert!(rows.iter().flat_map(|row| row.roots.clone()).eq(0..2));
        }
        assert!(choose_window(WINDOW_GROUPS + 1, &[]).is_empty());
    }

    #[test]
    fn retained_row_ranges_follow_identity_and_reject_changed_membership() {
        let ids = (1..=8).map(NodeId::new_unchecked).collect::<Vec<_>>();
        assert_eq!(remap_roots(&(1..4), &ids[..5], &ids[..6]), Some(1..4));
        let shifted = [ids[6], ids[7], ids[0], ids[1], ids[2], ids[3], ids[4]];
        assert_eq!(remap_roots(&(1..4), &ids[..5], &shifted), Some(3..6));
        let interrupted = [ids[0], ids[1], ids[7], ids[2], ids[3], ids[4]];
        assert_eq!(remap_roots(&(1..4), &ids[..5], &interrupted), None);
        assert_eq!(remap_roots(&(1..4), &ids[..5], &ids[..3]), None);
    }

    #[test]
    fn search_looks_ahead_instead_of_greedily_consuming_the_next_pair() {
        let candidates = vec![
            candidate(0, 1, 0, 2.),
            candidate(1, 1, 0, 2.),
            candidate(2, 1, 0, 2.),
            candidate(0, 2, 1, 1.9),
            candidate(1, 2, 1, 0.1),
        ];
        let chosen = choose_window(3, &candidates);
        assert_eq!(
            chosen.iter().map(|r| r.groups.clone()).collect::<Vec<_>>(),
            vec![0..1, 1..3]
        );
    }

    #[test]
    fn exhaustive_small_sequences_are_deterministic_legal_exact_partitions() {
        let mut selected_templates = std::collections::BTreeSet::new();
        for canvas in [360., 768., 1280., 1920.] {
            let masks = if canvas == 1280. {
                (0..3_usize.pow(6)).collect::<Vec<_>>()
            } else {
                vec![0, 1, 3_usize.pow(6) - 1]
            };
            let group_count = 6;
            for preferred_template in 1..TEMPLATES.len() {
                for mask in masks.iter().copied() {
                    let mut candidates = Vec::new();
                    for start in 0..group_count {
                        candidates.push(template_candidate(start, 0, canvas, 1.));
                        for (template, spans) in TEMPLATES.iter().enumerate().skip(1) {
                            let count = spans.len();
                            if start + count > group_count {
                                continue;
                            }
                            let cost = if template == preferred_template {
                                0.05
                            } else {
                                0.1 + template as f32 / 1_000.
                            };
                            let mut row = template_candidate(start, template, canvas, cost);
                            if (mask / 3_usize.pow(start as u32)) % 3 < count - 1 {
                                row.rejected = Some(RowRejection::TooTall);
                            }
                            candidates.push(row);
                        }
                    }
                    let rows = choose_window(group_count, &candidates);
                    assert_eq!(
                        rows.iter()
                            .flat_map(|row| row.groups.clone())
                            .collect::<Vec<_>>(),
                        (0..group_count).collect::<Vec<_>>()
                    );
                    assert!(rows.iter().all(RowCandidate::legal));
                    for row in &rows {
                        selected_templates.insert(row.template);
                        for (width, span) in row.widths.iter().zip(TEMPLATES[row.template]) {
                            let feasible = super::super::candidates::span_width(canvas, *span)
                                .expect("a selected track span has positive width");
                            assert!((width - feasible).abs() < 0.01);
                        }
                    }
                    assert_eq!(
                        rows.iter()
                            .map(|row| (row.groups.clone(), row.template))
                            .collect::<Vec<_>>(),
                        choose_window(group_count, &candidates)
                            .iter()
                            .map(|row| (row.groups.clone(), row.template))
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
        assert_eq!(selected_templates, (0..TEMPLATES.len()).collect());
    }

    #[test]
    fn hysteresis_keeps_legal_rows_but_never_retains_invalid_geometry() {
        let mut old = candidate(0, 2, 1, 1.);
        old.previous = true;
        let alternatives = vec![
            old.clone(),
            candidate(0, 1, 0, 0.48),
            candidate(1, 1, 0, 0.48),
        ];
        assert_eq!(
            choose_window(2, &alternatives).len(),
            1,
            "4% improvement does not rearrange the row"
        );
        old.widths[0] += 10.;
        assert!(
            !old.legal(),
            "a row wider than its actual tracks is invalid"
        );
        let escaped = choose_window(
            2,
            &[old, candidate(0, 1, 0, 0.48), candidate(1, 1, 0, 0.48)],
        );
        assert_eq!(escaped.len(), 2);
        assert!(escaped.iter().all(RowCandidate::legal));
    }
}
