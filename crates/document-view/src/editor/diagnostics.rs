//! Opt-in, content-free layout evidence. Counters are scoped to one synchronous
//! worker, not deltas from a shared cache that concurrent typing can contaminate.
use std::{
    cell::Cell,
    collections::{BTreeMap, HashMap},
    marker::PhantomData,
    rc::Rc,
    time::Instant,
};

use super::AdaptivePlan;
use crate::adaptive::{
    ListLayout,
    candidates::{ItemMeasurement, ListCandidate, ListDecision, Penalties, Rejection},
    rows::{RowCandidate, RowKind, RowRejection},
};
use serde::Serialize;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutTraceMode {
    #[default]
    Off,
    Summary,
    Details,
}

#[derive(Clone, Copy, Debug, Default, Serialize, PartialEq, Eq)]
pub(super) struct MeasurementCounts {
    pub wrap_requests: u64,
    pub wrap_cache_hits: u64,
    pub wrap_cache_misses: u64,
    pub intrinsic_requests: u64,
    pub intrinsic_cache_hits: u64,
    pub intrinsic_cache_misses: u64,
    pub shaping_calls: u64,
    pub table_attempts: u64,
    pub tables_measured: u64,
    pub table_cache_hits: u64,
    pub group_requests: u64,
    pub group_cache_hits: u64,
    pub groups_measured: u64,
    pub item_requests: u64,
    pub item_cache_hits: u64,
    pub items_measured: u64,
    pub geometry_requests: u64,
    pub geometry_cache_hits: u64,
    pub segments_laid_out: u64,
    pub geometry_cache_evictions: u64,
    pub published_geometry_reuses: u64,
    pub retained_extension_requests: u64,
    pub retained_extension_hits: u64,
    pub deferred_text_segments: u64,
}

thread_local! { static COUNTS: Cell<Option<MeasurementCounts>> = const { Cell::new(None) }; }

pub(super) fn count(update: impl FnOnce(&mut MeasurementCounts)) {
    COUNTS.with(|cell| {
        if let Some(mut counts) = cell.get() {
            update(&mut counts);
            cell.set(Some(counts));
        }
    });
}

pub(super) struct MeasurementScope {
    previous: Option<MeasurementCounts>,
    // A thread-local scope must not move to another executor thread.
    _thread: PhantomData<Rc<()>>,
}

impl MeasurementScope {
    pub fn new() -> Self {
        Self {
            previous: COUNTS.replace(Some(MeasurementCounts::default())),
            _thread: PhantomData,
        }
    }
    pub fn take_stage(&self) -> MeasurementCounts {
        COUNTS
            .replace(Some(MeasurementCounts::default()))
            .unwrap_or_default()
    }
}

impl Drop for MeasurementScope {
    fn drop(&mut self) {
        COUNTS.set(self.previous);
    }
}

#[derive(Clone, Debug, Serialize)]
struct Stage {
    name: &'static str,
    elapsed_ms: f64,
    measurements: MeasurementCounts,
}

#[derive(Clone, Debug, Serialize)]
struct RowDetail {
    root_interval: [usize; 2],
    group_ids: Vec<u64>,
    kind: &'static str,
    template: usize,
    widths: Vec<f32>,
    heights: Vec<f32>,
    height_estimated: bool,
    decision_provisional: bool,
    edit_locked: bool,
    rejected: Option<&'static str>,
    reason_codes: Vec<&'static str>,
    weighted_cost: f32,
    // Explicit labels accompany the normalized values in the report schema.
    normalized_penalties: [f32; 7],
    rendered_bounds_after_text_zoom_px: Option<[f32; 4]>,
}

#[derive(Clone, Debug, Serialize)]
struct ListCandidateDetail {
    columns: usize,
    row_columns: Vec<usize>,
    width: f32,
    items: Vec<ItemMeasurement>,
    rows: Vec<Penalties>,
    rejected: Option<&'static str>,
    reason_codes: Vec<&'static str>,
    weighted_cost: f32,
}

#[derive(Clone, Debug, Serialize)]
struct ListDetail {
    group_id: u64,
    layout: ListLayout,
    row_columns: Vec<usize>,
    retained_previous: bool,
    reason_codes: Vec<&'static str>,
    candidates: Vec<ListCandidateDetail>,
}

fn rejection(reason: RowRejection) -> &'static str {
    match reason {
        RowRejection::TooNarrow => "too_narrow",
        RowRejection::Unmeasured => "unmeasured",
        RowRejection::Overflow => "overflow",
        RowRejection::TooTall => "peer_too_tall",
        RowRejection::UnevenHeights => "uneven_peer_heights",
        RowRejection::EditLock => "edit_lock",
    }
}

fn row_reason_codes(row: &RowCandidate) -> Vec<&'static str> {
    let mut reasons = vec![match row.kind {
        RowKind::Opening => "AUTHORED_OPENING_OVERVIEW",
        RowKind::Stack => "SOURCE_ORDER_STACK",
        RowKind::Peer => "COMPACT_SIBLING_SECTIONS",
        RowKind::Technical => "COMPACT_TECHNICAL_SECTIONS",
        RowKind::IntroList => "ADJACENT_INDEPENDENT_LIST",
        RowKind::Explanation => "ADJACENT_EXPLANATION",
        RowKind::FigureExplanation => "FIGURE_LED_EXPLANATION",
        RowKind::ContentExplanation => "CONTENT_LED_EXPLANATION",
        RowKind::Aside => "ADJACENT_OPTIONAL_GUIDANCE",
        RowKind::Guidance => "ADJACENT_OPTIONAL_CALLOUTS",
        RowKind::Tables => "ADJACENT_TABLES",
        RowKind::Gallery => "CONSECUTIVE_IMAGES",
    }];
    if row.rejected.is_none() && row.widths.len() > 1 {
        reasons.push(match row.widths.len() {
            2 => "MEASURED_TWO_COLUMN_FIT",
            _ => "MEASURED_THREE_COLUMN_FIT",
        });
        if row
            .widths
            .windows(2)
            .any(|widths| (widths[0] - widths[1]).abs() >= 0.5)
        {
            reasons.push("MEASURED_UNEQUAL_TRACKS");
        }
    }
    if row.height_estimated {
        reasons.push("FALLBACK_UNMEASURED");
    }
    if row.decision_provisional {
        reasons.push("PENDING_RESOURCE");
    }
    if row.previous {
        reasons.push("RETAINED_PREVIOUS_LAYOUT");
    }
    if row.edit_locked {
        reasons.push("EDIT_LOCK");
    }
    if let Some(reason) = row.rejected {
        reasons.push(match reason {
            RowRejection::TooNarrow => "COLUMN_TOO_NARROW",
            RowRejection::Unmeasured => "MEASUREMENT_UNAVAILABLE",
            RowRejection::Overflow => "INTERNAL_OVERFLOW",
            RowRejection::TooTall => "PEER_TOO_TALL",
            RowRejection::UnevenHeights => "UNEVEN_PEER_HEIGHTS",
            RowRejection::EditLock => "EDIT_LOCK_REJECTED",
        });
    }
    reasons
}

fn list_rejection(reason: Rejection) -> &'static str {
    match reason {
        Rejection::ColumnTooNarrow => "column_too_narrow",
        Rejection::MeasurementUnavailable => "measurement_unavailable",
        Rejection::TooManyLines => "too_many_lines",
        Rejection::UnevenHeights => "uneven_heights",
        Rejection::Overflow => "overflow",
    }
}

fn list_candidate_reason_codes(candidate: &ListCandidate) -> Vec<&'static str> {
    let mut reasons = vec![if candidate.columns == 1 {
        "SOURCE_ORDER_STACK"
    } else {
        "SHORT_FLAT_LIST"
    }];
    if candidate.rejected.is_none() && candidate.columns > 1 {
        reasons.push(match candidate.columns {
            2 => "MEASURED_TWO_COLUMN_FIT",
            4 => "MEASURED_FOUR_COLUMN_FIT",
            _ => "MEASURED_THREE_COLUMN_FIT",
        });
    }
    if let Some(reason) = candidate.rejected {
        reasons.push(match reason {
            Rejection::ColumnTooNarrow => "COLUMN_TOO_NARROW",
            Rejection::MeasurementUnavailable => "MEASUREMENT_UNAVAILABLE",
            Rejection::TooManyLines => "TOO_MANY_ITEM_LINES",
            Rejection::UnevenHeights => "UNEVEN_ITEM_HEIGHTS",
            Rejection::Overflow => "INTERNAL_OVERFLOW",
        });
    }
    reasons
}

fn list_detail(group_id: u64, decision: &ListDecision) -> ListDetail {
    let mut reason_codes = vec![match decision.layout {
        ListLayout::Grid(2) => "MEASURED_TWO_COLUMN_FIT",
        ListLayout::Grid(4) => "MEASURED_FOUR_COLUMN_FIT",
        ListLayout::Grid(_) => "MEASURED_THREE_COLUMN_FIT",
        _ => "SOURCE_ORDER_STACK",
    }];
    if decision.retained_previous {
        reason_codes.push("RETAINED_PREVIOUS_LAYOUT");
    }
    ListDetail {
        group_id,
        layout: decision.layout,
        row_columns: decision.row_columns.clone(),
        retained_previous: decision.retained_previous,
        reason_codes,
        candidates: decision
            .candidates
            .iter()
            .map(|candidate| ListCandidateDetail {
                columns: candidate.columns,
                row_columns: candidate.row_columns.clone(),
                width: candidate.width,
                items: candidate.items.clone(),
                rows: candidate.rows.clone(),
                rejected: candidate.rejected.map(list_rejection),
                reason_codes: list_candidate_reason_codes(candidate),
                weighted_cost: candidate.cost(),
            })
            .collect(),
    }
}

impl From<&RowCandidate> for RowDetail {
    fn from(row: &RowCandidate) -> Self {
        let p = row.penalties;
        Self {
            root_interval: [row.roots.start, row.roots.end],
            group_ids: row.ids.iter().map(|id| id.get()).collect(),
            kind: match row.kind {
                RowKind::Opening => "opening",
                RowKind::Stack => "stack",
                RowKind::Peer => "peer_row",
                RowKind::Technical => "technical_row",
                RowKind::IntroList => "intro_list",
                RowKind::Explanation => "explanation_content",
                RowKind::FigureExplanation => "figure_explanation",
                RowKind::ContentExplanation => "content_explanation",
                RowKind::Aside => "prose_aside",
                RowKind::Guidance => "guidance_row",
                RowKind::Tables => "tables",
                RowKind::Gallery => "gallery",
            },
            template: row.template,
            widths: row.widths.clone(),
            heights: row.heights.clone(),
            height_estimated: row.height_estimated,
            decision_provisional: row.decision_provisional,
            edit_locked: row.edit_locked,
            rejected: row.rejected.map(rejection),
            reason_codes: row_reason_codes(row),
            weighted_cost: row.cost(),
            normalized_penalties: [
                p.discomfort,
                p.separation,
                p.reading_jump,
                p.imbalance,
                p.complexity,
                p.unused_width,
                p.change,
            ],
            rendered_bounds_after_text_zoom_px: None,
        }
    }
}

/// Only structural IDs, geometry and measurements are exported. No text, file
/// paths, URLs, HTML, formula sources or font-resource contents enter this DTO.
#[derive(Clone, Debug, Serialize)]
pub struct LayoutDiagnosticsReport {
    schema_version: u8,
    scope: &'static str,
    execution: &'static str,
    pub(super) original_source_bytes: usize,
    pub(super) sequence: u64,
    pub(super) document_generation: u64,
    pub(super) geometry_generation: u64,
    pub(super) queue_wait_ms: f64,
    pub(super) dispatch_ms: f64,
    pub(super) result_wait_ms: f64,
    worker_ms: f64,
    diagnostic_collection_ms: f64,
    #[serde(skip)]
    pub(super) ready_at: Option<Instant>,
    pub(super) commit_ms: f64,
    pub(super) committed: bool,
    pub(super) explicitly_requested: bool,
    pub(super) discard_reason: Option<&'static str>,
    pub(super) stale_results_total: u64,
    pub(super) committed_results_total: u64,
    /// Sampled from committed geometry/offset, before the following paint.
    pub(super) anchor_displacement_at_commit_px: Option<f32>,
    stages: Vec<Stage>,
    canvas_width: f32,
    text_zoom: f32,
    viewport_height: f32,
    root_count: usize,
    segment_count: usize,
    unresolved_image_dimensions: usize,
    dp_windows: usize,
    candidate_scope: &'static str,
    measured_root_windows: Vec<[usize; 2]>,
    deferred_windows: usize,
    invalid_row_windows: usize,
    pub(super) planner_failed: bool,
    chosen_rows: usize,
    row_candidates: usize,
    list_candidates: usize,
    changed_row_groups: usize,
    changed_lists: usize,
    row_rejections: BTreeMap<&'static str, usize>,
    penalty_order: [&'static str; 7],
    details_truncated: bool,
    chosen: Vec<RowDetail>,
    candidates: Vec<RowDetail>,
    lists: Vec<ListDetail>,
    inline_enumerations: Vec<ListDetail>,
}

pub(super) struct WorkerTrace {
    mode: LayoutTraceMode,
    start: Instant,
    last: Instant,
    counts: MeasurementScope,
    stages: Vec<Stage>,
}

impl WorkerTrace {
    pub fn start(mode: LayoutTraceMode) -> Option<Self> {
        (mode != LayoutTraceMode::Off).then(|| {
            let start = Instant::now();
            Self {
                mode,
                start,
                last: start,
                counts: MeasurementScope::new(),
                stages: Vec::with_capacity(5),
            }
        })
    }
    pub fn stage(&mut self, name: &'static str) {
        let now = Instant::now();
        self.stages.push(Stage {
            name,
            elapsed_ms: (now - self.last).as_secs_f64() * 1000.,
            measurements: self.counts.take_stage(),
        });
        self.last = now;
    }
    pub fn finish(
        self,
        plan: &AdaptivePlan,
        previous: &AdaptivePlan,
        root_count: usize,
        segment_count: usize,
        pending_images: usize,
    ) -> LayoutDiagnosticsReport {
        let worker_ms = self.start.elapsed().as_secs_f64() * 1000.;
        let collection_start = Instant::now();
        let rows = &plan.measured_rows;
        let mut row_rejections = BTreeMap::new();
        for row in &rows.candidates {
            if let Some(reason) = row.rejected {
                *row_rejections.entry(rejection(reason)).or_default() += 1;
            }
        }
        let details = self.mode == LayoutTraceMode::Details;
        let mut list_ids = if details {
            plan.measured_lists.keys().copied().collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        list_ids.sort();
        let lists = list_ids
            .into_iter()
            .take(12)
            .filter_map(|id| {
                plan.measured_lists
                    .get(&id)
                    .map(|decision| list_detail(id.get(), decision))
            })
            .collect();
        let mut inline_ids = if details {
            plan.measured_inline_lists
                .keys()
                .copied()
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        inline_ids.sort();
        let inline_enumerations = inline_ids
            .into_iter()
            .take(12)
            .map(|id| {
                let mut detail = list_detail(id.get(), &plan.measured_inline_lists[&id]);
                detail
                    .reason_codes
                    .push("EXPLICIT_INTRODUCTORY_ENUMERATION");
                detail
            })
            .collect();
        let mut report = LayoutDiagnosticsReport {
            inline_enumerations,
            schema_version: 2,
            scope: "whole_document",
            execution: "background_reflow",
            original_source_bytes: 0,
            sequence: 0,
            document_generation: 0,
            geometry_generation: 0,
            queue_wait_ms: 0.,
            dispatch_ms: 0.,
            result_wait_ms: 0.,
            worker_ms,
            diagnostic_collection_ms: 0.,
            ready_at: None,
            commit_ms: 0.,
            committed: false,
            explicitly_requested: false,
            discard_reason: None,
            stale_results_total: 0,
            committed_results_total: 0,
            anchor_displacement_at_commit_px: None,
            stages: self.stages,
            canvas_width: plan.canvas,
            text_zoom: 1.,
            viewport_height: rows.viewport,
            root_count,
            segment_count,
            unresolved_image_dimensions: pending_images,
            dp_windows: rows.windows,
            candidate_scope: if plan.measurement_ranges.is_some() {
                "visible_and_lookahead_windows"
            } else {
                "whole_document"
            },
            measured_root_windows: plan
                .measurement_ranges
                .as_ref()
                .map(|ranges| {
                    ranges
                        .iter()
                        .map(|range| [range.start, range.end])
                        .collect()
                })
                .unwrap_or_default(),
            deferred_windows: rows.deferred_windows,
            invalid_row_windows: rows.validation_fallbacks,
            planner_failed: false,
            chosen_rows: rows.chosen.len(),
            row_candidates: rows.candidates.len(),
            list_candidates: plan
                .measured_lists
                .iter()
                .filter(|(id, _)| plan.measures_root(**id))
                .map(|(_, decision)| decision.candidates.len())
                .sum(),
            changed_row_groups: changed_rows(previous, plan),
            changed_lists: plan
                .lists
                .iter()
                .filter(|(id, list)| {
                    previous
                        .lists
                        .get(id)
                        .is_none_or(|old| old.layout != list.layout)
                })
                .count()
                + previous
                    .lists
                    .keys()
                    .filter(|id| !plan.lists.contains_key(id))
                    .count(),
            row_rejections,
            penalty_order: [
                "discomfort",
                "separation",
                "reading_jump",
                "imbalance",
                "complexity",
                "unused_width",
                "change",
            ],
            details_truncated: details
                && (rows.chosen.len() > 32
                    || rows.candidates.len() > 96
                    || plan.measured_lists.len() > 12
                    || plan.measured_inline_lists.len() > 12),
            lists,
            chosen: if details {
                rows.chosen.iter().take(32).map(RowDetail::from).collect()
            } else {
                Vec::new()
            },
            candidates: if details {
                rows.candidates
                    .iter()
                    .take(96)
                    .map(RowDetail::from)
                    .collect()
            } else {
                Vec::new()
            },
        };
        report.diagnostic_collection_ms = collection_start.elapsed().as_secs_f64() * 1000.;
        report.ready_at = Some(Instant::now());
        report
    }
}

impl LayoutDiagnosticsReport {
    pub(super) fn attach_rendered_bounds(
        &mut self,
        snapshot: &document_core::DocumentSnapshot,
        components: &super::ComponentIndex,
        width: f32,
        zoom: f32,
    ) {
        let started = Instant::now();
        self.text_zoom = zoom;
        for row in &mut self.chosen {
            let mut bounds: Option<[f32; 4]> = None;
            for ordinal in row.root_interval[0]..row.root_interval[1] {
                let Some(component) = snapshot
                    .blocks()
                    .get(ordinal)
                    .and_then(|block| components.get(&block.id()))
                else {
                    continue;
                };
                let next = [
                    component.left_fraction * width,
                    component.top,
                    component.right_fraction * width,
                    component.bottom,
                ];
                bounds = Some(bounds.map_or(next, |old| {
                    [
                        old[0].min(next[0]),
                        old[1].min(next[1]),
                        old[2].max(next[2]),
                        old[3].max(next[3]),
                    ]
                }));
            }
            row.rendered_bounds_after_text_zoom_px = bounds;
        }
        self.diagnostic_collection_ms += started.elapsed().as_secs_f64() * 1000.;
        self.ready_at = Some(Instant::now());
    }
}

fn changed_rows(previous: &AdaptivePlan, next: &AdaptivePlan) -> usize {
    fn placements(plan: &AdaptivePlan) -> HashMap<u64, Vec<u64>> {
        let mut result = HashMap::new();
        for row in &plan.measured_rows.chosen {
            for (ordinal, id) in row.ids.iter().enumerate() {
                let kind = match row.kind {
                    RowKind::Opening => 10,
                    RowKind::Guidance => 11,
                    RowKind::Stack => 0,
                    RowKind::Peer => 1,
                    RowKind::Technical => 6,
                    RowKind::IntroList => 2,
                    RowKind::Explanation => 3,
                    RowKind::FigureExplanation => 8,
                    RowKind::ContentExplanation => 9,
                    RowKind::Aside => 7,
                    RowKind::Tables => 4,
                    RowKind::Gallery => 5,
                };
                let mut signature = vec![kind, row.template as u64, ordinal as u64];
                signature.extend(row.ids.iter().map(|id| id.get()));
                signature.extend(row.widths.iter().map(|width| u64::from(width.to_bits())));
                result.insert(id.get(), signature);
            }
        }
        result
    }
    let old = placements(previous);
    let new = placements(next);
    new.iter()
        .filter(|(id, signature)| old.get(id) != Some(signature))
        .count()
        + old.keys().filter(|id| !new.contains_key(id)).count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn automatic_layout_diagnostics_explain_nominations_fit_and_rejections() {
        let canvas = 1280.;
        let widths = crate::adaptive::rows::TEMPLATES[2]
            .iter()
            .map(|span| crate::adaptive::candidates::span_width(canvas, *span).unwrap())
            .collect::<Vec<_>>();
        let mut row = RowCandidate {
            canvas,
            roots: 0..2,
            groups: 0..2,
            ids: vec![
                document_core::NodeId::new_unchecked(1),
                document_core::NodeId::new_unchecked(2),
            ],
            parts: vec![0..1, 1..2],
            kind: RowKind::Tables,
            template: 2,
            widths,
            heights: vec![120., 180.],
            penalties: Penalties::default(),
            rejected: None,
            previous: false,
            flow_rows: 1,
            edit_locked: false,
            height_estimated: false,
            decision_provisional: false,
        };
        assert_eq!(
            row_reason_codes(&row),
            [
                "ADJACENT_TABLES",
                "MEASURED_TWO_COLUMN_FIT",
                "MEASURED_UNEQUAL_TRACKS"
            ]
        );
        row.rejected = Some(RowRejection::TooTall);
        assert_eq!(row_reason_codes(&row), ["ADJACENT_TABLES", "PEER_TOO_TALL"]);

        let decision = crate::adaptive::candidates::choose_list(
            6,
            canvas,
            crate::adaptive::PROSE_WIDTH,
            None,
            false,
            false,
            |_, _, _| {
                Some(ItemMeasurement {
                    lines: 1,
                    height: 28.,
                    preferred_width: 120.,
                    overflow: false,
                })
            },
        );
        let detail = list_detail(7, &decision);
        assert_eq!(detail.group_id, 7);
        assert!(
            detail.candidates[2]
                .reason_codes
                .contains(&"MEASURED_THREE_COLUMN_FIT")
        );
        assert!(
            detail
                .candidates
                .iter()
                .any(|candidate| candidate.reason_codes.contains(&"SHORT_FLAT_LIST"))
        );
    }

    #[gpui::test]
    fn tracing_does_not_change_geometry_source_or_plan_and_bounds_detail_output(
        cx: &mut gpui::TestAppContext,
    ) {
        use crate::editor::{PreparedDocumentView, ReflowViewport, measurement::FontMeasurement};
        cx.update(|cx| {
            let source = (0..120)
                .map(|index| {
                    format!(
                        "## Section {index}\n\nPrivate body https://example.test/private-token\n\n"
                    )
                })
                .collect::<String>();
            let document = document_core::Document::from_markdown(source.as_str()).unwrap();
            let snapshot = document.snapshot();
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let previous = AdaptivePlan::default();
            let prepare = |trace_mode| {
                PreparedDocumentView::prepare_snapshot_with_images(
                    &snapshot,
                    &Default::default(),
                    None,
                    ReflowViewport {
                        published_geometry: None,
                        width: 1100.,
                        height: 800.,
                        zoom: 1.,
                        preview_edit_node: None,
                        expanded_code_tail: None,
                        editing_node: None,
                        table_layout_lock: None,
                        html_disclosures: std::sync::Arc::default(),
                        html_loaded_images: std::sync::Arc::default(),
                        trace_mode,
                        visible_roots: None,
                        resource_generation: 0,
                    },
                    &previous,
                    &measurement,
                )
            };
            let (plain, _, disabled) = prepare(LayoutTraceMode::Off);
            let (traced, _, report) = prepare(LayoutTraceMode::Details);
            let report = report.unwrap();
            assert!(disabled.is_none());
            assert_eq!(plain.document_height, traced.document_height);
            assert_eq!(plain.paint_order, traced.paint_order);
            assert_eq!(
                plain
                    .visual_lines
                    .iter()
                    .map(|line| (
                        line.projected_range(),
                        line.y,
                        line.inset,
                        line.width_fraction
                    ))
                    .collect::<Vec<_>>(),
                traced
                    .visual_lines
                    .iter()
                    .map(|line| (
                        line.projected_range(),
                        line.y,
                        line.inset,
                        line.width_fraction
                    ))
                    .collect::<Vec<_>>()
            );
            assert_eq!(snapshot.serialize().unwrap(), source);
            assert_eq!(changed_rows(&plain.adaptive, &traced.adaptive), 0);
            assert_eq!(report.root_count, 240);
            assert_eq!(report.schema_version, 2);
            assert!(report.details_truncated);
            assert!(report.chosen.len() <= 32 && report.candidates.len() <= 96);
            assert!(report.chosen.iter().all(|row| !row.reason_codes.is_empty()));
            assert_eq!(report.stages.len(), 5);
            assert!(report.stages.iter().all(|stage| stage.elapsed_ms >= 0.));
            assert!(!format!("{report:?}").contains("private-token"));
            assert_eq!(COUNTS.get(), None);
        });
    }

    #[test]
    fn counters_are_opt_in_stage_scoped_and_thread_local() {
        count(|counts| counts.shaping_calls += 99);
        assert_eq!(COUNTS.get(), None);
        let scope = MeasurementScope::new();
        count(|counts| counts.shaping_calls += 1);
        std::thread::spawn(|| {
            let scope = MeasurementScope::new();
            count(|counts| counts.shaping_calls += 7);
            assert_eq!(scope.take_stage().shaping_calls, 7);
        })
        .join()
        .unwrap();
        {
            let nested = MeasurementScope::new();
            count(|counts| counts.wrap_cache_hits += 3);
            assert_eq!(nested.take_stage().wrap_cache_hits, 3);
        }
        assert_eq!(scope.take_stage().shaping_calls, 1);
        assert_eq!(scope.take_stage(), MeasurementCounts::default());
        drop(scope);
        assert_eq!(COUNTS.get(), None);
    }
}
