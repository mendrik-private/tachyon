//! Bounded, deterministic source-order row search. No editor or renderer access.
use std::ops::Range;

use document_core::NodeId;

use super::candidates::Penalties;
use super::groups::{GroupKind, RelationshipKind};
use super::{AdaptivePlan, LayoutSlot, PROSE_WIDTH, compact_section};
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
];

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct GroupMeasurement {
    pub height: f32,
    pub preferred_width: f32,
    pub overflow: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowKind {
    Stack,
    Peer,
    Explanation,
    Tables,
    Gallery,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowRejection {
    TooNarrow,
    Unmeasured,
    Overflow,
    TooTall,
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
            && (1..=3).contains(&self.groups.len())
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
    id: NodeId,
    roots: Range<usize>,
    peer: Option<(Option<NodeId>, u8, u8)>,
    explanation: bool,
    table_parent: Option<Option<NodeId>>,
    gallery: Option<NodeId>,
    pending_gallery: bool,
    boundary: bool,
}

fn remap_roots(range: &Range<usize>, old: &[NodeId], new: &[NodeId]) -> Option<Range<usize>> {
    let ids = old.get(range.clone())?;
    let first = *ids.first()?;
    let start = new.iter().position(|id| *id == first)?;
    let end = start.checked_add(ids.len())?;
    (new.get(start..end)? == ids).then_some(start..end)
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
    let parts = old_row
        .parts
        .iter()
        .map(|part| remap_roots(part, &old.root_ids, &plan.root_ids))
        .collect::<Option<Vec<_>>>();
    let bounds = window
        .iter()
        .position(|unit| unit.roots.start == roots.start)
        .zip(window.iter().position(|unit| unit.roots.end == roots.end));
    let retained = if old_row.canvas <= canvas + 0.01 {
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
                    .map(|(part, width)| measure(part.clone(), *width, row.kind == RowKind::Peer))
                    .collect::<Option<Vec<_>>>();
                if let Some(measurements) = measurements
                    .filter(|values| values.iter().all(|m| m.height.is_finite() && m.height > 0.))
                {
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
    for candidate in candidates.iter_mut().filter(|row| overlaps(&row.roots)) {
        // If the old widths cannot fit, use the nearest source-order stack;
        // do not jump the caret into a different two/three-column template.
        if retained.is_some() || candidate.kind != RowKind::Stack {
            candidate.rejected = Some(RowRejection::EditLock);
        }
    }
    if let Some(row) = retained {
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
    canvas: f32,
    viewport: f32,
    previous: Option<&AdaptivePlan>,
    keep: bool,
    mut measure: impl FnMut(Range<usize>, f32, bool) -> Option<GroupMeasurement>,
) {
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
        for (&node, &slot) in &old.slots {
            if plan.measured_rows.chosen.iter().any(|row| {
                (row.kind != RowKind::Stack || row.edit_locked)
                    && row.ids.first() == Some(&slot.group)
            }) && projection.segment_for_node(node).is_some()
            {
                plan.slots.insert(node, slot);
            }
        }
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
            for (template, spans) in TEMPLATES.iter().enumerate().skip(1) {
                let (kind, consume, parts) = if unit.explanation && spans.len() == 2 {
                    let split = unit.roots.end - 1;
                    // Prefix headings stay full-width; the source-adjacent
                    // paragraph and example are the only moved content.
                    (
                        RowKind::Explanation,
                        1,
                        vec![split - 1..split, split..unit.roots.end],
                    )
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
                            .map(|unit| unit.roots.end - 1..unit.roots.end)
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
                } else if unit.peer.is_some()
                    && i + spans.len() <= window.len()
                    && window[i..i + spans.len()]
                        .iter()
                        .all(|u| u.peer == unit.peer)
                {
                    (
                        RowKind::Peer,
                        spans.len(),
                        window[i..i + spans.len()]
                            .iter()
                            .map(|u| u.roots.clone())
                            .collect(),
                    )
                } else {
                    continue;
                };
                let widths = spans
                    .iter()
                    .map(|s| super::candidates::span_width(canvas, *s).unwrap_or(0.))
                    .collect::<Vec<_>>();
                let mut rejection = widths
                    .iter()
                    .any(|w| *w < 260.)
                    .then_some(RowRejection::TooNarrow);
                let mut measurements = Vec::new();
                if rejection.is_none() {
                    for (part, width) in parts.iter().zip(&widths) {
                        match measure(part.clone(), *width, kind == RowKind::Peer) {
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
                if rejection.is_none() {
                    if measurements.iter().any(|m| m.overflow) {
                        rejection = Some(RowRejection::Overflow);
                    } else if viewport <= 0.
                        || measurements.iter().any(|m| m.height > viewport * 0.7)
                    {
                        rejection = Some(RowRejection::TooTall);
                    }
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
                    penalties: score(kind, &measurements, &widths, viewport, true, false),
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
                        &[canvas.min(PROSE_WIDTH)],
                        viewport,
                        true,
                        false,
                    );
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
            candidates[i].penalties.change = if had_previous && !candidates[i].previous {
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
                    RowKind::Peer | RowKind::Tables => {
                        row.parts.iter().eq(units.iter().map(|unit| &unit.roots))
                    }
                    RowKind::Explanation => {
                        units.len() == 1
                            && row.roots.len() >= 2
                            && row.parts.as_slice()
                                == [
                                    row.roots.end - 2..row.roots.end - 1,
                                    row.roots.end - 1..row.roots.end,
                                ]
                    }
                    RowKind::Gallery => row
                        .parts
                        .iter()
                        .cloned()
                        .eq(units.iter().map(|unit| unit.roots.end - 1..unit.roots.end)),
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
        flow_rows: if unit.explanation { 2 } else { 1 },
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
            for ordinal in first..group.roots.end {
                units.push(Unit {
                    id: roots[ordinal].id(),
                    roots: if ordinal == first { group.roots.start } else { ordinal }..ordinal + 1,
                    peer: None,
                    explanation: false,
                    table_parent: None,
                    gallery: Some(group.id),
                    pending_gallery,
                    boundary: ordinal == first
                        && matches!(roots[group.roots.start], BlockNode::Heading(h) if h.level <= 2),
                });
            }
            g += 1;
            continue;
        }
        let mut unit = Unit {
            id: group.id,
            roots: group.roots.clone(),
            peer: None,
            gallery: None,
            pending_gallery: false,
            table_parent: match &roots[group.roots.clone()] {
                [BlockNode::Table(_)] => Some(group.section),
                [BlockNode::Heading(heading), BlockNode::Table(_)] if heading.level >= 3 => {
                    Some(plan.groups.sections[&heading.id].parent)
                }
                _ => None,
            },
            explanation: group.kind == GroupKind::ExplanationContent
                && group
                    .relationships
                    .iter()
                    .any(|r| r.kind == RelationshipKind::AdjacentExplanation),
            boundary: group.kind == GroupKind::Barrier
                || matches!(roots[group.roots.start], BlockNode::Heading(h) if h.level <= 2),
        };
        if let Some((level, end, shape)) = compact_section(roots, group.roots.start) {
            let last = plan.groups.groups[g..]
                .iter()
                .position(|group| group.roots.end >= end)
                .map(|offset| g + offset);
            if let Some(last) = last
                && plan.groups.groups[last].roots.end == end
            {
                unit.roots.end = end;
                unit.peer = Some((plan.groups.sections[&group.id].parent, level, shape));
                unit.explanation = false;
                g = last;
            }
        }
        units.push(unit);
        g += 1;
    }
    units
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
                group: row.ids[0],
                item,
                columns: spans.len(),
                cards: row.kind == RowKind::Peer,
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
                    if projection
                        .segment_for_node(*node)
                        .is_some_and(|s| s.top_level_node_id == root.id())
                    {
                        let mut node_slot = slot;
                        if row.kind == RowKind::Stack {
                            let wide = matches!(
                                root,
                                BlockNode::Heading(_)
                                    | BlockNode::CodeBlock(_)
                                    | BlockNode::Table(_)
                                    | BlockNode::Image(_)
                            );
                            node_slot.fixed_canvas = Some(if wide {
                                row.canvas
                            } else {
                                row.canvas.min(PROSE_WIDTH)
                            });
                        }
                        plan.slots.insert(*node, node_slot);
                    }
                }
            }
        }
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
            0.2
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
        complexity: (columns.saturating_sub(1) as f32 / 3.).clamp(0., 1.),
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

/// n <= 40; each row consumes 1..=3 contiguous groups. Previous template is
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
            for (index, row) in candidates.iter().enumerate().filter(|(_, row)| {
                row.groups.start == i
                    && row.groups.end <= n
                    && (1..=3).contains(&row.groups.len())
                    && row.legal()
            }) {
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
    fn gallery_scaling_is_not_prose_wrapping_discomfort() {
        let images = [GroupMeasurement {
            height: 177.,
            preferred_width: 976.,
            overflow: false,
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
            },
            GroupMeasurement {
                height: 211.,
                preferred_width: 518.,
                overflow: false,
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
        for mask in 0..729_usize {
            let mut candidates = Vec::new();
            for start in 0..6 {
                candidates.push(candidate(start, 1, 0, 1.));
                for count in 2..=3 {
                    if start + count <= 6 {
                        let mut row = candidate(start, count, if count == 2 { 1 } else { 6 }, 0.1);
                        if (mask / 3_usize.pow(start as u32)) % 3 < count - 1 {
                            row.rejected = Some(RowRejection::TooTall);
                        }
                        candidates.push(row);
                    }
                }
            }
            let rows = choose_window(6, &candidates);
            assert_eq!(
                rows.iter()
                    .flat_map(|r| r.groups.clone())
                    .collect::<Vec<_>>(),
                (0..6).collect::<Vec<_>>()
            );
            assert!(rows.iter().all(RowCandidate::legal));
            assert_eq!(
                rows.iter()
                    .map(|r| (r.groups.clone(), r.template))
                    .collect::<Vec<_>>(),
                choose_window(6, &candidates)
                    .iter()
                    .map(|r| (r.groups.clone(), r.template))
                    .collect::<Vec<_>>()
            );
        }
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
