use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash as _, Hasher as _},
    ops::Range,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use document_core::{
    Affinity, AlertKind, BlockNode, BlockStyle, ColumnAlignment, Document, DocumentError,
    DocumentPosition, DocumentSnapshot, EditCommand, InlineFormat, InlineStyle, InsertBlockKind,
    NodeId, RectangularSelection, Revision, RichClipboard, Selection, TableBorder, TextSelection,
};
use gpui::{
    Anchor, AnyElement, App, BorderStyle, Bounds, BoxShadow, ClipboardItem, ContentMask, Context,
    CursorStyle, DismissEvent, Element, ElementId, ElementInputHandler, Entity, EntityInputHandler,
    FocusHandle, Focusable, FontStyle, FontWeight, GlobalElementId, Hitbox, ImageSource,
    KeyBinding, LayoutId, MouseButton, MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit,
    PaintQuad, Pixels, Point, Resource, Role, ScrollHandle, ScrollWheelEvent, ShapedLine,
    StrikethroughStyle, Style, StyledImage as _, Subscription, Task, TextRun, Toggled,
    UTF16Selection, UnderlineStyle, Window, actions, anchored, deferred, div, fill, hash, img,
    outline, point, prelude::*, px, relative, rgb, rgba, size,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, IconNamed as _, Sizable as _, Theme,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenu, PopupMenuItem},
    scroll::{Scrollbar, ScrollbarMode},
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::adaptive::{
    AdaptivePlan, CARD_PADDING, LAYOUT_GAP, LAYOUT_HEADER, LayoutSlot, ListLayout,
};
use crate::theme::DocumentStyle;
use crate::{
    ButtonAccessibilityExt as _, Minimap, MinimapAlertTone, MinimapCodeTone, SharedDocumentSession,
    TachyonPalette, TextProjection,
    minimap::{MinimapSourceKind, MinimapSourceLine},
};

mod arrangement;
#[cfg(test)]
mod bibliography;
mod compact_tree;
mod editorial;
mod figure_flow;
mod footnotes;
mod inline_lists;
mod label_rows;
mod line_breaks;
mod metadata;
mod metrics;
mod nested_measures;
mod paragraph_endings;
mod prose_flow;
mod quotes;
#[cfg(test)]
mod recorded_bugs;
mod resource;
#[cfg(test)]
mod signals;
mod task_strips;
mod typography;
#[cfg(test)]
use arrangement::build_measured_adaptive_plan;
use arrangement::{
    ComponentIndex, build_arranged_visual_lines, build_edit_locked_adaptive_plan,
    build_measured_visual_lines, component_geometry,
};
mod geometry_cache;
mod published_geometry;
use published_geometry::{GeometryInputs, PublishedGeometry};
mod measurement;
use measurement::FontMeasurement;
mod code_gutter;
mod code_panel;
mod drag_scroll;
mod html_disclosure;
mod html_edit;
mod html_images;
mod image_state;
mod map_preview;
mod preview_navigation;
mod preview_selection;
use html_edit::{CompositionOrigin, HtmlSelection};
mod accessibility;
mod diagnostics;
#[cfg(test)]
mod diagrams;
mod display_math;
mod inline_math;
mod reflow;
mod resource_batch;
mod search;
mod table_records;
mod table_resize;
#[cfg(feature = "layout-validation")]
mod validation;
pub use diagnostics::{LayoutDiagnosticsReport, LayoutTraceMode};

const KEY_CONTEXT: &str = "RichDocumentEditor";
const MATH_SCROLL_CONTEXT: &str = "MathScrollViewport";
const TABLE_EDGE_CONTEXT: &str = "TableEdgeControl";

fn editor_button(
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    palette: TachyonPalette,
    cx: &App,
) -> Button {
    Button::new(id)
        .small()
        .custom(
            gpui_component::button::ButtonCustomVariant::new(cx)
                .foreground(rgb(palette.text).into())
                .hover(rgb(palette.selection).into())
                .active(rgb(palette.border).into()),
        )
        .icon(Icon::default().path(format!("tachyon/{icon}.svg")))
        .accessible_name(label.split(" — ").next().unwrap_or(label))
        .tooltip(label)
}

fn table_menu_item(
    editor: Entity<RichDocumentEditor>,
    label: &'static str,
    icon: IconName,
    command: EditCommand,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .icon(icon)
        .on_click(move |_, window, cx| {
            editor.update(cx, |editor, cx| {
                editor.apply_structural_command(command.clone(), window, cx);
            });
        })
}

fn table_edge_menu(
    menu: PopupMenu,
    editor: Entity<RichDocumentEditor>,
    target: TableCellTarget,
    edge: TableEdge,
    row_count: usize,
    column_count: usize,
) -> PopupMenu {
    let item = |label, icon, command| table_menu_item(editor.clone(), label, icon, command);
    let menu = match edge {
        TableEdge::Top => menu.item(item(
            "Insert row above",
            IconName::ArrowUp,
            EditCommand::InsertTableRow {
                table_id: target.table_id,
                index: target.row,
            },
        )),
        TableEdge::Right => menu.item(item(
            "Insert column right",
            IconName::ArrowRight,
            EditCommand::InsertTableColumn {
                table_id: target.table_id,
                index: target.column + 1,
            },
        )),
        TableEdge::Bottom => menu.item(item(
            "Insert row below",
            IconName::ArrowDown,
            EditCommand::InsertTableRow {
                table_id: target.table_id,
                index: target.row + 1,
            },
        )),
        TableEdge::Left => menu.item(item(
            "Insert column left",
            IconName::ArrowLeft,
            EditCommand::InsertTableColumn {
                table_id: target.table_id,
                index: target.column,
            },
        )),
    };
    match edge {
        TableEdge::Top if target.row == 0 => menu.separator().item(
            item(
                "Delete column",
                IconName::Delete,
                EditCommand::DeleteTableColumn {
                    table_id: target.table_id,
                    index: target.column,
                },
            )
            .disabled(column_count <= 1),
        ),
        TableEdge::Left if target.column == 0 => menu.separator().item(
            item(
                "Delete row",
                IconName::Delete,
                EditCommand::DeleteTableRow {
                    table_id: target.table_id,
                    index: target.row,
                },
            )
            .disabled(row_count <= 1),
        ),
        _ => menu,
    }
    .min_w(px(208.))
    .max_w(px(208.))
}
const LINE_HEIGHT: f32 = 28.8;
const OUTLINE_JUMP_STEPS: u32 = 15;
const OUTLINE_JUMP_FRAME: Duration = Duration::from_millis(8);
const SHAPED_LINE_CACHE_CAPACITY: usize = 2_048;
const BODY_REFERENCE_COLUMNS: usize = 78;
const CODE_BLOCK_PADDING: f32 = 16.;
const CODE_HEADER_HEIGHT: f32 = 32.;
const ALERT_CONTENT_INSET: f32 = 48.;
const NUMBERED_LIST_EXTRA_GAP: f32 = 8.;
const ALERT_HEADER_HEIGHT: f32 = 48.;
const ALERT_BOTTOM_PADDING: f32 = 20.;
const MIN_ZOOM: f32 = 0.75;
const MAX_ZOOM: f32 = 2.0;
const ZOOM_STEP: f32 = 0.1;
const TOOLBAR_FADE_STEPS: u32 = 8;
const TOOLBAR_FADE_FRAME: Duration = Duration::from_millis(15);
const TOOLBAR_SETTLE_DELAY: Duration = Duration::from_millis(100);
// Exponential velocity decay: retain ~20% of the release velocity at 800 ms
// for a perceptible, gradual tail even on light wheel input. Elapsed-time
// integration keeps travel independent of refresh rate. Wayland supplies raw
// continuous deltas, not trackpad inertia.
const MOMENTUM_DECAY_SECONDS: f32 = 0.50;
// Only a velocity-sampling horizon, never a delay before animation starts.
const MOMENTUM_INPUT_WINDOW: Duration = Duration::from_millis(50);
const MAX_MOMENTUM_DISTANCE: f32 = 2400.;
type SourceImageDimensions = HashMap<u64, (u32, u32)>;
type NodeImageDimensions = HashMap<NodeId, (String, (u32, u32))>;
pub type SharedImageDimensions = Arc<Mutex<(u64, SourceImageDimensions)>>;

#[derive(Debug, PartialEq, Eq)]
enum ClipboardPaste {
    RichMarkdown(String),
    PlainText(String),
}

fn clipboard_paste(item: &ClipboardItem) -> Option<ClipboardPaste> {
    if let Some(rich) = item
        .metadata()
        .and_then(|metadata| RichClipboard::from_json(metadata).ok())
    {
        return Some(ClipboardPaste::RichMarkdown(rich.markdown));
    }

    item.text().map(ClipboardPaste::PlainText)
}

/// CPU-only projection and visual geometry that can be prepared away from the
/// GPUI thread, then installed atomically when a document finishes loading.
pub struct PreparedDocumentView {
    projection: TextProjection,
    visual_lines: Arc<Vec<VisualLineSpec>>,
    paint_order: Arc<Vec<usize>>,
    document_height: f32,
    adaptive: AdaptivePlan,
    components: Arc<ComponentIndex>,
    published_geometry: Option<Arc<PublishedGeometry>>,
    recovery: Option<reflow::Failed>,
}

type ReflowOutput = (
    PreparedDocumentView,
    NodeImageDimensions,
    Option<LayoutDiagnosticsReport>,
);

struct ReflowControl<'a> {
    deadline: &'a reflow::Deadline,
    recovery: Option<&'a reflow::Recovery<ReflowOutput>>,
}

#[derive(Clone)]
struct ReflowViewport {
    published_geometry: Option<Arc<PublishedGeometry>>,
    width: f32,
    height: f32,
    zoom: f32,
    preview_edit_node: Option<NodeId>,
    expanded_code_tail: Option<NodeId>,
    editing_node: Option<NodeId>,
    table_layout_lock: Option<crate::projection::TableLayoutLock>,
    html_disclosures: Arc<rustc_hash::FxHashMap<NodeId, crate::html::DisclosureState>>,
    html_loaded_images: html_images::LoadedImages,
    trace_mode: LayoutTraceMode,
    visible_roots: Option<Range<usize>>,
    resource_generation: u64,
}

impl PreparedDocumentView {
    #[must_use]
    pub fn prepare(document: &Document) -> Self {
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let adaptive = AdaptivePlan::build(&projection, 760., None, false);
        let visual_lines =
            build_arranged_visual_lines(&projection, &HashMap::new(), 760., &adaptive);
        let paint_order = visual_line_paint_order(&visual_lines);
        let document_height = visual_document_height(&visual_lines);
        let components = component_geometry(&projection, &visual_lines, 760., 1., &paint_order);
        Self {
            projection,
            visual_lines: Arc::new(visual_lines),
            paint_order: Arc::new(paint_order),
            document_height,
            adaptive,
            components: Arc::new(components),
            published_geometry: None,
            recovery: None,
        }
    }

    #[cfg(test)]
    fn prepare_snapshot_with_images(
        snapshot: &DocumentSnapshot,
        source_dimensions: &SourceImageDimensions,
        document_directory: Option<&std::path::Path>,
        viewport: ReflowViewport,
        previous: &AdaptivePlan,
        measurement: &FontMeasurement,
    ) -> ReflowOutput {
        Self::try_prepare_snapshot_with_images(
            snapshot,
            source_dimensions,
            document_directory,
            viewport,
            previous,
            measurement,
            ReflowControl {
                deadline: &reflow::Deadline::unlimited(),
                recovery: None,
            },
        )
        .expect("synchronous fixture preparation")
    }

    fn try_prepare_snapshot_with_images(
        snapshot: &DocumentSnapshot,
        source_dimensions: &SourceImageDimensions,
        document_directory: Option<&std::path::Path>,
        viewport: ReflowViewport,
        previous: &AdaptivePlan,
        measurement: &FontMeasurement,
        control: ReflowControl<'_>,
    ) -> Result<ReflowOutput, reflow::Failed> {
        let deadline = control.deadline;
        deadline.check()?;
        let ReflowViewport {
            published_geometry,
            width: layout_width,
            height: viewport_height,
            zoom: zoom_factor,
            preview_edit_node,
            expanded_code_tail,
            editing_node,
            table_layout_lock,
            html_disclosures,
            html_loaded_images,
            trace_mode,
            visible_roots,
            resource_generation,
        } = viewport;
        let mut trace = diagnostics::WorkerTrace::start(trace_mode);
        let mut projection = TextProjection::from_snapshot(snapshot);
        if editing_node.is_some() {
            projection.retain_reading_modes(&previous.reading_modes);
            projection.retain_quote_roles(&previous.quote_roles, editing_node);
            projection.retain_bibliography(&previous.bibliography, editing_node);
            projection.retain_value_roles(&previous.editorials, editing_node);
        }
        projection.preview_edit_node = preview_edit_node;
        projection.expanded_code_tail = expanded_code_tail;
        projection.command_strip_lock =
            editing_node.map(|id| (id, previous.command_strips.get(&id).copied()));
        projection.install_table_layout_lock(table_layout_lock);
        projection.html_disclosures = html_disclosures;
        projection.retain_html_disclosures();
        html_images::bind(&mut projection, &html_loaded_images, document_directory);
        if let Some(trace) = &mut trace {
            trace.stage("projection");
        }
        deadline.check()?;
        let active_tables = visible_roots.as_ref().map(|scope| {
            let mut ranges = previous.windows_for(scope);
            if let Some(ordinal) = editing_node
                .and_then(|node| projection.segment_for_node(node))
                .and_then(|s| previous.root_ordinal(s.top_level_node_id))
            {
                ranges.extend(previous.windows_for(&(ordinal..ordinal + 1)));
            }
            let active_roots = projection
                .roots()
                .enumerate()
                .filter(|(ordinal, _)| ranges.iter().any(|range| range.contains(ordinal)))
                .map(|(_, root)| root.id())
                .collect::<HashSet<_>>();
            // Windows own top-level groups, while width measurements are
            // keyed by the actual table, including quote/list/alert descendants.
            projection
                .segments()
                .iter()
                .filter(|segment| active_roots.contains(&segment.top_level_node_id))
                .filter_map(|segment| segment.context.table_cell.map(|(id, _, _)| id))
                .collect::<HashSet<_>>()
        });
        measurement.measure_tables_in_scope(&mut projection, active_tables.as_ref());
        if let Some(trace) = &mut trace {
            trace.stage("table_measurement");
        }
        deadline.check()?;
        let image_dimensions =
            bind_image_dimensions(&projection, source_dimensions, document_directory);
        if let Some(trace) = &mut trace {
            trace.stage("loaded_image_dimensions");
        }
        deadline.check()?;
        let stack_geometry =
            if let Some(recovery) = control.recovery.filter(|_| editing_node.is_none()) {
                let stack_plan =
                    AdaptivePlan::build(&projection, layout_width / zoom_factor, None, false);
                let input = GeometryInputs {
                    projection: &projection,
                    images: &image_dimensions,
                    plan: &stack_plan,
                    width: layout_width,
                    zoom: zoom_factor,
                    measurement,
                };
                let previous_stack = published_geometry
                    .as_ref()
                    .map(|geometry| geometry.stack.as_ref().unwrap_or(geometry));
                let geometry = match previous_stack {
                    Some(geometry) if geometry.matches(&input) => geometry.clone(),
                    previous => Arc::new(PublishedGeometry::build_with_previous(
                        input,
                        previous.map(AsRef::as_ref),
                    )),
                };
                recovery.store((
                    Self {
                        projection: projection.clone(),
                        visual_lines: geometry.lines.clone(),
                        paint_order: geometry.paint_order.clone(),
                        document_height: geometry.height,
                        adaptive: stack_plan,
                        components: geometry.components.clone(),
                        published_geometry: Some(geometry.clone()),
                        recovery: Some(reflow::Failed::StackTimedOut),
                    },
                    image_dimensions.clone(),
                    None,
                ));
                deadline.check()?;
                if let Some(trace) = &mut trace {
                    trace.stage("recovery_stack");
                }
                Some(geometry)
            } else {
                None
            };
        // The optimizer is optional. It borrows immutable source/projection;
        // discard all partial candidates on unwind, then render a fresh stack.
        // Rendering, table measurement and cancellation still use the outer
        // worker supervisor rather than re-entering their own failure paths.
        let planned = reflow::prepare(|| {
            #[cfg(any(test, feature = "layout-validation"))]
            if deadline.fail_planner {
                panic!("synthetic adaptive planner failure");
            }
            Ok(build_edit_locked_adaptive_plan(
                &projection,
                layout_width / zoom_factor,
                viewport_height / zoom_factor,
                Some(previous),
                false,
                arrangement::LayoutMeasurement {
                    text: measurement,
                    images: Some(&image_dimensions),
                    scope: visible_roots,
                    resource_generation,
                },
                editing_node,
            ))
        });
        let planner_failed = planned.is_err();
        if editing_node.is_some()
            && let Err(failure) = &planned
        {
            // A recovery composition must not dismantle a focused edit lock.
            // Keep the published view until focus/inputs permit a safe retry.
            return Err(*failure);
        }
        let adaptive = planned.unwrap_or_else(|_| {
            AdaptivePlan::build(&projection, layout_width / zoom_factor, None, false)
        });
        if let Some(trace) = &mut trace {
            trace.stage("planning");
        }
        deadline.check()?;
        let input = GeometryInputs {
            projection: &projection,
            images: &image_dimensions,
            plan: &adaptive,
            width: layout_width,
            zoom: zoom_factor,
            measurement,
        };
        let geometry = match stack_geometry
            .as_ref()
            .filter(|geometry| geometry.matches(&input))
        {
            Some(geometry) => geometry.clone(),
            None => match published_geometry {
                Some(geometry) if geometry.matches(&input) => {
                    diagnostics::count(|counts| counts.published_geometry_reuses += 1);
                    geometry
                }
                previous => {
                    let mut geometry =
                        PublishedGeometry::build_with_previous(input, previous.as_deref());
                    geometry.stack = stack_geometry;
                    Arc::new(geometry)
                }
            },
        };
        if let Some(trace) = &mut trace {
            trace.stage("geometry_and_rendered_extensions");
        }
        deadline.check()?;
        let report = trace.map(|trace| {
            let mut report = trace.finish(
                &adaptive,
                previous,
                snapshot.blocks().len(),
                projection.segments().len(),
                projection
                    .image_segments()
                    .count()
                    .saturating_sub(image_dimensions.len()),
            );
            report.planner_failed = planner_failed;
            report.original_source_bytes = snapshot.source_spine().original().len();
            report.attach_rendered_bounds(
                snapshot,
                &geometry.components,
                layout_width,
                zoom_factor,
            );
            report
        });
        deadline.check()?;
        Ok((
            Self {
                projection,
                visual_lines: geometry.lines.clone(),
                paint_order: geometry.paint_order.clone(),
                document_height: geometry.height,
                adaptive,
                components: geometry.components.clone(),
                published_geometry: Some(geometry),
                recovery: planner_failed.then_some(reflow::Failed::PlannerPanicked),
            },
            image_dimensions,
            report,
        ))
    }

    #[must_use]
    pub fn line_count(&self) -> usize {
        self.visual_lines.len()
    }

    /// Applies a non-structural text-node snapshot update without rebuilding
    /// unrelated projection or layout data.
    pub fn refresh_text_node(
        &mut self,
        snapshot: &document_core::DocumentSnapshot,
        node_id: NodeId,
    ) -> bool {
        if self.adaptive.slots.contains_key(&node_id)
            || self.adaptive.lead == Some(node_id)
            || self.adaptive.label_rows.contains_key(&node_id)
        {
            return false;
        }
        self.published_geometry = None;
        let Some(refresh) = refresh_text_node_geometry(
            &mut self.projection,
            Arc::make_mut(&mut self.visual_lines),
            Arc::make_mut(&mut self.paint_order),
            &mut self.document_height,
            TextRefreshRequest {
                snapshot,
                node_id,
                image_dimensions: &HashMap::new(),
                layout_width: 760.,
                zoom_factor: 1.,
                measurement: None,
            },
        ) else {
            return false;
        };
        match refresh.components {
            ComponentRefresh::Unchanged => {}
            ComponentRefresh::ShiftSuffixUp {
                old_y_after,
                paint_end,
            } => Arc::make_mut(&mut self.components).shift_suffix_up(
                old_y_after,
                refresh.y_delta,
                paint_end,
            ),
            ComponentRefresh::ReplaceSimpleAndShiftUp(rebase) => {
                Arc::make_mut(&mut self.components).replace_simple_and_shift_up(rebase)
            }
            ComponentRefresh::Rebuild => {
                self.components = Arc::new(component_geometry(
                    &self.projection,
                    &self.visual_lines,
                    760.,
                    1.,
                    &self.paint_order,
                ));
            }
        }
        true
    }
}

#[derive(Clone, Copy)]
struct VisualLineStyle {
    font_size: f32,
    line_height: f32,
    space_above: f32,
    space_below: f32,
}

#[derive(Clone)]
#[repr(C)]
struct VisualLineSpec {
    // Source coordinates stay local to a stable projection segment. Ordinary
    // typing changes only the projection's segment offsets; it never rewrites
    // every following visual line.
    source: LineSourceRange,
    y: f32,
    compact_tree: bool,
    table_cell_first: bool,
    flow_geometry: bool,
    payload: Arc<VisualLinePayload>,
    style: VisualLineStyle,
    inset: f32,
    /// External group separation, excluded from table/card background bounds.
    gap_before: f32,
    x_fraction: f32,
    width_fraction: f32,
}

#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
struct LineSourceRange {
    projection_start: Arc<crate::projection::ProjectionOffset>,
    start: u32,
    end: u32,
}

impl LineSourceRange {
    fn new(projection: &TextProjection, node_id: NodeId, range: Range<usize>) -> Option<Self> {
        let segment = projection.segment_index_for_node(node_id)?;
        let segment = projection.segments().get(segment)?;
        let projection_start = segment.projection_start.get();
        Some(Self {
            projection_start: segment.projection_start.clone(),
            start: u32::try_from(range.start.checked_sub(projection_start)?).ok()?,
            end: u32::try_from(range.end.checked_sub(projection_start)?).ok()?,
        })
    }

    fn projected(&self) -> Range<usize> {
        let start = self.projection_start.get();
        start + self.start as usize..start + self.end as usize
    }

    fn shift_within_chunk(
        &mut self,
        chunk: &Arc<crate::projection::ProjectionOffset>,
        delta: isize,
    ) -> Option<()> {
        if !Arc::ptr_eq(&self.projection_start, chunk) || delta == 0 {
            return Some(());
        }
        self.start = u32::try_from((self.start as usize).checked_add_signed(delta)?).ok()?;
        self.end = u32::try_from((self.end as usize).checked_add_signed(delta)?).ok()?;
        Some(())
    }

    fn normalize_for_cache(&mut self, projection_local_start: usize) -> Option<()> {
        let local = u32::try_from(projection_local_start).ok()?;
        self.start = self.start.checked_sub(local)?;
        self.end = self.end.checked_sub(local)?;
        Some(())
    }

    fn rebind_from_cache(&mut self, segment: &crate::ProjectionSegment) -> Option<()> {
        let local = u32::try_from(segment.projection_local_start()).ok()?;
        self.start = self.start.checked_add(local)?;
        self.end = self.end.checked_add(local)?;
        self.projection_start = segment.projection_start.clone();
        Some(())
    }

    #[cfg(test)]
    fn absolute(range: Range<usize>) -> Self {
        Self {
            projection_start: Arc::new(crate::projection::ProjectionOffset::new(0)),
            start: u32::try_from(range.start).expect("test line start fits u32"),
            end: u32::try_from(range.end).expect("test line end fits u32"),
        }
    }
}

#[derive(Clone, Default)]
struct VisualLinePayload {
    html_preview: Option<Arc<crate::html::HtmlPreview>>,
    display_math: Option<Arc<crate::math::BlockFormula>>,
    diagram: Option<Arc<crate::diagram::BlockDiagram>>,
    inline_math: Option<Arc<inline_math::InlineLine>>,
    table_cell: Option<(NodeId, usize, usize, usize)>,
    table_record: Option<table_records::Geometry>,
    record_label: Option<Arc<table_records::Label>>,
    table_row_y: f32,
    table_row_height: f32,
    slot: Option<LayoutSlot>,
    label_row: Option<(label_rows::Part, f32)>,
    code_line: Option<code_gutter::CodeLine>,
}

impl VisualLinePayload {
    fn is_empty(&self) -> bool {
        self.html_preview.is_none()
            && self.display_math.is_none()
            && self.diagram.is_none()
            && self.inline_math.is_none()
            && self.table_cell.is_none()
            && self.table_record.is_none()
            && self.record_label.is_none()
            && self.table_row_y == 0.
            && self.table_row_height == 0.
            && self.slot.is_none()
            && self.label_row.is_none()
            && self.code_line.is_none()
    }
}

thread_local! {
    static EMPTY_VISUAL_LINE_PAYLOAD: Arc<VisualLinePayload> =
        Arc::new(VisualLinePayload::default());
}

fn visual_line_payload(payload: VisualLinePayload) -> Arc<VisualLinePayload> {
    if payload.is_empty() {
        EMPTY_VISUAL_LINE_PAYLOAD.with(Arc::clone)
    } else {
        Arc::new(payload)
    }
}

impl std::ops::Deref for VisualLineSpec {
    type Target = VisualLinePayload;

    fn deref(&self) -> &Self::Target {
        &self.payload
    }
}

impl std::ops::DerefMut for VisualLineSpec {
    fn deref_mut(&mut self) -> &mut Self::Target {
        self.payload_mut()
    }
}

impl VisualLineSpec {
    fn projected_range(&self) -> Range<usize> {
        self.source.projected()
    }

    fn projected_start(&self) -> usize {
        self.projected_range().start
    }

    fn projected_end(&self) -> usize {
        self.projected_range().end
    }

    fn set_projected_range(&mut self, range: Range<usize>) -> Option<()> {
        let projection_start = self.source.projection_start.get();
        self.source.start = u32::try_from(range.start.checked_sub(projection_start)?).ok()?;
        self.source.end = u32::try_from(range.end.checked_sub(projection_start)?).ok()?;
        Some(())
    }

    fn normalize_for_cache(&mut self, projection_local_start: usize) -> Option<()> {
        self.source.normalize_for_cache(projection_local_start)
    }

    fn rebind_from_cache(&mut self, segment: &crate::ProjectionSegment) -> Option<()> {
        self.source.rebind_from_cache(segment)
    }

    fn payload_mut(&mut self) -> &mut VisualLinePayload {
        Arc::make_mut(&mut self.payload)
    }

    fn set_slot(&mut self, slot: Option<LayoutSlot>) {
        self.flow_geometry = slot.is_some() || self.table_cell.is_some();
        self.payload_mut().slot = slot;
    }
}

impl VisualLineSpec {
    fn command_strip(&self) -> bool {
        self.code_line.is_some_and(|code| code.strip)
    }

    fn code_header_height(&self) -> f32 {
        if self.command_strip() {
            0.
        } else {
            CODE_HEADER_HEIGHT
        }
    }

    fn command_trailing(&self, zoom: f32) -> f32 {
        if self.command_strip() {
            code_panel::STRIP_TRAILING * zoom
        } else {
            0.
        }
    }

    fn rendered_code_preview(&self) -> bool {
        self.display_math
            .as_ref()
            .is_some_and(|math| !math.source_visible)
            || self
                .diagram
                .as_ref()
                .is_some_and(|diagram| !diagram.source_visible)
    }

    fn preview_extent(&self) -> f32 {
        self.display_math.as_ref().map_or_else(
            || self.diagram.as_ref().map_or(0., |d| d.extent()),
            |math| math.extent(),
        )
    }

    fn preview_image(&self, dark: bool) -> Option<PreviewImage<'_>> {
        if let Some(math) = &self.display_math {
            let image = math.for_dark(dark);
            return Some(PreviewImage {
                image: &image.image,
                width: image.width,
                height: image.height,
                baseline: image.baseline,
            });
        }
        let image = self.diagram.as_ref()?.for_dark(dark);
        Some(PreviewImage {
            image: &image.image,
            width: image.width,
            height: image.height,
            baseline: image.height,
        })
    }

    fn code_gutter(&self) -> f32 {
        self.code_line.map_or(0., |code| code.width)
    }
}

struct PreviewImage<'a> {
    image: &'a Arc<gpui::Image>,
    width: f32,
    height: f32,
    baseline: f32,
}

impl VisualLineStyle {
    const BODY: Self = Self {
        font_size: DocumentStyle::REFERENCE_SIZE,
        line_height: DocumentStyle::REFERENCE_LEADING,
        space_above: 0.,
        space_below: 16.,
    };
}

actions!(
    rich_document_editor,
    [
        Backspace,
        Delete,
        Left,
        Right,
        WordLeft,
        WordRight,
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectWordLeft,
        SelectWordRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        SelectHome,
        SelectEnd,
        DocumentStart,
        DocumentEnd,
        SelectDocumentStart,
        SelectDocumentEnd,
        Copy,
        Cut,
        Paste,
        PasteAsMarkdown,
        Undo,
        Redo,
        Save,
        InspectLayout,
        FormatBold,
        FormatItalic,
        FormatStrike,
        FormatCode,
        FormatLink,
        OpenLink,
        FormatParagraph,
        FormatHeading1,
        FormatHeading2,
        FormatHeading3,
        FormatQuote,
        InsertOrderedList,
        InsertBulletedList,
        Enter,
        HardBreak,
        NextTableCell,
        PreviousTableCell,
        ExitTable,
        ActivateTableEdge,
        NextTableEdgeControl,
        PreviousTableEdgeControl,
        Dismiss,
    ]
);

actions!(
    math_scroll_viewport,
    [
        ScrollMathLeft,
        ScrollMathRight,
        ScrollMathStart,
        ScrollMathEnd,
        ExitMathScroll,
        EditMathSource,
    ]
);

pub fn init(cx: &mut App) {
    #[cfg(feature = "layout-validation")]
    validation::init(cx);
    html_disclosure::init_controls(cx);
    if !cx.has_global::<Theme>() {
        gpui_component::init(cx);
    }
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(KEY_CONTEXT)),
        KeyBinding::new("delete", Delete, Some(KEY_CONTEXT)),
        KeyBinding::new("left", Left, Some(KEY_CONTEXT)),
        KeyBinding::new("right", Right, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-left", WordLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-right", WordRight, Some(KEY_CONTEXT)),
        KeyBinding::new("up", Up, Some(KEY_CONTEXT)),
        KeyBinding::new("down", Down, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-left", SelectWordLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-right", SelectWordRight, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-up", SelectUp, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-down", SelectDown, Some(KEY_CONTEXT)),
        KeyBinding::new("home", Home, Some(KEY_CONTEXT)),
        KeyBinding::new("end", End, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-home", SelectHome, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-end", SelectEnd, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-home", DocumentStart, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-end", DocumentEnd, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-home", SelectDocumentStart, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-end", SelectDocumentEnd, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-a", SelectAll, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-c", Copy, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-x", Cut, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-v", Paste, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-v", PasteAsMarkdown, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-z", Undo, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-z", Redo, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-s", Save, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-alt-shift-l", InspectLayout, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-b", FormatBold, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-i", FormatItalic, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-x", FormatStrike, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-e", FormatCode, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-k", FormatLink, Some(KEY_CONTEXT)),
        KeyBinding::new("alt-enter", OpenLink, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-alt-0", FormatParagraph, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-alt-1", FormatHeading1, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-alt-2", FormatHeading2, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-alt-3", FormatHeading3, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-alt-4", InsertOrderedList, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-alt-5", InsertBulletedList, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-alt-q", FormatQuote, Some(KEY_CONTEXT)),
        KeyBinding::new("enter", Enter, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-enter", HardBreak, Some(KEY_CONTEXT)),
        KeyBinding::new("tab", NextTableCell, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-tab", PreviousTableCell, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-enter", ExitTable, Some(KEY_CONTEXT)),
        KeyBinding::new("escape", Dismiss, Some(KEY_CONTEXT)),
        KeyBinding::new("left", ScrollMathLeft, Some(MATH_SCROLL_CONTEXT)),
        KeyBinding::new("right", ScrollMathRight, Some(MATH_SCROLL_CONTEXT)),
        KeyBinding::new("home", ScrollMathStart, Some(MATH_SCROLL_CONTEXT)),
        KeyBinding::new("end", ScrollMathEnd, Some(MATH_SCROLL_CONTEXT)),
        KeyBinding::new("escape", ExitMathScroll, Some(MATH_SCROLL_CONTEXT)),
        KeyBinding::new("enter", EditMathSource, Some(MATH_SCROLL_CONTEXT)),
        KeyBinding::new("enter", ActivateTableEdge, Some(TABLE_EDGE_CONTEXT)),
        KeyBinding::new("space", ActivateTableEdge, Some(TABLE_EDGE_CONTEXT)),
        KeyBinding::new("tab", NextTableEdgeControl, Some(TABLE_EDGE_CONTEXT)),
        KeyBinding::new(
            "shift-tab",
            PreviousTableEdgeControl,
            Some(TABLE_EDGE_CONTEXT),
        ),
    ]);
}

#[derive(Clone, Debug)]
pub enum EditorEvent {
    Ready,
    Changed,
    ViewChanged,
    SaveRequested,
    OpenLocalDocument {
        path: PathBuf,
        fragment: Option<String>,
    },
    LinkFailed(String),
    LayoutDiagnostics(Arc<LayoutDiagnosticsReport>),
    RetryImage {
        source: String,
        document_directory: Option<PathBuf>,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorViewState {
    pub selection: Range<usize>,
    pub reversed: bool,
    pub scroll_y: f32,
    pub scroll_anchor: Option<EditorScrollAnchor>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct EditorScrollAnchor {
    pub node_id: NodeId,
    pub node_text_hint: String,
    pub node_text_offset: usize,
    pub projection_offset: usize,
    pub intra_line_offset: f32,
}

#[derive(Clone)]
struct PaintedLine {
    range: Range<usize>,
    layout: ShapedLine,
    bounds: Bounds<Pixels>,
    line_height: Pixels,
    horizontal_owner: Option<NodeId>,
    content_mask: Option<ContentMask<Pixels>>,
    alignment: ColumnAlignment,
}

#[derive(Clone, Debug, PartialEq)]
struct SemanticNodeSpec {
    node_id: NodeId,
    role: Role,
    label: String,
    image_link: Option<String>,
    resource_link: Option<String>,
    math_markup: Option<Arc<crate::math::semantics::MathMarkup>>,
    level: Option<usize>,
    row_index: Option<usize>,
    column_index: Option<usize>,
    toggled: Option<Toggled>,
    bounds: SemanticBounds,
    children: Vec<SemanticNodeSpec>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct SemanticBounds {
    x_fraction: f32,
    width_fraction: f32,
    y: f32,
    height: f32,
}

impl SemanticBounds {
    fn union(self, other: Self) -> Self {
        let left = self.x_fraction.min(other.x_fraction);
        let right =
            (self.x_fraction + self.width_fraction).max(other.x_fraction + other.width_fraction);
        let top = self.y.min(other.y);
        let bottom = (self.y + self.height).max(other.y + other.height);
        Self {
            x_fraction: left,
            width_fraction: (right - left).max(0.),
            y: top,
            height: (bottom - top).max(0.),
        }
    }
}

struct MaskedQuad {
    quad: PaintQuad,
    content_mask: Option<ContentMask<Pixels>>,
}

fn append_alert_chrome(
    chrome: &mut Vec<MaskedQuad>,
    bounds: Bounds<Pixels>,
    signal: crate::theme::SignalStyle,
    zoom: f32,
) {
    // One native rounded quad owns the fill and all four edges. A separate
    // full-height accent rail protrudes past the circular corner silhouette.
    chrome.push(MaskedQuad {
        quad: fill(bounds, rgb(signal.paper))
            .corner_radii(px(DocumentStyle::RADIUS * zoom))
            .border_color(rgb(signal.rule))
            .border_widths(gpui::Edges {
                top: px(zoom),
                right: px(zoom),
                bottom: px(zoom),
                left: px(zoom),
            }),
        content_mask: None,
    });
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ShapeCacheKey {
    node_id: Option<NodeId>,
    node_revision: Revision,
    fragment: Range<usize>,
    font_fingerprint: u64,
    font_size_bits: u32,
    width_bits: u32,
    scale_bits: u32,
    marked_fragment: Option<Range<usize>>,
    typography: (bool, bool),
}

struct ShapeCacheInput<'a> {
    snapshot: &'a document_core::DocumentSnapshot,
    segment: Option<&'a crate::ProjectionSegment>,
    range: &'a Range<usize>,
    runs: &'a [TextRun],
    palette: TachyonPalette,
    font_size: f32,
    width: f32,
    scale: f32,
    marked: Option<&'a Range<usize>>,
}

struct BoundedLru<K, V> {
    entries: HashMap<K, (V, u64)>,
    capacity: usize,
    clock: u64,
}

impl<K: Clone + Eq + std::hash::Hash, V: Clone> BoundedLru<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            entries: HashMap::new(),
            capacity: capacity.max(1),
            clock: 0,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        self.clock = self.clock.wrapping_add(1);
        let (value, last_used) = self.entries.get_mut(key)?;
        *last_used = self.clock;
        Some(value.clone())
    }

    fn insert(&mut self, key: K, value: V) {
        self.clock = self.clock.wrapping_add(1);
        if self.entries.len() >= self.capacity
            && !self.entries.contains_key(&key)
            && let Some(oldest) = self
                .entries
                .iter()
                .min_by_key(|(_, (_, last_used))| *last_used)
                .map(|(key, _)| key.clone())
        {
            self.entries.remove(&oldest);
        }
        self.entries.insert(key, (value, self.clock));
    }

    fn clear(&mut self) {
        self.entries.clear();
    }

    fn retain(&mut self, mut keep: impl FnMut(&K) -> bool) {
        self.entries.retain(|key, _| keep(key));
    }
}

#[derive(Clone, Copy)]
struct TableResizeDrag {
    table_id: NodeId,
    column: usize,
    pointer_x: Pixels,
    edge_x: Pixels,
    initial_width: f32,
    current_width: f32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct TableCellTarget {
    table_id: NodeId,
    row: usize,
    column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TableEdge {
    Top,
    Right,
    Bottom,
    Left,
}

#[derive(Clone, Copy)]
struct TableSelectionDrag {
    table_id: NodeId,
    anchor_row: usize,
    anchor_column: usize,
}

pub struct RichDocumentEditor {
    semantic_cache: accessibility::SemanticCache,
    document: SharedDocumentSession,
    selection: Selection,
    projection: TextProjection,
    measurement: Arc<FontMeasurement>,
    measured_layout: bool,
    // A settled selection may defer optional recomposition, but not correction
    // of line measures invalidated by a reader's font or zoom change.
    text_environment_pending: bool,
    projected_generation: u64,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    math_scroll_handles: HashMap<NodeId, ScrollHandle>,
    link_input: Entity<InputState>,
    find: search::FindState,
    link_popover_visible: bool,
    image_source_input: Entity<InputState>,
    image_alt_input: Entity<InputState>,
    image_popover_node: Option<NodeId>,
    document_directory: Option<PathBuf>,
    shared_image_dimensions: Option<SharedImageDimensions>,
    html_image_loader: Option<gpui::AnyImageCache>,
    html_image_cache: html_images::HtmlImageCache,
    image_layout_dimensions: NodeImageDimensions,
    image_dimensions_generation: u64,
    requested_image_dimensions_generation: u64,
    image_resource_batch: resource_batch::ResourceBatch,
    image_resource_wake: Option<Task<()>>,
    layout_width: f32,
    requested_layout_width: f32,
    reflow: reflow::State,
    layout_trace_mode: LayoutTraceMode,
    layout_trace_sequence: u64,
    layout_trace_committed: u64,
    layout_trace_discarded: u64,
    layout_trace_requested: bool,
    marked_range: Option<Range<usize>>,
    html_selection: Option<HtmlSelection>,
    disclosure_anchor: Option<html_disclosure::DisclosureAnchor>,
    html_anchor_jump: Option<html_disclosure::HtmlAnchorJump>,
    /// Ownership of session preedit, with an HTML caret to restore on cancel.
    composition_origin: Option<CompositionOrigin>,
    visual_lines: Arc<Vec<VisualLineSpec>>,
    paint_order: Arc<Vec<usize>>,
    document_height: f32,
    geometry_generation: u64,
    painted_lines: Vec<PaintedLine>,
    preferred_x: Option<Pixels>,
    element_bounds: Option<Bounds<Pixels>>,
    painted_viewport_height: f32,
    is_selecting: bool,
    drag_scroll: Option<drag_scroll::DragScroll>,
    drag_scroll_generation: u64,
    table_resize_drag: Option<TableResizeDrag>,
    table_selection_drag: Option<TableSelectionDrag>,
    table_hover: Option<TableCellTarget>,
    table_edge_menu: Option<(Entity<PopupMenu>, Point<Pixels>, Subscription)>,
    toolbar_visible: bool,
    toolbar_opacity: f32,
    toolbar_animation_generation: u64,
    toolbar_animation_task: Option<Task<()>>,
    jump_generation: u64,
    momentum_remaining: f32,
    momentum_last_input: Option<Instant>,
    momentum_frame_time: Option<Instant>,
    momentum_generation: u64,
    momentum_extrapolated: Option<f32>,
    horizontal_scrolls: HashMap<NodeId, f32>,
    horizontal_metrics: HashMap<NodeId, (f32, f32)>,
    shaped_line_cache: RefCell<BoundedLru<ShapeCacheKey, ShapedLine>>,
    copied_code: Option<(NodeId, Instant)>,
    last_error: Option<String>,
    has_painted: bool,
    zoom_factor: f32,
    requested_zoom_factor: f32,
    typography: typography::Options,
    adaptive: AdaptivePlan,
    components: Arc<ComponentIndex>,
    published_geometry: Option<Arc<PublishedGeometry>>,
    last_edit_at: Option<Instant>,
    layout_resume_task: Option<Task<()>>,
    layout_focus: Option<NodeId>,
    layout_replan_pending: bool,
}

impl RichDocumentEditor {
    #[must_use]
    pub fn new(document: Document, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let prepared = PreparedDocumentView::prepare(&document);
        let projected_generation = 0;
        Self::with_session_parts(
            SharedDocumentSession::new(document),
            projected_generation,
            prepared,
            window,
            cx,
        )
    }

    #[must_use]
    pub fn with_session(
        document: SharedDocumentSession,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let prepared = document.with_document(PreparedDocumentView::prepare);
        let projected_generation = document.generation();
        Self::with_session_parts(document, projected_generation, prepared, window, cx)
    }

    fn with_session_parts(
        document: SharedDocumentSession,
        projected_generation: u64,
        prepared: PreparedDocumentView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let selection = document.snapshot().selection().clone();
        let link_input = cx.new(|cx| InputState::new(window, cx).placeholder("Link destination"));
        let find = search::FindState::new(window, cx);
        let image_source_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Image source"));
        let image_alt_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Alternative text"));
        let focus_handle = cx.focus_handle();
        cx.on_blur(&focus_handle, window, |editor, _, cx| {
            if editor.table_resize_drag.take().is_some() {
                cx.notify();
            }
            editor.is_selecting = false;
            editor.stop_drag_scroll();
            if editor.layout_focus.take().is_some() {
                editor.projection.table_layout_lock = None;
                editor.measured_layout = false;
                editor.layout_replan_pending = true;
                editor.geometry_generation = editor.geometry_generation.wrapping_add(1);
                cx.notify();
            }
        })
        .detach();
        cx.on_focus(&focus_handle, window, |_, _, cx| cx.notify())
            .detach();
        Self {
            semantic_cache: Default::default(),
            document,
            selection,
            projection: prepared.projection,
            measurement: Arc::new(FontMeasurement::new(
                cx.text_system().clone(),
                cx.theme().font_family.clone(),
                1.,
            )),
            measured_layout: false,
            text_environment_pending: true,
            projected_generation,
            focus_handle,
            scroll_handle: ScrollHandle::new(),
            math_scroll_handles: HashMap::new(),
            link_input,
            find,
            link_popover_visible: false,
            image_source_input,
            image_alt_input,
            image_popover_node: None,
            document_directory: None,
            shared_image_dimensions: None,
            html_image_loader: None,
            html_image_cache: html_images::HtmlImageCache::default(),
            image_layout_dimensions: HashMap::new(),
            image_dimensions_generation: 0,
            requested_image_dimensions_generation: 0,
            image_resource_batch: Default::default(),
            image_resource_wake: None,
            layout_width: 760.,
            requested_layout_width: 760.,
            reflow: reflow::State::default(),
            layout_trace_mode: LayoutTraceMode::Off,
            layout_trace_sequence: 0,
            layout_trace_committed: 0,
            layout_trace_discarded: 0,
            layout_trace_requested: false,
            marked_range: None,
            html_selection: None,
            disclosure_anchor: None,
            html_anchor_jump: None,
            composition_origin: None,
            visual_lines: prepared.visual_lines,
            paint_order: prepared.paint_order,
            document_height: prepared.document_height,
            geometry_generation: 1,
            painted_lines: Vec::new(),
            preferred_x: None,
            element_bounds: None,
            painted_viewport_height: 0.,
            is_selecting: false,
            drag_scroll: None,
            drag_scroll_generation: 0,
            table_resize_drag: None,
            table_selection_drag: None,
            table_hover: None,
            table_edge_menu: None,
            toolbar_visible: false,
            toolbar_opacity: 0.,
            toolbar_animation_generation: 0,
            toolbar_animation_task: None,
            jump_generation: 0,
            momentum_remaining: 0.,
            momentum_last_input: None,
            momentum_frame_time: None,
            momentum_generation: 0,
            momentum_extrapolated: None,
            horizontal_scrolls: HashMap::new(),
            horizontal_metrics: HashMap::new(),
            shaped_line_cache: RefCell::new(BoundedLru::new(SHAPED_LINE_CACHE_CAPACITY)),
            copied_code: None,
            last_error: None,
            has_painted: false,
            zoom_factor: 1.,
            requested_zoom_factor: 1.,
            typography: typography::Options::default(),
            adaptive: prepared.adaptive,
            components: prepared.components,
            published_geometry: prepared.published_geometry,
            last_edit_at: None,
            layout_resume_task: None,
            layout_focus: None,
            layout_replan_pending: false,
        }
    }

    #[must_use]
    pub fn document(&self) -> &SharedDocumentSession {
        &self.document
    }

    #[must_use]
    pub fn shared_session(&self) -> SharedDocumentSession {
        self.document.clone()
    }

    pub fn perform_undo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.undo(&Undo, window, cx);
    }

    pub fn perform_redo(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.redo(&Redo, window, cx);
    }

    #[must_use]
    pub fn last_error(&self) -> Option<&str> {
        self.last_error.as_deref().or_else(|| {
            self.reflow.error(
                self.document.id(),
                self.document.generation(),
                self.geometry_generation,
            )
        })
    }

    pub fn replace_document(&mut self, document: Document, cx: &mut Context<Self>) {
        let prepared = PreparedDocumentView::prepare(&document);
        self.replace_document_prepared(document, prepared, cx);
    }

    pub fn replace_document_prepared(
        &mut self,
        mut document: Document,
        prepared: PreparedDocumentView,
        cx: &mut Context<Self>,
    ) {
        self.table_resize_drag = None;
        let previous_offset = self.cursor_offset();
        if let Some(position) = prepared.projection.position_at(
            previous_offset.min(prepared.projection.text().len()),
            Affinity::Downstream,
        ) {
            let _ = document.apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(position),
            )));
        }
        let selection = document.snapshot().selection().clone();
        self.document.replace(document);
        self.selection = selection;
        self.install_prepared(prepared);
        self.projected_generation = self.document.generation();
        self.reset_document_view_caches();
        cx.notify();
    }

    pub fn attach_shared_session(
        &mut self,
        document: SharedDocumentSession,
        cx: &mut Context<Self>,
    ) {
        let prepared = document.with_document(PreparedDocumentView::prepare);
        self.attach_shared_session_prepared(document, prepared, cx);
    }

    pub fn attach_shared_session_prepared(
        &mut self,
        document: SharedDocumentSession,
        mut prepared: PreparedDocumentView,
        cx: &mut Context<Self>,
    ) {
        let same_session = self.document.ptr_eq(&document);
        if self.cancel_owned_composition_on_detach() && same_session {
            // A supplied view of our own preedit is obsolete after rollback.
            prepared = document.with_document(PreparedDocumentView::prepare);
        }
        self.document = document;
        self.selection = self.document.snapshot().selection().clone();
        self.install_prepared(prepared);
        self.projected_generation = self.document.generation();
        self.reset_document_view_caches();
        cx.notify();
    }

    fn cancel_owned_composition_on_detach(&mut self) -> bool {
        if self.composition_origin.take().is_some() && self.document.composition_active() {
            // No content event/history entry: unpublished text is abandoned,
            // and the session generation wakes surviving views on their sync.
            return self.document.cancel_composition().is_ok();
        }
        false
    }

    fn install_prepared(&mut self, prepared: PreparedDocumentView) {
        self.projection = prepared.projection;
        self.prune_math_scroll_handles();
        self.visual_lines = prepared.visual_lines;
        self.paint_order = prepared.paint_order;
        self.document_height = prepared.document_height;
        self.adaptive = prepared.adaptive;
        self.components = prepared.components;
        self.published_geometry = prepared.published_geometry;
        self.refresh_html_selection();
        self.geometry_generation = self.geometry_generation.wrapping_add(1);
    }

    pub fn sync_shared_session(&mut self, cx: &mut Context<Self>) -> bool {
        let generation = self.document.generation();
        if generation == self.projected_generation {
            cx.notify();
            return false;
        }
        let previous_selection = self.selected_byte_range();
        self.refresh_projection();
        let snapshot = self.document.snapshot();
        if !selection_is_valid(&snapshot, &self.selection) {
            let (range, reversed) = previous_selection;
            let end = self.projection.text().len();
            let start = self.position_for_offset(range.start.min(end), Affinity::Downstream);
            let finish = self.position_for_offset(range.end.min(end), Affinity::Upstream);
            self.selection = match (start, finish) {
                (Some(start), Some(finish)) if reversed => Selection::Text(TextSelection {
                    anchor: finish,
                    head: start,
                }),
                (Some(start), Some(finish)) => Selection::Text(TextSelection {
                    anchor: start,
                    head: finish,
                }),
                _ => snapshot.selection().clone(),
            };
        }
        self.shaped_line_cache.borrow_mut().clear();
        self.marked_range = None;
        self.toolbar_visible = false;
        self.toolbar_opacity = 0.;
        self.toolbar_animation_task.take();
        self.link_popover_visible = false;
        self.image_popover_node = None;
        cx.notify();
        true
    }

    fn prune_math_scroll_handles(&mut self) {
        self.math_scroll_handles.retain(|id, _| {
            self.projection.block(*id).is_some_and(|block| {
                crate::math::is_math(block) || crate::diagram::is_diagram(block)
            })
        });
    }

    fn reset_document_view_caches(&mut self) {
        self.is_selecting = false;
        self.stop_drag_scroll();
        self.find.invalidate();
        self.html_selection = None;
        self.disclosure_anchor = None;
        self.html_anchor_jump = None;
        self.composition_origin = None;
        self.layout_trace_requested = false;
        self.layout_focus = None;
        self.layout_replan_pending = false;
        self.measured_layout = false;
        self.stop_momentum();
        self.text_environment_pending = true;
        self.marked_range = None;
        self.toolbar_visible = false;
        self.toolbar_opacity = 0.;
        self.toolbar_animation_task.take();
        self.link_popover_visible = false;
        self.image_popover_node = None;
        self.image_layout_dimensions.clear();
        self.image_dimensions_generation = 0;
        self.requested_image_dimensions_generation = 0;
        self.html_image_cache = html_images::HtmlImageCache::default();
        self.image_resource_batch = Default::default();
        self.image_resource_wake = None;
        self.requested_layout_width = self.layout_width;
        self.requested_zoom_factor = self.zoom_factor;
        self.reflow.reset_document();
        self.horizontal_scrolls.clear();
        self.horizontal_metrics.clear();
        self.math_scroll_handles.clear();
        self.shaped_line_cache.borrow_mut().clear();
        self.last_error = None;
        self.has_painted = false;
        self.last_edit_at = None;
        self.layout_resume_task.take();
    }

    pub fn set_document_directory(&mut self, directory: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.document_directory != directory {
            self.html_image_cache = html_images::HtmlImageCache::default();
            self.projection.html_images = Arc::default();
            self.measured_layout = false;
            self.geometry_generation = self.geometry_generation.wrapping_add(1);
        }
        self.document_directory = directory;
        cx.notify();
    }

    /// Enable local diagnostic events. This does not replan, change layout
    /// preferences, or enter content history. The app owns output policy.
    pub fn set_layout_trace_mode(&mut self, mode: LayoutTraceMode) {
        self.layout_trace_mode = mode;
    }

    fn request_layout_trace(&mut self, _: &InspectLayout, _: &mut Window, cx: &mut Context<Self>) {
        if self.layout_trace_mode == LayoutTraceMode::Off {
            return;
        }
        self.layout_trace_requested = true;
        self.layout_replan_pending = true;
        // Preserve source, selection, active edit locks and reading anchor.
        // A request during a job invalidates only that job's eventual commit.
        self.geometry_generation = self.geometry_generation.saturating_add(1);
        cx.notify();
    }

    pub fn set_image_dimensions(
        &mut self,
        dimensions: SharedImageDimensions,
        cx: &mut Context<Self>,
    ) {
        self.shared_image_dimensions = Some(dimensions);
        cx.notify();
    }

    fn visible_planning_roots(&self) -> Option<Range<usize>> {
        let (top, height) = self.scroll_metrics();
        if height <= 0. {
            return Some(0..1);
        }
        let visible = self.components.visible_range(
            &self.visual_lines,
            &self.paint_order,
            (top - height * 0.25).max(0.),
            top + height * 2.,
        );
        let mut first = usize::MAX;
        let mut last = 0;
        for &index in &self.paint_order[visible] {
            if let Some(ordinal) = self
                .projection
                .segment_for_range(&self.visual_lines[index].projected_range())
                .and_then(|s| self.adaptive.root_ordinal(s.top_level_node_id))
            {
                first = first.min(ordinal);
                last = last.max(ordinal + 1);
            }
        }
        (first < last).then_some(first..last)
    }

    fn sync_image_dimensions(&mut self, width: f32, cx: &mut Context<Self>) {
        self.sync_image_dimensions_at(width, Instant::now(), cx);
    }

    fn publish_painted_bounds(&mut self, bounds: Bounds<Pixels>, cx: &mut Context<Self>) {
        let width_changed = self.element_bounds.is_none_or(|old| {
            (f32::from(old.size.width) - f32::from(bounds.size.width)).abs() >= 0.5
        });
        // Element height is the document's height, not its scroll viewport.
        // Read the updated scroll container after layout/paint instead.
        let viewport_height = self.scroll_metrics().1;
        let height_changed = (viewport_height - self.painted_viewport_height).abs() >= 0.5;
        self.painted_viewport_height = viewport_height;
        self.element_bounds = Some(bounds);
        if (width_changed || height_changed)
            && (self.marked_range.is_some() || self.document.composition_active())
        {
            // Composition keeps the last committed line topology stable, but
            // paint already clips those lines to the newly configured window.
            // Reveal against that same painted width until commit may reflow.
            self.keep_offset_visible_at_width(self.cursor_offset(), f32::from(bounds.size.width));
            cx.notify();
        }
        if !self.has_painted {
            self.has_painted = true;
            cx.emit(EditorEvent::Ready);
            cx.notify();
        } else if width_changed || height_changed {
            // Wayland can configure a new size between render and paint.
            // A notification during paint can be consumed by this draw;
            // schedule from the actual bounds now rather than depending
            // on a later pointer event to re-enter Render with that width.
            self.sync_image_dimensions(f32::from(bounds.size.width), cx);
        }
    }

    fn selection_defers_reflow(&self, width: f32, height: f32) -> bool {
        self.is_selecting
            || self.marked_range.is_some()
            || self.document.composition_active()
            || (!self.selected_byte_range().0.is_empty()
                && !self.text_environment_pending
                && (width - self.layout_width).abs() < 0.5
                && (self.adaptive.measured_rows.viewport - height / self.zoom_factor).abs() < 0.5)
    }

    fn sync_image_dimensions_at(&mut self, width: f32, now: Instant, cx: &mut Context<Self>) {
        if let Some(last_edit) = self.last_edit_at
            && last_edit.elapsed() < Duration::from_millis(800)
        {
            if self.layout_resume_task.is_none() {
                let remaining = Duration::from_millis(800).saturating_sub(last_edit.elapsed());
                self.layout_resume_task = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(remaining).await;
                    let _ = this.update(cx, |this, cx| {
                        this.layout_resume_task.take();
                        cx.notify();
                    });
                }));
            }
            return;
        }
        let dimensions = self
            .shared_image_dimensions
            .as_ref()
            .and_then(|dimensions| dimensions.lock().ok());
        let dimensions_generation = dimensions.as_ref().map_or(0, |dimensions| dimensions.0);
        let width = width.max(1.);
        if (width - self.layout_width).abs() >= 0.5 {
            self.table_resize_drag = None;
        }
        let viewport_height = self.scroll_metrics().1;
        // Batch only asynchronous reading/scroll reflow. Synchronous editing
        // refreshes must keep their original viewport + caret work bound.
        let visible_roots = self
            .visible_planning_roots()
            .map(|scope| self.adaptive.planning_batch(scope));
        let environment_current = self.measured_layout
            && !self.text_environment_pending
            && !self.layout_replan_pending
            && (width - self.layout_width).abs() < 0.5
            && (self.requested_zoom_factor - self.zoom_factor).abs() < f32::EPSILON
            && (self.adaptive.measured_rows.viewport - viewport_height / self.zoom_factor).abs()
                < 0.5
            && visible_roots
                .as_ref()
                .is_none_or(|scope| self.adaptive.covers(scope));
        if environment_current && dimensions_generation == self.image_dimensions_generation {
            return;
        }
        self.image_resource_batch
            .observe(dimensions_generation, now);
        // Update the commit guard even while batching: an in-flight result
        // must not publish old dimensions while newer resources are pending.
        self.requested_image_dimensions_generation = dimensions_generation;
        self.requested_layout_width = width;
        self.requested_zoom_factor = self.zoom_factor;
        if self.reflow.is_active() {
            return;
        }
        if environment_current && let Some(delay) = self.image_resource_batch.delay(now) {
            if self.image_resource_wake.is_none() {
                self.image_resource_wake = Some(cx.spawn(async move |this, cx| {
                    cx.background_executor().timer(delay).await;
                    let _ = this.update(cx, |this, cx| {
                        this.image_resource_wake = None;
                        cx.notify();
                    });
                }));
            }
            return;
        }
        if self.selection_defers_reflow(width, viewport_height) {
            return;
        }
        let key = reflow::Key {
            session: self.document.id(),
            document: self.document.generation(),
            geometry: self.geometry_generation,
            width: width.to_bits(),
            height: viewport_height.to_bits(),
            zoom: self.zoom_factor.to_bits(),
            resources: dimensions_generation,
            focus: self.layout_focus,
        };
        let Some(ticket) = self.reflow.begin(key) else {
            return;
        };
        // Arming does not manipulate focus. Consume only after a real blur
        // permits an ordinary reading-layout reflow (the harness opens Find).
        #[cfg(feature = "layout-validation")]
        let native_fault = self
            .layout_focus
            .is_none()
            .then(|| self.reflow.native_fault.take())
            .flatten();
        #[cfg(feature = "layout-validation")]
        let native_hold = (native_fault == Some(validation::Fault::Timeout)).then(|| {
            cx.background_executor()
                .timer(reflow::TIMEOUT + Duration::from_secs(8))
        });
        #[cfg(test)]
        let fail_worker = std::mem::take(&mut self.reflow.fail_next);
        #[cfg(test)]
        let hold_worker = self.reflow.hold_next.take();
        let deadline = reflow::Deadline::new();
        #[cfg(test)]
        let deadline = {
            let mut deadline = deadline;
            deadline.fail_planner = std::mem::take(&mut self.reflow.fail_next_planner);
            deadline
        };
        let worker_deadline = deadline.clone();
        #[cfg(feature = "layout-validation")]
        let worker_deadline = {
            let mut deadline = worker_deadline;
            deadline.fail_planner |= native_fault == Some(validation::Fault::Panic);
            deadline
        };
        let dispatch_start = (self.layout_trace_mode != LayoutTraceMode::Off).then(Instant::now);
        let source_dimensions = dimensions
            .as_ref()
            .map(|dimensions| dimensions.1.clone())
            .unwrap_or_default();
        drop(dimensions);

        self.image_resource_batch.dispatched();
        self.image_resource_wake = None;
        let snapshot = self.document.snapshot();
        let document_generation = self.document.generation();
        let geometry_generation = self.geometry_generation;
        let document_directory = self.document_directory.clone();
        let zoom_factor = self.zoom_factor;
        let previous = self.adaptive.clone();
        let published_geometry = self.published_geometry.clone();
        let preview_edit_node = self.projection.preview_edit_node;
        let expanded_code_tail = self.projection.expanded_code_tail;
        let html_disclosures = self.projection.html_disclosures.clone();
        let html_loaded_images = self.html_image_cache.loaded.clone();
        let editing_node = self.layout_focus;
        let table_layout_lock = if (width - self.layout_width).abs() < 0.5 {
            self.projection.table_layout_lock.clone()
        } else {
            None
        };
        let measurement = self.measurement.clone();
        let trace_mode = self.layout_trace_mode;
        let queued = (trace_mode != LayoutTraceMode::Off).then(Instant::now);
        let dispatch_ms = dispatch_start.map_or(0., |start| start.elapsed().as_secs_f64() * 1000.);
        if queued.is_some() {
            self.layout_trace_sequence += 1;
        }
        let trace_sequence = self.layout_trace_sequence;
        let explicitly_requested = std::mem::take(&mut self.layout_trace_requested);
        let recovery = Arc::new(reflow::Recovery::default());
        let watchdog_recovery = recovery.clone();
        let observer_recovery = recovery.clone();
        let timer = cx.background_executor().timer(deadline.remaining());
        let watchdog_deadline = deadline.clone();
        let watchdog = cx.spawn(async move |this, cx| {
            timer.await;
            watchdog_deadline.cancel();
            _ = this.update(cx, |this, cx| {
                this.expire_reflow(ticket, &watchdog_recovery, cx);
            });
        });
        let reflow = cx
            .background_executor()
            .spawn_dedicated(move |_| async move {
                let outcome = reflow::prepare(|| {
                    #[cfg(test)]
                    if fail_worker {
                        panic!("synthetic reflow failure");
                    }
                    let queue_wait_ms =
                        queued.map_or(0., |queued| queued.elapsed().as_secs_f64() * 1000.);
                    let (prepared, images, mut report) =
                        PreparedDocumentView::try_prepare_snapshot_with_images(
                            &snapshot,
                            &source_dimensions,
                            document_directory.as_deref(),
                            ReflowViewport {
                                published_geometry,
                                width,
                                height: viewport_height,
                                zoom: zoom_factor,
                                preview_edit_node,
                                expanded_code_tail,
                                editing_node,
                                table_layout_lock,
                                html_disclosures,
                                html_loaded_images,
                                trace_mode,
                                visible_roots,
                                resource_generation: dimensions_generation,
                            },
                            &previous,
                            &measurement,
                            ReflowControl {
                                deadline: &worker_deadline,
                                recovery: Some(&recovery),
                            },
                        )?;
                    if let Some(report) = &mut report {
                        report.queue_wait_ms = queue_wait_ms;
                        report.dispatch_ms = dispatch_ms;
                        report.sequence = trace_sequence;
                        report.explicitly_requested = explicitly_requested;
                        report.document_generation = document_generation;
                        report.geometry_generation = geometry_generation;
                    }
                    Ok((prepared, images, report))
                });
                #[cfg(test)]
                if let Some(hold) = hold_worker {
                    _ = hold.await;
                }
                #[cfg(feature = "layout-validation")]
                if let Some(hold) = native_hold {
                    eprintln!("TACHYON_LAYOUT_VALIDATION holding-timeout");
                    hold.await;
                    eprintln!("TACHYON_LAYOUT_VALIDATION released-timeout");
                }
                outcome
            });
        cx.spawn(async move |this, cx| {
            let mut outcome = reflow.await;
            // The observer owns this timer. Normal completion cancels it;
            // expiration never detaches or abandons the synchronous worker.
            drop(watchdog);
            let timed_out = deadline.check().is_err();
            if timed_out {
                outcome = Err(reflow::Failed::TimedOut);
            }
            let _ = this.update(cx, |this, cx| {
                if timed_out {
                    this.expire_reflow(ticket, &observer_recovery, cx);
                }
                let had_error = this
                    .reflow
                    .error(
                        this.document.id(),
                        this.document.generation(),
                        this.geometry_generation,
                    )
                    .is_some();
                if !this.reflow.finish(ticket, outcome.as_ref().err().copied()) {
                    #[cfg(feature = "layout-validation")]
                    if native_fault == Some(validation::Fault::Timeout) {
                        eprintln!("TACHYON_LAYOUT_VALIDATION discarded-late-result");
                    }
                    cx.notify();
                    return;
                }
                if had_error
                    != this
                        .reflow
                        .error(
                            this.document.id(),
                            this.document.generation(),
                            this.geometry_generation,
                        )
                        .is_some()
                {
                    // The application shell owns the status banner. Notify it
                    // on both failure and recovery, not just the editor view.
                    cx.emit(EditorEvent::ViewChanged);
                }
                let Ok(output) = outcome else {
                    // Keep the last published content and all authoring state.
                    // Only the failed request is suppressed; changed inputs can
                    // retry, and a stale document's error cannot replace status.
                    cx.notify();
                    return;
                };
                this.commit_reflow(ticket, output, cx);
            });
        })
        .detach();
    }

    fn expire_reflow(
        &mut self,
        ticket: reflow::Ticket,
        recovery: &reflow::Recovery<ReflowOutput>,
        cx: &mut Context<Self>,
    ) {
        if self.reflow.timeout(ticket) {
            if let Some(stack) = recovery.take() {
                self.commit_reflow(ticket, stack, cx);
            }
            cx.emit(EditorEvent::ViewChanged);
            cx.notify();
        }
    }

    fn commit_reflow(
        &mut self,
        ticket: reflow::Ticket,
        output: ReflowOutput,
        cx: &mut Context<Self>,
    ) {
        let (mut prepared, image_dimensions, mut report) = output;
        let reflow::Key {
            session,
            document: document_generation,
            geometry: geometry_generation,
            width,
            height,
            zoom,
            resources: dimensions_generation,
            focus: editing_node,
        } = ticket.key;
        let width = f32::from_bits(width);
        let viewport_height = f32::from_bits(height);
        let zoom_factor = f32::from_bits(zoom);
        let commit_start = report.as_ref().map(|_| Instant::now());
        if let Some(report) = &mut report {
            report.result_wait_ms = report
                .ready_at
                .take()
                .map_or(0., |ready| ready.elapsed().as_secs_f64() * 1000.);
        }
        let current_image_generation = self
            .shared_image_dimensions
            .as_ref()
            .and_then(|dimensions| dimensions.lock().ok().map(|dimensions| dimensions.0))
            .unwrap_or(0);
        if self.document.id() == session
            && self.document.generation() == document_generation
            && self.geometry_generation == geometry_generation
            && self.layout_focus == editing_node
            && !self.selection_defers_reflow(width, viewport_height)
            && self
                .last_edit_at
                .is_none_or(|edit| edit.elapsed() >= Duration::from_millis(800))
            && self.requested_image_dimensions_generation == dimensions_generation
            && current_image_generation == dimensions_generation
            && (self.requested_layout_width - width).abs() < 0.5
            && (self.requested_zoom_factor - zoom_factor).abs() < f32::EPSILON
            && (self.scroll_metrics().1 - viewport_height).abs() < 0.5
        {
            let scroll_y = self.scroll_metrics().0;
            let snapshot = self.document.snapshot();
            let cursor = self.cursor_offset();
            // The old lines belong to the last committed viewport. Testing
            // them against the newly shortened viewport would discard a
            // previously visible editing caret before it can be anchored.
            let previous_viewport_height = self.adaptive.measured_rows.viewport * self.zoom_factor;
            let previous_viewport_height = if previous_viewport_height > 0. {
                previous_viewport_height
            } else {
                viewport_height
            };
            let caret = self
                .layout_focus
                .filter(|_| self.selected_byte_range().0.is_empty())
                .filter(|_| {
                    self.table_caret_scroll_target(cursor)
                        .is_none_or(|(owner, target)| {
                            (target - self.horizontal_scrolls.get(&owner).copied().unwrap_or(0.))
                                .abs()
                                < 0.5
                        })
                })
                .and_then(|_| {
                    self.visual_lines.iter().find(|line| {
                        line.projected_start() <= cursor
                            && cursor <= line.projected_end()
                            && self
                                .projection
                                .segment_for_range(&line.projected_range())
                                .is_some_and(|segment| Some(segment.node_id) == self.layout_focus)
                            && line.y >= scroll_y
                            && line.y < scroll_y + previous_viewport_height
                    })
                });
            let preserve_caret = caret.is_some();
            let anchor = caret
                .and_then(|line| {
                    scroll_anchor_for_line(&snapshot, &self.projection, line, 0.).map(
                        |mut anchor| {
                            anchor.node_text_offset += cursor - line.projected_start();
                            anchor.projection_offset = cursor;
                            (anchor, line.y - scroll_y)
                        },
                    )
                })
                .or_else(|| {
                    capture_scroll_anchor(&snapshot, &self.projection, &self.visual_lines, scroll_y)
                        .map(|anchor| (anchor, 0.))
                });
            self.image_layout_dimensions = image_dimensions;
            self.layout_width = width;
            self.image_dimensions_generation = dimensions_generation;
            // Reflow preserves the current reading anchor below. It
            // must not cancel a user's ongoing inertial scroll: the
            // next frame continues from the corrected current offset.
            let recovery = prepared.recovery;
            let code_width =
                code_gutter::active_width(&self.projection, &self.visual_lines, editing_node);
            if code_gutter::restore_active_width(
                &prepared.projection,
                &mut prepared.visual_lines,
                code_width,
            ) {
                prepared.published_geometry = None;
            }
            self.install_prepared(prepared);
            if let Some(failure) = recovery {
                self.reflow.stack_committed(
                    reflow::Key {
                        geometry: self.geometry_generation,
                        ..ticket.key
                    },
                    failure,
                );
                cx.emit(EditorEvent::ViewChanged);
            }
            self.measured_layout = true;
            self.text_environment_pending = false;
            self.layout_replan_pending = false;
            if let Some(report) = &mut report {
                report.committed = true;
            }
            if let Some((anchor, viewport_y)) = anchor
                && let Some(y) =
                    resolve_scroll_anchor(&snapshot, &self.projection, &self.visual_lines, &anchor)
            {
                let x = self.scroll_handle.offset().x;
                self.scroll_handle
                    .set_offset(point(x, px(-(y - viewport_y).max(0.))));
                if preserve_caret {
                    // Preserve its former position when it fits, but keep the
                    // full caret line inside a shorter/recomposed viewport.
                    // An old edit deliberately scrolled away is not revealed.
                    self.keep_offset_visible(cursor);
                }
                if let Some(report) = &mut report {
                    report.anchor_displacement_at_commit_px =
                        Some(y - self.scroll_metrics().0 - viewport_y);
                }
            }
            if let Some(displacement) = self.restore_disclosure_anchor()
                && let Some(report) = &mut report
            {
                report.anchor_displacement_at_commit_px = Some(displacement);
            }
            self.finish_html_anchor_jump(cx);
            self.finish_find_reveal(cx);
        } else if let Some(report) = &mut report {
            report.discard_reason = Some(if self.document.id() != session {
                "session_changed"
            } else if self.document.generation() != document_generation {
                "document_changed"
            } else if self.geometry_generation != geometry_generation {
                "geometry_changed"
            } else if self.layout_focus != editing_node {
                "focus_changed"
            } else if self.selection_defers_reflow(width, viewport_height) {
                "selection_or_composition"
            } else if self.requested_image_dimensions_generation != dimensions_generation
                || current_image_generation != dimensions_generation
            {
                "image_dimensions_changed"
            } else {
                "viewport_zoom_or_recent_edit"
            });
        }
        if let Some(mut report) = report.take()
            && self.layout_trace_mode != LayoutTraceMode::Off
        {
            if report.committed {
                self.layout_trace_committed += 1;
            } else {
                self.layout_trace_discarded += 1;
            }
            report.committed_results_total = self.layout_trace_committed;
            report.stale_results_total = self.layout_trace_discarded;
            report.commit_ms =
                commit_start.map_or(0., |start| start.elapsed().as_secs_f64() * 1000.);
            cx.emit(EditorEvent::LayoutDiagnostics(Arc::new(report)));
        }
        cx.notify();
    }

    pub fn jump_to_node(
        &mut self,
        node_id: document_core::NodeId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(segment) = self
            .projection
            .segments()
            .iter()
            .find(|segment| segment.node_id == node_id)
        else {
            window.play_system_bell();
            return;
        };
        let offset = segment.projection_start();
        let y = self
            .visual_lines
            .iter()
            .find(|line| line.projected_range().contains(&offset) || line.projected_end() == offset)
            .map_or(0., |line| line.y);
        let start_y = self.scroll_metrics().0;
        // Outline/find navigation supplies a document-projection offset, not
        // a byte offset inside the temporary HTML text selection.
        self.html_selection = None;
        self.move_to(offset, window, cx);
        self.jump_generation = self.jump_generation.saturating_add(1);
        let generation = self.jump_generation;
        if cx.reduce_motion() || (start_y - y).abs() < 1. {
            let x = self.scroll_handle.offset().x;
            self.scroll_handle.set_offset(point(x, px(-y)));
            cx.emit(EditorEvent::ViewChanged);
            cx.notify();
            return;
        }
        cx.spawn(async move |this, cx| {
            for step in 1..=OUTLINE_JUMP_STEPS {
                cx.background_executor().timer(OUTLINE_JUMP_FRAME).await;
                let keep_running = this
                    .update(cx, |editor, cx| {
                        if editor.jump_generation != generation {
                            return false;
                        }
                        let next = outline_jump_position(start_y, y, step);
                        let x = editor.scroll_handle.offset().x;
                        editor.scroll_handle.set_offset(point(x, px(-next)));
                        editor.toolbar_visible = false;
                        editor.toolbar_opacity = 0.;
                        editor.link_popover_visible = false;
                        cx.emit(EditorEvent::ViewChanged);
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        })
        .detach();
    }

    #[must_use]
    pub fn scroll_handle(&self) -> ScrollHandle {
        self.scroll_handle.clone()
    }

    pub fn scroll_metrics(&self) -> (f32, f32) {
        let scroll_y: f32 = (-self.scroll_handle.offset().y).into();
        let viewport_height: f32 = self.scroll_handle.bounds().size.height.into();
        (scroll_y.max(0.), viewport_height.max(0.))
    }

    #[must_use]
    pub fn active_heading_node(&self) -> Option<NodeId> {
        let (scroll_y, viewport_height) = self.scroll_metrics();
        self.components.active_heading(scroll_y, viewport_height)
    }

    /// Rebuilds the navigation miniature from the exact line records used by
    /// this editor for painting, hit testing, scrolling, and outline jumps.
    pub fn rebuild_minimap(&self, minimap: &mut Minimap, minimap_height: f32) {
        let lines = self.visual_lines.iter().filter_map(|line| {
            minimap_source_line(&self.projection, line, self.layout_width, self.zoom_factor)
        });
        minimap.rebuild_rendered(
            lines,
            self.document_height,
            self.layout_width,
            minimap_height,
        );
    }

    #[must_use]
    pub fn geometry_generation(&self) -> u64 {
        self.geometry_generation
    }

    #[must_use]
    pub fn composition_active(&self) -> bool {
        self.document.composition_active()
    }

    /// Commits provisional IME text as one undoable edit. Explicit file saves
    /// use this path before taking their immutable save snapshot.
    pub fn commit_pending_composition(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<bool, DocumentError> {
        if !self.document.composition_active() {
            return Ok(false);
        }
        if self.composition_origin.is_none() {
            return Err(DocumentError::CompositionAlreadyActive);
        }
        if matches!(self.composition_origin, Some(CompositionOrigin::Html(_)))
            && self.marked_range.is_none()
        {
            return self.cancel_pending_composition(cx);
        }
        let snapshot = self.document.commit_composition()?;
        self.composition_origin = None;
        self.selection = snapshot.selection().clone();
        self.marked_range = None;
        self.last_error = None;
        cx.emit(EditorEvent::Changed);
        cx.notify();
        Ok(true)
    }

    /// Cancels provisional IME text without creating an undo entry or a dirty
    /// editor event, restoring both content and selection from composition start.
    pub fn cancel_pending_composition(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<bool, DocumentError> {
        if self.html_selection.take().is_some() {
            self.is_selecting = false;
            cx.notify();
            return Ok(true);
        }
        if !self.document.composition_active() {
            return Ok(false);
        }
        if self.composition_origin.is_none() {
            return Err(DocumentError::CompositionAlreadyActive);
        }
        let snapshot = self.document.cancel_composition()?;
        self.selection = snapshot.selection().clone();
        self.marked_range = None;
        let origin = match self.composition_origin.take() {
            Some(CompositionOrigin::Html(selection)) => Some(selection),
            _ => None,
        };
        self.refresh_projection();
        if let Some(mut selection) = origin {
            // Cancellation restores the original source and IDs, but allocates
            // a fresh revision. Rebind only this explicitly restored origin;
            // ordinary reflows must still reject stale cross-preview ranges.
            if let Some(previous) = &selection.cross {
                let next = preview_selection::PreviewSelectionProjection::build(self);
                if previous.text == next.text {
                    selection.cross = Some(Arc::new(next));
                    self.html_selection = Some(selection);
                }
            } else {
                self.html_selection = Some(selection);
            }
            self.refresh_html_selection();
        }
        self.shaped_line_cache.borrow_mut().clear();
        self.last_error = None;
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
        Ok(true)
    }

    pub fn body_justified(&self) -> bool {
        self.typography.justify
    }

    pub fn hyphenation_enabled(&self) -> bool {
        self.typography.hyphenate
    }

    pub fn toggle_body_justification(&mut self, cx: &mut Context<Self>) {
        self.typography.justify = !self.typography.justify;
        self.typography_changed(cx);
    }

    pub fn toggle_hyphenation(&mut self, cx: &mut Context<Self>) {
        self.typography.hyphenate = !self.typography.hyphenate;
        self.typography_changed(cx);
    }

    fn typography_changed(&mut self, cx: &mut Context<Self>) {
        self.projection.table_layout_lock = None;
        self.published_geometry = None;
        self.measured_layout = false;
        self.text_environment_pending = true;
        self.geometry_generation = self.geometry_generation.wrapping_add(1);
        self.shaped_line_cache.borrow_mut().clear();
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
    }

    #[must_use]
    pub fn zoom_percent(&self) -> u16 {
        (self.zoom_factor * 100.).round() as u16
    }

    #[must_use]
    pub fn can_zoom_in(&self) -> bool {
        self.zoom_factor < MAX_ZOOM
    }

    #[must_use]
    pub fn can_zoom_out(&self) -> bool {
        self.zoom_factor > MIN_ZOOM
    }

    pub fn zoom_in(&mut self, cx: &mut Context<Self>) {
        self.set_zoom_factor(self.zoom_factor + ZOOM_STEP, cx);
    }

    pub fn zoom_out(&mut self, cx: &mut Context<Self>) {
        self.set_zoom_factor(self.zoom_factor - ZOOM_STEP, cx);
    }

    pub fn reset_zoom(&mut self, cx: &mut Context<Self>) {
        self.set_zoom_factor(1., cx);
    }

    fn set_zoom_factor(&mut self, zoom_factor: f32, cx: &mut Context<Self>) {
        let next = ((zoom_factor * 10.).round() / 10.).clamp(MIN_ZOOM, MAX_ZOOM);
        if (next - self.zoom_factor).abs() < f32::EPSILON {
            return;
        }
        let previous = self.zoom_factor;
        self.table_resize_drag = None;
        let scroll_y = self.scroll_metrics().0;
        self.zoom_factor = next;
        self.projection.table_layout_lock = None;
        self.requested_zoom_factor = next;
        // A zoom key is not an edit. Retain the canonical projection and
        // already-rendered resources, show the new scale immediately, then
        // let the revision/generation-checked background job remeasure widths.
        // In particular, do not run Blitz CSS/layout/raster work on input.
        self.published_geometry = None;
        scale_visual_lines(
            Arc::make_mut(&mut self.visual_lines).as_mut_slice(),
            next / previous,
        );
        self.measured_layout = false;
        self.text_environment_pending = true;
        self.refresh_visual_index();
        self.shaped_line_cache.borrow_mut().clear();
        let x = self.scroll_handle.offset().x;
        self.scroll_handle
            .set_offset(point(x, px(-(scroll_y * next / previous).max(0.))));
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
    }

    pub fn set_scroll_y(&mut self, scroll_y: f32, cx: &mut Context<Self>) {
        self.jump_generation = self.jump_generation.saturating_add(1);
        let x = self.scroll_handle.offset().x;
        self.scroll_handle
            .set_offset(point(x, px(-scroll_y.max(0.))));
        self.animate_toolbar(false, cx);
        self.link_popover_visible = false;
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
    }

    #[must_use]
    pub fn view_state(&self) -> EditorViewState {
        let (selection, reversed) = self
            .projection
            .selection_range(&self.selection)
            .unwrap_or((0..0, false));
        let scroll_y = self.scroll_metrics().0;
        let snapshot = self.document.snapshot();
        EditorViewState {
            selection,
            reversed,
            scroll_y,
            scroll_anchor: capture_scroll_anchor(
                &snapshot,
                &self.projection,
                &self.visual_lines,
                scroll_y,
            ),
        }
    }

    #[cfg(feature = "layout-validation")]
    #[must_use]
    pub fn validation_viewport_bounds(&self) -> [f32; 4] {
        let bounds = self.scroll_handle.bounds();
        [
            bounds.left().into(),
            bounds.top().into(),
            bounds.right().into(),
            bounds.bottom().into(),
        ]
    }

    #[cfg(feature = "layout-validation")]
    #[must_use]
    pub fn validation_caret_bounds(&self) -> Option<[f32; 4]> {
        if let Some(selection) = &self.html_selection {
            let caret = self.html_caret_bounds(selection.head)?;
            return Some([
                caret.left().into(),
                caret.top().into(),
                caret.right().into(),
                caret.bottom().into(),
            ]);
        }
        let (selected, _) = self.selected_byte_range();
        if !selected.is_empty() {
            return None;
        }
        let line = painted_line_for_offset(&self.painted_lines, selected.start).filter(|line| {
            line.range.contains(&selected.start) || line.range.end == selected.start
        })?;
        let local = selected
            .start
            .saturating_sub(line.range.start)
            .min(line.range.len());
        let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
            + shaped_x_for_index(&line.layout, local);
        Some([
            x.into(),
            line.bounds.top().into(),
            (x + px(1.5)).into(),
            line.bounds.bottom().into(),
        ])
    }

    #[cfg(feature = "layout-validation")]
    #[must_use]
    pub fn validation_rtl_line_bounds(&self) -> Vec<[f32; 4]> {
        self.painted_lines
            .iter()
            .filter(|line| {
                line.layout.text.chars().any(|character| {
                    matches!(
                        unicode_bidi::bidi_class(character),
                        unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL
                    )
                })
            })
            .map(|line| {
                let left = aligned_text_left(line.bounds, &line.layout, line.alignment);
                [
                    left.into(),
                    line.bounds.top().into(),
                    (left + line.layout.width()).into(),
                    line.bounds.bottom().into(),
                ]
            })
            .collect()
    }

    pub fn restore_view_state(&mut self, state: &EditorViewState, cx: &mut Context<Self>) {
        self.html_selection = None;
        self.jump_generation = self.jump_generation.saturating_add(1);
        let end = self.projection.text().len();
        let range = state.selection.start.min(end)..state.selection.end.min(end);
        let start = self.position_for_offset(range.start, Affinity::Downstream);
        let finish = self.position_for_offset(range.end, Affinity::Upstream);
        if let (Some(start), Some(finish)) = (start, finish) {
            let (anchor, head) = if state.reversed {
                (finish, start)
            } else {
                (start, finish)
            };
            self.selection = Selection::Text(TextSelection { anchor, head });
        }
        self.refresh_source_focus();
        let restored_scroll = state
            .scroll_anchor
            .as_ref()
            .and_then(|anchor| {
                resolve_scroll_anchor(
                    &self.document.snapshot(),
                    &self.projection,
                    &self.visual_lines,
                    anchor,
                )
            })
            .unwrap_or(state.scroll_y)
            .max(0.);
        let x = self.scroll_handle.offset().x;
        self.scroll_handle
            .set_offset(point(x, px(-restored_scroll)));
        cx.notify();
    }

    fn refresh_projection(&mut self) {
        self.published_geometry = None;
        // Map the viewport by surviving canonical IDs: structural edits can
        // insert/remove roots, so old ordinal offsets alone are not anchors.
        let visible = self.visible_planning_roots().unwrap_or(0..1);
        let visible_ids: HashSet<_> = self
            .projection
            .roots()
            .enumerate()
            .filter(|(ordinal, _)| visible.contains(ordinal))
            .map(|(_, root)| root.id())
            .collect();
        let snapshot = self.document.snapshot();
        self.projected_generation = self.document.generation();
        let mut projection = TextProjection::from_snapshot(&snapshot);
        projection.expanded_code_tail =
            TextProjection::code_tail_for_selection(&snapshot, &self.selection);
        projection.preview_edit_node = match &self.selection {
            Selection::Text(selection)
                if display_math::has_editable_preview(&projection, selection.head.node_id) =>
            {
                Some(selection.head.node_id)
            }
            _ => None,
        };
        projection.reuse_table_measurements(&self.projection);
        projection.html_disclosures = self.projection.html_disclosures.clone();
        projection.retain_html_disclosures();
        html_images::bind(
            &mut projection,
            &self.html_image_cache.loaded,
            self.document_directory.as_deref(),
        );
        let editing_node = match &self.selection {
            Selection::Text(selection) => Some(selection.head.node_id),
            Selection::Table(selection) => projection
                .segments()
                .iter()
                .find(|segment| {
                    segment.context.table_cell
                        == Some((
                            selection.table_id,
                            selection.head_row,
                            selection.head_column,
                        ))
                })
                .map(|segment| segment.node_id)
                .or(self.layout_focus),
        };
        projection.retain_table_layout_lock(&self.projection, editing_node);
        projection.command_strip_lock = code_panel::active_strip(
            &self.projection,
            &self.visual_lines,
            editing_node,
            self.zoom_factor,
        );
        let code_width =
            code_gutter::active_width(&self.projection, &self.visual_lines, editing_node);
        if editing_node.is_some() {
            projection.retain_reading_modes(&self.adaptive.reading_modes);
            projection.retain_quote_roles(&self.adaptive.quote_roles, editing_node);
            projection.retain_bibliography(&self.adaptive.bibliography, editing_node);
            projection.retain_value_roles(&self.adaptive.editorials, editing_node);
        }
        let mut visible_ordinals = projection
            .roots()
            .enumerate()
            .filter(|(_, root)| visible_ids.contains(&root.id()))
            .map(|(ordinal, _)| ordinal);
        let scope = visible_ordinals.next().map_or(0..1, |first| {
            first..visible_ordinals.last().unwrap_or(first) + 1
        });
        // Structural edits (including HTML conversion, paste and undo) can
        // create new figure IDs without any new resource arrival. Rebind the
        // already-loaded source dimensions before reserving their geometry.
        self.image_layout_dimensions = self
            .shared_image_dimensions
            .as_ref()
            .and_then(|dimensions| dimensions.lock().ok())
            .map(|dimensions| {
                bind_image_dimensions(
                    &projection,
                    &dimensions.1,
                    self.document_directory.as_deref(),
                )
            })
            .unwrap_or_default();
        self.projection = projection;
        self.prune_math_scroll_handles();
        self.adaptive = build_edit_locked_adaptive_plan(
            &self.projection,
            self.layout_width / self.zoom_factor,
            self.scroll_metrics().1 / self.zoom_factor,
            Some(&self.adaptive),
            true,
            arrangement::LayoutMeasurement {
                text: &self.measurement,
                images: Some(&self.image_layout_dimensions),
                scope: Some(scope),
                resource_generation: self.image_dimensions_generation,
            },
            editing_node,
        );
        // The retained-row path does not enumerate candidates. Measure tables
        // after determining its exact geometry windows, before building lines.
        // Nested table IDs differ from their owning top-level group's ID.
        let active_tables = self
            .projection
            .segments()
            .iter()
            .filter(|segment| self.adaptive.measures_root(segment.top_level_node_id))
            .filter_map(|segment| segment.context.table_cell.map(|(id, _, _)| id))
            .collect();
        self.measurement
            .measure_tables_in_scope(&mut self.projection, Some(&active_tables));
        self.visual_lines = Arc::new(build_measured_visual_lines(
            &self.projection,
            &self.image_layout_dimensions,
            self.layout_width / self.zoom_factor,
            &self.adaptive,
            Some(&self.measurement),
        ));
        self.measured_layout = true;
        scale_visual_lines(
            Arc::make_mut(&mut self.visual_lines).as_mut_slice(),
            self.zoom_factor,
        );
        code_gutter::restore_active_width(&self.projection, &mut self.visual_lines, code_width);
        self.refresh_visual_index();
        self.refresh_html_selection();
    }

    fn refresh_visual_index(&mut self) {
        self.published_geometry = None;
        self.paint_order = Arc::new(visual_line_paint_order(&self.visual_lines));
        self.document_height = visual_document_height(&self.visual_lines);
        self.components = Arc::new(component_geometry(
            &self.projection,
            &self.visual_lines,
            self.layout_width,
            self.zoom_factor,
            &self.paint_order,
        ));
        self.geometry_generation = self.geometry_generation.wrapping_add(1);
    }

    fn refresh_after_transaction(&mut self, result: &document_core::TransactionResult) {
        self.table_resize_drag = None;
        self.find.cancel_navigation();
        self.find.read_only_match = None;
        self.last_edit_at = Some(Instant::now());
        if let Some(node_id) = result.text_changed_node
            && self.refresh_text_node(node_id)
        {
            return;
        }
        self.refresh_projection();
        self.shaped_line_cache.borrow_mut().clear();
    }

    /// Updates one text segment or its coupled adaptive row without measuring
    /// unrelated content. Structural changes retain the general path.
    fn refresh_text_node(&mut self, node_id: NodeId) -> bool {
        self.published_geometry = None;
        let scroll_y = self.scroll_metrics().0;
        let snapshot = self.document.snapshot();
        let request = TextRefreshRequest {
            snapshot: &snapshot,
            node_id,
            image_dimensions: &self.image_layout_dimensions,
            layout_width: self.layout_width,
            zoom_factor: self.zoom_factor,
            measurement: Some(&self.measurement),
        };
        self.projection.expanded_code_tail =
            TextProjection::code_tail_for_selection(&snapshot, &self.selection);
        let refresh = if self.adaptive.slots.contains_key(&node_id)
            || self.adaptive.prose_flows.contains_key(&node_id)
            || self.adaptive.figure_flows.contains_key(&node_id)
            || self.adaptive.inline_lists.contains_key(&node_id)
            || self.adaptive.lead == Some(node_id)
            || self.adaptive.label_rows.contains_key(&node_id)
            || self
                .projection
                .segment_for_node(node_id)
                .is_some_and(|segment| segment.context.table_cell.is_some())
        {
            let result = refresh_arranged_text_node_geometry(
                &mut self.projection,
                Arc::make_mut(&mut self.visual_lines),
                Arc::make_mut(&mut self.paint_order),
                &self.adaptive,
                request,
            );
            if let Some(refresh) = &result {
                self.document_height = (self.document_height + refresh.y_delta).max(LINE_HEIGHT);
            }
            result
        } else {
            refresh_text_node_geometry(
                &mut self.projection,
                Arc::make_mut(&mut self.visual_lines),
                Arc::make_mut(&mut self.paint_order),
                &mut self.document_height,
                request,
            )
        };
        let Some(refresh) = refresh else {
            return false;
        };
        prose_flow::rebase_after_edit(&mut self.adaptive, &self.projection, node_id);
        figure_flow::rebase_after_edit(&mut self.adaptive, &self.projection, node_id);
        inline_lists::rebase_after_edit(&mut self.adaptive, &self.projection, node_id);
        if scroll_y >= refresh.old_y_after && refresh.y_delta.abs() > f32::EPSILON {
            let x = self.scroll_handle.offset().x;
            self.scroll_handle
                .set_offset(point(x, px(-(scroll_y + refresh.y_delta).max(0.))));
        }
        self.projected_generation = self.document.generation();
        self.geometry_generation = self.geometry_generation.wrapping_add(1);
        match refresh.components {
            ComponentRefresh::Unchanged => {}
            ComponentRefresh::ShiftSuffixUp {
                old_y_after,
                paint_end,
            } => Arc::make_mut(&mut self.components).shift_suffix_up(
                old_y_after,
                refresh.y_delta,
                paint_end,
            ),
            ComponentRefresh::ReplaceSimpleAndShiftUp(rebase) => {
                Arc::make_mut(&mut self.components).replace_simple_and_shift_up(rebase)
            }
            ComponentRefresh::Rebuild => {
                self.components = Arc::new(component_geometry(
                    &self.projection,
                    &self.visual_lines,
                    self.layout_width,
                    self.zoom_factor,
                    &self.paint_order,
                ));
            }
        }
        self.shaped_line_cache
            .borrow_mut()
            .retain(|key| key.node_id != Some(node_id));
        true
    }

    fn record_error(&mut self, error: DocumentError, window: &mut Window) {
        self.last_error = Some(error.to_string());
        window.play_system_bell();
    }

    fn sync_selection_to_document(&self) -> Result<(), DocumentError> {
        if self.document.composition_active() && self.composition_origin.is_none() {
            return Err(DocumentError::CompositionAlreadyActive);
        }
        if self.document.snapshot().selection() != &self.selection {
            self.document
                .apply(EditCommand::SetSelection(self.selection.clone()))?;
        }
        Ok(())
    }

    fn apply_command(
        &mut self,
        command: EditCommand,
    ) -> Result<document_core::TransactionResult, DocumentError> {
        if self.find.read_only_match.is_some()
            && !matches!(
                command,
                EditCommand::SetSelection(_)
                    | EditCommand::ConvertHtmlToMarkdown { .. }
                    | EditCommand::ConvertHtmlToMarkdownAt { .. }
                    | EditCommand::ToggleTask { .. }
            )
        {
            return Err(DocumentError::Html("This find result has no verified editable text target. Copy it, or click editable document text before changing content.".into()));
        }
        let command = if let Some(selection) = &self.html_selection {
            use document_core::HtmlTextEdit;
            let edit = match command {
                EditCommand::ReplaceSelection { text, .. } => HtmlTextEdit::Replace(text),
                EditCommand::PasteMarkdown { markdown } => HtmlTextEdit::PasteMarkdown(markdown),
                EditCommand::ToggleInlineSelection { format } => HtmlTextEdit::Format(format),
                EditCommand::SetLinkSelection { target } => HtmlTextEdit::Link(target),
                EditCommand::SplitSelection => HtmlTextEdit::Split,
                EditCommand::ConvertHtmlToMarkdown { .. }
                | EditCommand::ConvertHtmlToMarkdownAt { .. } => {
                    self.html_selection = None;
                    return self.apply_command(command);
                }
                _ => {
                    return Err(DocumentError::Html(
                        "Use Edit text to convert this fragment before that command".into(),
                    ));
                }
            };
            selection.command(edit)?
        } else {
            command
        };
        self.sync_selection_to_document()?;
        let result = self.document.apply(command)?;
        self.html_anchor_jump = None;
        self.html_selection = None;
        self.selection = result.selection.clone();
        Ok(result)
    }

    fn selected_byte_range(&self) -> (Range<usize>, bool) {
        if let Some(selection) = &self.html_selection {
            return (selection.range(), selection.head < selection.anchor);
        }
        self.projection
            .selection_range(&self.selection)
            .unwrap_or((0..0, false))
    }

    fn cursor_offset(&self) -> usize {
        let (range, reversed) = self.selected_byte_range();
        if reversed { range.start } else { range.end }
    }

    fn set_selection(
        &mut self,
        range: Range<usize>,
        reversed: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.html_anchor_jump = None;
        if let Some(selection) = &mut self.html_selection {
            if range.start > range.end || selection.text().get(range.clone()).is_none() {
                window.play_system_bell();
                return;
            }
            (selection.anchor, selection.head) = if reversed {
                (range.end, range.start)
            } else {
                (range.start, range.end)
            };
            cx.notify();
            return;
        }
        let range = self.snap_footnote_selection(range);
        let Some(start) = self.position_for_offset(range.start, Affinity::Downstream) else {
            window.play_system_bell();
            return;
        };
        let Some(end) = self.position_for_offset(range.end, Affinity::Upstream) else {
            window.play_system_bell();
            return;
        };
        let (anchor, head) = if reversed { (end, start) } else { (start, end) };
        if let Err(error) =
            self.apply_command(EditCommand::SetSelection(Selection::Text(TextSelection {
                anchor,
                head,
            })))
        {
            self.record_error(error, window);
            return;
        }
        self.last_error = None;
        self.sync_layout_focus(window, cx);
        self.refresh_source_focus();
        self.schedule_selection_toolbar(cx);
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
    }

    fn sync_layout_focus(&mut self, window: &Window, cx: &mut Context<Self>) {
        if self.is_selecting || self.marked_range.is_some() || self.document.composition_active() {
            return;
        }
        let next = if self.focus_handle.is_focused(window)
            && (self.measured_layout || self.layout_focus.is_some())
        {
            if let Some(selection) = &self.html_selection {
                Some(selection.node)
            } else {
                match &self.selection {
                    Selection::Text(selection) => Some(selection.head.node_id),
                    _ => None,
                }
            }
        } else {
            None
        };
        if next != self.layout_focus {
            self.layout_focus = next;
            self.projection.lock_table_for_node(next);
            self.layout_replan_pending = true;
            self.measured_layout = false;
            self.geometry_generation = self.geometry_generation.wrapping_add(1);
            cx.notify();
        }
    }

    fn refresh_source_focus(&mut self) {
        if self.is_selecting || self.marked_range.is_some() {
            return;
        }
        let next = match &self.selection {
            Selection::Text(selection)
                if display_math::has_editable_preview(&self.projection, selection.head.node_id) =>
            {
                Some(selection.head.node_id)
            }
            _ => None,
        };
        let next = if self.selected_byte_range().0.is_empty() {
            next
        } else {
            self.projection.preview_edit_node
        };
        let snapshot = self.document.snapshot();
        let tail = TextProjection::code_tail_for_selection(&snapshot, &self.selection);
        if next == self.projection.preview_edit_node && tail == self.projection.expanded_code_tail {
            return;
        }
        let scroll = self.scroll_metrics().0;
        let anchor = capture_scroll_anchor(&snapshot, &self.projection, &self.visual_lines, scroll);
        self.refresh_projection();
        self.shaped_line_cache.borrow_mut().clear();
        if let Some(anchor) = anchor
            && let Some(y) =
                resolve_scroll_anchor(&snapshot, &self.projection, &self.visual_lines, &anchor)
        {
            let x = self.scroll_handle.offset().x;
            self.scroll_handle.set_offset(point(x, px(-y.max(0.))));
        }
    }

    fn schedule_selection_toolbar(&mut self, cx: &mut Context<Self>) {
        let selected = !self.selected_byte_range().0.is_empty();
        if selected && !self.is_selecting {
            self.start_toolbar_transition(true, TOOLBAR_SETTLE_DELAY, cx);
        } else {
            self.animate_toolbar(false, cx);
        }
    }

    fn animate_toolbar(&mut self, show: bool, cx: &mut Context<Self>) {
        self.start_toolbar_transition(show, Duration::ZERO, cx);
    }

    fn start_toolbar_transition(&mut self, show: bool, delay: Duration, cx: &mut Context<Self>) {
        let target = f32::from(show);
        if delay.is_zero()
            && self.toolbar_visible == show
            && (self.toolbar_opacity - target).abs() < f32::EPSILON
        {
            return;
        }
        // Dropping the previous foreground task cancels its timer instead of
        // merely letting a stale generation spin until the fade completes.
        self.toolbar_animation_task.take();
        self.toolbar_animation_generation = self.toolbar_animation_generation.saturating_add(1);
        let generation = self.toolbar_animation_generation;
        let start = if show && !delay.is_zero() {
            self.toolbar_visible = false;
            self.toolbar_opacity = 0.;
            0.
        } else {
            if show {
                self.toolbar_visible = true;
            }
            self.toolbar_opacity
        };
        let reduce_motion = cx.reduce_motion();
        if reduce_motion && delay.is_zero() {
            self.toolbar_opacity = target;
            self.toolbar_visible = show;
            cx.notify();
            return;
        }
        let task = cx.spawn(async move |this, cx| {
            if !delay.is_zero() {
                cx.background_executor().timer(delay).await;
                let ready = this
                    .update(cx, |editor, cx| {
                        if editor.toolbar_animation_generation != generation
                            || editor.selected_byte_range().0.is_empty()
                            || editor.is_selecting
                        {
                            return false;
                        }
                        editor.toolbar_visible = true;
                        if reduce_motion {
                            editor.toolbar_opacity = target;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !ready || reduce_motion {
                    return;
                }
            }
            for step in 1..=TOOLBAR_FADE_STEPS {
                cx.background_executor().timer(TOOLBAR_FADE_FRAME).await;
                let keep_running = this
                    .update(cx, |editor, cx| {
                        if editor.toolbar_animation_generation != generation {
                            return false;
                        }
                        let progress = step as f32 / TOOLBAR_FADE_STEPS as f32;
                        let eased = progress * progress * (3. - 2. * progress);
                        editor.toolbar_opacity = start + (target - start) * eased;
                        if step == TOOLBAR_FADE_STEPS {
                            editor.toolbar_visible = show;
                        }
                        cx.notify();
                        true
                    })
                    .unwrap_or(false);
                if !keep_running {
                    break;
                }
            }
        });
        self.toolbar_animation_task = Some(task);
    }

    fn position_for_offset(&self, offset: usize, affinity: Affinity) -> Option<DocumentPosition> {
        if self.projection.segments().is_empty() {
            return None;
        }
        self.projection
            .position_at(offset.min(self.projection.text().len()), affinity)
    }

    fn replace_range(
        &mut self,
        range: Range<usize>,
        text: &str,
        typing: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.stop_momentum();
        if self.html_selection.is_some() {
            return self.replace_html_range(range, text, window, cx);
        }
        let Some(start) = self.position_for_offset(range.start, Affinity::Downstream) else {
            window.play_system_bell();
            return false;
        };
        let Some(end) = self.position_for_offset(range.end, Affinity::Upstream) else {
            window.play_system_bell();
            return false;
        };
        let selection = Selection::Text(TextSelection {
            anchor: start,
            head: end,
        });
        // Platform text input normally reports the editor's current
        // selection. Avoid publishing a redundant selection-only transaction
        // in that case: selection transactions intentionally terminate core
        // typing groups, so emitting one for every keystroke makes undo split
        // into one entry per character.
        let current_range = self.selected_byte_range();
        let result = if current_range.0 == range && !current_range.1 {
            self.apply_command(EditCommand::ReplaceSelection {
                text: text.to_owned(),
                typing,
            })
        } else {
            self.apply_command(EditCommand::SetSelection(selection))
                .and_then(|_| {
                    self.apply_command(EditCommand::ReplaceSelection {
                        text: text.to_owned(),
                        typing,
                    })
                })
        };
        match result {
            Ok(result) => {
                self.last_error = None;
                self.refresh_after_transaction(&result);
                cx.emit(EditorEvent::Changed);
                cx.notify();
                true
            }
            Err(error) => {
                self.record_error(error, window);
                false
            }
        }
    }

    fn move_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.find.read_only_match = None;
        self.stop_momentum();
        self.set_selection(offset..offset, false, window, cx);
    }

    fn select_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.find.read_only_match = None;
        let (mut range, mut reversed) = self.selected_byte_range();
        if reversed {
            range.start = offset;
        } else {
            range.end = offset;
        }
        if range.end < range.start {
            reversed = !reversed;
            range = range.end..range.start;
        }
        self.set_selection(range, reversed, window, cx);
    }

    fn previous_boundary(&self, offset: usize) -> usize {
        if let Some(boundary) = self.footnote_boundary(offset, false) {
            return boundary;
        }
        let text = self.editing_text();
        let offset = offset.min(text.len());
        text[..offset]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        if let Some(boundary) = self.footnote_boundary(offset, true) {
            return boundary;
        }
        let text = self.editing_text();
        let offset = offset.min(text.len());
        text[offset..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(text.len(), |(index, _)| offset + index)
    }

    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.navigate_preview_horizontal(-1, false, false, window, cx) {
            return;
        }
        let (range, _) = self.selected_byte_range();
        if range.is_empty() {
            let offset = self.cursor_offset();
            self.move_to(
                self.visual_horizontal_boundary(offset, -1)
                    .unwrap_or_else(|| self.previous_boundary(offset)),
                window,
                cx,
            );
        } else {
            self.move_to(self.selection_visual_edge(&range, true), window, cx);
        }
        self.keep_offset_visible(self.cursor_offset());
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.navigate_preview_horizontal(1, false, false, window, cx) {
            return;
        }
        let (range, _) = self.selected_byte_range();
        if range.is_empty() {
            let offset = self.cursor_offset();
            self.move_to(
                self.visual_horizontal_boundary(offset, 1)
                    .unwrap_or_else(|| self.next_boundary(offset)),
                window,
                cx,
            );
        } else {
            self.move_to(self.selection_visual_edge(&range, false), window, cx);
        }
        self.keep_offset_visible(self.cursor_offset());
    }

    fn word_left(&mut self, _: &WordLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.navigate_preview_horizontal(-1, true, false, window, cx) {
            return;
        }
        let (range, _) = self.selected_byte_range();
        let offset = if range.is_empty() {
            self.cursor_offset()
        } else {
            range.start
        };
        self.move_to(
            previous_word_boundary(self.editing_text(), offset),
            window,
            cx,
        );
    }

    fn word_right(&mut self, _: &WordRight, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.navigate_preview_horizontal(1, true, false, window, cx) {
            return;
        }
        let (range, _) = self.selected_byte_range();
        let offset = if range.is_empty() {
            self.cursor_offset()
        } else {
            range.end
        };
        self.move_to(next_word_boundary(self.editing_text(), offset), window, cx);
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.navigate_preview_horizontal(-1, false, true, window, cx) {
            return;
        }
        let offset = self.cursor_offset();
        self.select_to(
            self.visual_horizontal_boundary(offset, -1)
                .unwrap_or_else(|| self.previous_boundary(offset)),
            window,
            cx,
        );
        self.keep_offset_visible(self.cursor_offset());
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.navigate_preview_horizontal(1, false, true, window, cx) {
            return;
        }
        let offset = self.cursor_offset();
        self.select_to(
            self.visual_horizontal_boundary(offset, 1)
                .unwrap_or_else(|| self.next_boundary(offset)),
            window,
            cx,
        );
        self.keep_offset_visible(self.cursor_offset());
    }

    fn visual_horizontal_boundary(&self, offset: usize, direction: isize) -> Option<usize> {
        let line = painted_line_for_offset(&self.painted_lines, offset)?;
        let local = offset
            .saturating_sub(line.range.start)
            .min(line.range.len());
        shaped_visual_neighbor(&line.layout, local, direction)
            .map(|neighbor| line.range.start + neighbor)
    }

    fn selection_visual_edge(&self, range: &Range<usize>, left: bool) -> usize {
        let Some(line) = painted_line_for_offset(&self.painted_lines, range.start) else {
            return selection_collapse_offset(range, left);
        };
        if !line.range.contains(&range.end) && range.end != line.range.end {
            return selection_collapse_offset(range, left);
        }
        let start = range.start.saturating_sub(line.range.start);
        let end = range.end.saturating_sub(line.range.start);
        let start_x = shaped_x_for_index(&line.layout, start);
        let end_x = shaped_x_for_index(&line.layout, end);
        if (start_x <= end_x) == left {
            range.start
        } else {
            range.end
        }
    }

    fn select_word_left(
        &mut self,
        _: &SelectWordLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferred_x = None;
        if self.navigate_preview_horizontal(-1, true, true, window, cx) {
            return;
        }
        let offset = previous_word_boundary(self.editing_text(), self.cursor_offset());
        self.select_to(offset, window, cx);
    }

    fn select_word_right(
        &mut self,
        _: &SelectWordRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferred_x = None;
        if self.navigate_preview_horizontal(1, true, true, window, cx) {
            return;
        }
        let offset = next_word_boundary(self.editing_text(), self.cursor_offset());
        self.select_to(offset, window, cx);
    }

    fn vertical_target(&mut self, direction: isize) -> Option<usize> {
        let offset = self.cursor_offset();
        let current_index = visual_line_index_at_offset(&self.visual_lines, offset)?;
        let current = self.visual_lines.get(current_index)?;
        if direction > 0
            && let Some(tail) = code_panel::terminal_after_line(&self.projection, current)
        {
            return Some(tail);
        }
        let in_prose_flow = current.slot.is_some_and(|s| {
            self.adaptive.prose_flows.contains_key(&s.group)
                || self.adaptive.figure_flows.contains_key(&s.group)
                || self.adaptive.inline_lists.contains_key(&s.group)
        });
        let target_index = if in_prose_flow {
            // Reading columns continue in source order even when the next
            // line starts higher on screen. Peer cards retain spatial motion.
            current_index.checked_add_signed(direction)?
        } else {
            visual_vertical_neighbor(&self.visual_lines, current_index, direction)?
        };
        let target = self.visual_lines.get(target_index)?;
        if direction < 0
            && let Some(tail) = code_panel::terminal_after_line(&self.projection, target)
        {
            return Some(tail);
        }

        // Use exact shaping whenever both lines are currently painted. The
        // visual-line index remains authoritative for off-screen movement,
        // with a grapheme-safe proportional fallback until the target enters
        // the painted viewport.
        if let Some(current_painted) = painted_line_for_offset(&self.painted_lines, offset)
            && let Some(target_painted) = self
                .painted_lines
                .iter()
                .find(|line| line.range == target.projected_range())
        {
            let local = offset
                .saturating_sub(current_painted.range.start)
                .min(current_painted.layout.len());
            let mut preferred_x = self.preferred_x.unwrap_or(
                aligned_text_left(
                    current_painted.bounds,
                    &current_painted.layout,
                    current_painted.alignment,
                ) + shaped_x_for_index(&current_painted.layout, local),
            );
            if in_prose_flow
                && current
                    .slot
                    .zip(target.slot)
                    .is_some_and(|(a, b)| a.group == b.group && a.item != b.item)
            {
                // Preserve the local reading-column x across a band boundary.
                preferred_x += target_painted.bounds.left() - current_painted.bounds.left();
            }
            self.preferred_x = Some(preferred_x);
            let relative_x = (preferred_x
                - aligned_text_left(
                    target_painted.bounds,
                    &target_painted.layout,
                    target_painted.alignment,
                ))
            .max(px(0.));
            return Some(
                target.projected_start()
                    + shaped_index_for_x(&target_painted.layout, relative_x)
                        .min(target.projected_range().len()),
            );
        }

        let local = offset.saturating_sub(current.projected_start());
        let ratio = if current.projected_range().is_empty() {
            0.
        } else {
            local as f32 / current.projected_range().len() as f32
        };
        let candidate = target.projected_start()
            + (target.projected_range().len() as f32 * ratio)
                .round()
                .clamp(0., target.projected_range().len() as f32) as usize;
        Some(snap_offset_to_grapheme(
            self.projection.text(),
            target.projected_range(),
            candidate,
        ))
    }

    fn table_caret_scroll_target_at_width(
        &self,
        offset: usize,
        layout_width: f32,
    ) -> Option<(NodeId, f32)> {
        let line = self
            .visual_lines
            .iter()
            .find(|line| line.projected_range().contains(&offset))
            .or_else(|| {
                self.visual_lines
                    .iter()
                    .find(|line| line.projected_end() == offset)
            })?;
        let (owner, _, _, _) = line.table_cell?;
        let component = self.components.get(&owner)?;
        let shaped = self.measurement.shape_unwrapped(
            &self.projection,
            line.projected_range(),
            line.style.font_size,
        )?;
        let container = Bounds::new(point(px(0.), px(0.)), size(px(layout_width), px(1.)));
        let bounds = visual_line_bounds(self, line, container, false, 0.);
        let alignment = table_column_alignment(
            &self.projection,
            segment_for_line(&self.projection, &line.projected_range()),
        );
        let caret_x = f32::from(
            aligned_text_left(bounds, &shaped, alignment)
                + shaped_x_for_index(&shaped, offset - line.projected_start()),
        );
        let (left, viewport) =
            table_viewport_geometry(line, &self.projection, layout_width, self.zoom_factor);
        let current = self.horizontal_scrolls.get(&owner).copied().unwrap_or(0.);
        let target = if caret_x - current < left {
            caret_x - left
        } else if caret_x + 1.5 - current > left + viewport {
            caret_x + 1.5 - left - viewport
        } else {
            current
        };
        let content = layout_width * component.right_fraction - left;
        Some((
            owner,
            clamped_horizontal_scroll(current, target - current, viewport, content),
        ))
    }

    fn table_caret_scroll_target(&self, offset: usize) -> Option<(NodeId, f32)> {
        self.table_caret_scroll_target_at_width(offset, self.layout_width)
    }

    fn keep_offset_visible(&mut self, offset: usize) {
        let layout_width = self
            .element_bounds
            .map_or(self.layout_width, |bounds| f32::from(bounds.size.width));
        self.keep_offset_visible_at_width(offset, layout_width);
    }

    fn keep_offset_visible_at_width(&mut self, offset: usize, layout_width: f32) {
        // A reflow may replace complete records with a scrollable table. Use
        // the newly prepared cell/viewport, not the old painted scroll range.
        if let Some((owner, target)) = self.table_caret_scroll_target_at_width(offset, layout_width)
        {
            self.horizontal_scrolls.insert(owner, target);
        }
        let extent = if self.html_selection.is_some() {
            self.html_caret_bounds(offset)
                .zip(self.element_bounds)
                .map(|(caret, element)| {
                    (
                        f32::from(caret.top() - element.top()),
                        f32::from(caret.bottom() - element.top()),
                    )
                })
        } else {
            self.visual_lines
                .iter()
                .find(|line| line.projected_range().contains(&offset))
                .or_else(|| {
                    self.visual_lines
                        .iter()
                        .find(|line| line.projected_end() == offset)
                })
                .map(|line| (line.y, line.y + line.style.line_height))
        };
        let Some((line_top, line_bottom)) = extent else {
            return;
        };
        let (scroll_y, viewport_height) = self.scroll_metrics();
        if viewport_height <= 0. {
            return;
        }
        let next_scroll = if line_top < scroll_y {
            line_top
        } else if line_bottom > scroll_y + viewport_height {
            line_bottom - viewport_height
        } else {
            return;
        };
        let x = self.scroll_handle.offset().x;
        self.scroll_handle
            .set_offset(point(x, px(-next_scroll.max(0.))));
    }

    fn up(&mut self, _: &Up, window: &mut Window, cx: &mut Context<Self>) {
        if self.navigate_preview_vertical(-1, false, window, cx) {
            return;
        }
        if let Some(offset) = self.vertical_target(-1) {
            self.move_to(offset, window, cx);
            self.keep_offset_visible(offset);
        }
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        if self.navigate_preview_vertical(1, false, window, cx) {
            return;
        }
        if let Some(offset) = self.vertical_target(1) {
            self.move_to(offset, window, cx);
            self.keep_offset_visible(offset);
        }
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        if self.navigate_preview_vertical(-1, true, window, cx) {
            return;
        }
        if let Some(offset) = self.vertical_target(-1) {
            self.select_to(offset, window, cx);
            self.keep_offset_visible(offset);
        }
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        if self.navigate_preview_vertical(1, true, window, cx) {
            return;
        }
        if let Some(offset) = self.vertical_target(1) {
            self.select_to(offset, window, cx);
            self.keep_offset_visible(offset);
        }
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.set_selection(0..self.editing_text().len(), false, window, cx);
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_line_boundary(false, false, window, cx);
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_line_boundary(true, false, window, cx);
    }

    fn select_home(&mut self, _: &SelectHome, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_line_boundary(false, true, window, cx);
    }

    fn select_end(&mut self, _: &SelectEnd, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_line_boundary(true, true, window, cx);
    }

    fn navigate_document_boundary(
        &mut self,
        end: bool,
        extend: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.preferred_x = None;
        let offset = if end { self.editing_text().len() } else { 0 };
        // These commands address the canonical document projection. Leave a
        // temporary selection inside an HTML preview before resolving either
        // boundary, just like outline and find navigation do.
        self.html_selection = None;
        if extend {
            self.select_to(offset, window, cx);
        } else {
            self.move_to(offset, window, cx);
        }
        self.keep_offset_visible(offset);
    }

    fn document_start(&mut self, _: &DocumentStart, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_document_boundary(false, false, window, cx);
    }

    fn document_end(&mut self, _: &DocumentEnd, window: &mut Window, cx: &mut Context<Self>) {
        self.navigate_document_boundary(true, false, window, cx);
    }

    fn select_document_start(
        &mut self,
        _: &SelectDocumentStart,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_document_boundary(false, true, window, cx);
    }

    fn select_document_end(
        &mut self,
        _: &SelectDocumentEnd,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.navigate_document_boundary(true, true, window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        let (mut range, _) = self.selected_byte_range();
        if range.is_empty()
            && self.html_selection.is_none()
            && let Selection::Text(selection) = &self.selection
            && selection.head.text_offset == 0
            && let Some((_, item_id, _)) = self
                .document
                .snapshot()
                .list_item_containing(selection.head.node_id)
            && self.adjust_list_indent(item_id, false, window, cx)
        {
            return;
        }
        if range.is_empty() {
            range.start = self.previous_boundary(range.start);
        }
        if range.is_empty() {
            window.play_system_bell();
        } else {
            self.replace_range(range, "", true, window, cx);
        }
    }

    fn delete(&mut self, _: &Delete, window: &mut Window, cx: &mut Context<Self>) {
        let (mut range, _) = self.selected_byte_range();
        if range.is_empty() {
            range.end = self.next_boundary(range.end);
        }
        if range.is_empty() {
            window.play_system_bell();
        } else {
            self.replace_range(range, "", true, window, cx);
        }
    }

    fn copy(&mut self, _: &Copy, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(text) = &self.find.read_only_match {
            cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
            return;
        }
        if let Some(selection) = &self.html_selection {
            let snapshot = self.document.snapshot();
            if selection.range().is_empty() {
                return;
            }
            let range = if let Some(cross) = &selection.cross {
                cross.selection(selection.anchor, selection.head)
            } else {
                selection
                    .position(selection.anchor, Affinity::Downstream)
                    .zip(selection.position(selection.head, Affinity::Upstream))
                    .map(|(anchor, head)| document_core::PreviewSelection {
                        revision: snapshot.revision(),
                        anchor,
                        head,
                    })
            };
            let payload = range
                .ok_or_else(|| DocumentError::Html("Choose the preview text again".into()))
                .and_then(|range| snapshot.preview_clipboard_payload(&range));
            match payload {
                Ok(Some(payload)) => {
                    if let Some(text) = payload.plain_text {
                        let item = if let Some(metadata) = payload.rich_json {
                            ClipboardItem::new_string_with_metadata(text, metadata)
                        } else {
                            ClipboardItem::new_string(text)
                        };
                        #[cfg(target_os = "linux")]
                        let item = if let Some(html) = payload.html {
                            item.with_html(html)
                        } else {
                            item
                        };
                        cx.write_to_clipboard(item);
                    }
                }
                Ok(None) => {}
                Err(error) => self.record_error(error, window),
            }
            return;
        }
        if let Err(error) = self.sync_selection_to_document() {
            self.record_error(error, window);
            return;
        }
        let snapshot = self.document.snapshot();
        match snapshot.clipboard_payload() {
            Ok(Some(payload)) => {
                let Some(plain) = payload.plain_text else {
                    return;
                };
                if let Some(metadata) = payload.rich_json {
                    cx.write_to_clipboard(ClipboardItem::new_string_with_metadata(plain, metadata));
                } else {
                    cx.write_to_clipboard(ClipboardItem::new_string(plain));
                }
            }
            Ok(None) => {}
            Err(_) => {
                let (range, _) = self.selected_byte_range();
                if !range.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(
                        self.projection.text()[range].to_owned(),
                    ));
                }
            }
        }
    }

    fn cut(&mut self, _: &Cut, window: &mut Window, cx: &mut Context<Self>) {
        if self.html_selection.is_none() && matches!(self.selection, Selection::Table(_)) {
            self.copy(&Copy, window, cx);
            return;
        }
        let (range, _) = self.selected_byte_range();
        if range.is_empty() {
            return;
        }
        self.copy(&Copy, window, cx);
        self.replace_range(range, "", false, window, cx);
    }

    fn paste(&mut self, _: &Paste, window: &mut Window, cx: &mut Context<Self>) {
        let Some(item) = cx.read_from_clipboard() else {
            return;
        };
        match clipboard_paste(&item) {
            Some(ClipboardPaste::RichMarkdown(markdown)) => {
                self.apply_markdown_paste(markdown, window, cx);
            }
            Some(ClipboardPaste::PlainText(text)) => {
                if self.html_selection.is_none()
                    && text.contains('\t')
                    && self.paste_tsv(&text, window, cx)
                {
                    return;
                }
                let (range, _) = self.selected_byte_range();
                self.replace_range(range, &text, false, window, cx);
            }
            None => {}
        }
    }

    fn paste_as_markdown(
        &mut self,
        _: &PasteAsMarkdown,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(markdown) = cx.read_from_clipboard().and_then(|item| item.text()) {
            self.apply_markdown_paste(markdown, window, cx);
        }
    }

    fn apply_markdown_paste(
        &mut self,
        markdown: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.apply_command(EditCommand::PasteMarkdown { markdown }) {
            Ok(result) => {
                self.marked_range = None;
                self.last_error = None;
                self.refresh_after_transaction(&result);
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Err(error) => self.record_error(error, window),
        }
    }

    fn paste_tsv(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let snapshot = self.document.snapshot();
        let location = match &self.selection {
            Selection::Text(selection) => snapshot.table_cell_containing(selection.head.node_id),
            Selection::Table(selection) => {
                let (rows, columns) = selection.normalized();
                Some((selection.table_id, *rows.start(), *columns.start()))
            }
        };
        let Some((table_id, row, column)) = location else {
            return false;
        };
        drop(snapshot);
        match self.apply_command(EditCommand::PasteTsv {
            table_id,
            row,
            column,
            text: text.to_owned(),
        }) {
            Ok(result) => {
                self.refresh_after_transaction(&result);
                self.last_error = None;
                cx.emit(EditorEvent::Changed);
                cx.notify();
                true
            }
            Err(error) => {
                self.record_error(error, window);
                true
            }
        }
    }

    fn undo(&mut self, _: &Undo, window: &mut Window, cx: &mut Context<Self>) {
        self.find.cancel_navigation();
        self.find.read_only_match = None;
        if self.document.composition_active() {
            if let Err(error) = self.cancel_pending_composition(cx) {
                self.record_error(error, window);
            }
            return;
        }
        self.html_selection = None;
        match self.document.undo() {
            Ok(snapshot) => {
                self.selection = snapshot.selection().clone();
                self.marked_range = None;
                self.refresh_projection();
                self.keep_offset_visible(self.cursor_offset());
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Err(DocumentError::NothingToUndo) => window.play_system_bell(),
            Err(error) => self.record_error(error, window),
        }
    }

    fn redo(&mut self, _: &Redo, window: &mut Window, cx: &mut Context<Self>) {
        self.find.cancel_navigation();
        self.find.read_only_match = None;
        if self.document.composition_active() {
            if let Err(error) = self.cancel_pending_composition(cx) {
                self.record_error(error, window);
            }
            return;
        }
        self.html_selection = None;
        match self.document.redo() {
            Ok(snapshot) => {
                self.selection = snapshot.selection().clone();
                self.marked_range = None;
                self.refresh_projection();
                self.keep_offset_visible(self.cursor_offset());
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Err(DocumentError::NothingToRedo) => window.play_system_bell(),
            Err(error) => self.record_error(error, window),
        }
    }

    fn save(&mut self, _: &Save, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(EditorEvent::SaveRequested);
    }

    fn apply_selection_format(
        &mut self,
        format: InlineFormat,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.apply_command(EditCommand::ToggleInlineSelection { format }) {
            Ok(result) if !result.dirty_node_ids.is_empty() => {
                self.refresh_after_transaction(&result);
                self.animate_toolbar(true, cx);
                self.last_error = None;
                self.focus_handle.focus(window, cx);
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Ok(_) => window.play_system_bell(),
            Err(error) => self.record_error(error, window),
        }
    }

    fn apply_block_style(
        &mut self,
        style: BlockStyle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Selection::Text(selection) = self.selection.clone() else {
            window.play_system_bell();
            return;
        };
        match self.apply_command(EditCommand::SetBlockStyle {
            node_id: selection.head.node_id,
            style,
        }) {
            Ok(result) => {
                self.refresh_after_transaction(&result);
                self.animate_toolbar(!self.selected_byte_range().0.is_empty(), cx);
                self.last_error = None;
                self.focus_handle.focus(window, cx);
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Err(error) => self.record_error(error, window),
        }
    }

    fn format_bold(&mut self, _: &FormatBold, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_selection_format(InlineFormat::Bold, window, cx);
    }

    fn format_italic(&mut self, _: &FormatItalic, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_selection_format(InlineFormat::Italic, window, cx);
    }

    fn format_strike(&mut self, _: &FormatStrike, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_selection_format(InlineFormat::Strikethrough, window, cx);
    }

    fn format_code(&mut self, _: &FormatCode, window: &mut Window, cx: &mut Context<Self>) {
        self.apply_selection_format(InlineFormat::Code, window, cx);
    }

    fn format_link(&mut self, _: &FormatLink, window: &mut Window, cx: &mut Context<Self>) {
        self.show_link_editor(window, cx);
    }

    fn show_link_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let selection = self.selected_byte_range().0;
        if selection.is_empty() {
            window.play_system_bell();
            return;
        }
        let value = self
            .html_selection
            .is_none()
            .then(|| self.link_at_offset(selection.start))
            .flatten()
            .unwrap_or_else(|| "https://".into());
        self.link_input
            .update(cx, |input, cx| input.set_value(value, window, cx));
        self.link_input.read(cx).focus_handle(cx).focus(window, cx);
        self.link_popover_visible = true;
        self.animate_toolbar(false, cx);
        cx.notify();
    }

    fn apply_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.link_input.read(cx).value().trim().to_owned();
        let target = (!value.is_empty()).then_some(value);
        match self.apply_command(EditCommand::SetLinkSelection { target }) {
            Ok(result) if !result.dirty_node_ids.is_empty() => {
                self.refresh_after_transaction(&result);
                self.link_popover_visible = false;
                self.animate_toolbar(true, cx);
                self.last_error = None;
                self.focus_handle.focus(window, cx);
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Ok(_) => window.play_system_bell(),
            Err(error) => self.record_error(error, window),
        }
    }

    fn current_image(&self) -> Option<(NodeId, String, String)> {
        let Selection::Text(selection) = self.selection.clone() else {
            return None;
        };
        let BlockNode::Image(image) = self.projection.block(selection.head.node_id)? else {
            return None;
        };
        Some((image.id, image.source.clone(), image.alt.as_string()))
    }

    fn show_image_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((image_id, source, alt)) = self.current_image() else {
            window.play_system_bell();
            return;
        };
        self.image_source_input
            .update(cx, |input, cx| input.set_value(source, window, cx));
        self.image_alt_input
            .update(cx, |input, cx| input.set_value(alt, window, cx));
        self.image_source_input
            .read(cx)
            .focus_handle(cx)
            .focus(window, cx);
        self.image_popover_node = Some(image_id);
        self.animate_toolbar(false, cx);
        cx.notify();
    }

    fn apply_image_attributes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(image_id) = self.image_popover_node else {
            window.play_system_bell();
            return;
        };
        let source = self.image_source_input.read(cx).value().trim().to_owned();
        let alt = self.image_alt_input.read(cx).value().to_string();
        match self.apply_command(EditCommand::SetImageAttributes {
            image_id,
            source,
            alt,
        }) {
            Ok(result) if !result.dirty_node_ids.is_empty() => {
                self.image_layout_dimensions.remove(&image_id);
                self.refresh_after_transaction(&result);
                self.image_popover_node = None;
                self.animate_toolbar(true, cx);
                self.last_error = None;
                self.focus_handle.focus(window, cx);
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Ok(_) => {
                self.image_popover_node = None;
                self.animate_toolbar(true, cx);
                self.focus_handle.focus(window, cx);
                cx.notify();
            }
            Err(error) => self.record_error(error, window),
        }
    }

    fn retry_current_image(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let image = self.current_image().or_else(|| {
            let image_id = self.image_popover_node?;
            let BlockNode::Image(image) = self.projection.block(image_id)? else {
                return None;
            };
            Some((image.id, image.source.clone(), image.alt.as_string()))
        });
        let Some((node, source, _)) = image else {
            window.play_system_bell();
            return;
        };
        self.retry_image_source(node, &source, cx);
    }

    fn split_range(
        &mut self,
        range: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.html_selection.is_some() {
            if let Err(error) = self.validate_html_range(&range) {
                self.record_error(error, window);
                return false;
            }
            self.set_selection(range, false, window, cx);
            self.apply_structural_command(EditCommand::SplitSelection, window, cx);
            return true;
        }
        let Some(anchor) = self.position_for_offset(range.start, Affinity::Downstream) else {
            window.play_system_bell();
            return false;
        };
        let Some(head) = self.position_for_offset(range.end, Affinity::Upstream) else {
            window.play_system_bell();
            return false;
        };
        let result = self
            .apply_command(EditCommand::SetSelection(Selection::Text(TextSelection {
                anchor,
                head,
            })))
            .and_then(|_| self.apply_command(EditCommand::SplitSelection));
        match result {
            Ok(result) => {
                self.marked_range = None;
                self.last_error = None;
                self.refresh_after_transaction(&result);
                cx.emit(EditorEvent::Changed);
                cx.notify();
                true
            }
            Err(error) => {
                self.record_error(error, window);
                false
            }
        }
    }

    fn enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        self.split_range(self.selected_byte_range().0, window, cx);
    }

    fn hard_break(&mut self, _: &HardBreak, window: &mut Window, cx: &mut Context<Self>) {
        let (range, _) = self.selected_byte_range();
        self.replace_range(range, "  \n", false, window, cx);
    }

    fn table_cell(&mut self, reverse: bool, window: &mut Window, cx: &mut Context<Self>) {
        let snapshot = self.document.snapshot();
        let Selection::Text(selection) = &self.selection else {
            window.play_system_bell();
            return;
        };
        let Some((table_id, row, column)) = snapshot.table_cell_containing(selection.head.node_id)
        else {
            window.play_system_bell();
            return;
        };
        let Some(BlockNode::Table(table)) = snapshot.node(table_id) else {
            window.play_system_bell();
            return;
        };
        let columns = table.column_count();
        let current = row * columns + column;
        if reverse && current == 0 {
            window.play_system_bell();
            return;
        }
        let mut target = if reverse { current - 1 } else { current + 1 };
        let append_row = target >= table.row_count() * columns;
        let row_count = table.row_count();
        drop(snapshot);
        if append_row {
            if let Err(error) = self.apply_command(EditCommand::InsertTableRow {
                table_id,
                index: row_count,
            }) {
                self.record_error(error, window);
                return;
            }
            target = row_count * columns;
        }
        let snapshot = self.document.snapshot();
        let Some(BlockNode::Table(table)) = snapshot.node(table_id) else {
            window.play_system_bell();
            return;
        };
        let target_row = target / columns;
        let target_column = target % columns;
        let Some(position) = table
            .rows
            .get(target_row)
            .and_then(|row| row.cells.get(target_column))
            .and_then(|cell| first_editable_position(&cell.blocks))
        else {
            window.play_system_bell();
            return;
        };
        drop(snapshot);
        if let Err(error) = self.apply_command(EditCommand::SetSelection(Selection::Text(
            TextSelection::caret(position),
        ))) {
            self.record_error(error, window);
            return;
        }
        if append_row {
            self.refresh_projection();
            self.shaped_line_cache.borrow_mut().clear();
        }
        // Structural navigation is intentional caret movement, just like an
        // arrow key. Reveal its cell even when the table overflows the canvas.
        self.keep_offset_visible(self.cursor_offset());
        self.last_error = None;
        self.animate_toolbar(false, cx);
        if append_row {
            cx.emit(EditorEvent::Changed);
        }
        cx.notify();
    }

    fn next_table_cell(&mut self, _: &NextTableCell, window: &mut Window, cx: &mut Context<Self>) {
        // Native controls inside the editor share its ancestor key context.
        // Tab from their focus must traverse controls, not mutate the text
        // selection left behind in a table or list.
        if !self.focus_handle.is_focused(window) {
            window.focus_next(cx);
            return;
        }
        let snapshot = self.document.snapshot();
        let Selection::Text(selection) = &self.selection else {
            window.play_system_bell();
            return;
        };
        if snapshot
            .table_cell_containing(selection.head.node_id)
            .is_some()
        {
            drop(snapshot);
            self.table_cell(false, window, cx);
        } else if let Some((_, item_id, _)) = snapshot.list_item_containing(selection.head.node_id)
        {
            drop(snapshot);
            self.adjust_list_indent(item_id, true, window, cx);
        } else {
            // Outside structural editing, Tab follows the native control order
            // so rendered disclosures and overflowing formulas are reachable.
            window.focus_next(cx);
        }
    }

    fn previous_table_cell(
        &mut self,
        _: &PreviousTableCell,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.focus_handle.is_focused(window) {
            window.focus_prev(cx);
            return;
        }
        let snapshot = self.document.snapshot();
        let Selection::Text(selection) = &self.selection else {
            window.play_system_bell();
            return;
        };
        if snapshot
            .table_cell_containing(selection.head.node_id)
            .is_some()
        {
            drop(snapshot);
            self.table_cell(true, window, cx);
        } else if let Some((_, item_id, _)) = snapshot.list_item_containing(selection.head.node_id)
        {
            drop(snapshot);
            self.adjust_list_indent(item_id, false, window, cx);
        } else {
            window.focus_prev(cx);
        }
    }

    fn adjust_list_indent(
        &mut self,
        item_id: document_core::NodeId,
        increase: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let command = if increase {
            EditCommand::IndentListItem { item_id }
        } else {
            EditCommand::OutdentListItem { item_id }
        };
        match self.apply_command(command) {
            Ok(result) if !result.dirty_node_ids.is_empty() => {
                self.refresh_after_transaction(&result);
                self.last_error = None;
                cx.emit(EditorEvent::Changed);
                cx.notify();
                true
            }
            Ok(_) => {
                window.play_system_bell();
                false
            }
            Err(error) => {
                self.record_error(error, window);
                false
            }
        }
    }

    fn exit_table(&mut self, _: &ExitTable, window: &mut Window, cx: &mut Context<Self>) {
        let inside_table = matches!(&self.selection, Selection::Text(selection) if self
            .projection
            .segment_for_node(selection.head.node_id)
            .is_some_and(|segment| segment.context.table_cell.is_some()));
        if inside_table {
            self.insert_block(InsertBlockKind::Paragraph, window, cx);
        } else {
            window.play_system_bell();
        }
    }

    fn dismiss(&mut self, _: &Dismiss, window: &mut Window, cx: &mut Context<Self>) {
        self.table_resize_drag = None;
        self.is_selecting = false;
        self.stop_drag_scroll();
        self.table_selection_drag = None;
        self.table_hover = None;
        self.table_edge_menu = None;
        if let Err(error) = self.cancel_pending_composition(cx) {
            self.record_error(error, window);
        }
        self.link_popover_visible = false;
        self.image_popover_node = None;
        self.animate_toolbar(false, cx);
        cx.notify();
    }

    fn insert_block(&mut self, kind: InsertBlockKind, window: &mut Window, cx: &mut Context<Self>) {
        match self.apply_command(EditCommand::InsertBlockAfterSelection { kind }) {
            Ok(result) => {
                self.refresh_after_transaction(&result);
                self.last_error = None;
                self.focus_handle.focus(window, cx);
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Err(error) => self.record_error(error, window),
        }
    }

    fn apply_structural_command(
        &mut self,
        command: EditCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let preserve_view = matches!(
            &command,
            EditCommand::InsertTableRow { .. } | EditCommand::InsertTableColumn { .. }
        );
        let scroll_anchor = preserve_view.then(|| {
            let snapshot = self.document.snapshot();
            capture_scroll_anchor(
                &snapshot,
                &self.projection,
                &self.visual_lines,
                self.scroll_metrics().0,
            )
        });
        match self.apply_command(command) {
            Ok(result) if !result.dirty_node_ids.is_empty() => {
                self.table_hover = None;
                self.table_edge_menu = None;
                self.refresh_after_transaction(&result);
                self.last_error = None;
                self.focus_handle.focus(window, cx);
                let view_restored = scroll_anchor.flatten().is_some_and(|anchor| {
                    let snapshot = self.document.snapshot();
                    let Some(y) = resolve_scroll_anchor(
                        &snapshot,
                        &self.projection,
                        &self.visual_lines,
                        &anchor,
                    ) else {
                        return false;
                    };
                    let x = self.scroll_handle.offset().x;
                    self.scroll_handle.set_offset(point(x, px(-y.max(0.))));
                    true
                });
                if !view_restored {
                    // Deletions and structural edits without a surviving
                    // viewport anchor reveal the resulting selection.
                    self.keep_offset_visible(self.cursor_offset());
                }
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Ok(_) => window.play_system_bell(),
            Err(error) => self.record_error(error, window),
        }
    }

    fn index_for_mouse_position(&self, position: Point<Pixels>) -> usize {
        if let Some(index) = self.table_line_hit_index(position) {
            let line = &self.painted_lines[index];
            let local = shaped_index_for_x(
                &line.layout,
                position.x - aligned_text_left(line.bounds, &line.layout, line.alignment),
            );
            return line.range.start + local.min(line.range.len());
        }
        let Some(line) = self.painted_lines.iter().min_by(|left, right| {
            distance_to_bounds(position, &left.bounds)
                .total_cmp(&distance_to_bounds(position, &right.bounds))
        }) else {
            return 0;
        };
        let local = shaped_index_for_x(
            &line.layout,
            position.x - aligned_text_left(line.bounds, &line.layout, line.alignment),
        );
        line.range.start + local.min(line.range.len())
    }

    /// Resolve a pointer inside a table using the cell's horizontal extent and
    /// the complete row band. Looking only at the nearest text line makes
    /// columns that share a Y coordinate indistinguishable, and wrapped cells
    /// otherwise make the row's empty vertical space select a neighboring
    /// cell. The painted line remains the text line used for the final X→byte
    /// mapping; this helper only chooses the correct cell first.
    fn table_line_hit_index(&self, position: Point<Pixels>) -> Option<usize> {
        let table_inset = px(12. * self.zoom_factor);
        self.painted_lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                let segment = segment_for_line(&self.projection, &line.range)?;
                segment.context.table_cell?;
                let spec = self
                    .visual_lines
                    .iter()
                    .find(|candidate| candidate.projected_range() == line.range)?;

                let row_top = line.bounds.top() - px((spec.y - spec.table_row_y).max(0.));
                let row_bottom = row_top + px(spec.table_row_height.max(spec.style.line_height));
                if position.y < row_top || position.y > row_bottom {
                    return None;
                }

                // visual_line_bounds starts at the cell's content inset and
                // reserves the trailing cell padding. Recover those edges so
                // clicks in padding still resolve to the intended column.
                let cell_left = line.bounds.left() - table_inset;
                let cell_right = line.bounds.right() + px(8.);
                if position.x < cell_left || position.x > cell_right {
                    return None;
                }

                Some((distance_to_vertical_bounds(position.y, &line.bounds), index))
            })
            .min_by(|left, right| left.0.total_cmp(&right.0))
            .map(|(_, index)| index)
    }

    fn selection_toolbar_origin(&self) -> (f32, f32) {
        let (selection, _) = self.selected_byte_range();
        let Some(element) = self.element_bounds else {
            return (8., 0.);
        };
        if self.html_selection.is_some() {
            return self
                .html_caret_bounds(selection.start)
                .map_or((8., 0.), |caret| {
                    (
                        f32::from(caret.left() - element.left())
                            .clamp(8., (f32::from(element.size.width) - 440.).max(8.)),
                        (f32::from(caret.top() - element.top()) - 40.).max(0.),
                    )
                });
        }
        let Some(line) = painted_line_for_offset(&self.painted_lines, selection.start) else {
            return (8., 0.);
        };
        let local_index = selection
            .start
            .saturating_sub(line.range.start)
            .min(line.range.len());
        let left: f32 = (aligned_text_left(line.bounds, &line.layout, line.alignment)
            + shaped_x_for_index(&line.layout, local_index)
            - element.left())
        .into();
        let top: f32 = (line.bounds.top() - element.top()).into();
        let available: f32 = element.size.width.into();
        (
            left.clamp(8., (available - 440.).max(8.)),
            (top - 40.).max(0.),
        )
    }

    pub fn stop_momentum(&mut self) {
        self.momentum_remaining = 0.;
        self.momentum_last_input = None;
        self.momentum_frame_time = None;
        self.momentum_generation = self.momentum_generation.wrapping_add(1);
        self.momentum_extrapolated = None;
    }

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.table_resize_drag = None;
        self.is_selecting = false;
        self.stop_drag_scroll();
        self.table_selection_drag = None;
        self.find.cancel_navigation();
        self.find.read_only_match = None;
        self.html_anchor_jump = None;
        self.stop_momentum();
        self.focus_handle.focus(window, cx);
        if event.click_count == 1
            && event.modifiers.shift
            && !self.document.composition_active()
            && self.extend_preview_selection(event.position)
        {
            self.is_selecting = true;
            self.sync_layout_focus(window, cx);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if event.click_count == 1
            && event.modifiers.control
            && !event.modifiers.shift
            && let Some(target) = self.html_link_at(event.position)
        {
            self.open_link_target(&target, window, cx);
            cx.stop_propagation();
            return;
        }
        if event.click_count == 1
            && event.modifiers.control
            && !event.modifiers.shift
            && let Some(target) = self.table_cell_at(event.position)
        {
            let selection = RectangularSelection {
                table_id: target.table_id,
                anchor_row: target.row,
                anchor_column: target.column,
                head_row: target.row,
                head_column: target.column,
            };
            if let Err(error) =
                self.apply_command(EditCommand::SetSelection(Selection::Table(selection)))
            {
                self.record_error(error, window);
                return;
            }
            self.html_selection = None;
            self.table_selection_drag = Some(TableSelectionDrag {
                table_id: target.table_id,
                anchor_row: target.row,
                anchor_column: target.column,
            });
            self.table_hover = Some(target);
            self.animate_toolbar(false, cx);
            cx.emit(EditorEvent::ViewChanged);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if event.click_count == 1
            && !event.modifiers.control
            && !event.modifiers.shift
            && let Some((target, edge)) = self.table_edge_at(event.position)
        {
            self.open_table_edge_menu(target, edge, event.position, window, cx);
            cx.stop_propagation();
            return;
        }
        if let Some((node, preview, position)) = self.html_target_at(event.position)
            && let Some(byte) = preview.byte_for_position(position)
        {
            if self.document.composition_active() {
                return;
            }
            let anchor = self
                .html_selection
                .as_ref()
                .filter(|selection| event.modifiers.shift && selection.node == node)
                .map_or(byte, |selection| selection.anchor);
            let mut selection = HtmlSelection {
                cross: None,
                node,
                preview,
                anchor,
                head: byte,
            };
            if event.click_count == 2 {
                let range = word_range_at(&selection.preview.editable_text, byte);
                selection.anchor = range.start;
                selection.head = range.end;
            }
            self.html_selection = Some(selection);
            self.is_selecting = event.click_count == 1;
            self.preferred_x = None;
            self.animate_toolbar(false, cx);
            self.sync_layout_focus(window, cx);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        self.html_selection = None;
        if let Some(drag) = self.table_resize_at(event.position) {
            self.table_resize_drag = Some(drag);
            self.table_hover = None;
            self.table_edge_menu = None;
            self.animate_toolbar(false, cx);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if let Some(item_id) = self.task_item_at_marker(event.position) {
            self.toggle_task(item_id, window, cx);
            return;
        }
        let offset = self.index_for_mouse_position(event.position);
        if event.click_count == 2 {
            let range = word_range_at(self.projection.text(), offset);
            if !range.is_empty() {
                self.is_selecting = false;
                self.set_selection(range, false, window, cx);
                cx.stop_propagation();
                return;
            }
        }
        if event.click_count == 1
            && !event.modifiers.shift
            && let Some(target) = self.link_at_offset(offset)
        {
            self.open_link_target(&target, window, cx);
            cx.stop_propagation();
            return;
        }
        self.preferred_x = None;
        self.is_selecting = true;
        if event.modifiers.shift {
            self.select_to(offset, window, cx);
        } else {
            self.move_to(offset, window, cx);
        }
    }

    fn task_item_at_marker(&self, position: Point<Pixels>) -> Option<document_core::NodeId> {
        let line = self.painted_lines.iter().min_by(|left, right| {
            distance_to_bounds(position, &left.bounds)
                .total_cmp(&distance_to_bounds(position, &right.bounds))
        })?;
        if !task_checkbox_bounds(line.bounds, self.zoom_factor)
            .dilate(px(4. * self.zoom_factor))
            .contains(&position)
        {
            return None;
        }
        let segment = segment_for_line(&self.projection, &line.range)?;
        segment.context.task_checked?;
        if line.range.start != segment.projection_start() {
            return None;
        }
        self.document
            .snapshot()
            .list_item_containing(segment.node_id)
            .map(|(_, item_id, _)| item_id)
    }

    fn table_resize_at(&self, position: Point<Pixels>) -> Option<TableResizeDrag> {
        self.painted_lines.iter().find_map(|line| {
            let hit = table_resize::hit_bounds(line.bounds, self.zoom_factor, line.content_mask)?;
            if !hit.contains(&position) {
                return None;
            }
            let edge = line.bounds.right() + px(12. * self.zoom_factor);
            let segment = segment_for_line(&self.projection, &line.range)?;
            let (table_id, _, column) = segment.context.table_cell?;
            if self
                .projection
                .uses_record_layout(table_id, self.layout_width / self.zoom_factor)
            {
                // There is no horizontal column boundary in a stacked record.
                // Explicit widths remain available through the table toolbar.
                return None;
            }
            let BlockNode::Table(table) = self.projection.block(table_id)? else {
                return None;
            };
            let initial_width = table.columns.get(column)?.width.unwrap_or_else(|| {
                if let Some(spec) = self
                    .visual_lines
                    .iter()
                    .find(|spec| spec.projected_range() == line.range)
                {
                    return spec.width_fraction * self.layout_width / self.zoom_factor;
                }
                self.projection
                    .table_widths(table_id)
                    .and_then(|widths| widths.get(column))
                    .copied()
                    .unwrap_or(160.)
            });
            Some(TableResizeDrag {
                table_id,
                column,
                pointer_x: position.x,
                edge_x: edge,
                initial_width,
                current_width: initial_width,
            })
        })
    }

    fn table_cell_at(&self, position: Point<Pixels>) -> Option<TableCellTarget> {
        let line = self
            .painted_lines
            .get(self.table_line_hit_index(position)?)?;
        let segment = segment_for_line(&self.projection, &line.range)?;
        let (table_id, row, column) = segment.context.table_cell?;
        Some(TableCellTarget {
            table_id,
            row,
            column,
        })
    }

    fn table_cell_geometry(&self, line: &PaintedLine) -> Option<(TableCellTarget, Bounds<Pixels>)> {
        let element = self.element_bounds?;
        let segment = segment_for_line(&self.projection, &line.range)?;
        let (table_id, row, column) = segment.context.table_cell?;
        let spec = self
            .visual_lines
            .iter()
            .find(|candidate| candidate.projected_range() == line.range)?;
        if !spec.table_cell_first {
            return None;
        }
        let width: f32 = element.size.width.into();
        let horizontal_offset = self
            .horizontal_scrolls
            .get(&table_id)
            .copied()
            .unwrap_or(0.);
        Some((
            TableCellTarget {
                table_id,
                row,
                column,
            },
            Bounds::new(
                point(
                    element.left() + px(width * spec.x_fraction - horizontal_offset),
                    element.top() + px(spec.table_row_y),
                ),
                size(
                    px((width * spec.width_fraction).max(1.)),
                    px(spec.table_row_height.max(1.)),
                ),
            ),
        ))
    }

    fn table_edge_controls(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(target) = self.table_hover else {
            return Vec::new();
        };
        if self
            .projection
            .uses_record_layout(target.table_id, self.layout_width / self.zoom_factor)
        {
            return Vec::new();
        }
        let Some(element) = self.element_bounds else {
            return Vec::new();
        };
        let Some((_, global_cell)) = self.painted_lines.iter().find_map(|line| {
            let geometry = self.table_cell_geometry(line)?;
            (geometry.0 == target).then_some(geometry)
        }) else {
            return Vec::new();
        };
        let cell = Bounds::new(
            point(
                global_cell.left() - element.left(),
                global_cell.top() - element.top(),
            ),
            global_cell.size,
        );
        let editor = cx.entity();
        [
            TableEdge::Top,
            TableEdge::Right,
            TableEdge::Bottom,
            TableEdge::Left,
        ]
        .into_iter()
        .map(|edge| {
            table_edge_control(
                editor.clone(),
                target,
                edge,
                cell,
                self.zoom_factor,
                TachyonPalette::for_dark(cx.theme().is_dark()).accent,
            )
        })
        .collect()
    }

    fn table_edge_at(&self, position: Point<Pixels>) -> Option<(TableCellTarget, TableEdge)> {
        let target = self.table_hover.or_else(|| self.table_cell_at(position))?;
        if self
            .projection
            .uses_record_layout(target.table_id, self.layout_width / self.zoom_factor)
        {
            return None;
        }
        let (_, cell) = self.painted_lines.iter().find_map(|line| {
            let geometry = self.table_cell_geometry(line)?;
            (geometry.0 == target).then_some(geometry)
        })?;
        [
            TableEdge::Top,
            TableEdge::Right,
            TableEdge::Bottom,
            TableEdge::Left,
        ]
        .into_iter()
        .find(|edge| table_edge_hit_bounds(cell, *edge).contains(&position))
        .map(|edge| (target, edge))
    }

    fn open_table_edge_menu(
        &mut self,
        target: TableCellTarget,
        edge: TableEdge,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(BlockNode::Table(table)) = self.projection.block(target.table_id) else {
            return;
        };
        let row_count = table.row_count();
        let column_count = table.column_count();
        let menu_editor = cx.entity();
        let menu = PopupMenu::build(window, cx, move |menu, _, _| {
            table_edge_menu(
                menu,
                menu_editor.clone(),
                target,
                edge,
                row_count,
                column_count,
            )
        });
        let owner = cx.entity();
        let subscription = window.subscribe(&menu, cx, move |_, _: &DismissEvent, _, cx| {
            owner.update(cx, |editor, cx| {
                editor.table_edge_menu = None;
                cx.notify();
            });
        });
        menu.focus_handle(cx).focus(window, cx);
        self.table_hover = Some(target);
        self.table_edge_menu = Some((menu, position, subscription));
        self.animate_toolbar(false, cx);
        cx.notify();
    }

    fn update_table_drag_selection(
        &mut self,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(drag) = self.table_selection_drag else {
            return;
        };
        let Some(target) = self.table_cell_at(position) else {
            return;
        };
        if target.table_id != drag.table_id {
            return;
        }
        let next = RectangularSelection {
            table_id: drag.table_id,
            anchor_row: drag.anchor_row,
            anchor_column: drag.anchor_column,
            head_row: target.row,
            head_column: target.column,
        };
        if matches!(&self.selection, Selection::Table(selection) if *selection == next) {
            return;
        }
        if let Err(error) = self.apply_command(EditCommand::SetSelection(Selection::Table(next))) {
            self.record_error(error, window);
            self.table_selection_drag = None;
            return;
        }
        self.table_hover = Some(target);
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
    }

    fn link_at_offset(&self, offset: usize) -> Option<String> {
        let position = self.position_for_offset(offset, Affinity::Downstream)?;
        let block = self.projection.block(position.node_id)?;
        if let BlockNode::Image(image) = block {
            return image.link.as_ref().map(|link| link.target.0.clone());
        }
        let text = block.text()?;
        text.runs()
            .iter()
            .find(|run| {
                run.range.contains(&position.text_offset)
                    || (position.text_offset > 0 && run.range.end == position.text_offset)
            })
            .and_then(|run| {
                run.styles.iter().find_map(|style| match style {
                    InlineStyle::Link(target) => Some(target.0.clone()),
                    _ => None,
                })
            })
    }

    fn on_context_menu(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.table_edge_menu = None;
        if let Some(target) = self.table_cell_at(event.position) {
            // The context target is pointer-local. Keep the authored caret and
            // text selection intact; table commands receive this explicit
            // address instead of depending on selected text.
            self.table_hover = Some(target);
            self.stop_momentum();
            self.animate_toolbar(false, cx);
            cx.notify();
            return;
        }
        self.table_hover = None;
        if self.html_edit_command_at(event.position).is_some() {
            // Opening a local HTML command must not move the canonical caret
            // into a neighboring paragraph or convert any source bytes.
            self.stop_momentum();
            self.animate_toolbar(false, cx);
            return;
        }
        let offset = self
            .html_selection
            .as_ref()
            .and_then(|selection| selection.cross.as_ref())
            .and_then(|cross| cross.byte(&self.preview_position_at(event.position)?))
            .unwrap_or_else(|| self.index_for_mouse_position(event.position));
        let (selection, _) = self.selected_byte_range();
        if !selection.contains(&offset) {
            self.move_to(offset, window, cx);
        }
        self.animate_toolbar(false, cx);
        cx.notify();
    }

    fn html_edit_command_at(&self, position: Point<Pixels>) -> Option<EditCommand> {
        let (node_id, preview, position) = self.html_target_at(position)?;
        Some(EditCommand::ConvertHtmlToMarkdownAt {
            node_id,
            expected_source: preview.source.to_string(),
            position,
        })
    }

    fn html_target_at(
        &self,
        position: Point<Pixels>,
    ) -> Option<(
        NodeId,
        Arc<crate::html::HtmlPreview>,
        document_core::HtmlTextPosition,
    )> {
        let (node, preview, x, y) = self.html_preview_at(position)?;
        let position = preview.text_position_at(x, y)?;
        Some((node, preview, position))
    }

    fn html_link_at(&self, position: Point<Pixels>) -> Option<String> {
        let (_, preview, x, y) = self.html_preview_at(position)?;
        preview.link_at(x, y).map(|link| link.target.clone())
    }

    fn can_open_link(&self, target: &str) -> bool {
        document_core::resolve_link(target, self.document_directory.as_deref()).is_ok()
    }

    /// Navigate canonical headings and authored HTML IDs without a content
    /// transaction. Duplicate destinations resolve in source order.
    pub fn navigate_to_heading(
        &mut self,
        fragment: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.document.composition_active() {
            return false;
        }
        if fragment.is_empty() {
            self.html_selection = None;
            self.move_to(0, window, cx);
            self.set_scroll_y(0., cx);
            return true;
        }
        let heading = document_core::heading_node(self.document.snapshot().blocks(), fragment);
        let heading_offset = heading.and_then(|node| {
            self.projection
                .segments()
                .iter()
                .find(|segment| segment.node_id == node)
                .map(|segment| segment.projection_start())
        });
        if let Some(result) = self.navigate_to_html_anchor(fragment, heading_offset, window, cx) {
            return result;
        }
        if let Some(node) = heading {
            self.jump_to_node(node, window, cx);
            true
        } else {
            false
        }
    }

    fn open_link_target(&mut self, target: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.document.composition_active() {
            return;
        }
        let error = match document_core::resolve_link(target, self.document_directory.as_deref()) {
            Ok(document_core::LinkDestination::External(url)) => {
                cx.open_url(&url);
                None
            }
            Ok(document_core::LinkDestination::Document { path, fragment }) => {
                cx.emit(EditorEvent::OpenLocalDocument { path, fragment });
                None
            }
            Ok(document_core::LinkDestination::Heading(fragment)) => (!self
                .navigate_to_heading(&fragment, window, cx))
            .then(|| format!("Heading not found: #{fragment}")),
            Err(error) => Some(error.to_string()),
        };
        if let Some(error) = error {
            cx.emit(EditorEvent::LinkFailed(error));
            window.play_system_bell();
        }
    }

    fn open_link(&mut self, _: &OpenLink, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(selection) = &self.html_selection {
            if let Some(target) = selection
                .preview
                .link_at_byte(selection.head)
                .map(|link| link.target.clone())
            {
                self.open_link_target(&target, window, cx);
            } else {
                window.play_system_bell();
            }
        } else if !self.document.composition_active() {
            if let Some(target) = self.link_at_offset(self.cursor_offset()) {
                self.open_link_target(&target, window, cx);
            } else if let Some(action) = self.footnote_at_offset(self.cursor_offset()) {
                self.navigate_footnote(action, window, cx);
            } else {
                window.play_system_bell();
            }
        }
    }

    fn html_preview_at(
        &self,
        position: Point<Pixels>,
    ) -> Option<(NodeId, Arc<crate::html::HtmlPreview>, f32, f32)> {
        let bounds = self.element_bounds?;
        let x = f32::from(position.x - bounds.origin.x);
        let y = f32::from(position.y - bounds.origin.y);
        let width = f32::from(bounds.size.width);
        for index in self
            .components
            .visible_range(&self.visual_lines, &self.paint_order, y, y)
        {
            let line = &self.visual_lines[self.paint_order[index]];
            let Some(preview) = &line.html_preview else {
                continue;
            };
            if y < line.y || y > line.y + line.style.line_height {
                continue;
            }
            let horizontal_offset = line
                .table_cell
                .map(|(table, _, _, _)| table)
                .or_else(|| {
                    segment_for_line(&self.projection, &line.projected_range()).map(|s| s.node_id)
                })
                .and_then(|owner| self.horizontal_scrolls.get(&owner).copied())
                .unwrap_or(0.);
            if line.table_cell.is_some() {
                let (left, available) =
                    table_viewport_geometry(line, &self.projection, width, self.zoom_factor);
                let cell_left = line.x_fraction * width - horizontal_offset;
                let cell_right = cell_left + line.width_fraction * width;
                if x < left.max(cell_left) || x > (left + available).min(cell_right) {
                    continue;
                }
            } else {
                let left = line.x_fraction * width + line.inset;
                let right = (line.x_fraction + line.width_fraction) * width - 8. * self.zoom_factor;
                if x < left || x > right {
                    continue;
                }
            }
            let local_x =
                (x + horizontal_offset - line.x_fraction * width - line.inset) / self.zoom_factor;
            let local_y = (y - line.y) / self.zoom_factor;
            if !(local_x >= 0.
                && local_x <= preview.width
                && local_y >= 0.
                && local_y <= preview.height)
            {
                continue;
            }
            let segment = self.projection.segment_for_range(&line.projected_range())?;
            let BlockNode::PreservedSource { source, .. } =
                self.projection.block(segment.node_id)?
            else {
                return None;
            };
            if source.as_ref() != preview.source.as_ref() {
                return None;
            }
            return Some((segment.node_id, preview.clone(), local_x, local_y));
        }
        None
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
        if self.table_selection_drag.is_some() {
            self.update_table_drag_selection(event.position, window, cx);
            self.table_selection_drag = None;
            self.is_selecting = false;
            self.stop_drag_scroll();
            self.animate_toolbar(false, cx);
            self.refresh_source_focus();
            cx.notify();
            return;
        }
        if self.is_selecting && self.drag_scroll.is_some() {
            self.extend_drag_selection(event.position, window, cx);
        }
        self.stop_drag_scroll();
        if self.html_selection.is_some() {
            self.is_selecting = false;
            self.sync_layout_focus(window, cx);
            self.schedule_selection_toolbar(cx);
            cx.notify();
            return;
        }
        if let Some(drag) = self.table_resize_drag.take() {
            let width = drag.width_at(event.position.x, self.zoom_factor);
            // Ignore a click (and tiny pointer jitter), including on automatic
            // columns: no authored metadata, dirty event, or undo entry.
            if f32::from(event.position.x - drag.pointer_x).abs() >= 2.
                && (width - drag.initial_width).abs() > f32::EPSILON
            {
                self.apply_structural_command(
                    EditCommand::SetTableColumnWidth {
                        table_id: drag.table_id,
                        column: drag.column,
                        width,
                    },
                    window,
                    cx,
                );
            }
            cx.notify();
            return;
        }
        self.is_selecting = false;
        self.animate_toolbar(
            !self.selected_byte_range().0.is_empty() || self.current_image().is_some(),
            cx,
        );
        self.refresh_source_focus();
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(drag) = &mut self.table_resize_drag {
            if event.pressed_button == Some(MouseButton::Left) {
                drag.current_width = drag.width_at(event.position.x, self.zoom_factor);
            } else {
                self.table_resize_drag = None;
            }
            cx.notify();
            return;
        }
        // Edge targets extend outside the cell. Retain that cell while moving
        // onto its knob; otherwise the control disappears before mouse-down.
        let table_hover = self
            .table_edge_at(event.position)
            .map(|(target, _)| target)
            .or_else(|| self.table_cell_at(event.position));
        if table_hover != self.table_hover {
            self.table_hover = table_hover;
            cx.notify();
        }
        if self.table_selection_drag.is_some() {
            if event.pressed_button == Some(MouseButton::Left) {
                self.update_table_drag_selection(event.position, window, cx);
            } else {
                self.table_selection_drag = None;
            }
            return;
        }
        if self.is_selecting && event.pressed_button != Some(MouseButton::Left) {
            self.is_selecting = false;
            self.stop_drag_scroll();
            return;
        }
        if self.is_selecting {
            self.extend_drag_selection(event.position, window, cx);
            self.update_drag_scroll(event.position, window, cx);
        }
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.touch_phase == gpui::TouchPhase::Cancelled {
            self.stop_momentum();
            cx.stop_propagation();
            return;
        }
        if event.touch_phase == gpui::TouchPhase::Started {
            self.stop_momentum();
        }
        let delta = event.delta.pixel_delta(px(LINE_HEIGHT));
        // A zero-delta release must not erase the last velocity or invalidate
        // the frame chain. The pinned Wayland backend sends only Moved;
        // coasting must not depend on receiving a release event.
        if delta.x == px(0.) && delta.y == px(0.) {
            cx.stop_propagation();
            return;
        }
        self.table_resize_drag = None;
        self.find.cancel_navigation();
        self.jump_generation = self.jump_generation.saturating_add(1);
        self.animate_toolbar(false, cx);
        self.link_popover_visible = false;
        let delta_x: f32 = if event.modifiers.shift && delta.x == px(0.) {
            delta.y.into()
        } else {
            delta.x.into()
        };
        if !event.modifiers.shift && delta.y.abs() >= px(delta_x.abs()) {
            if !cx.reduce_motion() {
                let dy: f32 = delta.y.into();
                let reversed = dy.signum() != self.momentum_remaining.signum();
                if reversed {
                    self.momentum_remaining = 0.;
                }
                let now = Instant::now();
                let continuous = matches!(event.delta, gpui::ScrollDelta::Pixels(_));
                if continuous {
                    let elapsed = self
                        .momentum_last_input
                        .map(|previous| now.duration_since(previous))
                        .filter(|elapsed| *elapsed <= MOMENTUM_INPUT_WINDOW)
                        .filter(|_| self.momentum_extrapolated.is_some());
                    self.momentum_remaining = match elapsed {
                        Some(elapsed) if !reversed => {
                            let velocity = dy / elapsed.as_secs_f32().clamp(0.008, 0.05);
                            0.4 * self.momentum_remaining + 0.6 * velocity * MOMENTUM_DECAY_SECONDS
                        }
                        _ => dy * 0.8,
                    }
                    .clamp(-MAX_MOMENTUM_DISTANCE, MAX_MOMENTUM_DISTANCE);
                    // Frames predict movement between pixel events. Credit it
                    // against the next input so active finger travel isn't
                    // applied twice. Never correct an overshoot backwards;
                    // carry that credit until subsequent input catches up.
                    let credit = if elapsed.is_some() && !reversed {
                        self.momentum_extrapolated.unwrap_or(0.).abs()
                    } else {
                        0.
                    };
                    let direct = dy.signum() * (dy.abs() - credit).max(0.);
                    self.momentum_extrapolated = Some(dy.signum() * (credit - dy.abs()).max(0.));
                    let offset = self.scroll_handle.offset();
                    let maximum = self.scroll_handle.max_offset().y;
                    self.scroll_handle.set_offset(point(
                        offset.x,
                        (offset.y + px(direct)).clamp(-maximum, px(0.)),
                    ));
                    cx.notify();
                } else {
                    self.momentum_extrapolated = None;
                    self.momentum_remaining = (self.momentum_remaining + dy)
                        .clamp(-MAX_MOMENTUM_DISTANCE, MAX_MOMENTUM_DISTANCE);
                    // Kick off an idle wheel promptly. Later impulses join the
                    // existing animation, without extra synthetic time steps.
                    if self.momentum_frame_time.is_none() {
                        self.advance_momentum(0.09, cx);
                    }
                }
                self.momentum_last_input = Some(now);
                if self.momentum_frame_time.is_none() && self.momentum_remaining != 0. {
                    self.momentum_frame_time = Some(now);
                    self.schedule_momentum_frame(window, cx);
                }
                cx.stop_propagation();
            } else {
                self.stop_momentum();
                let offset = self.scroll_handle.offset();
                let maximum = self.scroll_handle.max_offset().y;
                self.scroll_handle.set_offset(point(
                    offset.x,
                    (offset.y + delta.y).clamp(-maximum, px(0.)),
                ));
                cx.stop_propagation();
                cx.notify();
            }
            cx.emit(EditorEvent::ViewChanged);
            return;
        }
        self.stop_momentum();
        let owner = self.painted_lines.iter().find_map(|line| {
            line.horizontal_owner.filter(|_| {
                line.content_mask
                    .is_some_and(|mask| mask.bounds.contains(&event.position))
            })
        });
        let Some(owner) = owner else {
            return;
        };
        let Some((viewport, content)) = self.horizontal_metrics.get(&owner).copied() else {
            return;
        };
        let current = self.horizontal_scrolls.get(&owner).copied().unwrap_or(0.);
        // GPUI deltas describe content motion (left is negative), whereas our
        // local offset is the positive distance hidden before the viewport.
        let next = clamped_horizontal_scroll(current, -delta_x, viewport, content);
        if (next - current).abs() >= f32::EPSILON {
            self.horizontal_scrolls.insert(owner, next);
            cx.stop_propagation();
            cx.notify();
        }
    }

    fn schedule_momentum_frame(&self, window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        let generation = self.momentum_generation;
        window.on_next_frame(move |window, cx| {
            _ = this.update(cx, |editor, cx| {
                // A cancelled callback may still be queued, including when a
                // new gesture has already started. It must not touch that run.
                if editor.momentum_generation != generation {
                    return;
                }
                let Some(last) = editor.momentum_frame_time else {
                    return;
                };
                if cx.reduce_motion() {
                    editor.stop_momentum();
                    return;
                }
                let now = Instant::now();
                editor.momentum_frame_time = Some(now);
                let before = editor.scroll_handle.offset().y;
                let running = editor.advance_momentum(now.duration_since(last).as_secs_f32(), cx);
                if let Some(credit) = editor.momentum_extrapolated.as_mut() {
                    *credit += f32::from(editor.scroll_handle.offset().y - before);
                }
                if running {
                    editor.schedule_momentum_frame(window, cx);
                } else {
                    editor.stop_momentum();
                }
            });
        });
    }

    fn advance_momentum(&mut self, dt: f32, cx: &mut Context<Self>) -> bool {
        let remaining = self.momentum_remaining * (-dt.max(0.) / MOMENTUM_DECAY_SECONDS).exp();
        let delta = self.momentum_remaining - remaining;
        let offset = self.scroll_handle.offset();
        let current: f32 = offset.y.into();
        let maximum: f32 = self.scroll_handle.max_offset().y.into();
        let next = (current + delta).clamp(-maximum.max(0.), 0.);
        self.momentum_remaining = if (next - current - delta).abs() > 0.1 {
            0.
        } else {
            remaining
        };
        if self.momentum_remaining.abs() < 0.1 {
            self.momentum_remaining = 0.;
        }
        self.scroll_handle.set_offset(point(offset.x, px(next)));
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
        self.momentum_remaining != 0.
    }
}

impl gpui::EventEmitter<EditorEvent> for RichDocumentEditor {}

impl Drop for RichDocumentEditor {
    fn drop(&mut self) {
        self.cancel_owned_composition_on_detach();
    }
}

impl Focusable for RichDocumentEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl EntityInputHandler for RichDocumentEditor {
    fn text_for_range(
        &mut self,
        range_utf16: Range<usize>,
        actual_range: &mut Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<String> {
        if let Some(selection) = &self.html_selection {
            let text = selection.text();
            let range = utf16_range_in_text(text, range_utf16);
            actual_range.replace(
                text[..range.start].encode_utf16().count()
                    ..text[..range.end].encode_utf16().count(),
            );
            return text.get(range).map(ToOwned::to_owned);
        }
        let range = self.projection.range_from_utf16(range_utf16);
        actual_range.replace(self.projection.range_to_utf16(range.clone())?);
        self.projection.text().get(range).map(ToOwned::to_owned)
    }

    fn selected_text_range(
        &mut self,
        _: bool,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<UTF16Selection> {
        if let Some(selection) = &self.html_selection {
            let text = selection.text();
            let range = selection.range();
            return Some(UTF16Selection {
                range: text[..range.start].encode_utf16().count()
                    ..text[..range.end].encode_utf16().count(),
                reversed: selection.head < selection.anchor,
            });
        }
        let (range, reversed) = self.selected_byte_range();
        Some(UTF16Selection {
            range: self.projection.range_to_utf16(range)?,
            reversed,
        })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked_range
            .clone()
            .and_then(|range| self.projection.range_to_utf16(range))
    }

    fn unmark_text(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Err(error) = self.commit_pending_composition(cx) {
            self.record_error(error, window);
        }
        self.marked_range = None;
        cx.notify();
    }

    fn replace_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document.composition_active() && self.composition_origin.is_none() {
            self.record_error(DocumentError::CompositionAlreadyActive, window);
            return;
        }
        // Platform IME cancellation is reported as replacement of the marked
        // range with empty text. Restore the composition snapshot instead of
        // committing an empty edit, which would serialize untouched Markdown
        // around the provisional range.
        if text.is_empty() && self.document.composition_active() {
            if let Err(error) = self.cancel_pending_composition(cx) {
                self.record_error(error, window);
            }
            return;
        }
        if let Some(selection) = &self.html_selection {
            let range = range_utf16.map_or_else(
                || selection.range(),
                |range| utf16_range_in_text(selection.text(), range),
            );
            if text == "\n" {
                self.split_range(range, window, cx);
            } else {
                self.replace_html_range(range, text, window, cx);
            }
            return;
        }
        let range = range_utf16
            .map(|range| self.projection.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_byte_range().0);
        let edited_node = self
            .projection
            .position_at(range.start, Affinity::Downstream)
            .map(|position| position.node_id);

        if self.document.composition_active() {
            match self.document.update_composition(text.to_owned()) {
                Ok(snapshot) => {
                    self.selection = snapshot.selection().clone();
                    match self.document.commit_composition() {
                        Ok(snapshot) => {
                            self.composition_origin = None;
                            self.selection = snapshot.selection().clone();
                            self.marked_range = None;
                            if edited_node.is_none_or(|node_id| !self.refresh_text_node(node_id)) {
                                self.refresh_projection();
                                self.shaped_line_cache.borrow_mut().clear();
                            }
                            cx.emit(EditorEvent::Changed);
                            cx.notify();
                        }
                        Err(error) => self.record_error(error, window),
                    }
                }
                Err(error) => self.record_error(error, window),
            }
        } else if text == "\n" {
            self.split_range(range, window, cx);
        } else {
            self.replace_range(range, text, true, window, cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        range_utf16: Option<Range<usize>>,
        new_text: &str,
        new_selected_range_utf16: Option<Range<usize>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.document.composition_active() && self.composition_origin.is_none() {
            self.record_error(DocumentError::CompositionAlreadyActive, window);
            return;
        }
        let starting_html = self.html_selection.is_some();
        let mut structural_start = starting_html;
        if let Some(mut origin) = self.html_selection.clone() {
            if let Some(range) = range_utf16.clone() {
                let range = utf16_range_in_text(origin.text(), range);
                origin.anchor = range.start;
                origin.head = range.end;
            }
            let target = origin
                .position(origin.anchor, Affinity::Downstream)
                .zip(origin.position(origin.head, Affinity::Upstream));
            let Some((anchor, head)) = target else {
                self.record_error(
                    DocumentError::Html("Choose the HTML text again".into()),
                    window,
                );
                return;
            };
            if let Err(error) =
                self.document
                    .begin_preview_composition(&document_core::PreviewSelection {
                        revision: origin.cross.as_ref().map_or_else(
                            || self.document.snapshot().revision(),
                            |cross| cross.revision,
                        ),
                        anchor,
                        head,
                    })
            {
                self.record_error(error, window);
                return;
            }
            self.composition_origin = Some(CompositionOrigin::Html(origin));
            self.html_selection = None;
        }
        let replacement = range_utf16
            .map(|range| self.projection.range_from_utf16(range))
            .or_else(|| self.marked_range.clone())
            .unwrap_or_else(|| self.selected_byte_range().0);

        if !self.document.composition_active() {
            let Some(anchor) = self.position_for_offset(replacement.start, Affinity::Downstream)
            else {
                window.play_system_bell();
                return;
            };
            let Some(head) = self.position_for_offset(replacement.end, Affinity::Upstream) else {
                window.play_system_bell();
                return;
            };
            structural_start |= anchor.node_id != head.node_id;
            if let Err(error) = self
                .document
                .begin_composition(TextSelection { anchor, head })
            {
                self.record_error(error, window);
                return;
            }
            self.composition_origin = Some(CompositionOrigin::Markdown);
        }

        match self.document.update_composition(new_text.to_owned()) {
            Ok(snapshot) => {
                self.last_error = None;
                self.selection = snapshot.selection().clone();
                let edited_node = match &self.selection {
                    Selection::Text(selection) => Some(selection.head.node_id),
                    _ => None,
                };
                if starting_html {
                    self.layout_focus = edited_node;
                }
                if structural_start
                    || edited_node.is_none_or(|node_id| !self.refresh_text_node(node_id))
                {
                    self.refresh_projection();
                    self.shaped_line_cache.borrow_mut().clear();
                }
                // Conversion changes projection offsets. Locate the inserted
                // text through its canonical caret after refreshing geometry.
                let end = self.selected_byte_range().0.end;
                let start = end.saturating_sub(new_text.len());
                self.marked_range = (!new_text.is_empty()).then_some(start..end);
                let selected = new_selected_range_utf16.map_or_else(
                    || end..end,
                    |range| {
                        let relative = utf16_range_in_text(new_text, range);
                        start + relative.start..start + relative.end
                    },
                );
                self.set_selection(selected, false, window, cx);
                self.keep_offset_visible(self.cursor_offset());
                cx.notify();
            }
            Err(error) => {
                if starting_html {
                    // Failed preparation must not leave a hidden composition
                    // or discard the original HTML caret.
                    let _ = self.cancel_pending_composition(cx);
                }
                self.record_error(error, window);
            }
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
        if let Some(selection) = &self.html_selection {
            let range = utf16_range_in_text(selection.text(), range_utf16);
            return self.html_caret_bounds(range.start);
        }
        let range = self.projection.range_from_utf16(range_utf16);
        let line = painted_line_for_offset(&self.painted_lines, range.start)?;
        let start = range
            .start
            .saturating_sub(line.range.start)
            .min(line.range.len());
        let end = range
            .end
            .saturating_sub(line.range.start)
            .min(line.range.len());
        Some(Bounds::from_corners(
            point(
                aligned_text_left(line.bounds, &line.layout, line.alignment)
                    + shaped_x_for_index(&line.layout, start)
                        .min(shaped_x_for_index(&line.layout, end)),
                line.bounds.top(),
            ),
            point(
                aligned_text_left(line.bounds, &line.layout, line.alignment)
                    + shaped_x_for_index(&line.layout, start)
                        .max(shaped_x_for_index(&line.layout, end)),
                line.bounds.bottom(),
            ),
        ))
    }

    fn character_index_for_point(
        &mut self,
        point: Point<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<usize> {
        if let Some(selection) = &self.html_selection {
            if let Some(cross) = &selection.cross {
                let byte = cross.byte(&self.preview_position_at(point)?)?;
                return Some(cross.text[..byte].encode_utf16().count());
            }
            let (node, preview, position) = self.html_target_at(point)?;
            if node != selection.node {
                return None;
            }
            let byte = preview.byte_for_position(position)?;
            return Some(preview.editable_text[..byte].encode_utf16().count());
        }
        self.projection
            .utf16_offset_for_byte(self.index_for_mouse_position(point))
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(selection) = &self.html_selection {
            let range = utf16_range_in_text(selection.text(), range_utf16);
            self.set_selection(range, false, window, cx);
            return;
        }
        self.set_selection(
            self.projection.range_from_utf16(range_utf16),
            false,
            window,
            cx,
        );
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        if let Some(selection) = &self.html_selection {
            return Some(selection.text().encode_utf16().count());
        }
        Some(self.projection.utf16_len())
    }
}

impl gpui::Render for RichDocumentEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_find(window, cx);
        self.sync_layout_focus(window, cx);
        let palette = TachyonPalette::for_dark(cx.theme().is_dark());
        if !self
            .measurement
            .matches(&cx.theme().font_family, self.zoom_factor)
            || self.measurement.typography != self.typography
        {
            self.measurement = Arc::new(
                FontMeasurement::new(
                    cx.text_system().clone(),
                    cx.theme().font_family.clone(),
                    self.zoom_factor,
                )
                .with_typography(self.typography),
            );
            self.projection.table_layout_lock = None;
            self.measured_layout = false;
            self.text_environment_pending = true;
            self.geometry_generation = self.geometry_generation.wrapping_add(1);
        }
        let layout_width = self
            .element_bounds
            .map_or(self.layout_width, |bounds| f32::from(bounds.size.width));
        // First paint establishes the actual canvas and notifies us via Ready.
        // Do not seed measured-layout hysteresis with the provisional 760 px
        // preparation width before that geometry exists.
        if self.element_bounds.is_some() {
            self.sync_image_dimensions(layout_width, cx);
        }
        let (toolbar_left, toolbar_top) = self.selection_toolbar_origin();
        let active_image = self.current_image();
        let image_tools = active_image.as_ref().map(|_| {
            div()
                .flex()
                .gap(px(2.))
                .child(
                    Button::new("edit-image")
                        .ghost()
                        .small()
                        .label("Image…")
                        .tooltip("Edit image")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.show_image_editor(window, cx);
                        })),
                )
                .child(
                    Button::new("retry-image")
                        .ghost()
                        .small()
                        .label("Retry")
                        .tooltip("Retry loading image")
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.retry_current_image(window, cx);
                        })),
                )
        });
        let link_popover = self.link_popover_visible.then(|| {
            div()
                .id("link-popover")
                .absolute()
                .top(px(toolbar_top))
                .left(px(toolbar_left))
                .flex()
                .items_center()
                .w(px(360.))
                .h(px(40.))
                .p(px(4.))
                .gap(px(4.))
                .rounded(px(8.))
                .bg(rgb(palette.floating))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .child(div().flex_1().child(Input::new(&self.link_input).small()))
                .child(
                    div()
                        .id("apply-link")
                        .px(px(8.))
                        .py(px(5.))
                        .rounded(px(4.))
                        .bg(rgb(palette.accent))
                        .text_color(rgb(palette.surface_quiet))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.apply_link(window, cx);
                        }))
                        .child("Apply"),
                )
        });
        let image_popover = self.image_popover_node.map(|_| {
            div()
                .id("image-popover")
                .absolute()
                .top(px(toolbar_top))
                .left(px(toolbar_left))
                .flex()
                .flex_col()
                .w(px(440.))
                .p(px(6.))
                .gap(px(4.))
                .rounded(px(8.))
                .bg(rgb(palette.floating))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .child(Input::new(&self.image_source_input).small())
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(4.))
                        .child(
                            div()
                                .flex_1()
                                .child(Input::new(&self.image_alt_input).small()),
                        )
                        .child(
                            div()
                                .id("apply-image")
                                .px(px(8.))
                                .py(px(5.))
                                .rounded(px(4.))
                                .bg(rgb(palette.accent))
                                .text_color(rgb(palette.surface_quiet))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_image_attributes(window, cx);
                                }))
                                .child("Apply"),
                        )
                        .child(
                            div()
                                .id("retry-image-popover")
                                .px(px(8.))
                                .py(px(5.))
                                .rounded(px(4.))
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.retry_current_image(window, cx);
                                }))
                                .child("Retry"),
                        ),
                )
        });
        let scroll_y: f32 = (-self.scroll_handle.offset().y).into();
        let viewport_height: f32 = self.scroll_handle.bounds().size.height.into();
        let viewport_height = viewport_height.max(800.);
        let image_top = (scroll_y - viewport_height).max(0.);
        let image_bottom = scroll_y + viewport_height * 2.;
        let visible_order = self
            .components
            .visible_range(
                &self.visual_lines,
                &self.paint_order,
                image_top,
                image_bottom,
            )
            .map(|order_index| self.paint_order[order_index])
            .collect::<Vec<_>>();
        self.request_html_images(&visible_order, window, cx);
        let mut inline_math_elements = Vec::new();
        for index in &visible_order {
            let line = &self.visual_lines[*index];
            let Some(inline) = &line.inline_math else {
                continue;
            };
            let scale = line.style.font_size / inline.font_size;
            let segment = segment_for_line(&self.projection, &line.projected_range());
            let horizontal_offset = segment
                .and_then(|s| s.context.table_cell)
                .and_then(|(id, _, _)| self.horizontal_scrolls.get(&id).copied())
                .unwrap_or(0.);
            let text_bounds = visual_line_bounds(
                self,
                line,
                Bounds::new(
                    point(px(0.), px(0.)),
                    size(px(self.layout_width), px(self.document_height)),
                ),
                false,
                horizontal_offset,
            );
            let slack = (f32::from(text_bounds.size.width) - inline.width * scale).max(0.);
            let align = match table_column_alignment(&self.projection, segment) {
                ColumnAlignment::Right => slack,
                ColumnAlignment::Center => slack * 0.5,
                _ => 0.,
            };
            let (clip_left, clip_width) = if line.table_cell.is_some() {
                line.slot.map_or((0., self.layout_width), |slot| {
                    (
                        slot.left(self.layout_width / self.zoom_factor) * self.zoom_factor,
                        slot.width(self.layout_width / self.zoom_factor) * self.zoom_factor,
                    )
                })
            } else {
                (0., self.layout_width)
            };
            let baseline = (line.style.line_height - (inline.ascent + inline.descent) * scale)
                * 0.5
                + inline.ascent * scale;
            let mut images = div()
                .absolute()
                .top(px(line.y))
                .left(px(clip_left))
                .w(px(clip_width))
                .h(px(line.style.line_height))
                .overflow_hidden();
            for attachment in &inline.attachments {
                let formula = match &attachment.content {
                    inline_math::Content::Formula { light, dark } => {
                        if cx.theme().is_dark() {
                            dark
                        } else {
                            light
                        }
                    }
                    inline_math::Content::Reference {
                        number,
                        label,
                        advance_em,
                    } => {
                        let Some(note) = self.projection.footnotes.label(label) else {
                            continue;
                        };
                        let action = footnotes::Navigation::Definition(note.definition);
                        images = images.child(
                            footnotes::ReferencePresentation {
                                key: (*index, attachment.range.start),
                                number: *number,
                                action,
                                bounds: Bounds::new(
                                    point(
                                        px(f32::from(text_bounds.left()) - clip_left
                                            + align
                                            + attachment.x * scale),
                                        px(baseline - line.style.font_size),
                                    ),
                                    size(
                                        px(*advance_em * line.style.font_size),
                                        px(line.style.font_size),
                                    ),
                                ),
                                font_size: line.style.font_size,
                            }
                            .element(palette, cx),
                        );
                        continue;
                    }
                };
                let factor = line.style.font_size / crate::math::EM;
                images = images.child(
                    img(ImageSource::Image(formula.image.clone()))
                        .absolute()
                        .left(px(f32::from(text_bounds.left()) - clip_left
                            + align
                            + attachment.x * scale
                            + line.style.font_size / 9.))
                        .top(px(baseline - formula.baseline * factor))
                        .w(px(formula.width * factor))
                        .h(px(formula.height * factor))
                        .object_fit(ObjectFit::Contain),
                );
            }
            inline_math_elements.push(images.into_any_element());
        }
        let image_elements = visible_order
            .iter()
            .map(|index| &self.visual_lines[*index])
            .filter(|line| line.y + line.style.line_height >= image_top)
            .filter_map(|line| {
                let segment = segment_for_line(&self.projection, &line.projected_range())?;
                if line.projected_start() != segment.projection_start() {
                    return None;
                }
                let source = segment.context.image_source.as_deref()?;
                let BlockNode::Image(image) = self.projection.block(segment.node_id)? else {
                    return None;
                };
                let linked = matches!(self.projection.block(segment.node_id), Some(BlockNode::Image(image)) if image.link.is_some());
                let horizontal_offset = segment
                    .context
                    .table_cell
                    .and_then(|(table_id, _, _)| self.horizontal_scrolls.get(&table_id).copied())
                    .unwrap_or(0.);
                let image_source = ImageSource::Resource(resolved_image_resource(
                    source,
                    self.document_directory.as_deref(),
                ));
                let bounds = image_element_bounds(
                    self, line,
                    Bounds::new(point(px(0.), px(0.)), size(px(self.layout_width), px(self.document_height))),
                    horizontal_offset,
                );
                let (clip_left, clip_width) = if line.table_cell.is_some() {
                    table_viewport_geometry(line, &self.projection, self.layout_width, self.zoom_factor)
                } else {
                    (bounds.left().into(), bounds.size.width.into())
                };
                let footer_height = map_preview::footer_height(image) * self.zoom_factor;
                let image_height = (line.style.line_height - footer_height).max(1.);
                let map_footer = map_preview::render(
                    image, self.zoom_factor, bounds.size.width.into(),
                    image.link.as_ref().is_some_and(|link| self.can_open_link(&link.target.0)),
                    palette, cx.entity().downgrade(),
                );
                let image_state = image_state::Presentation::new(
                    image, bounds.size.width.into(), image_height,
                    self.zoom_factor, palette, cx.entity().downgrade(),
                );
                let loading_state = image_state.clone();
                Some(
                    div()
                        .id(("document-image", segment.node_id.get() as usize))
                        .when(linked, |this| this.cursor_pointer())
                        .absolute()
                        .top(bounds.top())
                        .left(px(clip_left))
                        .w(px(clip_width))
                        .h(bounds.size.height)
                        .overflow_hidden()
                        .child(
                            img(image_source)
                                .id(("document-image-content", segment.node_id.get() as usize))
                                .with_loading(move || loading_state.render(false))
                                .with_fallback(move || image_state.render(true))
                                .absolute()
                                .left(bounds.left() - px(clip_left))
                                .w(bounds.size.width)
                                .rounded(px(DocumentStyle::MEDIA_RADIUS * self.zoom_factor))
                                // A percentage height plus the image's intrinsic
                                // aspect ratio can exceed the reserved row. Give
                                // the image the same definite height as geometry.
                                .h(px(image_height))
                                .min_h_0()
                                .max_h(px(image_height))
                                .object_fit(ObjectFit::Contain),
                        )
                        .when_some(map_footer, |view, footer| view.child(
                            div().absolute().top(px(image_height))
                                .left(bounds.left() - px(clip_left)).child(footer)
                        )),
                )
            })
            .collect::<Vec<_>>();
        let mut component_chrome = Vec::<AnyElement>::new();
        let mut accessible_math_scrolls = HashMap::new();
        let mut outer_container_headers = Vec::<AnyElement>::new();
        let mut rendered_alert_headers = HashSet::new();
        let mut rendered_code_headers = HashSet::new();
        for index in &visible_order {
            let Some(line) = self.visual_lines.get(*index) else {
                continue;
            };
            let Some(segment) = segment_for_line(&self.projection, &line.projected_range()) else {
                continue;
            };
            let cell_chrome_start = component_chrome.len();
            if let Some(preview) = &line.html_preview {
                let node_id = segment.node_id;
                let viewport = if line.table_cell.is_some() {
                    preview.width * self.zoom_factor
                } else {
                    (self.layout_width * line.width_fraction - line.inset - 8. * self.zoom_factor)
                        .max(1.)
                };
                let offset = if line.table_cell.is_none() {
                    self.horizontal_scrolls.get(&node_id).copied().unwrap_or(0.)
                } else {
                    0.
                };
                let body = div()
                    .relative()
                    .when(line.table_cell.is_none(), |body| {
                        body.left(px(-offset))
                            .w(px(preview.width * self.zoom_factor))
                    })
                    .when(!preview.text_hits.is_empty(), |body| body.cursor_text())
                    .child(
                        img(ImageSource::Image(preview.image.clone()))
                            .w(px(preview.width * self.zoom_factor))
                            .h(px(preview.height * self.zoom_factor))
                            .object_fit(ObjectFit::Contain),
                    )
                    .children(self.html_link_chrome(node_id, preview))
                    .children(self.html_disclosure_chrome(node_id, preview, window, cx))
                    .children(self.html_selection_chrome(
                        node_id,
                        palette,
                        self.focus_handle.is_focused(window) && window.is_window_active(),
                    ))
                    .into_any_element();
                // Canonical table cells already have one shared clip below.
                // Only standalone fragments need their own overflow viewport.
                let body = if line.table_cell.is_none() {
                    div()
                        .relative()
                        .w(px(viewport))
                        .h(px(preview.height * self.zoom_factor))
                        .overflow_hidden()
                        .child(body)
                        .into_any_element()
                } else {
                    body
                };
                component_chrome.push(
                    div()
                        .absolute()
                        .top(px(line.y))
                        .left(relative(line.x_fraction))
                        .ml(px(line.inset))
                        .w(px(viewport))
                        .child(body)
                        .into_any_element(),
                );
            }
            if let Some((alert_id, kind)) = segment.context.alert.as_ref()
                && rendered_alert_headers.insert(*alert_id)
                && let Some(component) = self.components.get(alert_id)
            {
                let title = match self.projection.block(*alert_id) {
                    Some(BlockNode::Alert {
                        title: Some(title), ..
                    }) => title.as_string(),
                    _ => alert_default_title(kind),
                };
                let color = alert_color(palette, kind);
                let (alert_leading, alert_header) = alert_layout_insets(
                    &self.projection,
                    segment,
                    line.width_fraction * self.layout_width / self.zoom_factor,
                );
                let headers = if line.table_cell.is_some()
                    && self.projection.container_cell(*alert_id).is_none()
                {
                    &mut outer_container_headers
                } else {
                    &mut component_chrome
                };
                headers.push(
                    div()
                        .absolute()
                        .top(px(component.top - alert_header * self.zoom_factor
                            + if alert_header < ALERT_HEADER_HEIGHT {
                                16.
                            } else {
                                9.
                            } * self.zoom_factor))
                        .left(relative(component.left_fraction))
                        .ml(px(
                            -alert_leading * self.zoom_factor + 16. * self.zoom_factor
                        ))
                        .flex()
                        .items_center()
                        .gap(px(9. * self.zoom_factor))
                        .text_size(px(18. * self.zoom_factor))
                        .font_family(DocumentStyle::HEADLINE_FONT_FAMILY)
                        .font_weight(FontWeight::EXTRA_BOLD)
                        .text_color(rgb(color))
                        .child(
                            Icon::new(alert_icon(kind))
                                .with_size(px(20. * self.zoom_factor))
                                .text_color(rgb(color)),
                        )
                        .child(title)
                        .into_any_element(),
                );
            }
            if let Some(BlockNode::CodeBlock(code)) = self.projection.block(segment.node_id)
                && rendered_code_headers.insert(segment.node_id)
                && let Some(first) = self
                    .components
                    .get(&segment.node_id)
                    .and_then(|component| self.visual_lines.get(component.first_line))
            {
                let preview = first.preview_image(cx.theme().is_dark());
                let preview_extent = first.preview_extent();
                let diagram = first.diagram.is_some();
                let preview_name = first
                    .diagram
                    .as_ref()
                    .map(|diagram| diagram.accessible_name());
                let preview_hint = first.diagram.as_ref().map(|diagram| diagram.edit_hint());
                let rendered_equation = first.rendered_code_preview();
                let header_top = if first.command_strip() {
                    first.y
                } else {
                    first.y
                        - (CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING + preview_extent)
                            * self.zoom_factor
                };
                let header_left = first.inset - CODE_BLOCK_PADDING * self.zoom_factor;
                let math = crate::math::is_math(&BlockNode::CodeBlock(code.clone()));
                let language = if math {
                    if preview.is_some() {
                        "Math · source"
                    } else {
                        "Math · source only"
                    }
                } else {
                    code.language.as_deref().unwrap_or("Plain text")
                }
                .to_owned();
                let code_text = code.content.as_string();
                let copy_node = segment.node_id;
                let copied = self.copied_code.is_some_and(|(id, _)| id == copy_node);
                let code_colors = code_palette(palette, &BlockNode::CodeBlock(code.clone()));
                if let Some(preview) = preview {
                    let node_id = segment.node_id;
                    let scroll_handle =
                        self.math_scroll_handles.entry(node_id).or_default().clone();
                    let available = (self.layout_width * first.width_fraction
                        - first.inset
                        - if rendered_equation {
                            8.
                        } else {
                            CODE_BLOCK_PADDING
                        } * self.zoom_factor)
                        .max(1.);
                    let overflow = preview.width * self.zoom_factor > available;
                    let preview_top = if rendered_equation {
                        first.y
                    } else {
                        header_top + (CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING) * self.zoom_factor
                    };
                    let focus = Some({
                        let id =
                            ElementId::Name(format!("math-scroll-focus-{}", node_id.get()).into());
                        window
                            .use_keyed_state(id, cx, |_, cx| cx.focus_handle().tab_stop(true))
                            .read(cx)
                            .clone()
                    });
                    let mut viewport = div()
                        .id(("math-preview", node_id.get() as usize))
                        .debug_selector(move || {
                            if diagram {
                                "diagram-viewport".into()
                            } else {
                                "display-math-viewport".into()
                            }
                        })
                        // Block layout can enlarge an image to its rounded
                        // intrinsic aspect ratio, clipping the denominator
                        // against this exact-height scrolling viewport.
                        .flex()
                        .absolute()
                        .top(px(preview_top))
                        .left(relative(first.x_fraction))
                        .ml(px(first.inset))
                        .w(px(available))
                        .h(px(preview.height * self.zoom_factor))
                        .overflow_x_scroll()
                        .track_scroll(&scroll_handle)
                        .child(
                            img(ImageSource::Image(preview.image.clone()))
                                .debug_selector(move || {
                                    if diagram {
                                        "diagram-image".into()
                                    } else {
                                        "display-math-image".into()
                                    }
                                })
                                .w(px(preview.width * self.zoom_factor))
                                .h(px(preview.height * self.zoom_factor))
                                .ml(px(
                                    if rendered_equation
                                        && first
                                            .diagram
                                            .as_ref()
                                            .is_none_or(|diagram| diagram.centered())
                                    {
                                        ((available - preview.width * self.zoom_factor) / 2.)
                                            .max(0.)
                                    } else {
                                        0.
                                    },
                                ))
                                .flex_shrink_0()
                                .object_fit(ObjectFit::Contain),
                        );
                    if let Some(focus) = focus {
                        let mouse_focus = focus.clone();
                        viewport = viewport
                            .role(Role::Group)
                            .aria_label(preview_name.unwrap_or(if overflow { "Scrollable rendered formula" } else { "Rendered equation" }))
                            .aria_description("Enter edits the source. Use Left and Right to pan, Home and End for either edge. Escape returns to the document.")
                            .aria_keyshortcuts("Enter Left Right Home End Escape")
                            .key_context(MATH_SCROLL_CONTEXT)
                            .track_focus(&focus)
                            .tab_stop(true)
                            .rounded(px(3.))
                            .focus(|style| {
                                style.shadow(vec![BoxShadow {
                                    color: rgb(palette.accent).into(),
                                    offset: point(px(0.), px(0.)),
                                    blur_radius: px(0.),
                                    spread_radius: px(2.),
                                    inset: true,
                                }])
                            })
                            .tooltip(move |window, cx| {
                                gpui_component::tooltip::Tooltip::new(
                                    preview_hint.unwrap_or("Edit formula · Enter. Pan · Left / Right"),
                                )
                                .build(window, cx)
                            })
                            .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                                if overflow {
                                    mouse_focus.focus(window, cx);
                                } else {
                                    this.edit_code_preview_source(node_id, window, cx);
                                }
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(move |this, _: &EditMathSource, window, cx| {
                                window.prevent_default();
                                this.edit_code_preview_source(node_id, window, cx);
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(move |this, _: &ScrollMathLeft, window, cx| {
                                window.prevent_default();
                                this.scroll_math_accessibly(node_id, -1., cx);
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(move |this, _: &ScrollMathRight, window, cx| {
                                window.prevent_default();
                                this.scroll_math_accessibly(node_id, 1., cx);
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(move |this, _: &ScrollMathStart, window, cx| {
                                window.prevent_default();
                                this.set_accessible_math_scroll(node_id, 0., cx);
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(move |this, _: &ScrollMathEnd, window, cx| {
                                window.prevent_default();
                                this.set_accessible_math_scroll(node_id, f32::MAX, cx);
                                cx.stop_propagation();
                            }))
                            .on_action(cx.listener(move |this, _: &ExitMathScroll, window, cx| {
                                window.prevent_default();
                                this.focus_handle.focus(window, cx);
                                cx.stop_propagation();
                            }));
                    }
                    component_chrome.push(viewport.into_any_element());
                    if overflow {
                        accessible_math_scrolls.insert(node_id, scroll_handle.clone());
                        // Use the existing preview/source gap, not the image's
                        // viewport: an overlay would obscure low glyphs and
                        // fraction denominators. This is view state only.
                        component_chrome.push(
                            div()
                                .debug_selector(move || {
                                    if diagram {
                                        "diagram-scrollbar".into()
                                    } else {
                                        "display-math-scrollbar".into()
                                    }
                                })
                                .absolute()
                                .top(px(preview_top + preview.height * self.zoom_factor))
                                .left(relative(first.x_fraction))
                                .ml(px(first.inset))
                                .w(px(available))
                                .h(px(DocumentStyle::EQUATION_SCROLL_RAIL * self.zoom_factor))
                                .child(
                                    Scrollbar::horizontal(&scroll_handle)
                                        .id(("math-scrollbar", node_id.get() as usize))
                                        .viewport_from_layout()
                                        .mode(ScrollbarMode::Always),
                                )
                                .into_any_element(),
                        );
                    }
                    debug_assert!(preview.baseline <= preview.height);
                }
                if !rendered_equation {
                    component_chrome.push(
                        div()
                            .absolute()
                            .top(px(header_top))
                            .left(relative(first.x_fraction))
                            .ml(px(header_left
                                + if first.command_strip() {
                                    CODE_BLOCK_PADDING
                                } else {
                                    12.
                                } * self.zoom_factor))
                            .h(px(if first.command_strip() {
                                DocumentStyle::CODE_LEADING
                            } else {
                                CODE_HEADER_HEIGHT
                            } * self.zoom_factor))
                            .flex()
                            .items_center()
                            .font_family("Spline Sans Mono Tachyon")
                            .text_size(px(DocumentStyle::CAPTION_SIZE * self.zoom_factor))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(code_colors.secondary))
                            .child(language)
                            .into_any_element(),
                    );
                    component_chrome.push(
                        div()
                            .debug_selector(|| "code-copy-command".into())
                            .on_mouse_down(MouseButton::Left, |_, window, cx| {
                                // This is a document command, not a text hit. Stop
                                // the down event before the canvas moves selection.
                                window.prevent_default();
                                cx.stop_propagation();
                            })
                            .absolute()
                            .top(px(header_top))
                            .h(px(if first.command_strip() {
                                DocumentStyle::CODE_LEADING
                            } else {
                                CODE_HEADER_HEIGHT
                            } * self.zoom_factor))
                            .flex()
                            .items_center()
                            .right(px(self.layout_width
                                * (1. - first.x_fraction - first.width_fraction)
                                + (9.
                                    + if first.table_cell.is_some() {
                                        12.
                                    } else {
                                        first.slot.map_or(0., |s| s.inset())
                                    })
                                    * self.zoom_factor))
                            .child(
                                Button::new(("copy-code", segment.node_id.get() as usize))
                                    .ghost()
                                    .custom(
                                        gpui_component::button::ButtonCustomVariant::new(cx)
                                            .foreground(rgb(code_colors.text).into())
                                            .hover(rgb(code_colors.border).into()),
                                    )
                                    .small()
                                    .icon(if copied {
                                        IconName::Check
                                    } else {
                                        IconName::Copy
                                    })
                                    .label(if copied { "Copied" } else { "Copy" })
                                    .tooltip(if math {
                                        "Copy formula source"
                                    } else {
                                        "Copy code block"
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        cx.write_to_clipboard(ClipboardItem::new_string(
                                            code_text.clone(),
                                        ));
                                        let copied = (copy_node, Instant::now());
                                        this.copied_code = Some(copied);
                                        cx.notify();
                                        cx.spawn(async move |this, cx| {
                                            cx.background_executor()
                                                .timer(Duration::from_secs(2))
                                                .await;
                                            let _ = this.update(cx, |this, cx| {
                                                if this.copied_code == Some(copied) {
                                                    this.copied_code = None;
                                                    cx.notify();
                                                }
                                            });
                                        })
                                        .detach();
                                    })),
                            )
                            .into_any_element(),
                    );
                }
            }
            if let Some((table, _, _, _)) = line.table_cell
                && component_chrome.len() > cell_chrome_start
            {
                let children = component_chrome.split_off(cell_chrome_start);
                let offset = self.horizontal_scrolls.get(&table).copied().unwrap_or(0.);
                let (viewport_left, viewport_width) = table_viewport_geometry(
                    line,
                    &self.projection,
                    self.layout_width,
                    self.zoom_factor,
                );
                let left = (line.x_fraction * self.layout_width - offset).max(viewport_left);
                let right = ((line.x_fraction + line.width_fraction) * self.layout_width - offset)
                    .min(viewport_left + viewport_width);
                if right > left {
                    // Preserve the document coordinate system inside a cell-
                    // clipped native subtree. This clips both paint and hit
                    // regions for HTML controls, titles, and Copy actions.
                    component_chrome.push(
                        div()
                            .absolute()
                            .left(px(left))
                            .top(px(line.table_row_y))
                            .w(px(right - left))
                            .h(px(line.table_row_height))
                            .overflow_hidden()
                            .child(
                                div()
                                    .absolute()
                                    .left(px(-left - offset))
                                    .top(px(-line.table_row_y))
                                    .w(px(self.layout_width))
                                    .h(px(self.document_height))
                                    .children(children),
                            )
                            .into_any_element(),
                    );
                }
            }
        }
        component_chrome.extend(outer_container_headers);
        component_chrome.extend(self.table_edge_controls(cx));
        component_chrome.extend(self.table_resize_feedback(palette));
        let semantics = window
            .is_a11y_active()
            .then(|| {
                if self.measured_layout && self.momentum_remaining == 0. {
                    Some(self.semantic_cache.get(
                        &self.projection,
                        &self.visual_lines,
                        self.layout_width,
                    ))
                } else {
                    // Initial fallback geometry is replaced by authoritative font,
                    // resource, and viewport measurement. Do not publish a huge
                    // provisional native tree only to rebuild it moments later.
                    // During later reflows and active scrolling the last
                    // complete semantic revision remains available until its
                    // measured replacement can be published once at rest.
                    self.semantic_cache.current()
                }
            })
            .flatten();
        let context_editor = cx.entity();
        let context_focus = self.focus_handle.clone();
        let toolbar_focus = self.focus_handle.clone();
        let has_text_selection = !self.selected_byte_range().0.is_empty();
        let has_copy_selection =
            has_text_selection || matches!(self.selection, Selection::Table(_));
        let table_edge_popup = self.table_edge_menu.as_ref().map(|(menu, position, _)| {
            deferred(
                anchored()
                    .position(*position)
                    .anchor(Anchor::TopLeft)
                    .child(menu.clone()),
            )
            .with_priority(2)
        });
        let editor = div()
            .id("rich-document-editor")
            .role(Role::MultilineTextInput)
            .aria_label("Markdown document editor")
            .aria_description("Rendered rich Markdown; use arrow keys to move and typing to edit")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_x_hidden()
            // The editor owns wheel integration; GPUI still clips and tracks
            // programmatic scrolling, but must not apply a second wheel delta.
            .overflow_y_hidden()
            .track_scroll(&self.scroll_handle)
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(if self.table_resize_drag.is_some() {
                CursorStyle::ResizeColumn
            } else {
                CursorStyle::IBeam
            })
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::word_left))
            .on_action(cx.listener(Self::word_right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_word_left))
            .on_action(cx.listener(Self::select_word_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
            .on_action(cx.listener(Self::select_home))
            .on_action(cx.listener(Self::select_end))
            .on_action(cx.listener(Self::document_start))
            .on_action(cx.listener(Self::document_end))
            .on_action(cx.listener(Self::select_document_start))
            .on_action(cx.listener(Self::select_document_end))
            .on_action(cx.listener(Self::copy))
            .on_action(cx.listener(Self::cut))
            .on_action(cx.listener(Self::paste))
            .on_action(cx.listener(Self::paste_as_markdown))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::format_bold))
            .on_action(cx.listener(Self::format_italic))
            .on_action(cx.listener(Self::format_strike))
            .on_action(cx.listener(Self::format_code))
            .on_action(cx.listener(Self::format_link))
            .on_action(cx.listener(|this, _: &FormatParagraph, window, cx| {
                this.apply_block_style(BlockStyle::Paragraph, window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatHeading1, window, cx| {
                this.apply_block_style(BlockStyle::Heading(1), window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatHeading2, window, cx| {
                this.apply_block_style(BlockStyle::Heading(2), window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatHeading3, window, cx| {
                this.apply_block_style(BlockStyle::Heading(3), window, cx);
            }))
            .on_action(cx.listener(|this, _: &FormatQuote, window, cx| {
                this.apply_block_style(BlockStyle::BlockQuote, window, cx);
            }))
            .on_action(cx.listener(|this, _: &InsertOrderedList, window, cx| {
                this.insert_block(InsertBlockKind::OrderedList, window, cx);
            }))
            .on_action(cx.listener(|this, _: &InsertBulletedList, window, cx| {
                this.insert_block(InsertBlockKind::UnorderedList, window, cx);
            }))
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::open_link))
            .on_action(cx.listener(Self::hard_break))
            .on_action(cx.listener(Self::next_table_cell))
            .on_action(cx.listener(Self::previous_table_cell))
            .on_action(cx.listener(Self::exit_table))
            .on_action(cx.listener(Self::dismiss))
            .on_action(cx.listener(Self::request_layout_trace))
            .on_action(cx.listener(Self::restore_authored_disclosures))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_context_menu))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .child(DocumentTextElement {
                editor: cx.entity(),
                semantics,
                math_scroll_handles: accessible_math_scrolls,
            })
            .children(component_chrome)
            .children(self.render_footnote_numbers(&visible_order, palette, cx))
            .children(self.render_task_summaries(&visible_order, palette))
            .children(self.render_tree_context(&visible_order, palette))
            .children(image_elements)
            .children(inline_math_elements)
            .when(self.toolbar_visible, |editor| {
                editor.child(
                    div()
                        .id("selection-toolbar")
                        .absolute()
                        .top(px(toolbar_top))
                        .left(px(toolbar_left))
                        .flex()
                        .items_center()
                        .h(px(44.))
                        .px(px(4.))
                        .gap(px(3.))
                        .rounded(px(8.))
                        .bg(rgb(palette.floating))
                        .border_1()
                        .border_color(rgb(palette.border))
                        .shadow_lg()
                        .opacity(self.toolbar_opacity)
                        .cursor(CursorStyle::PointingHand)
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .child(
                            editor_button("format-link", "link", "Link — Ctrl+K", palette, cx)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_link_editor(window, cx);
                                })),
                        )
                        .child(
                            editor_button("format-bold", "bold", "Bold — Ctrl+B", palette, cx)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_selection_format(InlineFormat::Bold, window, cx);
                                })),
                        )
                        .child(
                            editor_button(
                                "format-italic",
                                "italic",
                                "Italic — Ctrl+I",
                                palette,
                                cx,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.apply_selection_format(InlineFormat::Italic, window, cx);
                                },
                            )),
                        )
                        .child(
                            editor_button("format-strike", "strike", "Strikethrough", palette, cx)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_selection_format(
                                        InlineFormat::Strikethrough,
                                        window,
                                        cx,
                                    );
                                })),
                        )
                        .child(
                            editor_button("format-code", "code", "Inline code", palette, cx)
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_selection_format(InlineFormat::Code, window, cx);
                                })),
                        )
                        .child(
                            Button::new("text-style-menu")
                                .ghost()
                                .small()
                                .label("Text")
                                .tooltip("Text style")
                                .dropdown_menu(move |menu, _window, _cx| {
                                    menu.action_context(toolbar_focus.clone())
                                        .min_w(px(320.))
                                        .max_w(px(320.))
                                        .menu_with_icon(
                                            "Text",
                                            IconName::CaseSensitive,
                                            Box::new(FormatParagraph),
                                        )
                                        .menu_with_icon(
                                            "Heading 1",
                                            IconName::ALargeSmall,
                                            Box::new(FormatHeading1),
                                        )
                                        .menu_with_icon(
                                            "Heading 2",
                                            IconName::ALargeSmall,
                                            Box::new(FormatHeading2),
                                        )
                                        .menu_with_icon(
                                            "Heading 3",
                                            IconName::ALargeSmall,
                                            Box::new(FormatHeading3),
                                        )
                                        .menu_with_icon(
                                            "Numbered list",
                                            IconName::SortAscending,
                                            Box::new(InsertOrderedList),
                                        )
                                        .menu_with_icon(
                                            "Bulleted list",
                                            IconName::Dash,
                                            Box::new(InsertBulletedList),
                                        )
                                        .separator()
                                        .menu_with_icon(
                                            "Quote",
                                            IconName::BookOpen,
                                            Box::new(FormatQuote),
                                        )
                                        .menu_with_icon(
                                            "Inline code",
                                            IconName::SquareTerminal,
                                            Box::new(FormatCode),
                                        )
                                        .menu("Strikethrough", Box::new(FormatStrike))
                                }),
                        )
                        .children(image_tools),
                )
            })
            .children(link_popover)
            .children(image_popover)
            .children(table_edge_popup)
            .context_menu(move |menu, window, cx| {
                let menu = if let Some(target) = context_editor
                    .read(cx)
                    .html_link_at(window.mouse_position())
                {
                    let open_target = target.clone();
                    let editor = context_editor.clone();
                    let menu = if context_editor.read(cx).can_open_link(&target) {
                        menu.item(
                            PopupMenuItem::new("Open link · Alt+Enter")
                                .icon(IconName::ExternalLink)
                                .on_click(move |_, window, cx| {
                                    editor.update(cx, |editor, cx| {
                                        editor.open_link_target(&open_target, window, cx)
                                    })
                                }),
                        )
                    } else {
                        menu.label("Opening this link type is not supported")
                    };
                    menu.item(
                        PopupMenuItem::new("Copy link address")
                            .icon(IconName::Copy)
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(target.clone()))
                            }),
                    )
                    .separator()
                } else {
                    menu
                };
                let menu = if !context_editor
                    .read(cx)
                    .projection
                    .html_disclosures
                    .is_empty()
                {
                    menu.menu(
                        "Restore authored disclosures",
                        Box::new(html_disclosure::RestoreAuthoredDisclosures),
                    )
                    .separator()
                } else {
                    menu
                };
                let table_context = {
                    let editor = context_editor.read(cx);
                    editor
                        .table_cell_at(window.mouse_position())
                        .or(editor.table_hover)
                        .and_then(|target| {
                            let BlockNode::Table(table) =
                                editor.projection.block(target.table_id)?
                            else {
                                return None;
                            };
                            let top_level = editor
                                .projection
                                .roots()
                                .any(|root| root.id() == target.table_id);
                            Some((
                                target,
                                table.columns.get(target.column)?.alignment,
                                table.row_count(),
                                table.column_count(),
                                top_level,
                            ))
                        })
                };
                let menu =
                    if let Some((target, alignment, rows, columns, top_level)) = table_context {
                        let table_editor = context_editor.clone();
                        menu.submenu("Table", window, cx, move |menu, _, _| {
                            let item = |label, icon, command| {
                                table_menu_item(table_editor.clone(), label, icon, command)
                            };
                            menu.item(item(
                                "Insert row above",
                                IconName::ArrowUp,
                                EditCommand::InsertTableRow {
                                    table_id: target.table_id,
                                    index: target.row,
                                },
                            ))
                            .item(item(
                                "Insert row below",
                                IconName::ArrowDown,
                                EditCommand::InsertTableRow {
                                    table_id: target.table_id,
                                    index: target.row + 1,
                                },
                            ))
                            .item(item(
                                "Insert column left",
                                IconName::ArrowLeft,
                                EditCommand::InsertTableColumn {
                                    table_id: target.table_id,
                                    index: target.column,
                                },
                            ))
                            .item(item(
                                "Insert column right",
                                IconName::ArrowRight,
                                EditCommand::InsertTableColumn {
                                    table_id: target.table_id,
                                    index: target.column + 1,
                                },
                            ))
                            .separator()
                            .item(item(
                                "Duplicate row",
                                IconName::Copy,
                                EditCommand::DuplicateTableRow {
                                    table_id: target.table_id,
                                    index: target.row,
                                },
                            ))
                            .item(item(
                                "Duplicate column",
                                IconName::Copy,
                                EditCommand::DuplicateTableColumn {
                                    table_id: target.table_id,
                                    index: target.column,
                                },
                            ))
                            .item(item(
                                "Clear cell contents",
                                IconName::CircleX,
                                EditCommand::ClearTableCell {
                                    table_id: target.table_id,
                                    row: target.row,
                                    column: target.column,
                                },
                            ))
                            .separator()
                            .item(
                                item(
                                    "Align left",
                                    IconName::CaseSensitive,
                                    EditCommand::SetTableColumnAlignment {
                                        table_id: target.table_id,
                                        column: target.column,
                                        alignment: ColumnAlignment::Left,
                                    },
                                )
                                .checked(matches!(
                                    alignment,
                                    ColumnAlignment::None | ColumnAlignment::Left
                                )),
                            )
                            .item(
                                item(
                                    "Align center",
                                    IconName::CaseSensitive,
                                    EditCommand::SetTableColumnAlignment {
                                        table_id: target.table_id,
                                        column: target.column,
                                        alignment: ColumnAlignment::Center,
                                    },
                                )
                                .checked(alignment == ColumnAlignment::Center),
                            )
                            .item(
                                item(
                                    "Align right",
                                    IconName::CaseSensitive,
                                    EditCommand::SetTableColumnAlignment {
                                        table_id: target.table_id,
                                        column: target.column,
                                        alignment: ColumnAlignment::Right,
                                    },
                                )
                                .checked(alignment == ColumnAlignment::Right),
                            )
                            .separator()
                            .item(
                                item(
                                    "Delete row",
                                    IconName::Delete,
                                    EditCommand::DeleteTableRow {
                                        table_id: target.table_id,
                                        index: target.row,
                                    },
                                )
                                .disabled(rows <= 1),
                            )
                            .item(
                                item(
                                    "Delete column",
                                    IconName::Delete,
                                    EditCommand::DeleteTableColumn {
                                        table_id: target.table_id,
                                        index: target.column,
                                    },
                                )
                                .disabled(columns <= 1),
                            )
                            .item(
                                item(
                                    "Delete table",
                                    IconName::Delete,
                                    EditCommand::DeleteBlock {
                                        node_id: target.table_id,
                                    },
                                )
                                .disabled(!top_level),
                            )
                        })
                        .separator()
                    } else {
                        menu
                    };
                if let Some(command) = context_editor
                    .read(cx)
                    .html_edit_command_at(window.mouse_position())
                {
                    let editor = context_editor.clone();
                    return menu
                        .min_w(px(264.))
                        .action_context(context_focus.clone())
                        .item(
                            PopupMenuItem::new("Edit text here")
                                .icon(IconName::CaseSensitive)
                                .on_click(move |_, window, cx| {
                                    editor.update(cx, |editor, cx| {
                                        editor.apply_structural_command(
                                            command.clone(),
                                            window,
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .separator()
                        .label("Converts HTML styling to Markdown");
                }
                let menu = menu
                    .min_w(px(264.))
                    .max_w(px(264.))
                    .action_context(context_focus.clone())
                    .menu_with_icon("Undo", IconName::Undo2, Box::new(Undo))
                    .menu_with_icon("Redo", IconName::Redo2, Box::new(Redo))
                    .separator()
                    .menu_with_disabled("Cut", Box::new(Cut), !has_text_selection)
                    .menu_with_icon_and_disabled(
                        "Copy",
                        IconName::Copy,
                        Box::new(Copy),
                        !has_copy_selection,
                    )
                    .menu("Paste", Box::new(Paste))
                    .menu("Paste as Markdown", Box::new(PasteAsMarkdown))
                    .menu("Select all", Box::new(SelectAll))
                    .separator()
                    .label("Insert block");
                [
                    (
                        "Paragraph",
                        IconName::CaseSensitive,
                        InsertBlockKind::Paragraph,
                    ),
                    (
                        "Heading",
                        IconName::ALargeSmall,
                        InsertBlockKind::Heading(2),
                    ),
                    (
                        "Numbered list",
                        IconName::SortAscending,
                        InsertBlockKind::OrderedList,
                    ),
                    ("List", IconName::Dash, InsertBlockKind::UnorderedList),
                    (
                        "Task list",
                        IconName::CircleCheck,
                        InsertBlockKind::TaskList,
                    ),
                    ("Quote", IconName::BookOpen, InsertBlockKind::BlockQuote),
                    (
                        "Code block",
                        IconName::SquareTerminal,
                        InsertBlockKind::CodeBlock,
                    ),
                    ("Image", IconName::Frame, InsertBlockKind::Image),
                    ("Table", IconName::LayoutDashboard, InsertBlockKind::Table),
                    (
                        "Thematic break",
                        IconName::Minus,
                        InsertBlockKind::ThematicBreak,
                    ),
                ]
                .into_iter()
                .fold(menu, |menu, (label, icon, kind)| {
                    let editor = context_editor.clone();
                    menu.item(PopupMenuItem::new(label).icon(icon).on_click(
                        move |_, window, cx| {
                            editor.update(cx, |editor, cx| {
                                editor.insert_block(kind, window, cx);
                            });
                        },
                    ))
                })
            });
        div()
            .flex()
            .flex_col()
            .size_full()
            .children(self.find_bar(palette, cx))
            .child(div().flex_1().min_h_0().child(editor))
            .map(|element| {
                #[cfg(feature = "layout-validation")]
                let element = element
                    .key_context("LayoutValidation")
                    .on_action(cx.listener(Self::arm_planner_panic))
                    .on_action(cx.listener(Self::arm_planner_timeout));
                element
            })
    }
}

struct DocumentTextElement {
    editor: Entity<RichDocumentEditor>,
    semantics: Option<std::rc::Rc<accessibility::SemanticTree>>,
    math_scroll_handles: HashMap<NodeId, ScrollHandle>,
}

struct TextPrepaintState {
    semantic_bounds: Bounds<Pixels>,
    semantic_scale: f32,
    semantic_actions: accessibility::ActionTargets,
    lines: Vec<PaintedLine>,
    decorations: Vec<PaintedLine>,
    checkboxes: Vec<PaintedCheckbox>,
    resource_icons: Vec<Bounds<Pixels>>,
    metric_icons: Vec<(Bounds<Pixels>, ContentMask<Pixels>)>,
    link_hitboxes: Vec<Hitbox>,
    resize_hitboxes: Vec<Hitbox>,
    chrome: Vec<MaskedQuad>,
    overlays: Vec<MaskedQuad>,
    caret: Option<MaskedQuad>,
    horizontal_metrics: HashMap<NodeId, (f32, f32)>,
    math_scroll_handles: HashMap<NodeId, ScrollHandle>,
}

struct PaintedCheckbox {
    bounds: Bounds<Pixels>,
    checked: bool,
    content_mask: Option<ContentMask<Pixels>>,
}

fn task_checkbox_bounds(line: Bounds<Pixels>, zoom: f32) -> Bounds<Pixels> {
    let side = px(DocumentStyle::CHECKBOX_SIZE * zoom);
    Bounds::new(
        point(
            line.left() - side - px(DocumentStyle::CHECKBOX_TEXT_GAP * zoom),
            line.top() + (line.size.height - side) / 2.,
        ),
        size(side, side),
    )
}

fn table_edge_hit_bounds(cell: Bounds<Pixels>, edge: TableEdge) -> Bounds<Pixels> {
    match edge {
        TableEdge::Top => Bounds::new(
            point(cell.center().x - px(14.), cell.top() - px(7.)),
            size(px(28.), px(14.)),
        ),
        TableEdge::Right => Bounds::new(
            point(cell.right() - px(7.), cell.center().y - px(14.)),
            size(px(14.), px(28.)),
        ),
        TableEdge::Bottom => Bounds::new(
            point(cell.center().x - px(14.), cell.bottom() - px(7.)),
            size(px(28.), px(14.)),
        ),
        TableEdge::Left => Bounds::new(
            point(cell.left() - px(7.), cell.center().y - px(14.)),
            size(px(14.), px(28.)),
        ),
    }
}

// The pinned component exposes a visible `label`, but not a separate name.
// Its Button already owns an element ID; this local adapter exposes GPUI's
// semantic name without inserting a glyph or altering native button behavior.
struct TableEdgeButton(Button);

impl InteractiveElement for TableEdgeButton {
    fn interactivity(&mut self) -> &mut gpui::Interactivity {
        self.0.interactivity()
    }
}

impl StatefulInteractiveElement for TableEdgeButton {}

fn table_edge_control(
    editor: Entity<RichDocumentEditor>,
    target: TableCellTarget,
    edge: TableEdge,
    cell: Bounds<Pixels>,
    zoom: f32,
    accent: u32,
) -> AnyElement {
    let hit = table_edge_hit_bounds(cell, edge);
    // Keep both axes on the same pixel grid at fractional document zoom.
    let knob_side = px((3. * zoom).round());
    let label = match edge {
        TableEdge::Top => "Row above menu",
        TableEdge::Right => "Column right menu",
        TableEdge::Bottom => "Row below menu",
        TableEdge::Left => "Column left menu",
    };
    let menu_editor = editor.clone();
    let button = Button::new(ElementId::Name(
        format!(
            "table-edge-{}-{}-{}-{edge:?}",
            target.table_id.get(),
            target.row,
            target.column
        )
        .into(),
    ))
    .text()
    .rounded(px(0.))
    .absolute()
    .left(hit.left())
    .top(hit.top())
    .w(hit.size.width)
    .h(hit.size.height)
    .debug_selector(move || format!("table-edge-{edge:?}"))
    .p_0()
    .cursor_pointer()
    .key_context(TABLE_EDGE_CONTEXT)
    .on_action(move |_: &ActivateTableEdge, window, cx| {
        window.prevent_default();
        editor.update(cx, |editor, cx| {
            let position = editor.element_bounds.map_or(hit.bottom_left(), |bounds| {
                bounds.origin + hit.bottom_left()
            });
            editor.open_table_edge_menu(target, edge, position, window, cx);
        });
        cx.stop_propagation();
    })
    .on_action(|_: &NextTableEdgeControl, window, cx| {
        window.focus_next(cx);
        cx.stop_propagation();
    })
    .on_action(|_: &PreviousTableEdgeControl, window, cx| {
        window.focus_prev(cx);
        cx.stop_propagation();
    })
    .child(
        div()
            .debug_selector(move || format!("table-knob-{edge:?}"))
            .size(knob_side)
            .flex_shrink_0()
            .rounded(knob_side / 2.)
            .bg(rgb(accent)),
    )
    .tooltip(label)
    .on_click(move |_, window, cx| {
        let position = window.mouse_position();
        menu_editor.update(cx, |editor, cx| {
            editor.open_table_edge_menu(target, edge, position, window, cx);
        });
    });
    TableEdgeButton(button)
        .aria_label(label)
        .0
        .into_any_element()
}

impl PaintedCheckbox {
    fn paint(&self, palette: TachyonPalette, window: &mut Window, cx: &App) {
        let zoom = f32::from(self.bounds.size.width) / 18.;
        let radius = px(4. * zoom);
        window.with_content_mask(self.content_mask, |window| {
            window.paint_quad(
                fill(
                    self.bounds,
                    rgb(if self.checked {
                        palette.accent
                    } else {
                        palette.page
                    }),
                )
                .corner_radii(radius),
            );
            if self.checked {
                // Reuse the toolkit's vector icon and SVG atlas; never shape a
                // font-dependent tick or rasterize an icon on every scroll frame.
                let _ = window.paint_svg(
                    self.bounds.inset(px(1. * zoom)),
                    IconName::Check.path(),
                    None,
                    Default::default(),
                    rgb(palette.page).into(),
                    cx,
                );
            } else {
                window.paint_quad(
                    outline(self.bounds, rgb(palette.secondary), BorderStyle::Solid)
                        .border_widths(px(1.5 * zoom))
                        .corner_radii(radius),
                );
            }
        });
    }
}

impl IntoElement for DocumentTextElement {
    type Element = Self;

    fn into_element(self) -> Self::Element {
        self
    }
}

impl Element for DocumentTextElement {
    type RequestLayoutState = ();
    type PrepaintState = TextPrepaintState;

    fn id(&self) -> Option<ElementId> {
        Some("document-content".into())
    }

    fn a11y_role(&self) -> Option<Role> {
        // Keep scrolling on a transparent geometry owner. The retained child
        // owns the document text range, so a pure scroll update cannot make
        // the AT-SPI adapter reconstruct every descendant's text.
        Some(Role::GenericContainer)
    }

    fn a11y_synthetic_children(
        &mut self,
        state: &mut Self::PrepaintState,
        builder: &mut gpui::A11ySubtreeBuilder,
    ) {
        if let Some(semantics) = &self.semantics {
            state.semantic_actions = semantics.publish(
                builder,
                state.semantic_bounds,
                state.semantic_scale,
                &state.math_scroll_handles,
            );
        } else {
            // Accessibility can activate before the first authoritative font
            // layout commits. Register the editor's native Text interface
            // with a valid empty range from that first adapter snapshot; the
            // retained document subtree replaces this leaf once measured.
            let id = builder.synthetic_node_id("pending-document-text-run");
            let mut text = gpui::accesskit::Node::new(Role::TextRun);
            text.set_value("");
            text.set_character_lengths(Vec::<u8>::new());
            text.set_bounds(gpui::accesskit::Rect::new(
                0.,
                0.,
                f64::from(state.semantic_bounds.size.width),
                f64::from(state.semantic_bounds.size.height),
            ));
            let inserted = builder.push_child(id, text);
            debug_assert!(inserted, "pending document text ID must be unique");
        }
    }

    fn source_location(&self) -> Option<&'static core::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, Self::RequestLayoutState) {
        let editor = self.editor.read(cx);
        let mut style = Style::default();
        style.size.width = relative(1.).into();
        style.size.height = px(editor.document_height).into();
        style.flex_shrink = 0.;
        (window.request_layout(style, [], cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        window: &mut Window,
        cx: &mut App,
    ) -> Self::PrepaintState {
        let editor = self.editor.read(cx);
        let palette = TachyonPalette::for_dark(cx.theme().is_dark());
        let text = editor.projection.text();
        let selected = if let Some(selection) = &editor.html_selection {
            selection
                .cross
                .as_ref()
                .and_then(|cross| cross.projected_range(selection.range()))
                .unwrap_or(usize::MAX..usize::MAX)
        } else {
            editor.selected_byte_range().0
        };
        let snapshot = editor.document.snapshot();
        let table_selection = match &editor.selection {
            _ if editor.html_selection.is_some() => None,
            Selection::Table(selection) => Some(*selection),
            Selection::Text(_) => None,
        };
        let marked = editor.marked_range.clone();
        let text_style = window.text_style();
        let mut lines = Vec::new();
        let mut decorations = Vec::new();
        let mut checkboxes = Vec::new();
        let mut resource_icons = Vec::new();
        let mut metric_icons = Vec::new();
        let mut link_hitboxes = Vec::new();
        let mut resize_hitboxes = Vec::new();
        let mut chrome = Vec::new();
        let mut overlays = Vec::new();
        let mut selected_code_panels = Vec::new();
        let mut caret = None;
        let mut horizontal_metrics = HashMap::<NodeId, (f32, f32)>::new();
        let mut painted_alerts = HashSet::new();
        let mut painted_code_blocks = HashSet::new();
        let mut painted_lists = HashSet::new();
        let mut painted_outline_guides = HashSet::new();
        let mut painted_timeline_events = HashSet::new();
        let mut painted_metadata = HashSet::new();
        let mut painted_metadata_strips = HashSet::new();
        let mut painted_quotes = HashSet::new();
        let mut painted_cards = HashSet::new();
        let mut painted_records = HashSet::new();
        let visible = window.content_mask().bounds;
        // Make the first editable viewport available before warming the normal
        // one-viewport overscan. The visible scene is complete; the immediately
        // requested follow-up frame then restores steady-state virtualization.
        let overscan = if editor.has_painted {
            visible.size.height
        } else {
            px(0.)
        };
        let overscan_top = visible.top() - overscan;
        let overscan_bottom = visible.bottom() + overscan;
        let local_overscan_top: f32 = (overscan_top - bounds.top()).into();
        let local_overscan_bottom: f32 = (overscan_bottom - bounds.top()).into();
        let visible_lines = editor.components.visible_range(
            &editor.visual_lines,
            &editor.paint_order,
            local_overscan_top,
            local_overscan_bottom,
        );

        for order_index in visible_lines {
            let spec = &editor.visual_lines[editor.paint_order[order_index]];
            let range = spec.projected_range();
            let visual_style = spec.style;
            let width: f32 = bounds.size.width.into();
            let segment = segment_for_line(&editor.projection, &range);
            let block = segment.and_then(|segment| editor.projection.block(segment.node_id));
            let cell_selected = table_selection
                .zip(segment.and_then(|segment| segment.context.table_cell))
                .is_some_and(|(selection, (table, row, column))| {
                    let (rows, columns) = selection.normalized();
                    selection.table_id == table && rows.contains(&row) && columns.contains(&column)
                });
            let is_code =
                matches!(block, Some(BlockNode::CodeBlock(_))) && !spec.rendered_code_preview();
            let horizontal_owner =
                segment.and_then(|segment| horizontal_scroll_owner(&editor.projection, segment));
            let horizontal_offset = horizontal_owner
                .and_then(|owner| editor.horizontal_scrolls.get(&owner).copied())
                .unwrap_or(0.);
            let line_bounds = visual_line_bounds(editor, spec, bounds, is_code, horizontal_offset);
            decorations.extend(table_records::paint_labels(
                editor,
                spec,
                line_bounds,
                palette,
                window,
            ));
            let (viewport_left, viewport_width) = if spec.table_cell.is_some() {
                table_viewport_geometry(spec, &editor.projection, width, editor.zoom_factor)
            } else if is_code {
                let inset = spec.slot.map_or(0., |s| s.inset()) * editor.zoom_factor;
                let leading = if spec.code_gutter() > 0. {
                    spec.inset + spec.code_gutter()
                } else {
                    inset
                };
                (
                    width * spec.x_fraction + leading,
                    (width * spec.width_fraction
                        - inset
                        - leading
                        - spec.command_trailing(editor.zoom_factor)
                        - if spec.command_strip() {
                            CODE_BLOCK_PADDING * editor.zoom_factor
                        } else {
                            0.
                        })
                    .max(1.),
                )
            } else if spec.html_preview.is_some() {
                (width * spec.x_fraction, width * spec.width_fraction)
            } else {
                (0., width)
            };
            // Publish the whole table extent before rejecting clipped cells.
            // Otherwise a column entirely outside the viewport never enters
            // the scroll range and can remain permanently unreachable.
            if let Some(owner) = horizontal_owner.filter(|_| spec.table_cell.is_some())
                && let Some(component) = editor.components.get(&owner)
            {
                record_horizontal_metrics(
                    &mut horizontal_metrics,
                    owner,
                    viewport_width,
                    width * component.right_fraction - viewport_left,
                );
            }
            let horizontal_viewport = Bounds::new(
                point(bounds.left() + px(viewport_left), line_bounds.top()),
                size(px(viewport_width), line_bounds.size.height),
            );
            let content_mask = if spec.table_cell.is_some() {
                // Cell-local list markers occupy the reserved left padding.
                // Clip at the cell edge, not the text origin, so they survive.
                let cell_line = Bounds::new(
                    point(
                        bounds.left() + px(width * spec.x_fraction - horizontal_offset),
                        line_bounds.top(),
                    ),
                    size(px(width * spec.width_fraction), line_bounds.size.height),
                );
                let Some(clipped) = intersect_bounds(cell_line, horizontal_viewport) else {
                    continue;
                };
                Some(ContentMask { bounds: clipped })
            } else {
                horizontal_owner.map(|_| ContentMask {
                    bounds: horizontal_viewport,
                })
            };
            let table_cell_bounds = spec.table_cell.map(|_| {
                Bounds::new(
                    point(
                        bounds.left() + px(width * spec.x_fraction - horizontal_offset),
                        bounds.top() + px(spec.table_row_y),
                    ),
                    size(
                        px((width * spec.width_fraction).max(1.)),
                        px(spec.table_row_height.max(1.)),
                    ),
                )
            });
            let table_content_mask = table_cell_bounds.and_then(|cell| {
                let row_viewport = Bounds::new(
                    point(bounds.left() + px(viewport_left), cell.top()),
                    size(px(viewport_width), cell.size.height),
                );
                intersect_bounds(cell, row_viewport).map(|bounds| ContentMask { bounds })
            });
            if line_bounds.bottom() < overscan_top || line_bounds.top() > overscan_bottom {
                continue;
            }
            if matches!(block, Some(BlockNode::ThematicBreak { .. })) {
                chrome.push(MaskedQuad {
                    quad: fill(
                        Bounds::new(line_bounds.origin, size(line_bounds.size.width, px(1.))),
                        rgb(palette.border),
                    ),
                    content_mask,
                });
            }
            if let Some(segment) = segment {
                if spec.label_row.is_some()
                    && let Some(slot) = editor
                        .adaptive
                        .label_rows
                        .get(&segment.node_id)
                        .and_then(|row| row.metadata())
                        .and_then(|value| value.slot)
                    && painted_metadata.insert(segment.node_id)
                    && let Some(component) = editor.components.get(&slot.group)
                {
                    for rule in metadata::rules(
                        component,
                        slot,
                        bounds.origin,
                        width,
                        editor.zoom_factor,
                        painted_metadata_strips.insert(slot.group),
                    ) {
                        if let Some(rule) = intersect_bounds(rule, visible) {
                            chrome.push(MaskedQuad {
                                quad: fill(rule, rgb(palette.border)),
                                content_mask: Some(ContentMask { bounds: visible }),
                            });
                        }
                    }
                }
                for owner in editor.adaptive.timeline_chain(segment.node_id) {
                    if let Some(timeline) = editor
                        .adaptive
                        .label_rows
                        .get(&owner)
                        .and_then(|row| row.timeline())
                        && painted_timeline_events.insert(owner)
                        && let Some(component) = editor.components.get(&owner)
                    {
                        let (dot, rail) = label_rows::timeline_marks(
                            component,
                            timeline.next.and_then(|node| editor.components.get(&node)),
                            timeline.slot.is_some(),
                            bounds.origin,
                            width,
                            editor.zoom_factor,
                        );
                        if let Some(rail) = rail.and_then(|rail| intersect_bounds(rail, visible)) {
                            chrome.push(MaskedQuad {
                                quad: fill(rail, rgb(palette.border)),
                                content_mask: Some(ContentMask { bounds: visible }),
                            });
                        }
                        chrome.push(MaskedQuad {
                            quad: fill(dot, rgb(palette.accent))
                                .corner_radii(px(4. * editor.zoom_factor)),
                            content_mask: Some(ContentMask { bounds: visible }),
                        });
                    }
                }
                let leading_container = compact_tree::leading_container_bounds(
                    segment,
                    spec,
                    &editor.projection,
                    &editor.components,
                    editor.layout_width,
                    editor.zoom_factor,
                );
                if spec.compact_tree
                    && segment.context.list_depth > 1
                    && (leading_container.is_some()
                        || (segment.context.list_item_container.is_none()
                            && segment.context.table_cell.is_none()
                            && (segment.context.quote_depth == 0
                                || segment.context.list_marker.is_some())
                            && matches!(block, Some(BlockNode::Paragraph(_)))))
                {
                    // Compact branches carry their parent explicitly. A local
                    // connector must not run through a re-anchored descendant.
                    let zoom = editor.zoom_factor;
                    let ordered = segment.context.list_ancestors.last().is_some_and(|id| {
                        matches!(editor.projection.block(*id), Some(BlockNode::List(list))
                            if matches!(list.kind, document_core::ListKind::Ordered { .. }))
                    });
                    let guide = leading_container
                        .map(|panel| Bounds::new(bounds.origin + panel.origin, panel.size))
                        .unwrap_or(line_bounds);
                    let x = guide.left()
                        - px((32. + if ordered { NUMBERED_LIST_EXTRA_GAP } else { 0. }) * zoom);
                    let rail = Bounds::new(
                        point(x, guide.top()),
                        size(
                            px(zoom),
                            guide.size.height
                                + if leading_container.is_some() {
                                    px(0.)
                                } else {
                                    px(spec.style.space_below)
                                },
                        ),
                    );
                    let first = spec.projected_start() == segment.projection_start();
                    let elbow = first.then(|| {
                        Bounds::new(
                            point(
                                x,
                                if leading_container.is_some() {
                                    guide.top() + px(DocumentStyle::REFERENCE_LEADING * zoom / 2.)
                                } else {
                                    line_bounds.center().y
                                },
                            ),
                            size(px(8. * zoom), px(zoom)),
                        )
                    });
                    for mark in std::iter::once(rail).chain(elbow) {
                        if let Some(mark) = intersect_bounds(mark, visible) {
                            chrome.push(MaskedQuad {
                                quad: fill(mark, rgb(palette.border)),
                                content_mask: Some(ContentMask { bounds: visible }),
                            });
                        }
                    }
                }
                for id in segment
                    .context
                    .list_ancestors
                    .iter()
                    .skip(1)
                    .filter(|_| !spec.compact_tree)
                {
                    if !painted_outline_guides.insert(*id) {
                        continue;
                    }
                    let Some(component) = editor.components.get(id) else {
                        continue;
                    };
                    let Some(BlockNode::List(list)) = editor.projection.block(*id) else {
                        continue;
                    };
                    if list
                        .items
                        .iter()
                        .next()
                        .and_then(|item| item.blocks.get(0))
                        .and_then(|block| editor.adaptive.label_rows.get(&block.id()))
                        .is_some_and(|columns| columns.timeline().is_some())
                    {
                        // The timeline owns this list's guide. Do not add a
                        // second parallel hierarchy rule beside its date rail.
                        continue;
                    }
                    let ordered = matches!(list.kind, document_core::ListKind::Ordered { .. });
                    let guide = outline_guide_bounds(
                        component,
                        bounds.origin,
                        width,
                        editor.zoom_factor,
                        ordered,
                    );
                    let mask = editor
                        .projection
                        .container_cell(*id)
                        .and(table_content_mask)
                        .map_or(visible, |mask| mask.bounds.intersect(&visible));
                    if let Some(guide) = intersect_bounds(guide, mask) {
                        chrome.push(MaskedQuad {
                            quad: fill(guide, rgb(palette.border)),
                            content_mask: Some(ContentMask { bounds: mask }),
                        });
                    }
                }
                let list_layout = editor
                    .adaptive
                    .lists
                    .get(&segment.top_level_node_id)
                    // A nested list in a cell is not the enclosing ordered
                    // item's stepper. Its own numbering remains ordinary.
                    .filter(|_| {
                        segment
                            .context
                            .table_cell
                            .and_then(|(table, _, _)| editor.projection.table_context(table))
                            .is_none_or(|table| {
                                segment.context.list_depth <= table.outer.list_depth
                            })
                    })
                    .map(|list| list.layout);
                if spec.slot.is_some_and(|slot| {
                    slot.cards
                        && !matches!(
                            slot.card_accent,
                            crate::adaptive::CardAccent::Open
                                | crate::adaptive::CardAccent::OpenLabeled
                        )
                        && painted_cards.insert((slot.group, slot.item))
                }) {
                    let slot = spec.slot.unwrap();
                    let card = Bounds::new(
                        point(
                            bounds.left() + px(width * spec.x_fraction),
                            bounds.top() + px(spec.table_row_y),
                        ),
                        size(px(width * spec.width_fraction), px(spec.table_row_height)),
                    );
                    if let crate::adaptive::CardAccent::Resource(resource) = slot.card_accent {
                        resource_icons.push(Bounds::new(
                            point(
                                card.left() + px(resource.inset() * editor.zoom_factor),
                                card.top() + px((resource.inset() + 4.) * editor.zoom_factor),
                            ),
                            size(px(16. * editor.zoom_factor), px(16. * editor.zoom_factor)),
                        ));
                        if resource.body_start.is_some() {
                            chrome.push(MaskedQuad {
                                quad: fill(card, rgb(palette.page))
                                    .corner_radii(px(DocumentStyle::RADIUS * editor.zoom_factor))
                                    .border_widths(px(editor.zoom_factor))
                                    .border_color(rgb(palette.border)),
                                content_mask: Some(ContentMask { bounds: visible }),
                            });
                        } else {
                            chrome.push(MaskedQuad {
                                quad: fill(
                                    Bounds::new(
                                        point(card.left(), card.bottom() - px(editor.zoom_factor)),
                                        size(card.size.width, px(editor.zoom_factor)),
                                    ),
                                    rgb(palette.border),
                                ),
                                content_mask: Some(ContentMask { bounds: visible }),
                            });
                        }
                    } else if let crate::adaptive::CardAccent::Editorial(kind) = slot.card_accent {
                        use crate::adaptive::editorial::Kind;
                        let (surface, border) = match kind {
                            Kind::Decision | Kind::Selected | Kind::Pros | Kind::Valid => {
                                (palette.accent_muted, palette.accent)
                            }
                            Kind::Cons | Kind::Invalid => {
                                (palette.signal(&AlertKind::Caution).paper, palette.border)
                            }
                            Kind::Example
                            | Kind::Request
                            | Kind::Response
                            | Kind::Metric
                            | Kind::Color => (palette.page, palette.border),
                        };
                        chrome.push(MaskedQuad {
                            quad: fill(card, rgb(surface))
                                .corner_radii(px(DocumentStyle::RADIUS * editor.zoom_factor))
                                .border_widths(px(editor.zoom_factor))
                                .border_color(rgb(border)),
                            content_mask: Some(ContentMask { bounds: visible }),
                        });
                        if kind == Kind::Color {
                            let zoom = editor.zoom_factor;
                            let edge = editorial::color_leading(slot.width(width / zoom)) - 24.;
                            let tile = Bounds::new(
                                card.origin + point(px(24. * zoom), px(24. * zoom)),
                                size(px(edge * zoom), px(edge * zoom)),
                            );
                            let color = editor
                                .adaptive
                                .editorials
                                .get(&segment.top_level_node_id)
                                .and_then(|m| m.color_value)
                                .and_then(|id| editor.projection.segment_for_node(id))
                                .and_then(|s| s.context.color_rgba);
                            // Alpha is shown over the documented Paper surface.
                            // Invalid/incomplete edits retain geometry but never
                            // retain a stale color or fabricate a replacement.
                            chrome.push(MaskedQuad {
                                quad: fill(tile, color.map_or_else(|| rgb(palette.page), rgba))
                                    .corner_radii(px(DocumentStyle::RADIUS * zoom))
                                    .border_widths(px(zoom))
                                    .border_color(rgb(palette.border)),
                                content_mask: Some(ContentMask { bounds: visible }),
                            });
                        }
                        if kind == Kind::Metric
                            && metrics::leading(slot.width(width / editor.zoom_factor)) > 0.
                        {
                            let zoom = editor.zoom_factor;
                            let tile = Bounds::new(
                                card.origin + point(px(24. * zoom), px(24. * zoom)),
                                size(px(48. * zoom), px(48. * zoom)),
                            );
                            chrome.push(MaskedQuad {
                                quad: fill(tile, rgb(palette.surface_quiet))
                                    .corner_radii(px(DocumentStyle::RADIUS * zoom)),
                                content_mask: Some(ContentMask { bounds: visible }),
                            });
                            metric_icons.push((
                                Bounds::new(
                                    tile.origin + point(px(12. * zoom), px(12. * zoom)),
                                    size(px(24. * zoom), px(24. * zoom)),
                                ),
                                ContentMask { bounds: visible },
                            ));
                        }
                    } else if slot.card_accent == crate::adaptive::CardAccent::Leading {
                        chrome.push(MaskedQuad {
                            quad: fill(card, rgb(palette.accent_muted))
                                .corner_radii(px(DocumentStyle::RADIUS * editor.zoom_factor))
                                .border_color(rgb(palette.accent))
                                .border_widths(gpui::Edges {
                                    top: px(0.),
                                    right: px(0.),
                                    bottom: px(0.),
                                    left: px(3. * editor.zoom_factor),
                                }),
                            content_mask: None,
                        });
                    } else {
                        chrome.push(MaskedQuad {
                            quad: fill(card, rgb(palette.surface_quiet))
                                .corner_radii(px(4. * editor.zoom_factor)),
                            content_mask: None,
                        });
                        chrome.push(MaskedQuad {
                            quad: outline(card, rgb(palette.border), BorderStyle::Solid)
                                .corner_radii(px(4. * editor.zoom_factor)),
                            content_mask: None,
                        });
                    }
                }
                if matches!(list_layout, Some(ListLayout::Steps | ListLayout::Checklist))
                    && painted_lists.insert(segment.top_level_node_id)
                    && let Some(component) = editor.components.get(&segment.top_level_node_id)
                {
                    let rail = Bounds::new(
                        point(
                            bounds.left()
                                + px(width * component.left_fraction
                                    - (15.5 + NUMBERED_LIST_EXTRA_GAP) * editor.zoom_factor),
                            bounds.top() + px(component.top + 20. * editor.zoom_factor),
                        ),
                        size(
                            px(1.),
                            px(
                                (component.bottom - component.top - 24. * editor.zoom_factor)
                                    .max(0.),
                            ),
                        ),
                    );
                    if list_layout == Some(ListLayout::Steps) {
                        chrome.push(MaskedQuad {
                            quad: fill(rail, rgb(palette.border)),
                            content_mask: None,
                        });
                    }
                }
                if let Some((alert_id, alert_kind)) = segment.context.alert.as_ref()
                    && painted_alerts.insert(*alert_id)
                    && let Some(alert_bounds) = visual_component_bounds(
                        editor,
                        bounds,
                        *alert_id,
                        alert_layout_insets(
                            &editor.projection,
                            segment,
                            width * spec.width_fraction / editor.zoom_factor,
                        )
                        .0 * editor.zoom_factor,
                        8. * editor.zoom_factor,
                        alert_layout_insets(
                            &editor.projection,
                            segment,
                            width * spec.width_fraction / editor.zoom_factor,
                        )
                        .1 * editor.zoom_factor,
                        ALERT_BOTTOM_PADDING * editor.zoom_factor,
                    )
                {
                    let signal = palette.signal(alert_kind);
                    let mut alert_bounds = alert_bounds;
                    let cell_local = editor.projection.container_cell(*alert_id).is_some();
                    if cell_local {
                        alert_bounds.origin.x -= px(horizontal_offset);
                    }
                    let start = chrome.len();
                    append_alert_chrome(&mut chrome, alert_bounds, signal, editor.zoom_factor);
                    if cell_local {
                        for quad in &mut chrome[start..] {
                            quad.content_mask = table_content_mask;
                        }
                    }
                }
                if is_code
                    && painted_code_blocks.insert(segment.node_id)
                    && let Some(code_bounds) = visual_component_bounds(
                        editor,
                        bounds,
                        segment.node_id,
                        CODE_BLOCK_PADDING * editor.zoom_factor,
                        if segment.context.table_cell.is_some() {
                            12. * editor.zoom_factor
                        } else {
                            spec.slot.map_or(0., |s| s.inset()) * editor.zoom_factor
                        },
                        (spec.code_header_height()
                            + CODE_BLOCK_PADDING
                            + editor
                                .components
                                .get(&segment.node_id)
                                .and_then(|component| editor.visual_lines.get(component.first_line))
                                .map_or(0., VisualLineSpec::preview_extent))
                            * editor.zoom_factor,
                        CODE_BLOCK_PADDING * editor.zoom_factor,
                    )
                {
                    let mut code_bounds = code_bounds;
                    let palette =
                        code_palette(palette, block.expect("a code panel has a code block"));
                    let code_mask = if segment.context.table_cell.is_some() {
                        code_bounds.origin.x -= px(horizontal_offset);
                        table_content_mask
                    } else {
                        None
                    };
                    if cell_selected {
                        // Whole-cell selection is painted above ordinary chrome.
                        // Restore the embedded panel's own selected surface above
                        // that fill, without covering neighboring prose or cells.
                        selected_code_panels.push(MaskedQuad {
                            quad: fill(code_bounds, rgb(palette.selection))
                                .corner_radii(px(DocumentStyle::RADIUS * editor.zoom_factor))
                                .border_widths(px(editor.zoom_factor))
                                .border_color(rgb(palette.border)),
                            content_mask: code_mask,
                        });
                    }
                    chrome.push(MaskedQuad {
                        quad: fill(code_bounds, rgb(palette.surface_quiet))
                            .corner_radii(px(DocumentStyle::RADIUS * editor.zoom_factor)),
                        content_mask: code_mask,
                    });
                    chrome.push(MaskedQuad {
                        quad: outline(code_bounds, rgb(palette.border), BorderStyle::Solid)
                            .corner_radii(px(DocumentStyle::RADIUS * editor.zoom_factor)),
                        content_mask: code_mask,
                    });
                    chrome.push(MaskedQuad {
                        quad: fill(
                            Bounds::new(
                                point(
                                    code_bounds.left(),
                                    code_bounds.top()
                                        + px(spec.code_header_height() * editor.zoom_factor),
                                ),
                                size(
                                    code_bounds.size.width,
                                    px(if spec.command_strip() { 0. } else { 1. }),
                                ),
                            ),
                            rgb(palette.border),
                        ),
                        content_mask: code_mask,
                    });
                }
                for &quote in segment.context.quote_ancestors.iter() {
                    if !painted_quotes.insert(quote) {
                        continue;
                    }
                    let pull = segment.context.quote_pull && segment.context.quote == Some(quote);
                    let Some(panel) = editor.components.get(&quote).and_then(|component| {
                        quotes::panel(component, bounds, editor.zoom_factor, pull)
                    }) else {
                        continue;
                    };
                    let mut panel = panel;
                    let quote_mask = if editor.projection.container_cell(quote).is_some() {
                        panel.origin.x -= px(horizontal_offset);
                        table_content_mask
                    } else {
                        None
                    };
                    chrome.push(MaskedQuad {
                        quad: fill(
                            panel,
                            rgb(if pull {
                                palette.accent_muted
                            } else {
                                palette.surface_quiet
                            }),
                        )
                        .corner_radii(px(DocumentStyle::RADIUS * editor.zoom_factor)),
                        content_mask: quote_mask,
                    });
                    if pull {
                        chrome.push(MaskedQuad {
                            quad: outline(panel, rgb(palette.border), BorderStyle::Solid)
                                .corner_radii(px(DocumentStyle::RADIUS * editor.zoom_factor)),
                            content_mask: quote_mask,
                        });
                        continue;
                    }
                    chrome.push(MaskedQuad {
                        quad: fill(
                            Bounds::new(
                                point(
                                    panel.left(),
                                    panel.top() + px(DocumentStyle::RADIUS * editor.zoom_factor),
                                ),
                                size(
                                    px(editor.zoom_factor),
                                    panel.size.height
                                        - px(2. * DocumentStyle::RADIUS * editor.zoom_factor),
                                ),
                            ),
                            rgb(palette.accent),
                        ),
                        content_mask: quote_mask,
                    });
                }
                if let Some(record) = spec.table_record
                    && let Some((table, row, _, _)) = spec.table_cell
                    && painted_records.insert((table, row))
                {
                    let panel = Bounds::new(
                        point(
                            bounds.left() + px(width * spec.x_fraction - horizontal_offset),
                            bounds.top() + px(record.top),
                        ),
                        size(px(width * spec.width_fraction), px(record.height)),
                    );
                    chrome.push(MaskedQuad {
                        quad: fill(panel, rgb(palette.page))
                            .corner_radii(px(DocumentStyle::RADIUS * editor.zoom_factor))
                            .border_widths(px(editor.zoom_factor))
                            .border_color(rgb(palette.border)),
                        content_mask: Some(ContentMask { bounds: visible }),
                    });
                }
                if segment.context.table_cell.is_some()
                    && spec.table_cell_first
                    && let (Some(cell_bounds), Some(cell_mask)) =
                        (table_cell_bounds, table_content_mask)
                {
                    let alternate_row = segment
                        .context
                        .table_cell
                        .is_some_and(|(_, row, _)| row % 2 == 1);
                    if spec.table_record.is_none()
                        && (segment.context.table_header || alternate_row)
                    {
                        chrome.push(MaskedQuad {
                            quad: fill(
                                cell_bounds,
                                if segment.context.table_header {
                                    rgb(palette.surface_quiet)
                                } else {
                                    rgba(TachyonPalette::with_alpha(palette.surface, 0x70))
                                },
                            ),
                            content_mask: Some(cell_mask),
                        });
                    }
                    if cell_selected {
                        overlays.push(MaskedQuad {
                            quad: fill(cell_bounds, rgb(palette.selection)),
                            content_mask: Some(cell_mask),
                        });
                    }
                    if spec.table_record.is_none() {
                        push_table_border(
                            &mut chrome,
                            cell_bounds,
                            segment.context.table_border.unwrap_or_default(),
                            Some(cell_mask.intersect(&ContentMask { bounds: visible })),
                            palette.border,
                            TableRuleScale {
                                display: window.scale_factor(),
                                zoom: editor.zoom_factor,
                            },
                            segment
                                .context
                                .table_cell
                                .map(|(_, row, column)| (row == 0, column == 0))
                                .unwrap_or_default(),
                        );
                    }
                }
                if segment.context.image_source.is_some() {
                    chrome.push(MaskedQuad {
                        quad: fill(
                            image_element_bounds(editor, spec, bounds, horizontal_offset),
                            rgb(palette.surface),
                        )
                        .corner_radii(px(DocumentStyle::MEDIA_RADIUS * editor.zoom_factor)),
                        content_mask,
                    });
                }
                if range.start == segment.projection_start()
                    && spec.label_row.is_none()
                    && !spec.slot.is_some_and(|slot| {
                        matches!(slot.card_accent, crate::adaptive::CardAccent::Resource(_))
                            || (slot.cards
                                && slot.group == segment.top_level_node_id
                                && slot.card_accent == crate::adaptive::CardAccent::OpenLabeled
                                && segment.context.list_depth == 1
                                && segment.context.ordered_list_depth == 0
                                && segment.context.task_checked.is_none())
                    })
                    && let Some(marker) = segment.context.list_marker.as_deref()
                {
                    let numbered = marker.ends_with('.');
                    let table_leading_edge = segment
                        .context
                        .table_cell
                        .and_then(|(table, _, _)| editor.projection.table_context(table))
                        .filter(|table| segment.context.list_depth == table.outer.list_depth)
                        .map(|_| bounds.left() + px(viewport_left));
                    let code_item = is_code && spec.table_cell.is_none();
                    let marker_line = if let Some(component) = leading_container {
                        Bounds::new(
                            bounds.origin + component.origin,
                            size(
                                component.size.width,
                                px(DocumentStyle::REFERENCE_LEADING * editor.zoom_factor),
                            ),
                        )
                    } else if code_item {
                        code_item_marker_line(spec, bounds, editor.zoom_factor)
                    } else {
                        line_bounds
                    };
                    let marker_mask =
                        if table_leading_edge.is_some() || code_item || leading_container.is_some()
                        {
                            None
                        } else {
                            content_mask
                        };
                    let numbered_tile = spec.slot.is_some_and(|slot| {
                        slot.card_accent == crate::adaptive::CardAccent::Numbered
                    });
                    let marker_bounds = if numbered_tile {
                        Bounds::new(
                            point(
                                line_bounds.left(),
                                line_bounds.top() - px(40. * editor.zoom_factor),
                            ),
                            size(px(64. * editor.zoom_factor), px(36. * editor.zoom_factor)),
                        )
                    } else {
                        list_marker_bounds(
                            marker_line,
                            list_layout == Some(ListLayout::Steps),
                            numbered,
                            table_leading_edge,
                            editor.zoom_factor,
                        )
                    };
                    if list_layout == Some(ListLayout::Steps) && marker.ends_with('.') {
                        chrome.push(MaskedQuad {
                            quad: fill(marker_bounds, rgb(palette.accent))
                                .corner_radii(px(13. * editor.zoom_factor)),
                            content_mask: marker_mask,
                        });
                    }
                    let task = segment.context.task_checked;
                    let marker = if list_layout == Some(ListLayout::Steps) || numbered_tile {
                        marker.trim_end_matches('.')
                    } else {
                        marker
                    };
                    if let Some(checked) = task {
                        checkboxes.push(PaintedCheckbox {
                            bounds: task_checkbox_bounds(marker_line, editor.zoom_factor),
                            checked,
                            content_mask: marker_mask,
                        });
                    }
                    if task.is_none() && !numbered {
                        chrome.push(MaskedQuad {
                            quad: fill(
                                Bounds::new(
                                    point(
                                        marker_bounds.center().x - px(3.5 * editor.zoom_factor),
                                        marker_bounds.center().y - px(3.5 * editor.zoom_factor),
                                    ),
                                    size(px(7. * editor.zoom_factor), px(7. * editor.zoom_factor)),
                                ),
                                rgb(palette.accent),
                            )
                            .corner_radii(px(3.5 * editor.zoom_factor)),
                            content_mask: marker_mask,
                        });
                    }
                    if task.is_none() && numbered {
                        let mut font = text_style.font();
                        font.family = DocumentStyle::BODY_FONT_FAMILY.into();
                        if numbered_tile {
                            font.family = DocumentStyle::HEADLINE_FONT_FAMILY.into();
                            font.weight = FontWeight::EXTRA_BOLD;
                        }
                        let marker_layout = window.text_system().shape_line(
                            marker.to_owned().into(),
                            px(if numbered_tile {
                                30.
                            } else if segment.context.bibliography.is_some() {
                                DocumentStyle::READING_SIZE
                            } else {
                                15.
                            } * editor.zoom_factor),
                            &[TextRun {
                                len: marker.len(),
                                font,
                                color: rgb(if segment.context.bibliography.is_some() {
                                    palette.secondary
                                } else if list_layout == Some(ListLayout::Steps) {
                                    palette.page
                                } else {
                                    palette.accent
                                })
                                .into(),
                                background_color: None,
                                underline: None,
                                strikethrough: None,
                            }],
                            None,
                        );
                        decorations.push(PaintedLine {
                            range: range.start..range.start,
                            layout: marker_layout,
                            bounds: marker_bounds,
                            line_height: px(if numbered_tile {
                                36.
                            } else if list_layout == Some(ListLayout::Steps) {
                                25. * editor.zoom_factor
                            } else {
                                visual_style.line_height
                            }),
                            horizontal_owner,
                            content_mask: marker_mask,
                            alignment: if numbered_tile {
                                ColumnAlignment::Left
                            } else {
                                ColumnAlignment::Center
                            },
                        });
                    }
                }
            }
            if let Some(block) = block
                && let Some((mut number, mut rail)) = code_gutter::paint(
                    spec,
                    line_bounds,
                    horizontal_offset,
                    editor.zoom_factor,
                    code_palette(palette, block),
                    window,
                )
            {
                number.content_mask = table_content_mask;
                rail.content_mask = table_content_mask;
                decorations.push(number);
                chrome.push(rail);
            }
            let line_text = if spec.html_preview.is_some()
                || spec.rendered_code_preview()
                || segment.is_some_and(|segment| segment.context.image_source.is_some())
            {
                ""
            } else if text.is_empty() {
                "Start writing…"
            } else {
                &text[range.clone()]
            };
            let runs = styled_runs(
                editor,
                &range,
                line_text.len(),
                &text_style,
                text.is_empty(),
                palette,
            );
            let runs = apply_marked_runs(runs, marked.as_ref(), &range, palette);
            let runs = if let Some(inline) = &spec.inline_math {
                inline_math::hide_sources(runs, &(0..range.len()), &inline.attachments)
            } else {
                runs
            };
            let mut cache_key = shape_cache_key(ShapeCacheInput {
                snapshot: &snapshot,
                segment,
                range: &range,
                runs: &runs,
                palette,
                font_size: visual_style.font_size,
                width: line_bounds.size.width.into(),
                scale: window.scale_factor(),
                marked: marked.as_ref(),
            });
            let prose = segment
                .is_some_and(|segment| typography::eligible(&editor.projection, segment))
                && spec.inline_math.is_none()
                && spec.html_preview.is_none();
            let hyphen = prose
                && editor.typography.hyphenate
                && segment.is_some_and(|segment| {
                    editor
                        .measurement
                        .hyphen_breaks(&editor.projection, segment)
                        .binary_search(&range.end)
                        .is_ok()
                });
            let justify = prose
                && editor.typography.justify
                && segment.is_some_and(|segment| {
                    typography::continues(
                        &editor.projection,
                        segment,
                        &range,
                        spec.label_row.map(|(part, _)| part),
                    )
                });
            cache_key.typography = (justify, hyphen);
            let cached = editor.shaped_line_cache.borrow_mut().get(&cache_key);
            let layout = cached.unwrap_or_else(|| {
                let mut runs = runs.clone();
                let display = typography::with_suffix(line_text, &mut runs, hyphen);
                let mut layout = window.text_system().shape_line(
                    display.into(),
                    px(visual_style.font_size),
                    &runs,
                    None,
                );
                if hyphen {
                    layout = layout.with_len(line_text.len());
                    layout.text = line_text.to_owned().into();
                }
                if justify {
                    layout = typography::justify(layout, line_bounds.size.width);
                }
                editor
                    .shaped_line_cache
                    .borrow_mut()
                    .insert(cache_key, layout.clone());
                layout
            });
            let layout = if let Some(inline) = &spec.inline_math {
                inline_math::compose(
                    layout,
                    &(0..range.len()),
                    &inline.attachments,
                    visual_style.font_size,
                )
            } else {
                layout
            };
            let alignment = table_column_alignment(&editor.projection, segment);
            let text_left = aligned_text_left(line_bounds, &layout, alignment);
            for link_bounds in
                inline_link_bounds(&editor.projection, &range, &layout, line_bounds, alignment)
            {
                window.with_content_mask(content_mask, |window| {
                    // Cursor-only regions: normal click-to-edit, selection and
                    // Ctrl-click activation continue through the editor.
                    if intersect_bounds(link_bounds, window.content_mask().bounds).is_some() {
                        link_hitboxes.push(window.insert_hitbox(link_bounds, Default::default()));
                    }
                });
            }
            if let Some(segment) = segment
                && let Some(badge) = segment.context.badge
                && let Some(local) = badge_range(segment, badge, &range)
            {
                let signal = badge.tone.style(palette);
                let start_x = shaped_x_for_index(&layout, local.start);
                let end_x = shaped_x_for_index(&layout, local.end);
                let left = start_x.min(end_x);
                let right = start_x.max(end_x);
                let inset = px(DocumentStyle::BADGE_INSET * editor.zoom_factor);
                let vertical = px(DocumentStyle::BADGE_VERTICAL_INSET * editor.zoom_factor);
                chrome.push(MaskedQuad {
                    quad: fill(
                        Bounds::new(
                            point(text_left + left - inset, line_bounds.top() - vertical),
                            size(
                                right - left + 2. * inset,
                                line_bounds.size.height + 2. * vertical,
                            ),
                        ),
                        rgb(signal.paper),
                    )
                    .corner_radii(line_bounds.size.height / 2. + vertical)
                    .border_widths(px(editor.zoom_factor))
                    .border_color(rgb(signal.rule)),
                    // The signal includes padding outside the glyph row.
                    // Clip to the complete cell/viewport, not the text line,
                    // or the pill's top and bottom become flat cut edges.
                    content_mask: table_content_mask
                        .or(content_mask)
                        .map(|mask| mask.intersect(&ContentMask { bounds: visible })),
                });
            }
            if let Some(owner) = horizontal_owner
                && segment.is_some_and(|segment| segment.context.table_cell.is_none())
            {
                record_horizontal_metrics(
                    &mut horizontal_metrics,
                    owner,
                    if is_code {
                        viewport_width
                    } else {
                        width * spec.width_fraction
                    },
                    spec.html_preview
                        .as_ref()
                        .map_or(f32::from(layout.width()), |preview| {
                            preview.width * editor.zoom_factor
                        })
                        + if is_code {
                            // Use the same inner viewport as the content mask.
                            // Counting the enclosing card's padding as visible
                            // code space leaves trailing source unreachable.
                            spec.inset + spec.code_gutter()
                                - (viewport_left - width * spec.x_fraction)
                                + CODE_BLOCK_PADDING * editor.zoom_factor
                        } else {
                            spec.inset + 8. * editor.zoom_factor
                        },
                );
            }

            let overlap = selected.start.max(range.start)..selected.end.min(range.end);
            if overlap.start < overlap.end {
                let start_x = if spec.rendered_code_preview() {
                    px(0.)
                } else {
                    shaped_x_for_index(&layout, overlap.start - range.start)
                };
                let end_x = if spec.rendered_code_preview() {
                    line_bounds.size.width
                } else {
                    shaped_x_for_index(&layout, overlap.end - range.start)
                };
                overlays.push(MaskedQuad {
                    quad: fill(
                        Bounds::from_corners(
                            point(text_left + start_x.min(end_x), line_bounds.top()),
                            point(text_left + start_x.max(end_x), line_bounds.bottom()),
                        ),
                        rgb(block.map_or(palette.selection, |block| {
                            code_palette(palette, block).selection
                        })),
                    ),
                    content_mask,
                });
            }
            if selected.is_empty()
                && !spec.rendered_code_preview()
                && selected.start >= range.start
                && selected.start <= range.end
                && caret.is_none()
            {
                caret = Some(MaskedQuad {
                    quad: fill(
                        Bounds::new(
                            point(
                                text_left
                                    + shaped_x_for_index(&layout, selected.start - range.start),
                                line_bounds.top() + px(2.),
                            ),
                            size(px(1.5), px(visual_style.line_height - 4.)),
                        ),
                        rgb(palette.accent),
                    ),
                    content_mask,
                });
            }
            if spec.table_cell.is_some()
                && spec.table_record.is_none()
                && let Some(hit) =
                    table_resize::hit_bounds(line_bounds, editor.zoom_factor, content_mask)
                && let Some(hit) = intersect_bounds(hit, visible)
            {
                resize_hitboxes.push(window.insert_hitbox(hit, Default::default()));
            }
            lines.push(PaintedLine {
                range,
                layout,
                bounds: line_bounds,
                line_height: px(visual_style.line_height),
                horizontal_owner,
                content_mask,
                alignment,
            });
        }

        overlays.extend(selected_code_panels);
        TextPrepaintState {
            semantic_bounds: bounds,
            semantic_scale: window.scale_factor(),
            semantic_actions: accessibility::ActionTargets::default(),
            lines,
            decorations,
            checkboxes,
            resource_icons,
            metric_icons,
            link_hitboxes,
            resize_hitboxes,
            chrome,
            overlays,
            caret,
            horizontal_metrics,
            math_scroll_handles: self.math_scroll_handles.clone(),
        }
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&gpui::InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut Self::RequestLayoutState,
        state: &mut Self::PrepaintState,
        window: &mut Window,
        cx: &mut App,
    ) {
        accessibility::register_actions(&state.semantic_actions, &self.editor, window);
        drag_scroll::register_outside_pointer(&self.editor, window);
        if self.editor.read(cx).table_resize_drag.is_none() {
            for hitbox in &state.link_hitboxes {
                window.set_cursor_style(CursorStyle::PointingHand, hitbox);
            }
        }
        for hitbox in &state.resize_hitboxes {
            window.set_cursor_style(CursorStyle::ResizeColumn, hitbox);
        }
        let focus_handle = self.editor.read(cx).focus_handle.clone();
        window.handle_input(
            &focus_handle,
            ElementInputHandler::new(bounds, self.editor.clone()),
            cx,
        );
        for masked in state.chrome.drain(..) {
            window.with_content_mask(masked.content_mask, |window| {
                window.paint_quad(masked.quad);
            });
        }
        for masked in state.overlays.drain(..) {
            window.with_content_mask(masked.content_mask, |window| {
                window.paint_quad(masked.quad);
            });
        }
        for decoration in &state.decorations {
            window.with_content_mask(decoration.content_mask, |window| {
                let _ = decoration.layout.paint(
                    decoration.bounds.origin,
                    decoration.line_height,
                    gpui_text_alignment(decoration.alignment),
                    Some(decoration.bounds.size.width),
                    window,
                    cx,
                );
            });
        }
        let palette = TachyonPalette::for_dark(cx.theme().is_dark());
        for icon in &state.resource_icons {
            let _ = window.paint_svg(
                *icon,
                IconName::ExternalLink.path(),
                None,
                Default::default(),
                rgb(palette.accent).into(),
                cx,
            );
        }
        for (icon, mask) in &state.metric_icons {
            window.with_content_mask(Some(*mask), |window| {
                let _ = window.paint_svg(
                    *icon,
                    IconName::File.path(),
                    None,
                    Default::default(),
                    rgb(palette.secondary).into(),
                    cx,
                );
            });
        }
        for checkbox in &state.checkboxes {
            checkbox.paint(palette, window, cx);
        }
        for line in &state.lines {
            window.with_content_mask(line.content_mask, |window| {
                let _ = line.layout.paint(
                    line.bounds.origin,
                    line.line_height,
                    gpui_text_alignment(line.alignment),
                    Some(line.bounds.size.width),
                    window,
                    cx,
                );
            });
        }
        if focus_handle.is_focused(window)
            && let Some(caret) = state.caret.take()
        {
            window.with_content_mask(caret.content_mask, |window| {
                window.paint_quad(caret.quad);
            });
        }
        self.editor.update(cx, |editor, cx| {
            editor.painted_lines.clone_from(&state.lines);
            editor
                .horizontal_metrics
                .clone_from(&state.horizontal_metrics);
            // A resized element can still be painting the previous committed
            // fractions while its replacement is prepared. Those temporary
            // metrics must not erase the user's component-local scroll.
            if (editor.layout_width - f32::from(bounds.size.width)).abs() < 0.5 {
                editor.horizontal_scrolls.retain(|owner, offset| {
                    let Some((viewport, content)) = state.horizontal_metrics.get(owner) else {
                        return false;
                    };
                    *offset = clamped_horizontal_scroll(*offset, 0., *viewport, *content);
                    true
                });
            }
            editor.publish_painted_bounds(bounds, cx);
        });
    }
}

fn shape_cache_key(input: ShapeCacheInput<'_>) -> ShapeCacheKey {
    let ShapeCacheInput {
        snapshot,
        segment,
        range,
        runs,
        palette,
        font_size,
        width,
        scale,
        marked,
    } = input;
    let (node_id, node_revision, fragment) = segment.map_or_else(
        || (None, snapshot.revision(), range.clone()),
        |segment| {
            let start =
                segment.node_range.start + range.start.saturating_sub(segment.projection_start());
            let end = start + range.len();
            (
                Some(segment.node_id),
                snapshot
                    .node_revision(segment.node_id)
                    .unwrap_or_else(|| snapshot.revision()),
                start..end,
            )
        },
    );
    let marked_fragment = marked.and_then(|marked| {
        let overlap = marked.start.max(range.start)..marked.end.min(range.end);
        (overlap.start < overlap.end)
            .then(|| overlap.start - range.start..overlap.end - range.start)
    });
    let mut font_hasher = DefaultHasher::new();
    palette.hash(&mut font_hasher);
    for run in runs {
        run.len.hash(&mut font_hasher);
        run.font.hash(&mut font_hasher);
        run.color.a.to_bits().hash(&mut font_hasher);
    }
    ShapeCacheKey {
        node_id,
        node_revision,
        fragment,
        font_fingerprint: font_hasher.finish(),
        font_size_bits: font_size.to_bits(),
        width_bits: width.to_bits(),
        scale_bits: scale.to_bits(),
        marked_fragment,
        typography: (false, false),
    }
}

#[cfg(test)]
fn display_line_ranges(text: &str) -> Vec<Range<usize>> {
    let mut lines = Vec::new();
    for_each_display_line_range(text, |range| lines.push(range));
    lines
}

fn for_each_display_line_range(text: &str, mut visit: impl FnMut(Range<usize>)) {
    if text.is_empty() {
        visit(0..0);
        return;
    }
    let mut start = 0;
    for (index, _) in text.match_indices('\n') {
        visit(start..index);
        start = index + 1;
    }
    visit(start..text.len());
}

#[derive(Clone, Copy)]
struct TableRuleScale {
    display: f32,
    zoom: f32,
}

fn push_table_border(
    output: &mut Vec<MaskedQuad>,
    bounds: Bounds<Pixels>,
    border: TableBorder,
    content_mask: Option<ContentMask<Pixels>>,
    border_color: u32,
    scale: TableRuleScale,
    outer_edges: (bool, bool),
) {
    if border != TableBorder::None
        && scale.display.is_finite()
        && scale.display > 0.
        && scale.zoom.is_finite()
        && scale.zoom > 0.
    {
        // Shared edges have one owner. Anchor to the device grid without
        // changing the authored unit: a logical rule can cover fractional
        // device pixels, while an explicit hairline is always one device pixel.
        let snap = |value: Pixels| px((f32::from(value) * scale.display).round() / scale.display);
        let left = snap(bounds.left());
        let right = snap(bounds.right());
        let top = snap(bounds.top());
        let bottom = snap(bounds.bottom());
        let stroke = px(if border == TableBorder::PhysicalPixel {
            1. / scale.display
        } else {
            scale.zoom
        });
        for (edge, horizontal) in [
            Some((
                Bounds::new(point(left, bottom - stroke), size(right - left, stroke)),
                true,
            )),
            Some((
                Bounds::new(point(right - stroke, top), size(stroke, bottom - top)),
                false,
            )),
            outer_edges.0.then(|| {
                (
                    Bounds::new(point(left, top), size(right - left, stroke)),
                    true,
                )
            }),
            outer_edges.1.then(|| {
                (
                    Bounds::new(point(left, top), size(stroke, bottom - top)),
                    false,
                )
            }),
        ]
        .into_iter()
        .flatten()
        {
            let Some(visible) =
                content_mask.map_or(Some(edge), |mask| intersect_bounds(edge, mask.bounds))
            else {
                continue;
            };
            if border != TableBorder::Dotted {
                output.push(MaskedQuad {
                    quad: fill(edge, rgb(border_color)),
                    content_mask,
                });
                continue;
            }
            // Iterate only the visible span, keeping the pattern phase tied
            // to the complete edge. A huge row must not generate offscreen dots
            // during scrolling or have its pattern slide with the viewport.
            let origin = f32::from(if horizontal { edge.left() } else { edge.top() });
            let start = f32::from(if horizontal {
                visible.left()
            } else {
                visible.top()
            });
            let end = f32::from(if horizontal {
                visible.right()
            } else {
                visible.bottom()
            });
            let period = f32::from(stroke) * 3.;
            let first = ((start - origin) / period).floor().max(0.);
            let count = ((end - start) / period).ceil() as usize + 2;
            for index in 0..count {
                let offset = px(origin + (first + index as f32) * period);
                let dot = Bounds::new(
                    if horizontal {
                        point(offset, edge.top())
                    } else {
                        point(edge.left(), offset)
                    },
                    size(stroke, stroke),
                );
                if intersect_bounds(dot, visible).is_some() {
                    output.push(MaskedQuad {
                        quad: fill(dot, rgb(border_color)).corner_radii(stroke / 2.),
                        content_mask: Some(ContentMask { bounds: visible }),
                    });
                }
            }
        }
    }
}

#[cfg(test)]
fn build_visual_lines(document: &Document, projection: &TextProjection) -> Vec<VisualLineSpec> {
    let _ = document;
    build_visual_lines_with_images(projection, &HashMap::new(), 760.)
}

fn minimap_source_line(
    projection: &TextProjection,
    line: &VisualLineSpec,
    layout_width: f32,
    zoom_factor: f32,
) -> Option<MinimapSourceLine> {
    let segment = segment_for_line(projection, &line.projected_range())?;
    let block = projection.block(segment.node_id)?;
    let text = projection
        .text()
        .get(line.projected_range())
        .unwrap_or_default();
    let first_visual_line = line.projected_start() == segment.projection_start();
    let top_level_is_task_list = matches!(
        projection.block(segment.top_level_node_id),
        Some(BlockNode::List(list)) if list.kind == document_core::ListKind::Task
    );
    let kind = if let Some((table_id, row, column, _)) = line.table_cell {
        MinimapSourceKind::Table {
            table_id,
            row,
            column,
        }
    } else if segment.context.image_source.is_some() || matches!(block, BlockNode::Image(_)) {
        MinimapSourceKind::Image
    } else if let BlockNode::Heading(heading) = block {
        MinimapSourceKind::Heading {
            level: heading.level,
        }
    } else if matches!(block, BlockNode::CodeBlock(_)) {
        MinimapSourceKind::Code(minimap_code_tone(text))
    } else if top_level_is_task_list {
        MinimapSourceKind::Task {
            checked: segment.context.task_checked.unwrap_or(false),
        }
    } else if segment.context.list_depth > 0 {
        MinimapSourceKind::List
    } else if line.projected_range().is_empty() {
        MinimapSourceKind::Placeholder
    } else {
        MinimapSourceKind::Text
    };
    let columns = estimated_wrap_columns(
        block,
        segment,
        projection,
        layout_width / zoom_factor.max(MIN_ZOOM),
    );
    Some(MinimapSourceLine {
        node_id: segment.node_id,
        top_level_node_id: segment.top_level_node_id,
        kind,
        y: line.y,
        height: line.style.line_height,
        inset: line.inset,
        x_fraction: line.x_fraction,
        width_fraction: line.width_fraction,
        fill_fraction: minimap_text_fill(text, columns, kind),
        first_visual_line,
        alert: segment
            .context
            .alert
            .as_ref()
            .map(|(id, kind)| (*id, minimap_alert_tone(kind))),
        quote_depth: segment.context.quote_depth,
    })
}

fn minimap_text_fill(text: &str, columns: usize, kind: MinimapSourceKind) -> f32 {
    if matches!(kind, MinimapSourceKind::Image) {
        return 1.;
    }
    let graphemes = text.trim_end().graphemes(true).count();
    if graphemes == 0 {
        return 0.18;
    }
    let reference = if columns == usize::MAX {
        72
    } else {
        columns.max(1)
    };
    (graphemes as f32 / reference as f32).clamp(0.2, 1.)
}

fn minimap_code_tone(text: &str) -> MinimapCodeTone {
    let trimmed = text.trim_start();
    if trimmed.starts_with("//")
        || trimmed.starts_with('#')
        || trimmed.starts_with("<!--")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
    {
        MinimapCodeTone::Comment
    } else if trimmed.contains(['\"', '\'']) {
        MinimapCodeTone::String
    } else if trimmed
        .split(|character: char| !character.is_alphanumeric() && character != '_')
        .any(|word| {
            matches!(
                word,
                "as" | "async"
                    | "await"
                    | "const"
                    | "enum"
                    | "fn"
                    | "for"
                    | "impl"
                    | "let"
                    | "match"
                    | "mod"
                    | "pub"
                    | "return"
                    | "struct"
                    | "trait"
                    | "use"
            )
        })
    {
        MinimapCodeTone::Keyword
    } else {
        MinimapCodeTone::Plain
    }
}

fn minimap_alert_tone(kind: &AlertKind) -> MinimapAlertTone {
    match kind {
        AlertKind::Note | AlertKind::Important => MinimapAlertTone::Info,
        AlertKind::Tip => MinimapAlertTone::Success,
        AlertKind::Warning => MinimapAlertTone::Warning,
        AlertKind::Caution => MinimapAlertTone::Error,
        AlertKind::Other(_) => MinimapAlertTone::Other,
    }
}

fn scale_visual_lines(lines: &mut [VisualLineSpec], scale: f32) {
    if (scale - 1.).abs() < f32::EPSILON {
        return;
    }
    for line in lines {
        line.style.font_size *= scale;
        line.style.line_height *= scale;
        line.style.space_above *= scale;
        line.style.space_below *= scale;
        line.inset *= scale;
        if line.code_line.is_some()
            && let Some(code) = &mut line.payload_mut().code_line
        {
            code.width *= scale;
        }
        line.gap_before *= scale;
        line.y *= scale;
        if line.flow_geometry {
            let payload = line.payload_mut();
            payload.table_row_y *= scale;
            payload.table_row_height *= scale;
            if let Some(record) = &mut payload.table_record {
                record.top *= scale;
                record.height *= scale;
            }
        }
        if line.record_label.is_some() {
            let payload = line.payload_mut();
            let label = Arc::make_mut(payload.record_label.as_mut().unwrap());
            label.width *= scale;
            label.height *= scale;
        }
        if line.label_row.is_some()
            && let Some((_, width)) = &mut line.payload_mut().label_row
        {
            *width *= scale;
        }
    }
}

fn capture_scroll_anchor(
    snapshot: &document_core::DocumentSnapshot,
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    scroll_y: f32,
) -> Option<EditorScrollAnchor> {
    let line = lines.iter().min_by(|left, right| {
        (left.y - scroll_y)
            .abs()
            .total_cmp(&(right.y - scroll_y).abs())
    })?;
    scroll_anchor_for_line(snapshot, projection, line, scroll_y - line.y)
}

fn scroll_anchor_for_line(
    snapshot: &document_core::DocumentSnapshot,
    projection: &TextProjection,
    line: &VisualLineSpec,
    intra_line_offset: f32,
) -> Option<EditorScrollAnchor> {
    let segment = projection.segment_for_range(&line.projected_range())?;
    let node_text_hint = snapshot
        .node(segment.node_id)
        .map(BlockNode::plain_text)
        .map(|text| text.chars().take(96).collect())?;
    Some(EditorScrollAnchor {
        node_id: segment.node_id,
        node_text_hint,
        node_text_offset: line
            .projected_range()
            .start
            .saturating_sub(segment.projection_start())
            .min(segment.node_range.len()),
        projection_offset: line.projected_start(),
        intra_line_offset,
    })
}

fn resolve_scroll_anchor(
    snapshot: &document_core::DocumentSnapshot,
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    anchor: &EditorScrollAnchor,
) -> Option<f32> {
    let matches_hint = |segment: &&crate::ProjectionSegment| {
        snapshot.node(segment.node_id).is_some_and(|block| {
            block
                .plain_text()
                .chars()
                .take(96)
                .eq(anchor.node_text_hint.chars())
        })
    };
    let segment = projection
        .segments()
        .iter()
        .find(|segment| segment.node_id == anchor.node_id && matches_hint(segment))
        .or_else(|| {
            projection
                .segments()
                .iter()
                .filter(matches_hint)
                .min_by_key(|segment| {
                    segment
                        .projection_start()
                        .abs_diff(anchor.projection_offset)
                })
        })?;
    let target = segment.projection_start() + anchor.node_text_offset.min(segment.projection_len());
    let belongs_to_segment = |line: &&VisualLineSpec| {
        projection
            .segment_for_range(&line.projected_range())
            .is_some_and(|candidate| candidate.node_id == segment.node_id)
    };
    // Wrapped ranges share their boundary: the preceding line's end equals
    // the following line's start. Captured viewport anchors store the visible
    // line's start, so prefer that exact downstream line before the inclusive
    // fallback used for mid-line and terminal offsets. Otherwise every reflow
    // can walk the viewport backward by one line.
    let line = lines
        .iter()
        .find(|line| line.projected_start() == target && belongs_to_segment(line))
        .or_else(|| {
            lines.iter().find(|line| {
                line.projected_start() <= target
                    && target <= line.projected_end()
                    && belongs_to_segment(line)
            })
        })?;
    Some((line.y + anchor.intra_line_offset).max(0.))
}

struct VisualTextRefresh {
    old_y_after: f32,
    y_delta: f32,
    components: ComponentRefresh,
}

enum ComponentRefresh {
    Unchanged,
    ShiftSuffixUp { old_y_after: f32, paint_end: usize },
    ReplaceSimpleAndShiftUp(arrangement::SimpleComponentRebase),
    Rebuild,
}

fn same_component_line_geometry(left: &VisualLineSpec, right: &VisualLineSpec) -> bool {
    left.y.to_bits() == right.y.to_bits()
        && left.x_fraction.to_bits() == right.x_fraction.to_bits()
        && left.width_fraction.to_bits() == right.width_fraction.to_bits()
        && left.inset.to_bits() == right.inset.to_bits()
        && left.style.line_height.to_bits() == right.style.line_height.to_bits()
        && left.slot == right.slot
        && left.table_cell == right.table_cell
        && left.table_row_y.to_bits() == right.table_row_y.to_bits()
        && left.table_row_height.to_bits() == right.table_row_height.to_bits()
}

fn component_refresh(
    old: &[VisualLineSpec],
    replacement: &[VisualLineSpec],
    y_delta: f32,
    old_y_after: f32,
    paint_end: usize,
) -> ComponentRefresh {
    let same = old.len() == replacement.len()
        && !old
            .iter()
            .zip(replacement)
            .any(|(left, right)| !same_component_line_geometry(left, right));
    if y_delta.abs() <= f32::EPSILON && same {
        ComponentRefresh::Unchanged
    } else if y_delta < 0. && same {
        ComponentRefresh::ShiftSuffixUp {
            old_y_after,
            paint_end,
        }
    } else {
        ComponentRefresh::Rebuild
    }
}

struct SimpleComponentRefreshRequest<'a> {
    projection: &'a TextProjection,
    node_id: NodeId,
    old: &'a [VisualLineSpec],
    replacement: &'a [VisualLineSpec],
    layout_width: f32,
    first_line: usize,
    old_y_after: f32,
    y_delta: f32,
    paint_start: usize,
}

fn simple_component_refresh(
    request: SimpleComponentRefreshRequest<'_>,
) -> Option<ComponentRefresh> {
    let SimpleComponentRefreshRequest {
        projection,
        node_id,
        old,
        replacement,
        layout_width,
        first_line,
        old_y_after,
        y_delta,
        paint_start,
    } = request;
    let segment = projection.segment_for_node(node_id)?;
    let rejected = y_delta >= 0.
        || old.is_empty()
        || segment.top_level_node_id != node_id
        || segment.context.alert.is_some()
        || segment.context.quote.is_some()
        || segment.context.table_cell.is_some()
        || !segment.context.list_ancestors.is_empty()
        || !segment.context.quote_ancestors.is_empty()
        || replacement
            .iter()
            .any(|line| line.slot.is_some() || line.table_cell.is_some());
    if rejected {
        return None;
    }
    let first = replacement.first()?;
    let geometry = arrangement::ComponentGeometry {
        left_fraction: replacement
            .iter()
            .map(|line| line.x_fraction + line.inset / layout_width.max(1.))
            .fold(f32::INFINITY, f32::min),
        right_fraction: replacement
            .iter()
            .map(|line| line.x_fraction + line.width_fraction)
            .fold(f32::NEG_INFINITY, f32::max),
        top: replacement
            .iter()
            .map(|line| line.y)
            .fold(first.y, f32::min),
        bottom: replacement
            .iter()
            .map(|line| line.y + line.style.line_height)
            .fold(first.y + first.style.line_height, f32::max),
        first_line,
    };
    let line_bottoms = replacement
        .iter()
        .map(|line| line.y + line.style.line_height)
        .collect::<Vec<_>>();
    Some(ComponentRefresh::ReplaceSimpleAndShiftUp(
        arrangement::SimpleComponentRebase {
            id: node_id,
            geometry,
            heading_y: matches!(projection.block(node_id), Some(BlockNode::Heading(_)))
                .then_some(first.y),
            old_y_after,
            y_delta,
            paint_start,
            paint_end: paint_start + old.len(),
            line_bottoms,
        },
    ))
}

struct TextRefreshRequest<'a> {
    snapshot: &'a document_core::DocumentSnapshot,
    node_id: NodeId,
    image_dimensions: &'a NodeImageDimensions,
    layout_width: f32,
    zoom_factor: f32,
    measurement: Option<&'a FontMeasurement>,
}

fn shift_retained_lines(
    lines: &mut [VisualLineSpec],
    coordinate_chunk: &Arc<crate::projection::ProjectionOffset>,
    byte_delta: isize,
    y_delta: f32,
) -> Option<()> {
    if byte_delta != 0 {
        // Visual lines remain in source order. Only the bounded remainder of
        // the edited projection chunk has chunk-local byte coordinates to
        // rebase; the first different handle starts the next chunk.
        for line in lines
            .iter_mut()
            .take_while(|line| Arc::ptr_eq(&line.source.projection_start, coordinate_chunk))
        {
            line.source
                .shift_within_chunk(coordinate_chunk, byte_delta)?;
        }
    }
    if y_delta.abs() > f32::EPSILON {
        for line in lines {
            line.y += y_delta;
            if line.flow_geometry {
                let payload = line.payload_mut();
                payload.table_row_y += y_delta;
                if let Some(record) = &mut payload.table_record {
                    record.top += y_delta;
                }
            }
        }
    }
    Some(())
}

/// Rebuild only one complete adaptive row (or the lead paragraph). Topology
/// and widths come from the retained plan; no candidate search or unrelated
/// text shaping is permitted on this typing path. Offset/index maintenance is
/// still linear in following content, just as in the ordinary text fast path.
fn refresh_arranged_text_node_geometry(
    projection: &mut TextProjection,
    visual_lines: &mut Vec<VisualLineSpec>,
    paint_order: &mut Vec<usize>,
    plan: &AdaptivePlan,
    request: TextRefreshRequest<'_>,
) -> Option<VisualTextRefresh> {
    let TextRefreshRequest {
        snapshot,
        node_id,
        image_dimensions,
        layout_width,
        zoom_factor,
        measurement,
    } = request;
    let segment = projection.segment_for_node(node_id)?;
    let table_cell = segment.context.table_cell;
    let node_first =
        visual_lines.partition_point(|line| line.projected_start() < segment.projection_start());
    let slot = visual_lines.get(node_first)?.slot;
    let code_width = visual_lines
        .get(node_first)?
        .code_line
        .map(|c| (c.width, c.strip));
    let prose_flow = plan.prose_flows.get(&node_id);
    let figure_flow = plan.figure_flows.get(&node_id);
    let inline_list = plan.inline_lists.get(&node_id);
    let in_row = |line: &VisualLineSpec| {
        if inline_list.is_some() {
            segment.projection_start() <= line.projected_start()
                && line.projected_end() <= segment.projection_end()
        } else if let Some(flow) = figure_flow {
            line.slot.is_some_and(|slot| slot.group == flow.text.group)
        } else if let Some(flow) = prose_flow {
            line.slot.is_some_and(|slot| slot.group == flow.group)
        } else if let Some(slot) = slot {
            line.slot.is_some_and(|candidate| candidate.same_row(slot))
        } else if let Some((table, row, _)) = table_cell {
            line.table_cell
                .is_some_and(|(id, candidate_row, _, _)| id == table && candidate_row == row)
        } else {
            segment.projection_start() <= line.projected_start()
                && line.projected_end() <= segment.projection_end()
        }
    };
    let first = visual_lines[..node_first]
        .iter()
        .rposition(|line| !in_row(line))
        .map_or(0, |i| i + 1);
    let end = node_first
        + visual_lines[node_first..]
            .iter()
            .position(|line| !in_row(line))
            .unwrap_or(visual_lines.len() - node_first);
    let leading = visual_lines.get(first)?.gap_before;
    let y_before = visual_lines[first].y - visual_lines[first].style.space_above - leading;
    let old_y_after = visual_lines[first..end]
        .iter()
        .map(|line| line.y + line.style.line_height + line.style.space_below)
        .fold(y_before, f32::max);
    let range_start = visual_lines[first].projected_start();
    let range_end = visual_lines.get(end.checked_sub(1)?)?.projected_end();
    let segment_start = projection
        .segments()
        .partition_point(|s| s.projection_end() < range_start);
    let segment_end = projection
        .segments()
        .partition_point(|s| s.projection_start() <= range_end);
    // A complete row is contiguous in paint order even though its columns
    // interleave internally. Check that invariant before touching projection.
    let paint_start = paint_order.partition_point(|index| *index < first);
    let paint_end = paint_order.partition_point(|index| *index < end);
    if paint_end - paint_start != end - first
        || paint_order[paint_start..paint_end]
            .iter()
            .any(|i| !(first..end).contains(i))
    {
        return None;
    }
    // Complete bounded rows only. Oversized/nested rows keep the established
    // general renderer fallback instead of silently omitting any content.
    if segment_end - segment_start > if table_cell.is_some() { 512 } else { 32 }
        || range_end - range_start > 64 * 1024
        || snapshot.node(node_id)?.text()?.len() > 64 * 1024
    {
        return None;
    }
    if table_cell.is_some() {
        projection.lock_table_for_node(Some(node_id));
    }
    projection.command_strip_lock =
        code_panel::active_strip(projection, visual_lines, Some(node_id), zoom_factor);
    let (_, byte_delta) = projection.refresh_text_node(snapshot, node_id)?;
    let coordinate_chunk = projection
        .segment_for_node(node_id)?
        .projection_start
        .clone();
    let mut replacement = arrangement::build_measured_visual_lines_for_segments(
        projection,
        image_dimensions,
        layout_width / zoom_factor,
        plan,
        measurement,
        segment_start..segment_end,
    );
    replacement.first_mut()?.gap_before = leading / zoom_factor;
    let new_y_after = position_visual_lines(
        &mut replacement,
        projection,
        layout_width / zoom_factor,
        y_before / zoom_factor,
    ) * zoom_factor;
    scale_visual_lines(&mut replacement, zoom_factor);
    code_gutter::retain_width(&mut replacement, projection, node_id, code_width);
    let y_delta = new_y_after - old_y_after;
    let components = component_refresh(
        &visual_lines[first..end],
        &replacement,
        y_delta,
        old_y_after,
        paint_end,
    );
    shift_retained_lines(
        &mut visual_lines[end..],
        &coordinate_chunk,
        byte_delta,
        y_delta,
    )?;
    let index_delta = replacement.len() as isize - (end - first) as isize;
    let replacement_order = visual_line_paint_order(&replacement);
    if index_delta != 0 {
        for index in &mut paint_order[paint_end..] {
            *index = index.checked_add_signed(index_delta)?;
        }
    }
    paint_order.splice(
        paint_start..paint_end,
        replacement_order.into_iter().map(|i| first + i),
    );
    visual_lines.splice(first..end, replacement);
    Some(VisualTextRefresh {
        old_y_after,
        y_delta,
        components,
    })
}

fn refresh_text_node_geometry(
    projection: &mut TextProjection,
    visual_lines: &mut Vec<VisualLineSpec>,
    paint_order: &mut Vec<usize>,
    document_height: &mut f32,
    request: TextRefreshRequest<'_>,
) -> Option<VisualTextRefresh> {
    let TextRefreshRequest {
        snapshot,
        node_id,
        image_dimensions,
        layout_width,
        zoom_factor,
        measurement,
    } = request;
    let old_segment = projection.segment_for_node(node_id)?.clone();
    if old_segment.context.table_cell.is_some() {
        return None;
    }
    let first_line = visual_lines
        .partition_point(|line| line.projected_start() < old_segment.projection_start());
    let after_lines =
        visual_lines.partition_point(|line| line.projected_start() <= old_segment.projection_end());
    let first = visual_lines.get(first_line)?;
    let old_math_extent = first.preview_extent();
    let code_width = first.code_line.map(|c| (c.width, c.strip));
    let last = visual_lines.get(after_lines.checked_sub(1)?)?;
    if projection
        .segment_for_range(&first.projected_range())
        .is_none_or(|segment| segment.node_id != node_id)
        || projection
            .segment_for_range(&last.projected_range())
            .is_none_or(|segment| segment.node_id != node_id)
    {
        return None;
    }

    let y_before = first.y - first.style.space_above - first.gap_before;
    let placement = (
        first.x_fraction,
        first.width_fraction,
        first.style.space_above,
        last.style.space_below,
        first.inset,
        first.gap_before,
    );
    let old_y_after = last.y + last.style.line_height + last.style.space_below;
    let old_line_count = after_lines - first_line;
    let paint_order_is_identity = paint_order.len() == visual_lines.len()
        && paint_order
            .iter()
            .enumerate()
            .all(|(index, painted)| index == *painted);
    let paint_start = paint_order
        .binary_search_by(|index| {
            visual_lines[*index]
                .y
                .total_cmp(&first.y)
                .then_with(|| index.cmp(&first_line))
        })
        .ok()?;
    if !paint_order
        .get(paint_start..paint_start + old_line_count)?
        .iter()
        .copied()
        .eq(first_line..after_lines)
    {
        return None;
    }
    projection.command_strip_lock =
        code_panel::active_strip(projection, visual_lines, Some(node_id), zoom_factor);
    let (_, byte_delta) = projection.refresh_text_node(snapshot, node_id)?;
    let coordinate_chunk = projection
        .segment_for_node(node_id)?
        .projection_start
        .clone();
    let mut segment = projection.segment_for_node(node_id)?.clone();
    segment.context.compact_outline = first.compact_tree;
    let mut replacement = build_visual_lines_for_segment(
        projection,
        &segment,
        image_dimensions,
        layout_width * placement.1 / zoom_factor,
        &[],
        measurement,
        None,
    );
    scale_visual_lines(&mut replacement, zoom_factor);
    code_gutter::retain_width(&mut replacement, projection, node_id, code_width);
    for line in &mut replacement {
        line.x_fraction = placement.0;
        line.width_fraction = placement.1;
        line.inset = placement.4;
    }
    if let Some(first) = replacement.first_mut() {
        let new_math_extent = first.preview_extent();
        first.style.space_above = placement.2 + (new_math_extent - old_math_extent) * zoom_factor;
        first.gap_before = placement.5;
    }
    if let Some(last) = replacement.last_mut() {
        last.style.space_below = placement.3;
    }
    let new_y_after = position_visual_lines(&mut replacement, projection, layout_width, y_before);
    let y_delta = new_y_after - old_y_after;
    let replacement_len = replacement.len();
    let mut components = component_refresh(
        &visual_lines[first_line..after_lines],
        &replacement,
        y_delta,
        old_y_after,
        paint_start + old_line_count,
    );
    if matches!(components, ComponentRefresh::Rebuild)
        && let Some(simple) = simple_component_refresh(SimpleComponentRefreshRequest {
            projection,
            node_id,
            old: &visual_lines[first_line..after_lines],
            replacement: &replacement,
            layout_width,
            first_line,
            old_y_after,
            y_delta,
            paint_start,
        })
    {
        components = simple;
    }
    shift_retained_lines(
        &mut visual_lines[after_lines..],
        &coordinate_chunk,
        byte_delta,
        y_delta,
    )?;
    visual_lines.splice(first_line..after_lines, replacement);
    if paint_order_is_identity {
        paint_order.truncate(first_line);
        paint_order.extend(first_line..visual_lines.len());
    } else {
        paint_order.drain(paint_start..paint_start + old_line_count);
        let index_delta =
            isize::try_from(replacement_len).ok()? - isize::try_from(old_line_count).ok()?;
        if index_delta != 0 {
            for index in &mut paint_order[paint_start..] {
                if *index >= after_lines {
                    *index = index.checked_add_signed(index_delta)?;
                }
            }
        }
        paint_order.splice(
            paint_start..paint_start,
            first_line..first_line + replacement_len,
        );
    }
    *document_height = (*document_height + y_delta).max(LINE_HEIGHT);
    Some(VisualTextRefresh {
        old_y_after,
        y_delta,
        components,
    })
}

#[cfg(test)]
fn build_visual_lines_with_images(
    projection: &TextProjection,
    image_dimensions: &NodeImageDimensions,
    layout_width: f32,
) -> Vec<VisualLineSpec> {
    let plan = AdaptivePlan::build(projection, layout_width, None, false);
    build_arranged_visual_lines(projection, image_dimensions, layout_width, &plan)
}

fn visual_document_height(lines: &[VisualLineSpec]) -> f32 {
    lines
        .iter()
        .map(|line| line.y + line.style.line_height + line.style.space_below)
        .fold(LINE_HEIGHT, f32::max)
}

fn visual_line_paint_order(lines: &[VisualLineSpec]) -> Vec<usize> {
    let mut order = (0..lines.len()).collect::<Vec<_>>();
    order.sort_by(|left, right| {
        lines[*left]
            .y
            .total_cmp(&lines[*right].y)
            .then_with(|| left.cmp(right))
    });
    order
}

#[cfg(test)]
fn visible_paint_order_range(
    lines: &[VisualLineSpec],
    order: &[usize],
    top: f32,
    bottom: f32,
) -> Range<usize> {
    if order.is_empty() || bottom < top {
        return 0..0;
    }
    let mut start = order
        .partition_point(|index| lines[*index].y < top)
        .saturating_sub(1);
    if let Some(index) = order.get(start) {
        let first_y = lines[*index].y;
        while start > 0 && lines[order[start - 1]].y == first_y {
            start -= 1;
        }
    }
    let end = order.partition_point(|index| lines[*index].y <= bottom);
    start.min(end)..end
}

fn build_visual_lines_for_segment(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    image_dimensions: &NodeImageDimensions,
    layout_width: f32,
    presentation_breaks: &[usize],
    measurement: Option<&FontMeasurement>,
    font_size_override: Option<f32>,
) -> Vec<VisualLineSpec> {
    let build = || {
        build_visual_lines_for_segment_uncached(
            projection,
            segment,
            image_dimensions,
            layout_width,
            presentation_breaks,
            measurement,
            font_size_override,
        )
    };
    if let Some(measurement) = measurement {
        measurement.segment_geometry(
            projection,
            segment,
            image_dimensions,
            layout_width,
            presentation_breaks,
            font_size_override,
        )
    } else {
        build()
    }
}

fn build_visual_lines_for_segment_uncached(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    image_dimensions: &NodeImageDimensions,
    layout_width: f32,
    presentation_breaks: &[usize],
    measurement: Option<&FontMeasurement>,
    font_size_override: Option<f32>,
) -> Vec<VisualLineSpec> {
    let Some(block) = projection.block(segment.node_id) else {
        return Vec::new();
    };
    let text = projection.text();
    let columns = estimated_wrap_columns(block, segment, projection, layout_width);
    let record = table_records::is_cell(projection, segment, layout_width);
    let segment_text = &text[segment.projection_range()];
    let mut lines = Vec::new();
    let table_cell = segment
        .context
        .table_cell
        .and_then(|(table_id, row, column)| {
            let BlockNode::Table(table) = projection.block(table_id)? else {
                return None;
            };
            Some((table_id, row, column, table.columns.len().max(1)))
        });
    let entity_title =
        record && table_cell.is_some_and(|(_, _, column, columns)| column == 0 && columns > 2);
    let html_inset = if table_cell.is_some() {
        12. + table_insets(segment, projection).1
    } else {
        container_inset(segment)
    };
    let html_width = if table_cell.is_some() {
        segment_text_width(segment, projection, layout_width)
    } else {
        layout_width - html_inset - 8.
    };
    // Blitz runs at geometry preparation, never in the scroll paint path.
    // The resulting image is retained by this exact-width line, independent
    // of the small cross-document cache's eviction policy.
    let empty_overrides = Default::default();
    let overrides = projection
        .html_disclosure_overrides(segment.node_id)
        .unwrap_or(&empty_overrides);
    let preview = match projection.html_images(segment.node_id) {
        Some(images) => {
            crate::html::block_preview_with_images(block, html_width, overrides, &images.resources)
        }
        None => crate::html::block_preview(block, html_width, overrides),
    };
    if let Some(preview) =
        preview.filter(|preview| table_cell.is_none() || preview.width <= html_width + 1.)
    {
        let preview_height = preview.height;
        return vec![VisualLineSpec {
            compact_tree: segment.context.compact_outline,
            source: LineSourceRange::new(projection, segment.node_id, segment.projection_range())
                .expect("visual line source belongs to its projection segment"),
            payload: visual_line_payload(VisualLinePayload {
                html_preview: Some(preview),
                table_cell,
                ..VisualLinePayload::default()
            }),
            style: with_component_spacing(
                projection,
                VisualLineStyle {
                    line_height: preview_height,
                    space_above: if table_cell.is_some() { 10. } else { 8. },
                    space_below: if table_cell.is_some() { 10. } else { 24. },
                    ..VisualLineStyle::BODY
                },
                segment,
                true,
                true,
            ),
            inset: html_inset,
            gap_before: 0.,
            y: 0.,
            x_fraction: 0.,
            width_fraction: 1.,
            table_cell_first: false,
            flow_geometry: table_cell.is_some(),
        }];
    }
    let image_height =
        image_reserved_height(projection, block, segment, image_dimensions, layout_width);
    let (alert_leading, alert_header) = alert_layout_insets(projection, segment, layout_width);
    let compact_title = layout_width < DocumentStyle::SINGLE_COLUMN_WIDTH
        && segment.context.table_cell.is_none()
        && matches!(block, BlockNode::Heading(heading) if heading.level == 1);
    let feature_label_end = if let BlockNode::Paragraph(paragraph) = block {
        crate::adaptive::authored_label_end(paragraph)
            .or_else(|| crate::adaptive::resource::classify(paragraph).and_then(|r| r.body_start))
            .map(|end| segment.projection_start() + end)
            .filter(|end| presentation_breaks.contains(end))
    } else {
        None
    };
    let font_for_range = |range: &Range<usize>| {
        font_size_override.unwrap_or_else(|| {
            if entity_title {
                table_records::TITLE_SIZE
            } else if compact_title {
                DocumentStyle::COMPACT_TITLE_SIZE
            } else if feature_label_end.is_some_and(|end| range.end <= end) {
                DocumentStyle::FEATURE_TITLE_SIZE
            } else {
                visual_line_style_for(projection, block, segment, range, image_height).font_size
            }
        })
    };
    // A table's comparison columns can be wide without making their prose
    // equally wide. Keep the cell bounds for chrome, alignment and hit testing;
    // only paragraph shaping uses the loaded font's normal reading measure.
    let text_width_for_range = |range: &Range<usize>| {
        let available = segment_text_width(segment, projection, layout_width);
        if table_cell.is_some()
            && matches!(block, BlockNode::Paragraph(_))
            && let Some(fonts) = measurement
        {
            available.min(fonts.prose_width(false, font_for_range(range)))
        } else {
            available
        }
    };
    // Authored chapter numbers stay in the source line. Continuations hang
    // under the title, not under its number. Use the same shaped prefix as
    // painting; this is prepared once, never recomputed during scrolling.
    let chapter_inset = if segment.context.bibliography.is_some() && segment.context.list_depth == 0
    {
        DocumentStyle::BIBLIOGRAPHY_HANG
    } else if let BlockNode::Heading(_) = block {
        let prefix = chapter_prefix_len(segment_text);
        if prefix > 0 && segment.context.table_cell.is_none() {
            let end = segment_text.len() - segment_text[prefix..].trim_start().len();
            measurement
                .and_then(|measure| {
                    measure.line_width(
                        projection,
                        segment.projection_start()..segment.projection_start() + end,
                        font_for_range(&segment.projection_range()),
                    )
                })
                .unwrap_or(end as f32 * font_for_range(&segment.projection_range()) * 0.55)
                .min(layout_width * 0.25)
        } else {
            0.
        }
    } else {
        0.
    };
    let code_width = code_gutter::width(block, segment_text, measurement);
    let command_leading = code_panel::strip_leading(projection, segment, layout_width, measurement);
    let mut code_number = 0;
    for_each_display_line_range(segment_text, |local_line| {
        if local_line.start == segment_text.len()
            && code_panel::hide_terminal_row(projection, segment, block)
        {
            return;
        }
        code_number += 1;
        let code_line = code_width.map(|width| code_gutter::CodeLine {
            number: if local_line.start < segment_text.len() {
                code_number
            } else {
                0
            },
            width: command_leading.unwrap_or(width),
            strip: command_leading.is_some(),
        });
        let logical_line = segment.projection_start() + local_line.start
            ..segment.projection_start() + local_line.end;
        let mut push_line = |range, inline: Option<inline_math::InlineLine>| {
            let mut style = visual_line_style_for(projection, block, segment, &range, image_height);
            if command_leading.is_some() && range.start == segment.projection_start() {
                style.space_above -= CODE_HEADER_HEIGHT;
            }
            style.font_size = font_for_range(&range);
            if entity_title {
                style.line_height = table_records::TITLE_LEADING;
            }
            if compact_title {
                style.line_height = DocumentStyle::COMPACT_TITLE_LEADING;
            }
            if segment.context.alert_first && range.start == segment.projection_start() {
                style.space_above -= ALERT_HEADER_HEIGHT - alert_header;
            }
            if let Some(inline) = &inline {
                style.line_height = style.line_height.max(inline.ascent + inline.descent + 4.);
            }
            let inset = if table_cell.is_some() {
                12. + table_insets(segment, projection).1
            } else {
                container_inset(segment)
            } + f32::from(matches!(block, BlockNode::CodeBlock(_)))
                * CODE_BLOCK_PADDING
                + if range.start > segment.projection_start() {
                    chapter_inset
                } else {
                    0.
                }
                + alert_leading
                - ALERT_CONTENT_INSET;
            let record_label = if range.start == segment.projection_start() {
                measurement.and_then(|fonts| {
                    table_records::label(projection, segment, layout_width, fonts)
                })
            } else {
                None
            };
            // Inline attachments are already line-local. Keep the retained
            // wrapper range node-local as well so unrelated edits never have
            // to copy its shared raster payload merely to rebase coordinates.
            let inline_math = inline.map(|mut inline| {
                inline.range = inline.range.start - segment.projection_start()
                    ..inline.range.end - segment.projection_start();
                Arc::new(inline)
            });
            let source = LineSourceRange::new(projection, segment.node_id, range.clone())
                .expect("visual line source belongs to its projection segment");
            lines.push(VisualLineSpec {
                compact_tree: segment.context.compact_outline,
                source,
                payload: visual_line_payload(VisualLinePayload {
                    inline_math,
                    table_cell,
                    table_record: record.then_some(table_records::Geometry {
                        top: 0.,
                        height: 0.,
                    }),
                    record_label,
                    code_line,
                    ..VisualLinePayload::default()
                }),
                style,
                inset,
                gap_before: 0.,
                y: 0.,
                x_fraction: 0.,
                width_fraction: 1.,
                table_cell_first: false,
                flow_geometry: table_cell.is_some() || record,
            });
        };
        let mut wrap = |range: Range<usize>| {
            if let Some(measurement) = measurement
                && inline_math::has_attachments(projection, segment.node_id)
                && let Some(lines) = inline_math::layout(
                    projection,
                    segment,
                    range.clone(),
                    text_width_for_range(&range),
                    font_for_range(&range),
                    measurement,
                )
            {
                for line in lines {
                    push_line(line.range.clone(), Some(line));
                }
                return;
            }
            if columns != usize::MAX
                && let Some(measurement) = measurement
                && let Some(ranges) = measurement.wrap(
                    projection,
                    segment,
                    range.clone(),
                    text_width_for_range(&range)
                        - if range.start > segment.projection_start() {
                            chapter_inset
                        } else {
                            0.
                        },
                    font_for_range(&range),
                )
            {
                if chapter_inset > 0.
                    && range.start == segment.projection_start()
                    && ranges.len() > 1
                {
                    let first = ranges[0].clone();
                    if let Some(tail) = measurement.wrap(
                        projection,
                        segment,
                        first.end..range.end,
                        (text_width_for_range(&range) - chapter_inset).max(1.),
                        font_for_range(&range),
                    ) {
                        push_line(first, None);
                        for part in tail {
                            push_line(part, None);
                        }
                        return;
                    }
                }
                for range in ranges {
                    push_line(range, None);
                }
            } else {
                for_each_wrap_line_range(text, range, columns, |range| push_line(range, None));
            }
        };
        let mut start = logical_line.start;
        for &offset in presentation_breaks {
            if offset > start && offset < logical_line.end {
                wrap(start..offset);
                start = offset;
            }
        }
        wrap(start..logical_line.end);
    });
    if let Some(preview) =
        crate::math::prepare_block(block, projection.preview_edit_node == Some(segment.node_id))
        && let Some(first) = lines.first_mut()
    {
        if preview.source_visible {
            first.style.space_above += preview.extent();
            first.display_math = Some(preview);
        } else {
            first
                .set_projected_range(segment.projection_range())
                .expect("formula source belongs to its projection segment");
            first.inset -= CODE_BLOCK_PADDING;
            first.code_line = None;
            first.style.space_above = DocumentStyle::EQUATION_GAP;
            first.style.space_below = DocumentStyle::EQUATION_GAP;
            first.style.line_height = preview.light.height
                + if preview.light.width > (layout_width - first.inset - 8.).max(1.) {
                    DocumentStyle::EQUATION_SCROLL_RAIL
                } else {
                    0.
                };
            first.display_math = Some(preview);
            lines.truncate(1);
        }
    }
    if let Some(preview) =
        crate::diagram::prepare_block(block, projection.preview_edit_node == Some(segment.node_id))
        && let Some(first) = lines.first_mut()
    {
        if preview.source_visible {
            first.style.space_above += preview.extent();
        } else {
            first
                .set_projected_range(segment.projection_range())
                .expect("diagram source belongs to its projection segment");
            first.inset -= CODE_BLOCK_PADDING;
            first.code_line = None;
            first.style.space_above = crate::diagram::GAP;
            first.style.space_below = crate::diagram::GAP;
            first.style.line_height = preview.light.height
                + if preview.light.width > (layout_width - first.inset - 8.).max(1.) {
                    DocumentStyle::EQUATION_SCROLL_RAIL
                } else {
                    0.
                };
        }
        let rendered = !preview.source_visible;
        first.diagram = Some(preview);
        if rendered {
            lines.truncate(1);
        }
    }
    if compact_tree::has_header(segment)
        && let Some(first) = lines.first_mut()
        && first.projected_start() == segment.projection_start()
    {
        first.style.space_above += compact_tree::HEADER;
    }
    lines
}

fn estimated_wrap_columns(
    block: &BlockNode,
    segment: &crate::ProjectionSegment,
    projection: &TextProjection,
    layout_width: f32,
) -> usize {
    if segment.context.image_source.is_some() {
        return usize::MAX;
    }
    let reference_columns = match block {
        BlockNode::Heading(heading) => match heading.level {
            1 => 28,
            2 => 38,
            _ => 48,
        },
        BlockNode::CodeBlock(_) => return usize::MAX,
        _ if segment.context.table_cell.is_some() => 92,
        _ => BODY_REFERENCE_COLUMNS,
    };
    ((reference_columns as f32 * segment_text_width(segment, projection, layout_width) / 760.)
        .floor()
        .max(1.)) as usize
}

fn segment_text_width(
    segment: &crate::ProjectionSegment,
    projection: &TextProjection,
    layout_width: f32,
) -> f32 {
    if table_records::is_cell(projection, segment, layout_width) {
        let record_width = segment
            .context
            .table_cell
            .and_then(|(id, _, _)| projection.record_layout(id, layout_width))
            .map_or(layout_width, |layout| layout.width);
        return (record_width
            - 2. * table_records::INSET
            - table_records::body_inset(projection, segment, layout_width))
        .max(1.);
    }
    let (outer, inner) = table_insets(segment, projection);
    let available_width = segment
        .context
        .table_cell
        .and_then(|(table_id, row, column)| {
            let widths = projection.fitted_table_row_widths(
                table_id,
                row,
                projection
                    .table_available_width(table_id, table_container_width(layout_width, outer)),
            )?;
            widths.get(column).copied()
        })
        .unwrap_or(layout_width);
    let inset = if segment.context.table_cell.is_some() {
        24. + inner
    } else {
        container_inset(segment) + 8. + alert_layout_insets(projection, segment, layout_width).0
            - ALERT_CONTENT_INSET
            + if segment.context.quote_depth > 0 {
                DocumentStyle::QUOTE_INSET * segment.context.quote_depth as f32 - 8.
            } else {
                0.
            }
    };
    (available_width
        - inset
        - if segment.context.alert.is_some() && segment.context.table_cell.is_none() {
            16.
        } else {
            0.
        })
    .max(1.)
}

/// Ancestor indentation belongs to the whole table, never to every column.
fn alert_layout_insets(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    width: f32,
) -> (f32, f32) {
    let compact = width >= 560.
        && segment.context.table_cell.is_none()
        && segment.context.alert.as_ref().is_some_and(|(id, _)| {
            *id == segment.top_level_node_id
                && matches!(projection.block(*id),
                Some(BlockNode::Alert { title, blocks, .. })
                    if blocks.len() == 1 && title.as_ref().is_none_or(|text| text.len() <= 12)
                    && matches!(blocks.get(0).map(AsRef::as_ref), Some(BlockNode::Paragraph(_))))
        });
    if compact {
        (160., 16.)
    } else {
        (ALERT_CONTENT_INSET, ALERT_HEADER_HEIGHT)
    }
}

fn container_inset(segment: &crate::ProjectionSegment) -> f32 {
    let list = if segment.context.compact_outline && segment.context.list_depth > 2 {
        compact_tree::INSET
    } else {
        segment.context.list_depth as f32 * 24.
            + segment.context.ordered_list_depth as f32 * NUMBERED_LIST_EXTRA_GAP
    };
    list + segment.context.task_list_depth as f32 * DocumentStyle::TASK_INDENT_EXTRA
        + segment.context.quote_depth as f32 * DocumentStyle::QUOTE_INSET
        + f32::from(segment.context.alert.is_some()) * ALERT_CONTENT_INSET
        + f32::from(segment.context.footnote.is_some()) * footnotes::INSET
}

fn table_insets(segment: &crate::ProjectionSegment, projection: &TextProjection) -> (f32, f32) {
    table_insets_for(segment, projection, segment.context.compact_outline)
}

fn table_insets_for(
    segment: &crate::ProjectionSegment,
    projection: &TextProjection,
    compact: bool,
) -> (f32, f32) {
    let Some(outer) = segment
        .context
        .table_cell
        .and_then(|(id, _, _)| projection.table_context(id))
        .map(|context| &context.outer)
    else {
        return (0., container_inset(segment));
    };
    let list = if compact && outer.list_depth > 2 {
        compact_tree::INSET
    } else {
        outer.list_depth as f32 * 24. + outer.ordered_list_depth as f32 * NUMBERED_LIST_EXTRA_GAP
    };
    let inherited = list
        + outer.task_list_depth as f32 * DocumentStyle::TASK_INDENT_EXTRA
        + outer.quote_depth as f32 * 24.
        + f32::from(outer.alert.is_some()) * ALERT_CONTENT_INSET;
    let local =
        segment.context.list_depth.saturating_sub(outer.list_depth) as f32 * 24.
            + segment
                .context
                .ordered_list_depth
                .saturating_sub(outer.ordered_list_depth) as f32
                * NUMBERED_LIST_EXTRA_GAP
            + segment
                .context
                .task_list_depth
                .saturating_sub(outer.task_list_depth) as f32
                * DocumentStyle::TASK_INDENT_EXTRA
            + segment
                .context
                .quote_depth
                .saturating_sub(outer.quote_depth) as f32
                * 24.
            + f32::from(
                segment.context.alert.as_ref().is_some_and(|(id, _)| {
                    outer.alert.as_ref().is_none_or(|(outer, _)| id != outer)
                }),
            ) * ALERT_CONTENT_INSET;
    (inherited, local)
}

/// A nested list's shared rail follows its own text edge, one hanging-indent
/// step to the left. Geometry is published once; painting only clips this
/// rectangle to the visible viewport (and its owning table cell, if any).
fn outline_guide_bounds(
    component: &arrangement::ComponentGeometry,
    origin: Point<Pixels>,
    width: f32,
    zoom: f32,
    ordered: bool,
) -> Bounds<Pixels> {
    let indent = 24. + if ordered { NUMBERED_LIST_EXTRA_GAP } else { 0. };
    Bounds::new(
        point(
            origin.x + px(width * component.left_fraction - indent * zoom),
            origin.y + px(component.top),
        ),
        size(px(zoom), px((component.bottom - component.top).max(0.))),
    )
}

fn step_number_bounds(line: Bounds<Pixels>, zoom: f32) -> Bounds<Pixels> {
    Bounds::new(
        point(
            line.left() - px((28. + NUMBERED_LIST_EXTRA_GAP) * zoom),
            line.center().y - px(12.5 * zoom),
        ),
        size(px(25. * zoom), px(25. * zoom)),
    )
}

fn code_item_marker_line(
    line: &VisualLineSpec,
    container: Bounds<Pixels>,
    zoom: f32,
) -> Bounds<Pixels> {
    // A list marker belongs outside the code pane, not in its text gutter, and
    // must remain anchored when the source is horizontally scrolled.
    Bounds::new(
        point(
            container.left()
                + container.size.width * line.x_fraction
                + px(line.inset - CODE_BLOCK_PADDING * zoom),
            container.top() + px(line.y),
        ),
        size(
            container.size.width * line.width_fraction,
            px(line.style.line_height),
        ),
    )
}

fn list_marker_bounds(
    line: Bounds<Pixels>,
    steps: bool,
    numbered: bool,
    table_leading_edge: Option<Pixels>,
    zoom: f32,
) -> Bounds<Pixels> {
    let mut marker = if steps {
        step_number_bounds(line, zoom)
    } else {
        Bounds::new(
            point(
                line.left()
                    - px((22.
                        + if numbered {
                            NUMBERED_LIST_EXTRA_GAP
                        } else {
                            0.
                        })
                        * zoom),
                line.top(),
            ),
            size(px(20. * zoom), line.size.height),
        )
    };
    if let Some(left) = table_leading_edge {
        marker.origin.x = left
            - px((24.
                + if numbered {
                    NUMBERED_LIST_EXTRA_GAP
                } else {
                    0.
                })
                * zoom);
    }
    marker
}

fn table_container_width(width: f32, inset: f32) -> f32 {
    // Keep the trailing edge inset too, without changing top-level tables.
    (width - inset - if inset > 0. { inset.min(24.) } else { 0. }).max(1.)
}

fn table_viewport_geometry(
    line: &VisualLineSpec,
    projection: &TextProjection,
    width: f32,
    zoom: f32,
) -> (f32, f32) {
    let (left, available) = line.slot.map_or((0., width), |slot| {
        (
            slot.left(width / zoom) * zoom,
            slot.width(width / zoom) * zoom,
        )
    });
    let inset = segment_for_line(projection, &line.projected_range()).map_or(0., |segment| {
        table_insets_for(segment, projection, line.compact_tree).0
    });
    (
        left + inset * zoom,
        table_container_width(available / zoom, inset) * zoom,
    )
}

fn position_visual_lines(
    lines: &mut [VisualLineSpec],
    projection: &TextProjection,
    layout_width: f32,
    y: f32,
) -> f32 {
    position_flow(
        lines,
        projection,
        FlowRegion {
            document: layout_width,
            width: layout_width,
            left: 0.,
            slots: true,
        },
        y,
        false,
    )
}

#[derive(Clone, Copy)]
struct FlowRegion {
    document: f32,
    width: f32,
    left: f32,
    slots: bool,
}

fn position_flow(
    lines: &mut [VisualLineSpec],
    projection: &TextProjection,
    region: FlowRegion,
    mut y: f32,
    skip_first_gap: bool,
) -> f32 {
    let layout_width = region.document;
    let mut index = 0;
    while index < lines.len() {
        if index != 0 || !skip_first_gap {
            y += lines[index].gap_before;
        }
        if region.slots
            && let Some(slot) = lines[index].slot
        {
            let end = lines[index..]
                .iter()
                .position(|line| line.slot.is_none_or(|candidate| !candidate.same_row(slot)))
                .map_or(lines.len(), |offset| index + offset);
            let mut row_height = 0_f32;
            let mut component_columns = Vec::new();
            let mut first = index;
            while first < end {
                let placement = lines[first].slot.unwrap();
                let last = lines[first..end]
                    .iter()
                    .position(|line| line.slot.is_none_or(|slot| !slot.same_column(placement)))
                    .map_or(end, |offset| first + offset);
                let height = position_flow(
                    &mut lines[first..last],
                    projection,
                    FlowRegion {
                        document: layout_width,
                        width: placement.width(layout_width),
                        left: placement.left(layout_width),
                        slots: false,
                    },
                    y,
                    true,
                ) - y;
                if matches!(
                    placement.card_accent,
                    crate::adaptive::CardAccent::Resource(_)
                        | crate::adaptive::CardAccent::Editorial(_)
                ) {
                    for line in &mut lines[first..last] {
                        line.table_row_y = y;
                        line.table_row_height = height;
                    }
                }
                row_height = row_height.max(height);
                if placement.align_components {
                    let component = lines[first..last].iter().position(|line| {
                        projection
                            .segment_for_range(&line.projected_range())
                            .is_some_and(|segment| {
                                matches!(
                                    projection.block(segment.top_level_node_id),
                                    Some(BlockNode::Table(_) | BlockNode::CodeBlock(_))
                                )
                            })
                    });
                    if let Some(component) = component {
                        let component = first + component;
                        let line = &lines[component];
                        let top = if line.table_cell.is_some() {
                            line.table_row_y
                        } else {
                            line.y - line.style.space_above
                        };
                        component_columns.push((component..last, top, height));
                    }
                }
                first = last;
            }
            // Align component borders, not the text baselines inside different
            // kinds of chrome. Resolve from current shaped lines on every row
            // rebuild, so wrapped headings and edited introductions participate.
            // Padding stays outside the component; its internal geometry moves
            // as a unit and subsequent document content uses the new row extent.
            if component_columns.len() == slot.columns {
                let anchor = component_columns
                    .iter()
                    .map(|(_, top, _)| *top)
                    .fold(y, f32::max);
                for (range, top, height) in component_columns {
                    let delta = anchor - top;
                    for line in &mut lines[range] {
                        line.y += delta;
                        line.table_row_y += delta;
                        if let Some(record) = &mut line.table_record {
                            record.top += delta;
                        }
                    }
                    row_height = row_height.max(height + delta);
                }
            }
            for line in &mut lines[index..end] {
                let peer_box = line.slot.is_some_and(|slot| {
                    slot.columns > 1
                        && matches!(slot.card_accent, crate::adaptive::CardAccent::Editorial(_))
                });
                if peer_box
                    || (line.table_cell.is_none()
                        && !line.slot.is_some_and(|slot| {
                            matches!(
                                slot.card_accent,
                                crate::adaptive::CardAccent::Resource(_)
                                    | crate::adaptive::CardAccent::Editorial(_)
                            )
                        }))
                {
                    line.table_row_y = y;
                    line.table_row_height = row_height;
                }
            }
            y += row_height;
            if lines
                .get(end)
                .and_then(|line| line.slot)
                .is_some_and(|next| next.group == slot.group)
            {
                // A paragraph wrapping below a figure continues at its next
                // baseline. Only genuine block/reading-band boundaries get
                // the module gutter; never insert a paragraph gap mid-sentence.
                let continuous_wrap = lines.get(end).is_some_and(|next| {
                    next.slot.is_some_and(|s| s.span == 12)
                        && lines[end - 1].projected_end() == next.projected_start()
                        && projection
                            .segment_for_range(&lines[end - 1].projected_range())
                            .map(|s| s.node_id)
                            == projection
                                .segment_for_range(&next.projected_range())
                                .map(|s| s.node_id)
                });
                if !continuous_wrap {
                    y += LAYOUT_GAP;
                }
            }
            index = end;
            continue;
        }
        if lines[index].label_row.is_some() {
            let owner = projection
                .segment_for_range(&lines[index].projected_range())
                .map(|s| s.node_id);
            let end = lines[index..]
                .iter()
                .position(|line| {
                    line.label_row.is_none()
                        || projection
                            .segment_for_range(&line.projected_range())
                            .map(|s| s.node_id)
                            != owner
                })
                .map_or(lines.len(), |offset| index + offset);
            if lines[index].label_row.unwrap().0.stacked() {
                for line in &mut lines[index..end] {
                    y += line.style.space_above;
                    line.y = y;
                    y += line.style.line_height;
                }
                y += lines[end - 1].style.space_below;
                index = end;
                continue;
            }
            y += lines[index].style.space_above;
            let mut bottoms = [y, y];
            for line in &mut lines[index..end] {
                let part = usize::from(line.label_row.unwrap().0 == label_rows::Part::Body);
                line.y = bottoms[part];
                bottoms[part] += line.style.line_height;
            }
            y = bottoms[0].max(bottoms[1]) + lines[end - 1].style.space_below;
            index = end;
            continue;
        }
        let Some((table_id, row, _, _)) = lines[index].table_cell else {
            y += lines[index].style.space_above;
            lines[index].y = y;
            y += lines[index].style.line_height + lines[index].style.space_below;
            index += 1;
            continue;
        };
        let end = lines[index..]
            .iter()
            .position(|line| {
                !matches!(line.table_cell, Some((candidate, candidate_row, _, _)) if candidate == table_id && candidate_row == row)
            })
            .map_or(lines.len(), |offset| index + offset);
        let inset = segment_for_line(projection, &lines[index].projected_range())
            .map_or(0., |segment| {
                table_insets_for(segment, projection, lines[index].compact_tree).0
            });
        if lines[index].table_record.is_some() {
            // Store outside rhythm on the first cell so local row edits and
            // complete document rebuilds reserve exactly the same space.
            let layout = projection.record_layout(table_id, region.width).unwrap();
            let mut next = index;
            let mut bottom = y;
            for column in 0..layout.columns {
                if !lines.get(next).is_some_and(|line| {
                    line.table_record.is_some()
                        && line.table_cell.is_some_and(|(id, _, _, _)| id == table_id)
                }) {
                    break;
                }
                let row = lines[next].table_cell.unwrap().1;
                let record_end = lines[next..].iter().position(|line| {
                    !matches!(line.table_cell, Some((id, r, _, _)) if id == table_id && r == row)
                }).map_or(lines.len(), |offset| next + offset);
                let record_region = FlowRegion {
                    width: layout.width,
                    left: region.left + column as f32 * (layout.width + table_records::RECORD_GAP),
                    ..region
                };
                bottom = bottom.max(table_records::position(
                    &mut lines[next..record_end],
                    record_region,
                    y,
                ));
                next = record_end;
            }
            y = bottom;
            index = next;
            continue;
        }
        let columns = lines[index].table_cell.map_or(1, |cell| cell.3);
        let available =
            projection.table_available_width(table_id, table_container_width(region.width, inset));
        let fitted = projection.fitted_table_row_widths(table_id, row, available);
        let widths = fitted.as_deref();
        let (outer_above, outer_below) = lines[index..end]
            .iter()
            .map(|line| table_outer_spacing(line, projection))
            .fold((0_f32, 0_f32), |(above, below), spacing| {
                (above.max(spacing.0), below.max(spacing.1))
            });
        // A container header/padding surrounds the entire row. Leaving it
        // inside the first/last cell misaligns columns and inflates row borders.
        y += outer_above;
        let mut column_heights = vec![0.; columns];
        for line in &mut lines[index..end] {
            let Some((_, _, column, columns)) = line.table_cell else {
                continue;
            };
            let column = column.min(columns.saturating_sub(1));
            let (above, below) = table_outer_spacing(line, projection);
            column_heights[column] += line.style.space_above - above;
            line.y = y + column_heights[column];
            let column_width = widths
                .and_then(|widths| widths.get(column))
                .copied()
                .unwrap_or(160.);
            let preceding_width = widths.map_or(160. * column as f32, |widths| {
                widths.iter().take(column).sum()
            });
            // These are resolved logical widths, not weights. Text wrapping
            // used the same values; stretching only the painted columns would
            // violate explicit sizing and leave premature line breaks behind.
            line.x_fraction = (region.left + inset + preceding_width) / layout_width.max(1.);
            line.width_fraction = column_width / layout_width.max(1.);
            column_heights[column] += line.style.line_height + line.style.space_below - below;
        }
        let row_height = column_heights.into_iter().fold(32., f32::max);
        let mut seen_columns = vec![false; columns];
        for line in &mut lines[index..end] {
            let Some((_, _, column, columns)) = line.table_cell else {
                continue;
            };
            let column = column.min(columns.saturating_sub(1));
            line.table_cell_first = !seen_columns[column];
            seen_columns[column] = true;
            line.table_row_y = y;
            line.table_row_height = row_height;
        }
        y += row_height + outer_below;
        index = end;
    }
    y
}

#[cfg(test)]
fn wrap_line_range(text: &str, line: Range<usize>, columns: usize) -> Vec<Range<usize>> {
    let mut ranges = Vec::new();
    for_each_wrap_line_range(text, line, columns, |range| ranges.push(range));
    ranges
}

fn for_each_wrap_line_range(
    text: &str,
    line: Range<usize>,
    columns: usize,
    mut visit: impl FnMut(Range<usize>),
) {
    if line.is_empty() || columns == usize::MAX {
        visit(line);
        return;
    }
    let mut start = line.start;
    let content_end = line.start + text[line.clone()].trim_end().len();
    while start < line.end {
        let mut count = 0;
        let mut candidate = None;
        let mut preferred = None;
        for (offset, character) in text[start..line.end].char_indices() {
            count += 1;
            let boundary = start + offset + character.len_utf8();
            if character.is_whitespace() {
                preferred = Some(boundary);
            }
            if count == columns {
                candidate = Some(boundary);
                break;
            }
        }
        let Some(candidate) = candidate.filter(|candidate| *candidate < line.end) else {
            visit(start..line.end);
            break;
        };
        if candidate >= content_end {
            visit(start..line.end);
            break;
        }
        let end = preferred
            .filter(|boundary| *boundary > start)
            .unwrap_or(candidate);
        visit(start..end);
        start = end;
    }
}

fn word_range_at(text: &str, offset: usize) -> Range<usize> {
    if text.is_empty() {
        return 0..0;
    }
    let mut offset = offset.min(text.len());
    while offset > 0 && !text.is_char_boundary(offset) {
        offset -= 1;
    }
    let is_word = |character: char| character.is_alphanumeric() || character == '_';
    let current = text[offset..]
        .chars()
        .next()
        .filter(|character| is_word(*character));
    let previous = text[..offset]
        .char_indices()
        .next_back()
        .filter(|(_, character)| is_word(*character));
    let probe = current
        .map(|_| offset)
        .or_else(|| previous.map(|(index, _)| index));
    let Some(probe) = probe else {
        return offset..offset;
    };

    let mut start = probe;
    for (index, character) in text[..probe].char_indices().rev() {
        if !is_word(character) {
            break;
        }
        start = index;
    }
    let mut end = probe;
    for (relative, character) in text[probe..].char_indices() {
        if !is_word(character) {
            break;
        }
        end = probe + relative + character.len_utf8();
    }
    start..end
}

fn visual_line_range_at(editor: &RichDocumentEditor, offset: usize) -> Range<usize> {
    let split = editor
        .visual_lines
        .partition_point(|line| line.projected_start() <= offset);
    split
        .checked_sub(1)
        .and_then(|index| editor.visual_lines.get(index))
        .filter(|line| line.projected_range().contains(&offset) || line.projected_end() == offset)
        .map(|line| line.projected_range())
        .unwrap_or_else(|| editor.projection.text().len()..editor.projection.text().len())
}

fn painted_line_for_offset(lines: &[PaintedLine], offset: usize) -> Option<&PaintedLine> {
    lines
        .iter()
        .find(|line| line.range.contains(&offset))
        .or_else(|| lines.iter().find(|line| line.range.end == offset))
        .or_else(|| lines.last())
}

fn distance_to_vertical_bounds(y: Pixels, bounds: &Bounds<Pixels>) -> f32 {
    if y < bounds.top() {
        (bounds.top() - y).into()
    } else if y > bounds.bottom() {
        (y - bounds.bottom()).into()
    } else {
        0.0
    }
}

fn selection_is_valid(snapshot: &document_core::DocumentSnapshot, selection: &Selection) -> bool {
    match selection {
        Selection::Text(selection) => {
            snapshot.validates_position(selection.anchor)
                && snapshot.validates_position(selection.head)
        }
        Selection::Table(selection) => {
            let Some(BlockNode::Table(table)) = snapshot.node(selection.table_id) else {
                return false;
            };
            selection.anchor_row < table.row_count()
                && selection.head_row < table.row_count()
                && selection.anchor_column < table.column_count()
                && selection.head_column < table.column_count()
        }
    }
}

fn distance_to_bounds(position: Point<Pixels>, bounds: &Bounds<Pixels>) -> f32 {
    let horizontal = if position.x < bounds.left() {
        f32::from(bounds.left() - position.x)
    } else if position.x > bounds.right() {
        f32::from(position.x - bounds.right())
    } else {
        0.
    };
    let vertical = distance_to_vertical_bounds(position.y, bounds);
    horizontal.mul_add(horizontal, vertical * vertical)
}

fn intersect_bounds(left: Bounds<Pixels>, right: Bounds<Pixels>) -> Option<Bounds<Pixels>> {
    let x1 = left.left().max(right.left());
    let y1 = left.top().max(right.top());
    let x2 = left.right().min(right.right());
    let y2 = left.bottom().min(right.bottom());
    (x2 > x1 && y2 > y1).then(|| Bounds::from_corners(point(x1, y1), point(x2, y2)))
}

fn visual_line_bounds(
    editor: &RichDocumentEditor,
    spec: &VisualLineSpec,
    container: Bounds<Pixels>,
    is_code: bool,
    horizontal_offset: f32,
) -> Bounds<Pixels> {
    let width: f32 = container.size.width.into();
    Bounds::new(
        point(
            container.left()
                + px(width * spec.x_fraction + spec.inset + spec.code_gutter() - horizontal_offset),
            container.top() + px(spec.y),
        ),
        size(
            px(spec.label_row.map_or_else(
                || {
                    (width * spec.width_fraction
                        - spec.inset
                        - spec.code_gutter()
                        - spec.command_trailing(editor.zoom_factor)
                        - if is_code {
                            (CODE_BLOCK_PADDING + spec.slot.map_or(0., |s| s.inset()))
                                * editor.zoom_factor
                        } else if spec.table_record.is_some() {
                            table_records::INSET * editor.zoom_factor
                        } else if spec.table_cell.is_some() {
                            12. * editor.zoom_factor
                        } else if let Some(slot) = spec.slot {
                            slot.inset() * editor.zoom_factor
                        } else {
                            8. * editor.zoom_factor
                        })
                    .max(1.)
                },
                |(_, width)| width,
            )),
            px(spec.style.line_height),
        ),
    )
}

/// Loaded images and their placeholders share one rectangle. A rich-cell
/// image uses its cell's text insets and never grows beyond its intrinsic size.
fn image_element_bounds(
    editor: &RichDocumentEditor,
    spec: &VisualLineSpec,
    container: Bounds<Pixels>,
    horizontal_offset: f32,
) -> Bounds<Pixels> {
    if spec.table_cell.is_some() {
        let mut bounds = visual_line_bounds(editor, spec, container, false, horizontal_offset);
        if let Some(segment) = segment_for_line(&editor.projection, &spec.projected_range())
            && let Some(BlockNode::Image(image)) = editor.projection.block(segment.node_id)
            && let Some((width, _)) =
                resolved_image_dimensions(image, &editor.image_layout_dimensions)
        {
            bounds.size.width = bounds.size.width.min(px(width as f32 * editor.zoom_factor));
        }
        bounds
    } else {
        let nested = segment_for_line(&editor.projection, &spec.projected_range())
            .is_some_and(|segment| segment.context.list_depth > 0);
        let inset = if nested { spec.inset } else { 0. };
        let mut bounds = Bounds::new(
            point(
                container.left() + container.size.width * spec.x_fraction + px(inset)
                    - px(horizontal_offset),
                container.top() + px(spec.y),
            ),
            size(
                (container.size.width * spec.width_fraction - px(inset)).max(px(1.)),
                px(spec.style.line_height),
            ),
        );
        if let Some(segment) = segment_for_line(&editor.projection, &spec.projected_range())
            && let Some(BlockNode::Image(image)) = editor.projection.block(segment.node_id)
            && map_preview::label(image).is_some()
            && let Some((width, _)) =
                resolved_image_dimensions(image, &editor.image_layout_dimensions)
        {
            bounds.size.width = bounds.size.width.min(px(width as f32 * editor.zoom_factor));
        }
        bounds
    }
}

fn visual_component_bounds(
    editor: &RichDocumentEditor,
    container: Bounds<Pixels>,
    owner: NodeId,
    leading_inset: f32,
    trailing_inset: f32,
    top_extra: f32,
    bottom_extra: f32,
) -> Option<Bounds<Pixels>> {
    let width: f32 = container.size.width.into();
    let component = editor.components.get(&owner)?;
    let left = width * component.left_fraction - leading_inset;
    let right = width * component.right_fraction - trailing_inset;
    let top = component.top;
    let bottom = component.bottom;
    (left.is_finite() && right > left && top.is_finite() && bottom > top).then(|| {
        Bounds::from_corners(
            point(
                container.left() + px(left),
                container.top() + px(top - top_extra),
            ),
            point(
                container.left() + px(right),
                container.top() + px(bottom + bottom_extra),
            ),
        )
    })
}

fn alert_color(palette: TachyonPalette, kind: &AlertKind) -> u32 {
    palette.signal(kind).ink
}

fn alert_icon(kind: &AlertKind) -> Icon {
    match kind {
        AlertKind::Note => Icon::new(IconName::Info),
        AlertKind::Tip => Icon::default().path("tachyon/lightbulb.svg"),
        AlertKind::Important => Icon::default().path("tachyon/flag.svg"),
        AlertKind::Warning => Icon::new(IconName::TriangleAlert),
        AlertKind::Caution => Icon::new(IconName::CircleX),
        AlertKind::Other(_) => Icon::new(IconName::Info),
    }
}

fn code_palette(mut palette: TachyonPalette, block: &BlockNode) -> TachyonPalette {
    let BlockNode::CodeBlock(code) = block else {
        return palette;
    };
    // One predictable technical treatment: structured exchange/configuration
    // examples stay light; executable languages and command strips are dark.
    if code.language.as_deref().is_some_and(|language| {
        matches!(
            language,
            "sh" | "bash"
                | "shell"
                | "zsh"
                | "console"
                | "rust"
                | "rs"
                | "typescript"
                | "ts"
                | "javascript"
                | "js"
                | "python"
                | "py"
        )
    }) {
        palette.surface_quiet = DocumentStyle::CODE_DARK;
        palette.text = DocumentStyle::CODE_TEXT;
        palette.selection = DocumentStyle::CODE_SELECTION;
        palette.secondary = 0xb2bdb7;
        palette.border = 0x35413f;
        palette.syntax_keyword = 0xc9b5e8;
        palette.syntax_string = 0xb9d993;
        palette.syntax_comment = 0xabb5ae;
        palette.syntax_number = 0xf0bc82;
    }
    palette
}

fn alert_default_title(kind: &AlertKind) -> String {
    match kind {
        AlertKind::Note => "Note".into(),
        AlertKind::Tip => "Tip".into(),
        AlertKind::Important => "Important".into(),
        AlertKind::Warning => "Warning".into(),
        AlertKind::Caution => "Caution".into(),
        AlertKind::Other(title) => title.clone(),
    }
}

fn table_column_alignment(
    projection: &TextProjection,
    segment: Option<&crate::ProjectionSegment>,
) -> ColumnAlignment {
    segment
        .and_then(|segment| segment.context.table_cell)
        .and_then(|(table_id, _, column)| {
            let BlockNode::Table(table) = projection.block(table_id)? else {
                return None;
            };
            table.columns.get(column).map(|column| column.alignment)
        })
        .unwrap_or(ColumnAlignment::None)
}

/// Use visual glyph cells rather than caret snapping so whitespace beside a
/// link stays editable text. Visual order also handles mixed-direction links.
fn inline_link_bounds(
    projection: &TextProjection,
    range: &Range<usize>,
    layout: &ShapedLine,
    bounds: Bounds<Pixels>,
    alignment: ColumnAlignment,
) -> Vec<Bounds<Pixels>> {
    let Some(segment) = projection.segment_for_range(range) else {
        return Vec::new();
    };
    let Some(text) = projection.block(segment.node_id).and_then(BlockNode::text) else {
        return Vec::new();
    };
    let node_start = segment.node_range.start + range.start - segment.projection_start();
    let links = text
        .runs()
        .iter()
        .filter(|run| {
            run.range.start < node_start + layout.text.len()
                && run.range.end > node_start
                && run
                    .styles
                    .iter()
                    .any(|style| matches!(style, InlineStyle::Link(_)))
        })
        .map(|run| &run.range)
        .collect::<Vec<_>>();
    if links.is_empty() {
        return Vec::new();
    }

    let left = aligned_text_left(bounds, layout, alignment);
    let mut glyphs = layout
        .runs
        .iter()
        .flat_map(|run| &run.glyphs)
        .collect::<Vec<_>>();
    glyphs.sort_by(|a, b| f32::from(a.position.x).total_cmp(&f32::from(b.position.x)));
    let mut result: Vec<Bounds<Pixels>> = Vec::new();
    for (index, glyph) in glyphs.iter().enumerate() {
        if !links
            .iter()
            .any(|range| range.contains(&(node_start + glyph.index)))
        {
            continue;
        }
        let start = left + glyph.position.x;
        let end = left
            + glyphs
                .get(index + 1)
                .map_or(layout.width(), |next| next.position.x);
        if end <= start {
            continue;
        }
        if let Some(previous) = result.last_mut()
            && previous.right() == start
        {
            previous.size.width = end - previous.left();
        } else {
            result.push(Bounds::from_corners(
                point(start, bounds.top()),
                point(end, bounds.bottom()),
            ));
        }
    }
    result
}

fn aligned_text_left(
    bounds: Bounds<Pixels>,
    layout: &ShapedLine,
    alignment: ColumnAlignment,
) -> Pixels {
    let slack = f32::from((bounds.size.width - layout.width()).max(px(0.)));
    bounds.left()
        + px(match alignment {
            ColumnAlignment::Center => slack * 0.5,
            ColumnAlignment::Right => slack,
            ColumnAlignment::None | ColumnAlignment::Left => 0.,
        })
}

#[derive(Clone, Copy, Debug, PartialEq)]
struct VisualCaretStop {
    offset: usize,
    x: f32,
}

/// Build visual caret stops from shaped cluster origins. Cosmic Text emits
/// glyphs in visual order while retaining logical UTF-8 cluster indices, so an
/// RTL run has decreasing indices as X increases. GPUI's stock helpers assume
/// increasing indices; keeping this adapter here makes hit testing and caret
/// movement follow the shaped line without changing canonical source order.
fn shaped_caret_stops(layout: &ShapedLine) -> Vec<VisualCaretStop> {
    let mut cluster_x = HashMap::<usize, f32>::new();
    for glyph in layout.runs.iter().flat_map(|run| &run.glyphs) {
        if glyph.index <= layout.text.len() {
            let x = f32::from(glyph.position.x);
            cluster_x
                .entry(glyph.index)
                .and_modify(|current| *current = current.min(x))
                .or_insert(x);
        }
    }
    let mut clusters = cluster_x.into_iter().collect::<Vec<_>>();
    clusters.sort_by(|left, right| left.1.total_cmp(&right.1));
    caret_stops_from_clusters(layout.text.as_ref(), f32::from(layout.width()), &clusters)
}

fn caret_stops_from_clusters(
    text: &str,
    width: f32,
    visual_clusters: &[(usize, f32)],
) -> Vec<VisualCaretStop> {
    if visual_clusters.is_empty() {
        return vec![VisualCaretStop { offset: 0, x: 0. }];
    }
    let mut logical_starts = visual_clusters
        .iter()
        .map(|(offset, _)| *offset)
        .filter(|offset| *offset <= text.len() && text.is_char_boundary(*offset))
        .collect::<Vec<_>>();
    logical_starts.sort_unstable();
    logical_starts.dedup();

    let mut stops = Vec::new();
    for (index, &(start, left)) in visual_clusters.iter().enumerate() {
        if start > text.len() || !text.is_char_boundary(start) {
            continue;
        }
        let end = logical_starts
            .iter()
            .copied()
            .find(|candidate| *candidate > start)
            .unwrap_or(text.len());
        let right = visual_clusters
            .get(index + 1)
            .map_or(width, |(_, x)| *x)
            .max(left);
        let rtl = visual_clusters
            .get(index + 1)
            .map(|(neighbor, _)| *neighbor < start)
            .or_else(|| {
                index
                    .checked_sub(1)
                    .and_then(|previous| visual_clusters.get(previous))
                    .map(|(neighbor, _)| start < *neighbor)
            })
            .unwrap_or(false);
        let graphemes = text[start..end]
            .grapheme_indices(true)
            .map(|(relative, _)| start + relative)
            .chain(std::iter::once(end))
            .collect::<Vec<_>>();
        let denominator = graphemes.len().saturating_sub(1).max(1) as f32;
        for (position, offset) in graphemes.into_iter().enumerate() {
            let fraction = position as f32 / denominator;
            let x = if rtl {
                right - (right - left) * fraction
            } else {
                left + (right - left) * fraction
            };
            stops.push(VisualCaretStop { offset, x });
        }
    }
    stops.sort_by(|left, right| {
        left.x
            .total_cmp(&right.x)
            .then_with(|| left.offset.cmp(&right.offset))
    });
    stops.dedup_by(|left, right| left.offset == right.offset && (left.x - right.x).abs() < 0.01);
    stops
}

fn shaped_x_for_index(layout: &ShapedLine, index: usize) -> Pixels {
    let target = index.min(layout.text.len());
    shaped_caret_stops(layout)
        .into_iter()
        .filter(|stop| stop.offset == target)
        .min_by(|left, right| left.x.total_cmp(&right.x))
        .map_or_else(|| layout.x_for_index(target), |stop| px(stop.x))
}

fn shaped_index_for_x(layout: &ShapedLine, x: Pixels) -> usize {
    let target = f32::from(x);
    shaped_caret_stops(layout)
        .into_iter()
        .min_by(|left, right| (left.x - target).abs().total_cmp(&(right.x - target).abs()))
        .map_or_else(|| layout.closest_index_for_x(x), |stop| stop.offset)
}

fn shaped_visual_neighbor(layout: &ShapedLine, index: usize, direction: isize) -> Option<usize> {
    let stops = shaped_caret_stops(layout);
    let current = stops
        .iter()
        .enumerate()
        .filter(|(_, stop)| stop.offset == index)
        .min_by(|(_, left), (_, right)| left.x.total_cmp(&right.x))?
        .0;
    if direction < 0 {
        stops[..current]
            .iter()
            .rev()
            .find(|stop| stop.x < stops[current].x - 0.01)
            .map(|stop| stop.offset)
    } else {
        stops[current + 1..]
            .iter()
            .find(|stop| stop.x > stops[current].x + 0.01)
            .map(|stop| stop.offset)
    }
}

fn gpui_text_alignment(alignment: ColumnAlignment) -> gpui::TextAlign {
    match alignment {
        ColumnAlignment::Center => gpui::TextAlign::Center,
        ColumnAlignment::Right => gpui::TextAlign::Right,
        ColumnAlignment::None | ColumnAlignment::Left => gpui::TextAlign::Left,
    }
}

fn horizontal_scroll_owner(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
) -> Option<NodeId> {
    if let Some((table_id, _, _)) = segment.context.table_cell {
        Some(table_id)
    } else if matches!(
        projection.block(segment.node_id),
        Some(BlockNode::CodeBlock(_) | BlockNode::PreservedSource { .. })
    ) {
        Some(segment.node_id)
    } else {
        None
    }
}

fn record_horizontal_metrics(
    metrics: &mut HashMap<NodeId, (f32, f32)>,
    owner: NodeId,
    viewport: f32,
    content: f32,
) {
    let entry = metrics.entry(owner).or_insert((viewport, viewport));
    entry.0 = viewport;
    entry.1 = entry.1.max(content);
}

fn clamped_horizontal_scroll(current: f32, delta: f32, viewport: f32, content: f32) -> f32 {
    (current + delta).clamp(0., (content - viewport).max(0.))
}

fn outline_jump_position(start: f32, target: f32, step: u32) -> f32 {
    let progress = (step.min(OUTLINE_JUMP_STEPS) as f32 / OUTLINE_JUMP_STEPS as f32).clamp(0., 1.);
    let eased = progress * progress * (3. - 2. * progress);
    start + (target - start) * eased
}

fn visual_line_style_for(
    projection: &TextProjection,
    block: &BlockNode,
    segment: &crate::ProjectionSegment,
    line: &Range<usize>,
    image_height: Option<f32>,
) -> VisualLineStyle {
    let first_line = line.start == segment.projection_start();
    let last_line = line.end == segment.projection_end()
        || (code_panel::hide_terminal_row(projection, segment, block)
            && line.end + 1 == segment.projection_end());
    if segment.context.image_source.is_some() {
        return with_component_spacing(
            projection,
            VisualLineStyle {
                font_size: 15.,
                line_height: image_height.unwrap_or(180.)
                    + match block {
                        BlockNode::Image(image) => map_preview::footer_height(image),
                        _ => 0.,
                    },
                space_above: if first_line { 8. } else { 0. },
                space_below: if last_line { 16. } else { 0. },
            },
            segment,
            first_line,
            last_line,
        );
    }
    if segment.context.table_cell.is_some() {
        let (font_size, line_height) = match block {
            BlockNode::Heading(heading) => DocumentStyle::heading(heading.level),
            BlockNode::CodeBlock(_) => (DocumentStyle::CODE_SIZE, DocumentStyle::CODE_LEADING),
            _ => (DocumentStyle::TABLE_SIZE, DocumentStyle::TABLE_LEADING),
        };
        return with_component_spacing(
            projection,
            VisualLineStyle {
                font_size,
                line_height,
                space_above: if first_line {
                    10. + if matches!(block, BlockNode::CodeBlock(_)) {
                        CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING
                    } else {
                        0.
                    }
                } else {
                    0.
                },
                space_below: if last_line {
                    10. + if matches!(block, BlockNode::CodeBlock(_)) {
                        CODE_BLOCK_PADDING
                    } else {
                        0.
                    }
                } else {
                    0.
                },
            },
            segment,
            first_line,
            last_line,
        );
    }
    if let Some(role) = segment.context.color_role {
        let (font_size, line_height) = if role == crate::signals::ColorRole::Label {
            (18., 24.)
        } else {
            (
                DocumentStyle::METADATA_SIZE,
                DocumentStyle::METADATA_LEADING,
            )
        };
        return VisualLineStyle {
            font_size,
            line_height,
            space_above: 0.,
            space_below: 0.,
        };
    }
    if let Some(role) = segment.context.metric {
        let (font_size, line_height) = match role {
            crate::metrics::TextRole::Label => (18., 24.),
            crate::metrics::TextRole::Value => (
                DocumentStyle::METRIC_VALUE_SIZE,
                DocumentStyle::METRIC_VALUE_LEADING,
            ),
            crate::metrics::TextRole::Context => (
                DocumentStyle::METADATA_SIZE,
                DocumentStyle::METADATA_LEADING,
            ),
        };
        return VisualLineStyle {
            font_size,
            line_height,
            space_above: 0.,
            space_below: 0.,
        };
    }
    let style = match block {
        BlockNode::Paragraph(_) if segment.context.bibliography.is_some() => VisualLineStyle {
            font_size: DocumentStyle::READING_SIZE,
            line_height: DocumentStyle::READING_LEADING,
            space_above: 0.,
            space_below: if last_line {
                DocumentStyle::BIBLIOGRAPHY_GAP
            } else {
                0.
            },
        },
        BlockNode::Paragraph(_) if segment.context.margin_note_anchor.is_some() => {
            VisualLineStyle {
                font_size: DocumentStyle::METADATA_SIZE,
                line_height: DocumentStyle::METADATA_LEADING,
                space_above: 0.,
                space_below: if last_line && !segment.context.quote_last {
                    8.
                } else {
                    0.
                },
            }
        }
        BlockNode::Paragraph(_) if segment.context.quote_attribution => VisualLineStyle {
            font_size: DocumentStyle::METADATA_SIZE,
            line_height: DocumentStyle::METADATA_LEADING,
            space_above: 0.,
            space_below: 0.,
        },
        BlockNode::Paragraph(_)
            if segment.context.quote.is_some()
                && segment.context.list_depth == 0
                && segment.context.table_cell.is_none() =>
        {
            VisualLineStyle {
                font_size: if segment.context.quote_pull {
                    DocumentStyle::PULL_QUOTE_SIZE
                } else {
                    DocumentStyle::READING_SIZE
                },
                line_height: if segment.context.quote_pull {
                    DocumentStyle::PULL_QUOTE_LEADING
                } else {
                    DocumentStyle::READING_LEADING
                },
                space_above: 0.,
                space_below: if !last_line || segment.context.quote_last {
                    0.
                } else if segment.context.quote_before_attribution {
                    8.
                } else {
                    16.
                },
            }
        }
        BlockNode::Paragraph(_) if segment.context.figure_text.is_some() => VisualLineStyle {
            font_size: DocumentStyle::CAPTION_SIZE,
            line_height: DocumentStyle::CAPTION_LEADING,
            space_above: 0.,
            space_below: 0.,
        },
        BlockNode::Paragraph(_) if segment.context.footnote.is_some() => VisualLineStyle {
            font_size: 14.,
            line_height: 20.,
            space_above: 0.,
            space_below: if last_line { 12. } else { 0. },
        },
        BlockNode::ThematicBreak { .. } => VisualLineStyle {
            font_size: VisualLineStyle::BODY.font_size,
            line_height: 1.,
            space_above: 0.,
            space_below: 0.,
        },
        BlockNode::Heading(heading) => {
            let (font_size, line_height) = DocumentStyle::heading(heading.level);
            VisualLineStyle {
                font_size,
                line_height,
                space_above: if first_line {
                    if segment.projection_start() == 0 {
                        8.
                    } else {
                        20.
                    }
                } else {
                    0.
                },
                space_below: if last_line { 7. } else { 0. },
            }
        }
        BlockNode::CodeBlock(_) => VisualLineStyle {
            font_size: DocumentStyle::CODE_SIZE,
            line_height: DocumentStyle::CODE_LEADING,
            space_above: if first_line {
                8. + CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING
            } else {
                0.
            },
            space_below: if last_line {
                16. + CODE_BLOCK_PADDING
            } else {
                0.
            },
        },
        _ => VisualLineStyle {
            font_size: if segment.context.narrative {
                DocumentStyle::READING_SIZE
            } else {
                DocumentStyle::REFERENCE_SIZE
            },
            line_height: if segment.context.narrative {
                DocumentStyle::READING_LEADING
            } else {
                DocumentStyle::REFERENCE_LEADING
            },
            space_below: if last_line {
                if segment.context.list_depth > 0
                    || segment
                        .context
                        .definition
                        .is_some_and(|(_, kind)| kind == document_core::DefinitionKind::Term)
                {
                    8.
                } else {
                    16.
                }
            } else {
                0.
            },
            ..VisualLineStyle::BODY
        },
    };
    with_component_spacing(projection, style, segment, first_line, last_line)
}

fn with_component_spacing(
    projection: &TextProjection,
    mut style: VisualLineStyle,
    segment: &crate::ProjectionSegment,
    first_line: bool,
    last_line: bool,
) -> VisualLineStyle {
    let (above, below) = component_spacing(projection, segment, first_line, last_line, None);
    style.space_above += above;
    style.space_below += below;
    style
}

fn table_outer_spacing(line: &VisualLineSpec, projection: &TextProjection) -> (f32, f32) {
    let Some(segment) = segment_for_line(projection, &line.projected_range()) else {
        return (0., 0.);
    };
    let Some(context) = segment
        .context
        .table_cell
        .and_then(|(id, _, _)| projection.table_context(id))
    else {
        return (0., 0.);
    };
    let (above, below) = component_spacing(
        projection,
        segment,
        line.projected_start() == segment.projection_start(),
        line.projected_end() == segment.projection_end(),
        Some(&context.containers),
    );
    let tree_header = line.compact_tree
        && segment.context.list_depth > 2
        && segment.context.list_branch_start
        && segment.context.list_marker.is_some()
        && segment.context.list_parent_label.is_some()
        && line.projected_start() == segment.projection_start()
        && segment
            .context
            .table_cell
            .is_some_and(|(id, _, _)| segment.context.list_item_container == Some(id));
    (
        above
            + if tree_header {
                compact_tree::HEADER
            } else {
                0.
            },
        below,
    )
}

fn component_spacing(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    first_line: bool,
    last_line: bool,
    ancestors: Option<&[NodeId]>,
) -> (f32, f32) {
    let Some(edges) = projection.container_edges(segment.node_id) else {
        return (0., 0.);
    };
    let sum = |ids: &[NodeId], before| {
        ids.iter()
            .filter(|id| ancestors.is_none_or(|ancestors| ancestors.contains(id)))
            .map(|id| match projection.block(*id) {
                Some(BlockNode::BlockQuote { .. }) => DocumentStyle::QUOTE_PADDING,
                Some(BlockNode::Alert { .. }) if before => ALERT_HEADER_HEIGHT,
                Some(BlockNode::Alert { .. }) => ALERT_BOTTOM_PADDING,
                _ => 0.,
            })
            .sum::<f32>()
    };
    (
        if first_line {
            sum(&edges.starts, true)
        } else {
            0.
        },
        if last_line {
            sum(&edges.ends, false)
        } else {
            0.
        },
    )
}

fn image_reserved_height(
    projection: &TextProjection,
    block: &BlockNode,
    segment: &crate::ProjectionSegment,
    dimensions: &NodeImageDimensions,
    layout_width: f32,
) -> Option<f32> {
    let BlockNode::Image(image) = block else {
        return None;
    };
    let (width, height) = resolved_image_dimensions(image, dimensions)?;
    let available = if segment.context.table_cell.is_some() {
        // Exactly the same fitted column and nested insets as text, including
        // stacked records. Column widths are logical pixels, not proportions.
        segment_text_width(segment, projection, layout_width)
    } else {
        (layout_width
            - if segment.context.list_depth > 0 {
                container_inset(segment)
            } else {
                0.
            })
        .max(1.)
    };
    Some(available.min(width.max(1) as f32) * height as f32 / width.max(1) as f32)
}

/// Source resources outlive canonical node IDs; binding never loads an image.
fn bind_image_dimensions(
    projection: &TextProjection,
    source_dimensions: &SourceImageDimensions,
    directory: Option<&std::path::Path>,
) -> NodeImageDimensions {
    projection
        .image_segments()
        .filter_map(|segment| {
            let source = segment.context.image_source.as_ref()?;
            let resource = resolved_image_resource(source, directory);
            source_dimensions
                .get(&hash(&resource))
                .copied()
                .map(|size| (segment.node_id, (source.clone(), size)))
        })
        .collect()
}

fn resolved_image_dimensions(
    image: &document_core::ImageNode,
    dimensions: &NodeImageDimensions,
) -> Option<(u32, u32)> {
    dimensions
        .get(&image.id)
        .filter(|(source, _)| source == &image.source)
        .map(|(_, dimensions)| *dimensions)
        .filter(|(width, height)| *width > 0 && *height > 0)
        .or(image
            .intrinsic_size
            .filter(|(width, height)| *width > 0 && *height > 0))
}

fn resolved_image_resource(source: &str, directory: Option<&std::path::Path>) -> Resource {
    if source.starts_with("http://") || source.starts_with("https://") {
        Resource::Uri(source.to_owned().into())
    } else {
        let path = PathBuf::from(source);
        Resource::Path(
            if path.is_absolute() {
                path
            } else {
                directory.map_or(path.clone(), |directory| directory.join(path))
            }
            .into(),
        )
    }
}

fn segment_for_line<'a>(
    projection: &'a TextProjection,
    line: &Range<usize>,
) -> Option<&'a crate::ProjectionSegment> {
    projection.segment_for_range(line)
}

#[cfg(test)]
fn active_heading_node_for_viewport(
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    scroll_y: f32,
    viewport_height: f32,
) -> Option<NodeId> {
    let viewport_top = scroll_y + 8.;
    let viewport_bottom = scroll_y + viewport_height;
    let mut first_visible = None;
    let mut active = None;
    for line in lines {
        let Some(node_id) = (|| {
            let segment = segment_for_line(projection, &line.projected_range())?;
            matches!(
                projection.block(segment.node_id),
                Some(BlockNode::Heading(_))
            )
            .then_some(segment.node_id)
        })() else {
            continue;
        };
        if line.y <= viewport_top {
            active = Some(node_id);
        } else if first_visible.is_none() && line.y < viewport_bottom {
            first_visible = Some(node_id);
        }
    }
    active.or(first_visible)
}

fn first_editable_position(blocks: &document_core::BlockSequence) -> Option<DocumentPosition> {
    for block in blocks {
        if block.text().is_some() {
            return Some(DocumentPosition::new(block.id(), 0, Affinity::Downstream));
        }
        let nested = match block.as_ref() {
            BlockNode::List(list) => list
                .items
                .iter()
                .find_map(|item| first_editable_position(&item.blocks)),
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => first_editable_position(blocks),
            BlockNode::Definition { blocks, .. } => first_editable_position(blocks),
            BlockNode::Table(table) => table.rows.iter().find_map(|row| {
                row.cells
                    .iter()
                    .find_map(|cell| first_editable_position(&cell.blocks))
            }),
            _ => None,
        };
        if nested.is_some() {
            return nested;
        }
    }
    None
}

fn semantic_document_tree(
    blocks: &document_core::BlockSequence,
    bounds: &HashMap<NodeId, SemanticBounds>,
) -> Vec<SemanticNodeSpec> {
    let mut section_heading = None;
    blocks
        .iter()
        .filter_map(|block| semantic_block_in_section(block, bounds, &mut section_heading))
        .collect()
}

fn semantic_block_in_section(
    block: &BlockNode,
    bounds: &HashMap<NodeId, SemanticBounds>,
    section_heading: &mut Option<String>,
) -> Option<SemanticNodeSpec> {
    if let BlockNode::Heading(heading) = block {
        *section_heading = Some(heading.content.as_string());
    }
    let mut spec = semantic_block(block, bounds)?;
    if spec.role == Role::Table
        && let Some(heading) = section_heading.as_deref()
    {
        spec.label = format!("{heading} table");
    }
    Some(spec)
}

fn semantic_block(
    block: &BlockNode,
    bounds: &HashMap<NodeId, SemanticBounds>,
) -> Option<SemanticNodeSpec> {
    let (role, label, level, children) = match block {
        BlockNode::List(list) => {
            let children = list
                .items
                .iter()
                .filter_map(|item| {
                    let children = semantic_document_tree(&item.blocks, bounds);
                    let item_bounds = union_semantic_bounds(&children)?;
                    let toggled = matches!(list.kind, document_core::ListKind::Task)
                        .then(|| item.checked.map(Toggled::from))
                        .flatten();
                    Some(SemanticNodeSpec {
                        node_id: item.id,
                        role: if matches!(list.kind, document_core::ListKind::Task) {
                            Role::CheckBox
                        } else {
                            Role::ListItem
                        },
                        label: sequence_plain_text(&item.blocks),
                        image_link: None,
                        resource_link: None,
                        math_markup: None,
                        level: None,
                        row_index: None,
                        column_index: None,
                        toggled,
                        bounds: item_bounds,
                        children,
                    })
                })
                .collect::<Vec<_>>();
            (Role::List, "List".into(), None, children)
        }
        BlockNode::Table(table) => {
            let children = table
                .rows
                .iter()
                .enumerate()
                .filter_map(|(row_index, row)| {
                    let children = row
                        .cells
                        .iter()
                        .enumerate()
                        .filter_map(|(column_index, cell)| {
                            let children = semantic_document_tree(&cell.blocks, bounds);
                            let cell_bounds = union_semantic_bounds(&children)?;
                            Some(SemanticNodeSpec {
                                node_id: cell.id,
                                role: if row_index == 0 {
                                    Role::ColumnHeader
                                } else {
                                    Role::Cell
                                },
                                label: sequence_plain_text(&cell.blocks),
                                image_link: None,
                                resource_link: None,
                                math_markup: None,
                                level: None,
                                row_index: Some(row_index + 1),
                                column_index: Some(column_index + 1),
                                toggled: None,
                                bounds: cell_bounds,
                                children,
                            })
                        })
                        .collect::<Vec<_>>();
                    let row_bounds = union_semantic_bounds(&children)?;
                    Some(SemanticNodeSpec {
                        node_id: row.id,
                        role: Role::Row,
                        label: format!("Row {}", row_index + 1),
                        image_link: None,
                        resource_link: None,
                        math_markup: None,
                        level: None,
                        row_index: Some(row_index + 1),
                        column_index: None,
                        toggled: None,
                        bounds: row_bounds,
                        children,
                    })
                })
                .collect::<Vec<_>>();
            (Role::Table, "Table".into(), None, children)
        }
        BlockNode::BlockQuote { blocks, .. } => (
            Role::Blockquote,
            sequence_plain_text(blocks),
            None,
            semantic_document_tree(blocks, bounds),
        ),
        BlockNode::Alert { blocks, .. } => (
            Role::Alert,
            sequence_plain_text(blocks),
            None,
            semantic_document_tree(blocks, bounds),
        ),
        BlockNode::FootnoteDefinition { blocks, .. } => (
            Role::Note,
            sequence_plain_text(blocks),
            None,
            semantic_document_tree(blocks, bounds),
        ),
        BlockNode::Definition { kind, blocks, .. } => (
            match kind {
                document_core::DefinitionKind::List => Role::DescriptionList,
                document_core::DefinitionKind::Term => Role::Term,
                document_core::DefinitionKind::Description => Role::Definition,
            },
            String::new(),
            None,
            semantic_document_tree(blocks, bounds),
        ),
        BlockNode::Heading(heading) => (
            Role::Heading,
            heading.content.as_string(),
            Some(heading.level as usize),
            Vec::new(),
        ),
        BlockNode::CodeBlock(code) => (
            if crate::math::is_math(block) {
                Role::Math
            } else {
                Role::Code
            },
            code.content.as_string(),
            None,
            Vec::new(),
        ),
        BlockNode::Image(image) => (Role::Image, image.alt.as_string(), None, Vec::new()),
        BlockNode::PreservedSource { description, .. } => {
            (Role::Paragraph, description.clone(), None, Vec::new())
        }
        BlockNode::ThematicBreak { .. } => {
            (Role::Splitter, "Thematic break".into(), None, Vec::new())
        }
        BlockNode::Paragraph(paragraph)
            if paragraph.content.runs().iter().any(|run| {
                run.styles
                    .iter()
                    .any(|style| matches!(style, InlineStyle::Link(_)))
            }) =>
        {
            (Role::Link, paragraph.content.as_string(), None, Vec::new())
        }
        _ => (Role::Paragraph, block.plain_text(), None, Vec::new()),
    };
    let node_bounds = bounds
        .get(&block.id())
        .copied()
        .or_else(|| union_semantic_bounds(&children))?;
    Some(SemanticNodeSpec {
        node_id: block.id(),
        role,
        label,
        image_link: match block {
            BlockNode::Image(image) => image.link.as_ref().map(|link| link.target.0.clone()),
            _ => None,
        },
        resource_link: match block {
            BlockNode::Paragraph(p) => crate::adaptive::resource::target(p).map(str::to_owned),
            _ => None,
        },
        math_markup: None,
        level,
        row_index: None,
        column_index: None,
        toggled: None,
        bounds: node_bounds,
        children,
    })
}

fn union_semantic_bounds(nodes: &[SemanticNodeSpec]) -> Option<SemanticBounds> {
    nodes
        .iter()
        .map(|node| node.bounds)
        .reduce(SemanticBounds::union)
}

fn semantic_bounds_for_lines(
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    indices: impl IntoIterator<Item = usize>,
) -> HashMap<NodeId, SemanticBounds> {
    let mut bounds = HashMap::new();
    let segments = projection.segments();
    let mut segment_index = 0;
    let mut previous_start = 0;
    for index in indices {
        let Some(line) = lines.get(index) else {
            continue;
        };
        let range = line.projected_range();
        if range.start < previous_start {
            segment_index = segments
                .partition_point(|segment| segment.projection_start() <= range.start)
                .saturating_sub(1);
        } else {
            while segment_index + 1 < segments.len()
                && segments[segment_index + 1].projection_start() <= range.start
            {
                segment_index += 1;
            }
        }
        previous_start = range.start;
        let candidate = segments.get(segment_index).filter(|segment| {
            let start = segment.projection_start();
            let end = start + segment.projection_len();
            (start <= range.start && range.start < end)
                || (start == range.start && end == range.end)
                || end == range.end
        });
        let Some(segment) = candidate.or_else(|| projection.segment_for_range(&range)) else {
            continue;
        };
        let line_bounds = SemanticBounds {
            x_fraction: line.x_fraction,
            width_fraction: line.width_fraction,
            y: line.y,
            height: line.style.line_height,
        };
        bounds
            .entry(segment.node_id)
            .and_modify(|current: &mut SemanticBounds| *current = current.union(line_bounds))
            .or_insert(line_bounds);
    }
    bounds
}

fn sequence_plain_text(blocks: &document_core::BlockSequence) -> String {
    blocks
        .iter()
        .map(|block| block.plain_text())
        .collect::<Vec<_>>()
        .join("\n")
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SyntaxKind {
    Plain,
    Keyword,
    String,
    Comment,
    Number,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SyntaxSpan {
    range: Range<usize>,
    kind: SyntaxKind,
}

fn syntax_spans(text: &str, language: Option<&str>) -> Vec<SyntaxSpan> {
    let mut spans: Vec<SyntaxSpan> = Vec::new();
    let bytes = text.as_bytes();
    let hash_comments = language.is_some_and(|language| {
        matches!(
            language.to_ascii_lowercase().as_str(),
            "python" | "py" | "bash" | "sh" | "shell" | "yaml" | "yml" | "toml"
        )
    });
    let keywords = |word: &str| {
        matches!(
            word,
            "as" | "async"
                | "await"
                | "break"
                | "class"
                | "const"
                | "continue"
                | "def"
                | "do"
                | "else"
                | "enum"
                | "false"
                | "fn"
                | "for"
                | "from"
                | "if"
                | "impl"
                | "import"
                | "in"
                | "let"
                | "loop"
                | "match"
                | "mod"
                | "move"
                | "mut"
                | "new"
                | "none"
                | "null"
                | "pub"
                | "ref"
                | "return"
                | "self"
                | "static"
                | "struct"
                | "super"
                | "trait"
                | "true"
                | "type"
                | "use"
                | "var"
                | "where"
                | "while"
        )
    };
    let mut index = 0;
    while index < bytes.len() {
        let start = index;
        let (end, kind) =
            if bytes[index..].starts_with(b"//") || (hash_comments && bytes[index] == b'#') {
                (bytes.len(), SyntaxKind::Comment)
            } else if matches!(bytes[index], b'\'' | b'"' | b'`') {
                let quote = bytes[index];
                index += 1;
                while index < bytes.len() {
                    if bytes[index] == b'\\' {
                        index = (index + 2).min(bytes.len());
                    } else if bytes[index] == quote {
                        index += 1;
                        break;
                    } else {
                        index += 1;
                    }
                }
                (index, SyntaxKind::String)
            } else if bytes[index].is_ascii_digit() {
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric()
                        || matches!(bytes[index], b'.' | b'_' | b'x'))
                {
                    index += 1;
                }
                (index, SyntaxKind::Number)
            } else if bytes[index].is_ascii_alphabetic() || bytes[index] == b'_' {
                index += 1;
                while index < bytes.len()
                    && (bytes[index].is_ascii_alphanumeric() || bytes[index] == b'_')
                {
                    index += 1;
                }
                let kind = if keywords(&text[start..index]) {
                    SyntaxKind::Keyword
                } else {
                    SyntaxKind::Plain
                };
                (index, kind)
            } else {
                index += text[index..].chars().next().map_or(1, char::len_utf8);
                (index, SyntaxKind::Plain)
            };
        if let Some(previous) = spans.last_mut()
            && previous.kind == kind
            && previous.range.end == start
        {
            previous.range.end = end;
        } else {
            spans.push(SyntaxSpan {
                range: start..end,
                kind,
            });
        }
        index = end;
    }
    if spans.is_empty() {
        spans.push(SyntaxSpan {
            range: 0..0,
            kind: SyntaxKind::Plain,
        });
    }
    spans
}

fn chapter_prefix_len(text: &str) -> usize {
    let prefix = text.split_whitespace().next().unwrap_or("");
    if prefix.len() <= 24
        && prefix.starts_with(|c: char| c.is_ascii_digit())
        && prefix
            .chars()
            .all(|c| c.is_ascii_digit() || c == '.' || c == ')')
        && text.len() > prefix.len()
    {
        prefix.len()
    } else {
        0
    }
}

/// Only enclose a complete label on one visual line. Wrapped labels remain
/// readable text, not chopped pills; source offsets still drive hit testing.
fn badge_range(
    segment: &crate::ProjectionSegment,
    badge: crate::signals::Badge,
    line: &Range<usize>,
) -> Option<Range<usize>> {
    let start = segment.projection_start() + badge.start.checked_sub(segment.node_range.start)?;
    let end = segment.projection_start() + badge.end.checked_sub(segment.node_range.start)?;
    (start >= line.start && end <= line.end).then(|| start - line.start..end - line.start)
}

fn styled_runs(
    editor: &RichDocumentEditor,
    line: &Range<usize>,
    line_len: usize,
    text_style: &gpui::TextStyle,
    placeholder: bool,
    palette: TachyonPalette,
) -> Vec<TextRun> {
    styled_projection_runs(
        &editor.projection,
        line,
        line_len,
        text_style,
        placeholder,
        palette,
    )
}

fn styled_projection_runs(
    projection: &TextProjection,
    line: &Range<usize>,
    line_len: usize,
    text_style: &gpui::TextStyle,
    placeholder: bool,
    palette: TachyonPalette,
) -> Vec<TextRun> {
    // Atomic rendered objects still own their complete source range, but the
    // native shaper receives no text. In particular, code highlighting must
    // not emit source-byte spans into that empty display string.
    if line_len == 0 {
        return Vec::new();
    }
    let mut base_font = text_style.font();
    base_font.family = DocumentStyle::BODY_FONT_FAMILY.into();
    base_font.weight = FontWeight::EXTRA_LIGHT;
    let mut base_color = rgb(palette.text).into();
    let mut base_background = None;
    let Some((segment, block)) = projection.segment_for_range(line).and_then(|segment| {
        projection
            .block(segment.node_id)
            .map(|block| (segment, block))
    }) else {
        return vec![TextRun {
            len: line_len,
            font: base_font,
            color: if placeholder {
                rgba(TachyonPalette::with_alpha(palette.secondary, 0x80)).into()
            } else {
                base_color
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    };
    if matches!(block, BlockNode::Paragraph(_))
        && segment.context.quote.is_some()
        && segment.context.list_depth == 0
        && segment.context.table_cell.is_none()
        && segment.context.quote_pull
    {
        base_font.style = FontStyle::Italic;
    }
    if segment.context.quote_attribution || segment.context.margin_note_anchor.is_some() {
        base_font.family = DocumentStyle::BODY_FONT_FAMILY.into();
        base_font.style = FontStyle::Normal;
        base_color = rgb(palette.secondary).into();
    }
    if segment.context.figure_text.is_some() {
        base_font.family = DocumentStyle::BODY_FONT_FAMILY.into();
        base_color = rgb(palette.secondary).into();
    }
    match block {
        BlockNode::Heading(_) => {
            base_font.family = DocumentStyle::HEADLINE_FONT_FAMILY.into();
            base_font.weight = FontWeight::EXTRA_BOLD;
            base_color = rgb(palette.heading).into();
        }
        BlockNode::CodeBlock(_) => {
            base_font.family = DocumentStyle::MONOSPACE_FONT_FAMILY.into();
        }
        BlockNode::PreservedSource { .. } => {
            base_font.family = DocumentStyle::MONOSPACE_FONT_FAMILY.into();
            base_color = rgb(palette.secondary).into();
            base_background = Some(rgb(palette.surface).into());
        }
        _ => {}
    }
    if segment.context.table_header || segment.context.table_property_key {
        base_font.weight = FontWeight::SEMIBOLD;
    }
    if let Some(role) = segment.context.metric {
        match role {
            crate::metrics::TextRole::Label | crate::metrics::TextRole::Value => {
                base_font.family = DocumentStyle::HEADLINE_FONT_FAMILY.into();
                base_font.weight = FontWeight::EXTRA_BOLD;
                base_color = rgb(palette.heading).into();
            }
            crate::metrics::TextRole::Context => {
                base_font.family = DocumentStyle::BODY_FONT_FAMILY.into();
                base_color = rgb(palette.secondary).into();
            }
        }
    }
    if matches!(block, BlockNode::Paragraph(_))
        && segment
            .context
            .definition
            .is_some_and(|(_, kind)| kind == document_core::DefinitionKind::Term)
    {
        base_font.weight = FontWeight::SEMIBOLD;
        base_color = rgb(palette.heading).into();
    }
    if let Some(role) = segment.context.color_role {
        use crate::signals::ColorRole;
        base_font.family = match role {
            ColorRole::Label => DocumentStyle::HEADLINE_FONT_FAMILY,
            ColorRole::Literal => DocumentStyle::MONOSPACE_FONT_FAMILY,
            ColorRole::Context => DocumentStyle::BODY_FONT_FAMILY,
        }
        .into();
        base_font.weight = if role == ColorRole::Label {
            FontWeight::EXTRA_BOLD
        } else {
            FontWeight::NORMAL
        };
        base_color = rgb(if role == ColorRole::Label {
            palette.heading
        } else {
            palette.secondary
        })
        .into();
    }
    if let BlockNode::CodeBlock(code) = block {
        let palette = code_palette(palette, block);
        let line_text = &projection.text()[line.clone()];
        return syntax_spans(line_text, code.language.as_deref())
            .into_iter()
            .map(|span| TextRun {
                len: span.range.len(),
                font: base_font.clone(),
                color: rgb(match span.kind {
                    SyntaxKind::Plain => palette.text,
                    SyntaxKind::Keyword => palette.syntax_keyword,
                    SyntaxKind::String => palette.syntax_string,
                    SyntaxKind::Comment => palette.syntax_comment,
                    SyntaxKind::Number => palette.syntax_number,
                })
                .into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            })
            .collect();
    }
    let Some(rich_text) = block.text() else {
        return vec![TextRun {
            len: line_len,
            font: base_font,
            color: base_color,
            background_color: base_background,
            underline: None,
            strikethrough: None,
        }];
    };

    let node_start = segment.node_range.start + line.start - segment.projection_start();
    let node_end = node_start + line_len;
    let label_end = segment.context.resource_title_end.or_else(|| {
        if segment.context.list_depth > 0
            && segment.context.bibliography.is_none()
            && segment.context.table_cell.is_none()
            && let BlockNode::Paragraph(paragraph) = block
        {
            crate::adaptive::authored_label_end(paragraph)
        } else {
            None
        }
    });
    let delimiter = label_end
        .or_else(|| match block {
            BlockNode::Paragraph(paragraph)
                if segment.context.metadata
                    || paragraph
                        .content
                        .runs()
                        .first()
                        .is_some_and(|run| run.styles.contains(&InlineStyle::Bold)) =>
            {
                crate::adaptive::authored_label_end(paragraph)
            }
            BlockNode::Heading(heading)
                if crate::adaptive::editorial::Kind::from_title(&heading.content.as_cow())
                    .is_some() =>
            {
                heading.content.as_cow().find(':').map(|index| index + 1)
            }
            _ => None,
        })
        .and_then(|end| {
            let text = rich_text.as_cow();
            let prefix = text.get(..end)?.trim_end();
            prefix.ends_with(':').then(|| prefix.len() - 1)
        });
    let mut result = Vec::new();
    for inline in rich_text.runs() {
        let span = inline.range.start.max(node_start)..inline.range.end.min(node_end);
        if span.start >= span.end {
            continue;
        }
        let mut boundaries: smallvec::SmallVec<[usize; 5]> =
            smallvec::smallvec![span.start, span.end];
        boundaries.extend(label_end.filter(|end| *end > span.start && *end < span.end));
        if let Some(delimiter) = delimiter {
            boundaries.extend(
                [delimiter, delimiter + 1]
                    .into_iter()
                    .filter(|end| *end > span.start && *end < span.end),
            );
        }
        if let Some(badge) = segment.context.badge {
            boundaries.extend(
                [badge.start, badge.end]
                    .into_iter()
                    .filter(|end| *end > span.start && *end < span.end),
            );
        }
        boundaries.sort_unstable();
        boundaries.dedup();
        for bounds in boundaries.windows(2) {
            let overlap = bounds[0]..bounds[1];
            let mut font = base_font.clone();
            let mut color = base_color;
            let mut background_color = base_background;
            let mut underline = None;
            let mut strikethrough = None;
            for style in &inline.styles {
                match style {
                    InlineStyle::Bold => font.weight = FontWeight::BOLD,
                    InlineStyle::Italic => font.style = FontStyle::Italic,
                    InlineStyle::Strikethrough => {
                        strikethrough = Some(StrikethroughStyle {
                            color: Some(color),
                            thickness: px(1.),
                        });
                    }
                    InlineStyle::Code => {
                        color = rgb(palette.syntax_number).into();
                        background_color = Some(rgb(palette.surface).into());
                    }
                    InlineStyle::Math { .. } => {
                        font.family = DocumentStyle::MONOSPACE_FONT_FAMILY.into();
                        background_color = Some(rgb(palette.surface).into());
                    }
                    InlineStyle::Link(_) => {
                        color = rgb(palette.accent).into();
                        underline = Some(UnderlineStyle {
                            color: Some(color),
                            thickness: px(1.),
                            wavy: false,
                        });
                    }
                    InlineStyle::Image { .. }
                    | InlineStyle::FootnoteReference(_)
                    | InlineStyle::PreservedHtml(_) => color = rgb(palette.accent).into(),
                }
            }
            if label_end.is_some_and(|end| overlap.end <= end) {
                font.weight = FontWeight::EXTRA_BOLD;
                if !inline
                    .styles
                    .iter()
                    .any(|style| matches!(style, InlineStyle::Link(_)))
                {
                    color = rgb(palette.heading).into();
                }
                if !inline.styles.contains(&InlineStyle::Code) && !segment.context.metadata {
                    font.family = DocumentStyle::HEADLINE_FONT_FAMILY.into();
                }
            }
            if let Some(badge) = segment.context.badge
                && overlap.start >= badge.start
                && overlap.end <= badge.end
            {
                font.weight = FontWeight::MEDIUM;
                color = rgb(badge.tone.style(palette).ink).into();
            }
            if segment.context.color_role == Some(crate::signals::ColorRole::Literal) {
                background_color = None;
            }
            if delimiter == Some(overlap.start) && overlap.len() == 1 {
                color = rgba(palette.label_delimiter()).into();
            }
            result.push(TextRun {
                len: overlap.len(),
                font,
                color,
                background_color,
                underline,
                strikethrough,
            });
        }
    }
    if result.is_empty() {
        result.push(TextRun {
            len: line_len,
            font: base_font,
            color: base_color,
            background_color: base_background,
            underline: None,
            strikethrough: None,
        });
    }
    if matches!(block, BlockNode::Heading(_)) {
        let prefix = chapter_prefix_len(&projection.text()[segment.projection_range()]);
        let mut styled = Vec::with_capacity(result.len() + 1);
        let mut offset = node_start;
        for mut run in result {
            let length = run.len;
            if offset < prefix {
                let mut number = run.clone();
                number.len = length.min(prefix - offset);
                number.color = rgb(palette.accent).into();
                number.font.family = "Public Sans Tachyon".into();
                number.font.weight = FontWeight::MEDIUM;
                run.len -= number.len;
                styled.push(number);
            }
            if run.len > 0 {
                styled.push(run);
            }
            offset += length;
        }
        return styled;
    }
    result
}

fn apply_marked_runs(
    runs: Vec<TextRun>,
    marked: Option<&Range<usize>>,
    line: &Range<usize>,
    palette: TachyonPalette,
) -> Vec<TextRun> {
    let Some(marked) = marked else {
        return runs;
    };
    let overlap = marked.start.max(line.start)..marked.end.min(line.end);
    if overlap.start >= overlap.end {
        return runs;
    }
    let local = overlap.start - line.start..overlap.end - line.start;
    let mut result = Vec::new();
    let mut cursor = 0;
    for run in runs {
        let run_range = cursor..cursor + run.len;
        let marked_start = run_range.start.max(local.start);
        let marked_end = run_range.end.min(local.end);
        // A mark elsewhere on the line must not extend this run toward it.
        // Native shapers require the runs to partition the text exactly.
        if marked_start >= marked_end {
            result.push(run);
            cursor = run_range.end;
            continue;
        }
        if run_range.start < marked_start {
            result.push(TextRun {
                len: marked_start - run_range.start,
                ..run.clone()
            });
        }
        if marked_start < marked_end {
            result.push(TextRun {
                len: marked_end - marked_start,
                underline: Some(UnderlineStyle {
                    color: Some(rgb(palette.accent).into()),
                    thickness: px(1.),
                    wavy: false,
                }),
                ..run.clone()
            });
        }
        if marked_end < run_range.end {
            result.push(TextRun {
                len: run_range.end - marked_end,
                ..run
            });
        }
        cursor = run_range.end;
    }
    result
}

fn utf16_range_in_text(text: &str, range: Range<usize>) -> Range<usize> {
    fn byte_for_utf16(text: &str, target: usize) -> usize {
        let mut units = 0;
        for (byte, character) in text.char_indices() {
            if units >= target || units + character.len_utf16() > target {
                return byte;
            }
            units += character.len_utf16();
        }
        text.len()
    }
    byte_for_utf16(text, range.start)..byte_for_utf16(text, range.end)
}

fn selection_collapse_offset(range: &Range<usize>, left: bool) -> usize {
    if left { range.start } else { range.end }
}

fn visual_line_index_at_offset(lines: &[VisualLineSpec], offset: usize) -> Option<usize> {
    // At a soft wrap (including a column boundary), the byte belongs to the
    // following line, matching painted caret/hit-test affinity. Choosing the
    // preceding endpoint would leave Down stuck at a column's first byte.
    let index = lines
        .partition_point(|line| line.projected_start() <= offset)
        .checked_sub(1)?;
    let line = &lines[index];
    (line.projected_range().contains(&offset) || line.projected_end() == offset).then_some(index)
}

fn visual_vertical_neighbor(
    lines: &[VisualLineSpec],
    current_index: usize,
    direction: isize,
) -> Option<usize> {
    let current_y = lines.get(current_index)?.y;
    let mut index = current_index;
    loop {
        index = index.checked_add_signed(direction)?;
        let candidate = lines.get(index)?;
        if (candidate.y - current_y) * direction as f32 > f32::EPSILON {
            return Some(index);
        }
    }
}

fn snap_offset_to_grapheme(text: &str, range: Range<usize>, candidate: usize) -> usize {
    let candidate = candidate.clamp(range.start, range.end);
    let mut nearest = range.start;
    for (offset, _) in text[range.clone()].grapheme_indices(true) {
        let absolute = range.start + offset;
        if absolute > candidate {
            break;
        }
        nearest = absolute;
    }
    if candidate == range.end {
        range.end
    } else {
        nearest
    }
}

fn previous_word_boundary(text: &str, offset: usize) -> usize {
    let mut cursor = offset.min(text.len());
    while cursor > 0 {
        let previous = text[..cursor]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index);
        let grapheme = &text[previous..cursor];
        if !grapheme.chars().all(char::is_whitespace) {
            let class = word_class(grapheme);
            cursor = previous;
            while cursor > 0 {
                let start = text[..cursor]
                    .grapheme_indices(true)
                    .next_back()
                    .map_or(0, |(index, _)| index);
                if word_class(&text[start..cursor]) != class {
                    break;
                }
                cursor = start;
            }
            return cursor;
        }
        cursor = previous;
    }
    cursor
}

fn next_word_boundary(text: &str, offset: usize) -> usize {
    let mut cursor = offset.min(text.len());
    while cursor < text.len() {
        let end = text[cursor..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(text.len(), |(index, _)| cursor + index);
        let grapheme = &text[cursor..end];
        if !grapheme.chars().all(char::is_whitespace) {
            let class = word_class(grapheme);
            cursor = end;
            while cursor < text.len() {
                let next_end = text[cursor..]
                    .grapheme_indices(true)
                    .nth(1)
                    .map_or(text.len(), |(index, _)| cursor + index);
                if word_class(&text[cursor..next_end]) != class {
                    break;
                }
                cursor = next_end;
            }
            return cursor;
        }
        cursor = end;
    }
    cursor
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum WordClass {
    Whitespace,
    Word,
    Punctuation,
}

fn word_class(grapheme: &str) -> WordClass {
    if grapheme.chars().all(char::is_whitespace) {
        WordClass::Whitespace
    } else if grapheme
        .chars()
        .all(|character| character.is_alphanumeric() || character == '_')
    {
        WordClass::Word
    } else {
        WordClass::Punctuation
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::PROSE_WIDTH;

    #[test]
    fn default_table_rules_follow_the_one_logical_pixel_token() {
        let mut output = Vec::new();
        push_table_border(
            &mut output,
            Bounds::new(point(px(10.), px(20.)), size(px(100.), px(40.))),
            TableBorder::default(),
            None,
            TachyonPalette::LIGHT.border,
            TableRuleScale {
                display: 1.,
                zoom: 1.,
            },
            (true, true),
        );
        assert_eq!(output.len(), 4);
        assert_eq!(output[0].quad.bounds.size.height, px(1.));
        assert_eq!(output[1].quad.bounds.size.width, px(1.));
    }

    #[test]
    fn table_border_units_scale_once_and_keep_shared_edges_single() {
        for display in [1., 1.25, 1.5, 2.] {
            for zoom in [0.75, 1., 1.5, 2.] {
                for border in [
                    TableBorder::LogicalPixel,
                    TableBorder::PhysicalPixel,
                    TableBorder::None,
                ] {
                    let mut output = Vec::new();
                    push_table_border(
                        &mut output,
                        Bounds::new(point(px(10.), px(20.)), size(px(100.), px(40.))),
                        border,
                        None,
                        TachyonPalette::LIGHT.border,
                        TableRuleScale { display, zoom },
                        (false, false),
                    );
                    if border == TableBorder::None {
                        assert!(output.is_empty());
                    } else {
                        assert_eq!(
                            output.len(),
                            2,
                            "shared top/left edges belong to adjacent cells"
                        );
                        let expected = px(if border == TableBorder::PhysicalPixel {
                            1. / display
                        } else {
                            zoom
                        });
                        assert_eq!(output[0].quad.bounds.size.height, expected);
                        assert_eq!(output[1].quad.bounds.size.width, expected);
                    }
                }
            }
        }
    }

    #[test]
    fn dotted_table_rules_are_bounded_by_the_viewport_and_keep_their_phase() {
        let paint = |top| {
            let mut output = Vec::new();
            push_table_border(
                &mut output,
                Bounds::new(point(px(0.), px(0.)), size(px(10_000.), px(100_000.))),
                TableBorder::Dotted,
                Some(ContentMask {
                    bounds: Bounds::new(point(px(9950.), px(top)), size(px(50.), px(100.))),
                }),
                TachyonPalette::LIGHT.border,
                TableRuleScale {
                    display: 1.,
                    zoom: 1.,
                },
                (false, false),
            );
            output
        };
        let before = paint(40_000.);
        let after = paint(40_001.);
        assert!(
            (30..=35).contains(&before.len()),
            "only visible dots, not the full 100,000px edge"
        );
        for dot in before.iter().chain(&after) {
            assert_eq!(dot.quad.bounds.size, size(px(1.), px(1.)));
            assert_eq!(dot.quad.bounds.left(), px(9999.));
            assert_eq!(f32::from(dot.quad.bounds.top()) % 3., 0.);
        }
        assert!(
            before.iter().skip(1).all(|dot| after
                .iter()
                .any(|other| other.quad.bounds == dot.quad.bounds)),
            "scrolling clips the same pattern; it must not restart at the visible edge"
        );
    }

    #[test]
    fn shaped_cluster_stops_follow_visual_order_without_changing_logical_offsets() {
        let ltr = caret_stops_from_clusters("abc", 30., &[(0, 0.), (1, 10.), (2, 20.)]);
        assert_eq!(
            ltr.iter()
                .map(|stop| (stop.offset, stop.x))
                .collect::<Vec<_>>(),
            vec![(0, 0.), (1, 10.), (2, 20.), (3, 30.)]
        );

        let rtl = caret_stops_from_clusters("אבג", 30., &[(4, 0.), (2, 10.), (0, 20.)]);
        assert_eq!(
            rtl.iter()
                .map(|stop| (stop.offset, stop.x))
                .collect::<Vec<_>>(),
            vec![(6, 0.), (4, 10.), (2, 20.), (0, 30.)]
        );
    }

    #[test]
    fn shaped_cluster_stops_interpolate_ligature_graphemes() {
        let stops = caret_stops_from_clusters("ffi", 30., &[(0, 0.)]);
        assert_eq!(
            stops
                .iter()
                .map(|stop| (stop.offset, stop.x))
                .collect::<Vec<_>>(),
            vec![(0, 0.), (1, 10.), (2, 20.), (3, 30.)]
        );
    }

    fn click_html_text(
        editor: &mut RichDocumentEditor,
        leaf: usize,
        byte: usize,
        window: &mut Window,
        cx: &mut Context<RichDocumentEditor>,
    ) {
        let bounds = Bounds::new(
            point(px(75.), px(0.)),
            size(px(800.), px(editor.document_height)),
        );
        editor.element_bounds = Some(bounds);
        let line = editor
            .visual_lines
            .iter()
            .find(|line| line.html_preview.is_some())
            .unwrap();
        let hit = line
            .html_preview
            .as_ref()
            .unwrap()
            .text_hits
            .iter()
            .find(|hit| hit.left.text_node == leaf && hit.left.byte_offset == byte)
            .unwrap();
        let [left, top, right, bottom] = hit.bounds;
        let position = point(
            bounds.left()
                + px(line.x_fraction * 800.
                    + line.inset
                    + (left + (right - left) * 0.1) * editor.zoom_factor),
            bounds.top() + px(line.y + ((top + bottom) * 0.5) * editor.zoom_factor),
        );
        editor.on_mouse_down(
            &MouseDownEvent {
                position,
                button: MouseButton::Left,
                click_count: 1,
                ..Default::default()
            },
            window,
            cx,
        );
        editor.on_mouse_up(
            &MouseUpEvent {
                position,
                button: MouseButton::Left,
                ..Default::default()
            },
            window,
            cx,
        );
        assert!(
            editor.html_selection.is_some(),
            "click must select rendered HTML text"
        );
    }

    #[gpui::test]
    fn link_cursor_regions_cover_wrapped_links_but_not_plain_text(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = format!(
            "Before [**reference link**](https://example.test) after.\n\n[{}](https://example.test/wrapped)\n",
            "wrapped link words ".repeat(40),
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        let (_, state) = cx.draw(point(px(0.), px(0.)), size(px(700.), px(900.)), |_, _| {
            DocumentTextElement {
                editor: editor.clone(),
                semantics: None,
                math_scroll_handles: HashMap::new(),
            }
        });
        let first = state
            .lines
            .iter()
            .find(|line| line.layout.text.starts_with("Before"))
            .unwrap();
        let hit = state
            .link_hitboxes
            .iter()
            .find(|hit| hit.top() == first.bounds.top())
            .unwrap();
        assert_eq!(
            hit.left(),
            first.bounds.left() + first.layout.x_for_index(7)
        );
        assert_eq!(
            hit.right(),
            first.bounds.left() + first.layout.x_for_index(21)
        );
        assert_eq!(hit.behavior, gpui::HitboxBehavior::Normal);
        assert!(
            state.link_hitboxes.len() > 2,
            "wrapped links need a region on each line"
        );
        for line in state
            .lines
            .iter()
            .filter(|line| line.layout.text.starts_with("wrapped"))
        {
            if line.bounds.intersects(&hit.content_mask.bounds) {
                assert!(
                    state
                        .link_hitboxes
                        .iter()
                        .any(|hit| hit.top() == line.bounds.top())
                );
            }
        }
        cx.update(|_, cx| {
            assert_eq!(
                editor.read(cx).document.snapshot().serialize().unwrap(),
                source
            );
        });
        assert!(
            cx.opened_url().is_none(),
            "painting hover regions must not open links"
        );
    }

    #[gpui::test]
    fn link_cursor_regions_follow_table_alignment_and_clipping(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "| Description | Destination |\n| --- | ---: |\n| Plain cell | [reference](https://example.test) |\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        let (_, state) = cx.draw(point(px(0.), px(0.)), size(px(700.), px(900.)), |_, _| {
            DocumentTextElement {
                editor: editor.clone(),
                semantics: None,
                math_scroll_handles: HashMap::new(),
            }
        });
        assert_eq!(state.link_hitboxes.len(), 1);
        let line = state
            .lines
            .iter()
            .find(|line| line.layout.text.as_ref() == "reference")
            .unwrap();
        let hit = &state.link_hitboxes[0];
        assert_eq!(line.alignment, ColumnAlignment::Right);
        assert_eq!(
            hit.left(),
            aligned_text_left(line.bounds, &line.layout, line.alignment)
        );
        assert_eq!(hit.right(), hit.left() + line.layout.width());
        let mask = line
            .content_mask
            .expect("table links must be clipped to their cell");
        assert!(hit.content_mask.bounds.left() >= mask.bounds.left());
        assert!(hit.content_mask.bounds.right() <= mask.bounds.right());
    }

    #[gpui::test]
    fn html_ctrl_click_opens_a_link_without_converting_or_moving_the_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "# Before\n\n<div><p>A <a href='https://example.test/reference'>reference link</a> and ordinary text.</p></div>\n\nAfter\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                click_html_text(editor, 0, 3, window, cx);
                let before = editor.html_selection.as_ref().unwrap().range();
                let line = editor
                    .visual_lines
                    .iter()
                    .find(|line| line.html_preview.is_some())
                    .unwrap();
                let b = line.html_preview.as_ref().unwrap().links[0].bounds[0];
                let origin = editor.element_bounds.unwrap();
                let position = point(
                    origin.left() + px(line.inset + (b[0] + b[2]) * 0.5),
                    origin.top() + px(line.y + (b[1] + b[3]) * 0.5),
                );
                editor.on_mouse_down(
                    &MouseDownEvent {
                        position,
                        button: MouseButton::Left,
                        click_count: 1,
                        modifiers: gpui::Modifiers {
                            control: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                assert_eq!(editor.html_selection.as_ref().unwrap().range(), before);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            })
        });
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.test/reference")
        );
    }

    #[gpui::test]
    fn html_link_keyboard_activation_preserves_editing_and_undo(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source =
            "<div><p>A <a href='https://example.test/keyboard'>reference link</a>.</p></div>\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| click_html_text(editor, 0, 3, window, cx))
        });
        assert!(
            cx.opened_url().is_none(),
            "ordinary click must not navigate"
        );
        cx.simulate_keystrokes("alt-enter");
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.test/keyboard")
        );
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                editor.replace_text_in_range(None, "X", window, cx);
                assert!(
                    editor
                        .document
                        .snapshot()
                        .serialize()
                        .unwrap()
                        .contains("https://example.test/keyboard")
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            })
        });
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.test/keyboard")
        );
    }

    #[gpui::test]
    fn html_heading_link_uses_duplicate_anchor_without_converting_source(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "<div><p><a href='#repeat-1'>Go to second</a></p></div>\n\n# Repeat\n\nFirst.\n\n# Repeat\n\nSecond.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| click_html_text(editor, 0, 1, window, cx));
        });
        cx.simulate_keystrokes("alt-enter");
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let snapshot = editor.document.snapshot();
                let node = document_core::heading_node(snapshot.blocks(), "repeat-1").unwrap();
                let expected = editor
                    .projection
                    .segments()
                    .iter()
                    .find(|s| s.node_id == node)
                    .unwrap()
                    .projection_start();
                assert_eq!(editor.cursor_offset(), expected);
                assert!(editor.html_selection.is_none());
                assert_eq!(snapshot.serialize().unwrap(), source);
                editor.replace_text_in_range(None, "x", window, cx);
                assert_eq!(
                    editor.document.snapshot().serialize().unwrap(),
                    source.replace("# Repeat\n\nSecond", "# xRepeat\n\nSecond")
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        });
        assert!(cx.opened_url().is_none());
    }

    #[gpui::test]
    fn unchanged_replan_reuses_published_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = include_str!("../../../performance/layout-fixtures/17-long-layout-spec.md");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        editor.update(cx, |editor, _| {
            let snapshot = editor.document.snapshot();
            let prepare = |previous: &AdaptivePlan, published_geometry| {
                PreparedDocumentView::prepare_snapshot_with_images(
                    &snapshot,
                    &HashMap::new(),
                    None,
                    ReflowViewport {
                        published_geometry,
                        width: 1280.,
                        height: 1000.,
                        zoom: 1.,
                        preview_edit_node: None,
                        expanded_code_tail: None,
                        editing_node: None,
                        table_layout_lock: None,
                        html_disclosures: Arc::default(),
                        html_loaded_images: Arc::default(),
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: Some(0..5),
                        resource_generation: 0,
                    },
                    previous,
                    &editor.measurement,
                )
                .0
            };
            let first = prepare(&editor.adaptive, None);
            let counts = diagnostics::MeasurementScope::new();
            let second = prepare(&first.adaptive, first.published_geometry.clone());
            let counts = counts.take_stage();
            assert_eq!(
                counts.geometry_requests, 0,
                "unchanged geometry must not revisit every segment"
            );
            assert_eq!(counts.published_geometry_reuses, 1);
            assert!(Arc::ptr_eq(&first.visual_lines, &second.visual_lines));
            assert!(Arc::ptr_eq(&first.components, &second.components));
            assert_eq!(first.document_height, second.document_height);
            assert_eq!(first.paint_order, second.paint_order);
            assert_eq!(snapshot.serialize().unwrap(), source);
            let old_lines = first.visual_lines.as_ref().clone();
            editor.install_prepared(second);
            editor.layout_width = 1280.;
            let node = editor
                .projection
                .roots()
                .find(|root| matches!(root, BlockNode::Paragraph(_)))
                .unwrap()
                .id();
            let result = editor
                .apply_command(EditCommand::ReplaceText {
                    node_id: node,
                    range: 0..0,
                    text: "東京 😀 inserted text ".into(),
                    selection_after: None,
                    typing: false,
                })
                .unwrap();
            editor.refresh_after_transaction(&result);
            assert!(editor.published_geometry.is_none());
            assert!(!Arc::ptr_eq(&first.visual_lines, &editor.visual_lines));
            arrangement::tests::assert_same_geometry(&old_lines, &first.visual_lines);
            let changed_snapshot = editor.document.snapshot();
            let changed = PreparedDocumentView::prepare_snapshot_with_images(
                &changed_snapshot,
                &HashMap::new(),
                None,
                ReflowViewport {
                    published_geometry: first.published_geometry.clone(),
                    width: 1280.,
                    height: 1000.,
                    zoom: 1.,
                    preview_edit_node: None,
                    expanded_code_tail: None,
                    editing_node: None,
                    table_layout_lock: None,
                    html_disclosures: Arc::default(),
                    html_loaded_images: Arc::default(),
                    trace_mode: LayoutTraceMode::Off,
                    visible_roots: Some(0..5),
                    resource_generation: 0,
                },
                &first.adaptive,
                &editor.measurement,
            )
            .0;
            assert!(!Arc::ptr_eq(&first.visual_lines, &changed.visual_lines));
            assert!(changed.projection.text().contains("東京 😀 inserted text"));
            let reference = build_measured_visual_lines(
                &changed.projection,
                &HashMap::new(),
                1280.,
                &changed.adaptive,
                Some(&editor.measurement),
            );
            arrangement::tests::assert_same_geometry(&changed.visual_lines, &reference);
            editor.document.undo().unwrap();
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            assert!(matches!(
                editor.document.undo(),
                Err(DocumentError::NothingToUndo)
            ));
        });
    }

    #[gpui::test]
    fn markdown_heading_navigation_and_missing_links_preserve_content(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "[Go](#caf%C3%A9)\n\n# Café\n\nText.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.move_to(1, window, cx);
                editor.open_link(&OpenLink, window, cx);
                let offset = editor.cursor_offset();
                assert_eq!(
                    &editor.projection.text()[offset..offset + "Café".len()],
                    "Café"
                );
                editor.open_link_target("#missing", window, cx);
                assert_eq!(editor.cursor_offset(), offset);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        });
        assert!(cx.opened_url().is_none());
    }

    #[gpui::test]
    fn html_unsafe_link_does_not_dispatch_to_the_platform(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "<div><p>A <a href='javascript:alert(1)'>reference link</a>.</p></div>\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                click_html_text(editor, 0, 3, window, cx);
                editor.open_link(&OpenLink, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            })
        });
        assert!(cx.opened_url().is_none());
    }

    #[gpui::test]
    fn direct_html_edit_paints_current_text_before_and_after_blur(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "# Before\n\n<div><p>Six independent ideas stay visible.</p><p><strong>Styled text</strong></p></div>\n\nAfter.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                click_html_text(editor, 0, 8, window, cx);
                editor.replace_text_in_range(None, "VISIBLE", window, cx);
                assert!(
                    editor
                        .document
                        .snapshot()
                        .serialize()
                        .unwrap()
                        .contains("Six indeVISIBLEpendent ideas")
                );
                assert!(
                    editor
                        .projection
                        .text()
                        .contains("Six indeVISIBLEpendent ideas")
                );
            });
            _ = window.draw(cx);
            editor
                .read(cx)
                .painted_lines
                .iter()
                .find(|line| line.layout.text.contains("VISIBLE"))
                .expect("the first edit must paint the newly inserted text");
            editor.update(cx, |editor, cx| {
                let last = editor.projection.roots().last().unwrap().id();
                editor.jump_to_node(last, window, cx);
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor
                .read(cx)
                .painted_lines
                .iter()
                .find(|line| line.layout.text.contains("VISIBLE"))
                .expect("blur/reflow must not paint pre-edit text");
            editor.update(cx, |editor, cx| {
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        });
    }

    #[gpui::test]
    fn direct_html_selection_and_first_typing_use_one_content_transaction(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "# Before\n\n<div><p>Repeat <strong>café</strong></p><p>Repeat <strong>café</strong> <a href='https://example.test'>link</a></p></div>\n\nAfter\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let original_selection = editor.selection.clone();
                click_html_text(editor, 1, 8, window, cx);
                assert_eq!(editor.selection, original_selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                let pending = editor.html_selection.as_ref().unwrap();
                assert_eq!(
                    pending
                        .preview
                        .position_for_byte(pending.head)
                        .unwrap()
                        .byte_offset,
                    8
                );
                let range = pending.range();
                let node = pending.node;
                assert!(editor.html_selection_chrome(node, TachyonPalette::for_dark(false), false).is_empty());
                assert_eq!(editor.html_selection_chrome(node, TachyonPalette::for_dark(false), true).len(), 1);
                editor.set_zoom_factor(1.5, cx);
                editor.refresh_projection();
                assert_eq!(editor.html_selection.as_ref().unwrap().range(), range);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                editor.left(&Left, window, cx);
                editor.right(&Right, window, cx);
                editor.replace_text_in_range(None, "X", window, cx);
                assert!(editor.last_error.is_none(), "{:?}", editor.last_error);
                assert!(editor.html_selection.is_none());
                let edited = editor.document.snapshot().serialize().unwrap();
                assert!(edited.contains("Repeat **café**"));
                assert!(
                    edited.contains("Repeat **cXafé** [link](https://example.test)"),
                    "{edited}"
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
                editor.redo(&Redo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), edited);
                editor.undo(&Undo, window, cx);
                click_html_text(editor, 1, 8, window, cx);
                let after = editor.document.snapshot().blocks().iter().next_back().unwrap().id();
                editor.jump_to_node(after, window, cx);
                assert!(editor.html_selection.is_none());
                assert!(matches!(&editor.selection, Selection::Text(selection) if selection.head.node_id == after));
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            })
        });
    }

    #[gpui::test]
    fn first_html_cell_edit_retains_focused_column_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = include_str!("../../../performance/layout-fixtures/36-rich-cell-panels.md");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.layout_width = 800.;
                editor.refresh_projection();
                let cell_geometry = |editor: &RichDocumentEditor| {
                    let table = editor
                        .projection
                        .roots()
                        .filter_map(|node| {
                            if let BlockNode::Table(table) = node {
                                Some(table.id)
                            } else {
                                None
                            }
                        })
                        .nth(1)
                        .unwrap();
                    editor
                        .visual_lines
                        .iter()
                        .filter(|line| {
                            line.table_cell
                                .is_some_and(|(id, row, _, _)| id == table && row == 0)
                        })
                        .map(|line| (line.x_fraction, line.width_fraction))
                        .collect::<Vec<_>>()
                };
                let before = cell_geometry(editor);
                click_html_text(editor, 1, 8, window, cx);
                assert!(
                    editor.projection.table_layout_lock.is_some(),
                    "HTML focus must lock its table"
                );
                editor.replace_text_in_range(None, "edited ", window, cx);
                assert!(editor.last_error.is_none(), "{:?}", editor.last_error);
                assert_eq!(
                    cell_geometry(editor),
                    before,
                    "first HTML edit must not resize active columns"
                );
                editor.sync_layout_focus(window, cx);
                editor.refresh_projection();
                assert_eq!(
                    cell_geometry(editor),
                    before,
                    "focused reflow must retain columns"
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        });
    }

    #[gpui::test]
    fn direct_html_selection_copies_semantic_text_and_handles_utf16(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "<div><p>One café</p><p>Two 😀 words</p></div>\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                click_html_text(editor, 1, 0, window, cx);
                editor.select_all(&SelectAll, window, cx);
                editor.copy(&Copy, window, cx);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().unwrap(),
                    "One café\n\nTwo 😀 words"
                );
                #[cfg(target_os = "linux")]
                assert_eq!(
                    cx.read_from_clipboard().unwrap().html(),
                    Some(document_core::inert_html_fragment(source).unwrap().html())
                );
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                editor.set_selected_text_range(13..15, window, cx);
                assert_eq!(
                    editor.selected_text_range(false, window, cx).unwrap().range,
                    13..15
                );
                let mut actual = None;
                assert_eq!(
                    editor
                        .text_for_range(13..15, &mut actual, window, cx)
                        .unwrap(),
                    "😀"
                );
                assert_eq!(actual, Some(13..15));
                editor.replace_text_in_range(None, "界", window, cx);
                assert!(
                    editor
                        .document
                        .snapshot()
                        .serialize()
                        .unwrap()
                        .contains("Two 界 words")
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                click_html_text(editor, 1, 0, window, cx);
                assert!(editor.cancel_pending_composition(cx).unwrap());
                assert!(editor.html_selection.is_none());
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            })
        });
    }

    #[gpui::test]
    fn direct_html_invalid_and_empty_edits_do_not_convert(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "<div>Editable café</div>\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                click_html_text(editor, 0, 1, window, cx);
                assert!(!editor.replace_html_range(999..1000, "X", window, cx));
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(!editor.replace_html_range(1..1, "", window, cx));
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(editor.html_selection.is_some());
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
            })
        });
    }

    #[gpui::test]
    fn html_ime_is_provisional_cancellable_and_one_undoable_conversion(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "# Before\n\n<div><p>Repeat <strong>café</strong></p><p>Repeat <strong>café</strong> <a href='https://example.test'>link</a></p></div>\n\nAfter\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                click_html_text(editor, 1, 8, window, cx);
                let original_html_range = editor.selected_byte_range();
                editor.replace_and_mark_text_in_range(None, "仮", Some(0..1), window, cx);
                assert!(
                    editor.composition_active(),
                    "HTML text input must start composition: {:?}",
                    editor.last_error
                );
                assert!(
                    editor
                        .document
                        .snapshot()
                        .serialize()
                        .unwrap()
                        .contains("Repeat **c仮afé**")
                );
                let mark = editor.marked_text_range(window, cx).unwrap();
                let selected = editor.selected_text_range(false, window, cx).unwrap();
                assert_eq!(selected.range, mark);
                let mut actual = None;
                assert_eq!(
                    editor
                        .text_for_range(mark, &mut actual, window, cx)
                        .unwrap(),
                    "仮"
                );
                editor.replace_and_mark_text_in_range(None, "😀界", Some(2..3), window, cx);
                assert!(
                    editor
                        .document
                        .snapshot()
                        .serialize()
                        .unwrap()
                        .contains("Repeat **c😀界afé**")
                );
                assert!(
                    !editor
                        .document
                        .snapshot()
                        .serialize()
                        .unwrap()
                        .contains('仮')
                );
                let selected = editor.selected_text_range(false, window, cx).unwrap();
                assert_eq!(
                    editor
                        .text_for_range(selected.range, &mut actual, window, cx)
                        .unwrap(),
                    "界"
                );
                editor.cancel_pending_composition(cx).unwrap();
                assert!(!editor.composition_active());
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert_eq!(editor.selected_byte_range(), original_html_range);
                assert!(editor.html_selection.is_some());
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
                editor.replace_and_mark_text_in_range(None, "仮", None, window, cx);
                editor.replace_text_in_range(None, "確定", window, cx);
                assert!(!editor.composition_active());
                let committed = editor.document.snapshot().serialize().unwrap();
                assert!(
                    committed.contains("Repeat **c確定afé** [link](https://example.test)"),
                    "{committed}"
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
            })
        });
    }

    #[test]
    fn quiet_label_colons_preserve_text_and_rich_run_boundaries() {
        let source = "- **Dócument:** Read [the file](https://example.test).\n\n- **Owner**: Élodie\n\n### Decision: Local files\n\nOrdinary prose has a colon: still prose.\n\n`https://example.test`\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        for palette in [TachyonPalette::LIGHT, TachyonPalette::DARK] {
            let mut muted = 0;
            for segment in projection.segments() {
                let range = segment.projection_range();
                let text = &projection.text()[range.clone()];
                let runs = styled_projection_runs(
                    &projection,
                    &range,
                    text.len(),
                    &gpui::TextStyle::default(),
                    false,
                    palette,
                );
                let mut offset = 0;
                for run in runs {
                    let end = offset + run.len;
                    assert!(text.is_char_boundary(end));
                    if run.color == rgba(palette.label_delimiter()).into() {
                        assert_eq!(&text[offset..end], ":");
                        muted += 1;
                    }
                    offset = end;
                }
                assert_eq!(offset, text.len());
            }
            assert_eq!(muted, 3);
        }
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn readme_uses_one_body_family_and_one_headline_family() {
        let source = include_str!("../../../README.md");
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let text_style = gpui::TextStyle {
            font_family: DocumentStyle::BODY_FONT_FAMILY.into(),
            ..Default::default()
        };
        for prefix in [
            "The divider between Files and Outline",
            "Find follows source order",
            "For copyable source examples",
            "Short independent lists can use",
        ] {
            let segment = projection
                .segments()
                .iter()
                .find(|segment| projection.text()[segment.projection_range()].starts_with(prefix))
                .unwrap_or_else(|| panic!("missing README paragraph: {prefix}"));
            let range = segment.projection_range();
            let runs = styled_projection_runs(
                &projection,
                &range,
                range.len(),
                &text_style,
                false,
                TachyonPalette::DARK,
            );
            assert!(
                runs.iter().all(
                    |run| run.font.family.as_ref() == DocumentStyle::BODY_FONT_FAMILY
                        && run.font.weight == FontWeight::EXTRA_LIGHT
                ),
                "paragraph {prefix:?} used {:?}",
                runs.iter()
                    .map(|run| run.font.family.as_ref())
                    .collect::<Vec<_>>()
            );
        }
        for prefix in ["Tachyon", "Adaptive layouts", "Storage and recovery"] {
            let segment = projection
                .segments()
                .iter()
                .find(|segment| {
                    projection.text()[segment.projection_range()].starts_with(prefix)
                        && matches!(
                            projection.block(segment.node_id),
                            Some(BlockNode::Heading(_))
                        )
                })
                .unwrap_or_else(|| panic!("missing README heading: {prefix}"));
            let range = segment.projection_range();
            let runs = styled_projection_runs(
                &projection,
                &range,
                range.len(),
                &text_style,
                false,
                TachyonPalette::DARK,
            );
            assert!(
                runs.iter().all(|run| run.font.family.as_ref()
                    == DocumentStyle::HEADLINE_FONT_FAMILY
                    && run.font.weight == FontWeight::EXTRA_BOLD),
                "heading {prefix:?} used {:?}",
                runs.iter()
                    .map(|run| run.font.family.as_ref())
                    .collect::<Vec<_>>()
            );
        }
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[gpui::test]
    fn html_preedit_style_runs_cover_only_their_original_text(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "<div><p>Before <em>editable rich text</em> and <a href='https://example.test'>a link</a> after.</p></div>\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                click_html_text(editor, 0, 12, window, cx);
                editor.replace_and_mark_text_in_range(None, "´", None, window, cx);
                assert!(editor.composition_active());
                let palette = TachyonPalette::for_dark(false);
                for spec in editor.visual_lines.iter() {
                    let text = &editor.projection.text()[spec.projected_range()];
                    if text.is_empty() {
                        continue;
                    }
                    let runs = styled_runs(
                        editor, &spec.projected_range(), text.len(), &window.text_style(), false, palette,
                    );
                    let original_lengths: Vec<_> = runs.iter().map(|run| run.len).collect();
                    let marked = apply_marked_runs(runs, editor.marked_range.as_ref(), &spec.projected_range(), palette);
                    assert_eq!(
                        marked.iter().map(|run| run.len).sum::<usize>(), text.len(),
                        "preedit decoration must not extend styled text: original {original_lengths:?}, marked {:?}",
                        marked.iter().map(|run| run.len).collect::<Vec<_>>(),
                    );
                    let mut end = 0;
                    for run in marked {
                        end += run.len;
                        assert!(text.is_char_boundary(end));
                    }
                }
            });
        });
    }

    #[gpui::test]
    fn partitioned_technical_table_edits_match_complete_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = include_str!("../../../performance/layout-fixtures/76-entity-records.md");
        for zoom in [1., 1.5, 2.] {
            for target in ["Standard", "Atlas"] {
                let (editor, cx) = cx.add_window_view(|window, cx| {
                    RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
                });
                editor.update(cx, |editor, cx| {
                    editor.layout_width = 833. * zoom;
                    editor.zoom_factor = zoom;
                    editor.measurement = Arc::new(FontMeasurement::new(
                        cx.text_system().clone(),
                        "Public Sans Tachyon".into(),
                        zoom,
                    ));
                    editor.measurement.measure_tables(&mut editor.projection);
                    editor.adaptive = build_measured_adaptive_plan(
                        &editor.projection,
                        833.,
                        1500.,
                        None,
                        false,
                        &editor.measurement,
                    );
                    let mut lines = build_measured_visual_lines(
                        &editor.projection,
                        &HashMap::new(),
                        833.,
                        &editor.adaptive,
                        Some(&editor.measurement),
                    );
                    scale_visual_lines(&mut lines, zoom);
                    editor.visual_lines = Arc::new(lines);
                    editor.refresh_visual_index();
                    let node = editor
                        .projection
                        .segments()
                        .iter()
                        .find(|s| editor.projection.text()[s.projection_range()] == *target)
                        .unwrap()
                        .node_id;
                    let original_slots = editor.adaptive.slots.clone();
                    assert_eq!(original_slots.get(&node).unwrap().columns, 2);
                    editor.selection = Selection::Text(TextSelection::caret(
                        DocumentPosition::new(node, 0, Affinity::Downstream),
                    ));
                    let inserted = "Additional source detail ".repeat(80);
                    let result = editor
                        .apply_command(EditCommand::ReplaceSelection {
                            text: inserted.clone(),
                            typing: true,
                        })
                        .unwrap();
                    let scope = diagnostics::MeasurementScope::new();
                    editor.refresh_after_transaction(&result);
                    let counts = scope.take_stage();
                    drop(scope);
                    assert!(
                        counts.wrap_requests <= 2,
                        "only the changed cell needs wrapping: {counts:?}"
                    );
                    assert_eq!(editor.adaptive.slots, original_slots);
                    assert_eq!(
                        editor.document.snapshot().serialize().unwrap(),
                        source
                            .replace(&format!("| {target} |"), &format!("| {inserted}{target} |"))
                    );
                    let mut expected = build_measured_visual_lines(
                        &editor.projection,
                        &HashMap::new(),
                        833.,
                        &editor.adaptive,
                        Some(&editor.measurement),
                    );
                    scale_visual_lines(&mut expected, zoom);
                    assert_eq!(*editor.paint_order, visual_line_paint_order(&expected));
                    assert_eq!(editor.visual_lines.len(), expected.len());
                    for (actual, expected) in editor.visual_lines.iter().zip(&expected) {
                        assert_eq!(actual.projected_range(), expected.projected_range());
                        assert_eq!(actual.slot, expected.slot);
                        assert_eq!(actual.inset, expected.inset);
                        for (a, b) in [
                            (actual.y, expected.y),
                            (actual.x_fraction, expected.x_fraction),
                            (actual.width_fraction, expected.width_fraction),
                            (actual.table_row_y, expected.table_row_y),
                            (actual.table_row_height, expected.table_row_height),
                        ] {
                            assert!((a - b).abs() < 0.01, "{target}, zoom {zoom}: {a} != {b}");
                        }
                    }
                    editor.document.undo().unwrap();
                    editor.refresh_projection();
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                });
            }
        }
    }

    #[gpui::test]
    fn table_typing_only_wraps_the_affected_row(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        for rows in [100, 600] {
            let source = format!(
                "# Table\n\n| Key | Value |\n| :-- | --: |\n| Target | Short |\n{}\n## Following\n\n{}",
                (0..rows)
                    .map(|i| format!("| Key {i} | Value {i} |\n"))
                    .collect::<String>(),
                "Unrelated trailing paragraph with original content.\n\n".repeat(500),
            );
            let (editor, cx) = cx.add_window_view(|window, cx| {
                RichDocumentEditor::new(
                    Document::from_markdown(source.as_str()).unwrap(),
                    window,
                    cx,
                )
            });
            editor.update(cx, |editor, _| {
                editor.layout_width = 1280.;
                editor.refresh_projection();
                let segment = editor
                    .projection
                    .segments()
                    .iter()
                    .find(|s| &editor.projection.text()[s.projection_range()] == "Short")
                    .unwrap();
                let node = segment.node_id;
                let table = segment.context.table_cell.unwrap().0;
                assert_eq!(
                    editor.projection.table_measurements(table).is_some(),
                    rows == 100
                );
                let before_widths = editor.projection.fitted_table_widths(table, 1280.).unwrap();
                editor.selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
                    node,
                    1,
                    Affinity::Downstream,
                )));
                let transaction = editor
                    .apply_command(EditCommand::ReplaceSelection {
                        text: "longer cell text ".repeat(16),
                        typing: true,
                    })
                    .unwrap();
                assert_eq!(transaction.text_changed_node, Some(node));
                let scope = diagnostics::MeasurementScope::new();
                editor.refresh_after_transaction(&transaction);
                let counts = scope.take_stage();
                drop(scope);
                assert_eq!(
                    counts.wrap_requests, 1,
                    "only the changed cell wraps; the other cell reuses geometry: {counts:?}"
                );
                assert_eq!(counts.geometry_requests, 2);
                assert_eq!(counts.geometry_cache_hits, 1);
                assert_eq!(
                    editor.projection.fitted_table_widths(table, 1280.).unwrap(),
                    before_widths
                );
                let expected = build_measured_visual_lines(
                    &editor.projection,
                    &HashMap::new(),
                    1280.,
                    &editor.adaptive,
                    Some(&editor.measurement),
                );
                assert_eq!(*editor.paint_order, visual_line_paint_order(&expected));
                assert_eq!(editor.visual_lines.len(), expected.len());
                for (actual, expected) in editor.visual_lines.iter().zip(&expected) {
                    assert_eq!(actual.projected_range(), expected.projected_range());
                    for (a, b) in [
                        (actual.y, expected.y),
                        (actual.table_row_y, expected.table_row_y),
                        (actual.table_row_height, expected.table_row_height),
                    ] {
                        assert!((a - b).abs() < 0.1, "{a} vs {b}");
                    }
                }
                editor.document.undo().unwrap();
                editor.refresh_projection();
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        }
    }

    #[gpui::test]
    fn no_wrap_image_alt_typing_reuses_exact_component_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = format!(
            "# Start\n\n![before](image.png)\n\n{}",
            "Trailing paragraph keeps the document index populated.\n\n".repeat(300)
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        editor.update(cx, |editor, _| {
            editor.layout_width = 1280.;
            editor.refresh_projection();
            let image = editor
                .projection
                .roots()
                .find(|block| matches!(block, BlockNode::Image(_)))
                .unwrap()
                .id();
            editor.selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
                image,
                6,
                Affinity::Downstream,
            )));
            let retained = editor.components.clone();
            let result = editor
                .apply_command(EditCommand::ReplaceSelection {
                    text: "x".into(),
                    typing: true,
                })
                .unwrap();
            assert_eq!(result.text_changed_node, Some(image));
            editor.refresh_after_transaction(&result);
            assert!(
                Arc::ptr_eq(&retained, &editor.components),
                "unchanged line placement must retain the exact component index"
            );
            assert_eq!(
                editor.document.snapshot().serialize().unwrap(),
                source.replace("![before]", "![beforex]")
            );

            let rebuilt = component_geometry(
                &editor.projection,
                &editor.visual_lines,
                editor.layout_width,
                editor.zoom_factor,
                &editor.paint_order,
            );
            let ids = editor
                .projection
                .segments()
                .iter()
                .flat_map(|segment| {
                    [segment.node_id, segment.top_level_node_id]
                        .into_iter()
                        .chain(segment.context.list_ancestors.iter().copied())
                        .chain(segment.context.quote_ancestors.iter().copied())
                })
                .collect::<HashSet<_>>();
            for id in ids {
                let actual = editor.components.get(&id);
                let expected = rebuilt.get(&id);
                assert_eq!(actual.is_some(), expected.is_some(), "component {id:?}");
                if let (Some(actual), Some(expected)) = (actual, expected) {
                    assert_eq!(actual.first_line, expected.first_line, "component {id:?}");
                    for (left, right) in [
                        (actual.left_fraction, expected.left_fraction),
                        (actual.right_fraction, expected.right_fraction),
                        (actual.top, expected.top),
                        (actual.bottom, expected.bottom),
                    ] {
                        assert_eq!(left.to_bits(), right.to_bits(), "component {id:?}");
                    }
                }
            }
            for top in [0., 500., 5_000., 20_000.] {
                assert_eq!(
                    editor.components.active_heading(top, 800.),
                    rebuilt.active_heading(top, 800.)
                );
                assert_eq!(
                    editor.components.visible_range(
                        &editor.visual_lines,
                        &editor.paint_order,
                        top,
                        top + 800.
                    ),
                    rebuilt.visible_range(
                        &editor.visual_lines,
                        &editor.paint_order,
                        top,
                        top + 800.
                    )
                );
            }
            editor.document.undo().unwrap();
            editor.refresh_projection();
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn nested_table_boundary_typing_matches_full_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let outer_source =
            include_str!("../../../performance/layout-fixtures/34-container-boundaries.md");
        let inner_source =
            include_str!("../../../performance/layout-fixtures/35-rich-table-cells.md");
        let cases = ["Setting", "3", "Name", "Waiting", "Check", "Required"]
            .into_iter()
            .map(|marker| (outer_source, marker))
            .chain(
                [
                    "Quoted cell text.",
                    "Quote neighbor.",
                    "First cell item",
                    "Second cell item",
                    "Cell notice text.",
                    "Nested quote in a quoted table.",
                    "First numbered cell item",
                    "Second numbered cell item",
                    "Numbered inside and outside.",
                ]
                .into_iter()
                .map(|marker| (inner_source, marker)),
            );
        for (source, marker) in cases {
            let (editor, cx) = cx.add_window_view(|window, cx| {
                RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
            });
            editor.update(cx, |editor, _| {
                editor.layout_width = 1000.;
                editor.refresh_projection();
                let node = editor
                    .projection
                    .segments()
                    .iter()
                    .find(|segment| {
                        segment.context.table_cell.is_some()
                            && &editor.projection.text()[segment.projection_range()] == marker
                    })
                    .unwrap()
                    .node_id;
                let (table, row, _) = editor
                    .projection
                    .segments()
                    .iter()
                    .find(|segment| segment.node_id == node)
                    .unwrap()
                    .context
                    .table_cell
                    .unwrap();
                let row_segments = editor
                    .projection
                    .segments()
                    .iter()
                    .filter(|segment| {
                        segment
                            .context
                            .table_cell
                            .is_some_and(|(id, r, _)| id == table && r == row)
                    })
                    .count();
                editor.selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
                    node,
                    0,
                    Affinity::Downstream,
                )));
                let transaction = editor
                    .apply_command(EditCommand::ReplaceSelection {
                        text: "additional wrapped cell text ".repeat(4),
                        typing: true,
                    })
                    .unwrap();
                let selection = editor.selection.clone();
                let edited = editor.document.snapshot().serialize().unwrap();
                let counts = diagnostics::MeasurementScope::new();
                editor.refresh_after_transaction(&transaction);
                let work = counts.take_stage();
                drop(counts);
                assert_eq!(
                    work.geometry_requests, row_segments as u64,
                    "only the edited row is rebuilt: {marker}: {work:?}"
                );
                let expected = build_measured_visual_lines(
                    &editor.projection,
                    &HashMap::new(),
                    1000.,
                    &editor.adaptive,
                    Some(&editor.measurement),
                );
                assert_eq!(editor.visual_lines.len(), expected.len());
                for (actual, expected) in editor.visual_lines.iter().zip(&expected) {
                    assert_eq!(actual.projected_range(), expected.projected_range());
                    for (actual, expected) in [
                        (actual.y, expected.y),
                        (actual.table_row_y, expected.table_row_y),
                        (actual.table_row_height, expected.table_row_height),
                    ] {
                        assert!(
                            (actual - expected).abs() < 0.1,
                            "{marker}: {actual} vs {expected}"
                        );
                    }
                }
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), edited);
                editor.document.undo().unwrap();
                editor.refresh_projection();
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        }
    }

    #[gpui::test]
    fn table_width_lock_survives_reflow_but_not_blur_or_zoom(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source =
            "# Table\n\n| Key | Value |\n| :-- | --: |\n| Name | Short |\n\nAfter table.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        editor.update(cx, |editor, cx| {
            editor.layout_width = 1280.;
            editor.refresh_projection();
            let segment = editor
                .projection
                .segments()
                .iter()
                .find(|s| &editor.projection.text()[s.projection_range()] == "Short")
                .unwrap();
            let node = segment.node_id;
            let table = segment.context.table_cell.unwrap().0;
            editor.selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
                node,
                1,
                Affinity::Downstream,
            )));
            editor.layout_focus = Some(node);
            let original = editor.projection.fitted_table_widths(table, 1280.).unwrap();
            let result = editor
                .apply_command(EditCommand::ReplaceSelection {
                    text: "reflow preserves long text 東京 🎉 ".repeat(3),
                    typing: true,
                })
                .unwrap();
            editor.refresh_after_transaction(&result);
            let snapshot = editor.document.snapshot();
            let edited_source = snapshot.serialize().unwrap();
            assert_eq!(
                editor.projection.block(table).unwrap().plain_text(),
                snapshot.node(table).unwrap().plain_text()
            );
            let prepare = |lock| {
                PreparedDocumentView::prepare_snapshot_with_images(
                    &snapshot,
                    &HashMap::new(),
                    None,
                    ReflowViewport {
                        published_geometry: None,
                        width: 1280.,
                        height: 1000.,
                        zoom: 1.,
                        preview_edit_node: None,
                        expanded_code_tail: None,
                        editing_node: Some(node),
                        table_layout_lock: lock,
                        html_disclosures: Arc::default(),
                        html_loaded_images: Arc::default(),
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: None,
                        resource_generation: 0,
                    },
                    &editor.adaptive,
                    &editor.measurement,
                )
                .0
            };
            let locked = prepare(editor.projection.table_layout_lock.clone());
            assert_eq!(
                locked.projection.fitted_table_widths(table, 1280.).unwrap(),
                original
            );
            assert_eq!(locked.projection.text(), editor.projection.text());
            let released = prepare(None);
            assert!(
                released
                    .projection
                    .fitted_table_widths(table, 1280.)
                    .unwrap()[1]
                    > original[1]
            );
            assert_eq!(
                editor.document.snapshot().serialize().unwrap(),
                edited_source
            );
            editor.projection.lock_table_for_node(None);
            assert!(!editor.projection.table_layout_is_locked(table));
            // Leaving a changed table must not leak its old measured widths
            // through the unchanged-table cache into a fresh projection.
            let mut fresh = TextProjection::from_snapshot(&snapshot);
            fresh.reuse_table_measurements(&editor.projection);
            assert!(fresh.table_measurements(table).is_none());
            editor.projection.lock_table_for_node(Some(node));
            assert!(editor.projection.table_layout_is_locked(table));
            editor.set_zoom_factor(2., cx);
            assert!(!editor.projection.table_layout_is_locked(table));
            editor.document.undo().unwrap();
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn adaptive_typing_measures_only_the_affected_row(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = format!(
            "{}\n\n## Long following chapter\n\n{}",
            include_str!("../../../performance/layout-fixtures/14-edit-lock.md"),
            (0..500)
                .map(|i| format!(
                    "Paragraph {i}. {}\n\n",
                    "Unrelated trailing content stays readable and retains its original order. "
                        .repeat(4)
                ))
                .collect::<String>()
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        editor.update(cx, |editor, _| {
            editor.layout_width = 1280.;
            editor.refresh_projection();
            // A real measured row, not a manually injected slot.
            editor.adaptive = build_measured_adaptive_plan(
                &editor.projection,
                1280.,
                1000.,
                None,
                false,
                &editor.measurement,
            );
            editor.visual_lines = Arc::new(build_measured_visual_lines(
                &editor.projection,
                &HashMap::new(),
                1280.,
                &editor.adaptive,
                Some(&editor.measurement),
            ));
            editor.refresh_visual_index();
            let node = editor
                .projection
                .segments()
                .iter()
                .find(|segment| {
                    editor
                        .projection
                        .block(segment.node_id)
                        .is_some_and(|block| block.plain_text() == "A short explanation.")
                })
                .unwrap()
                .node_id;
            assert!(
                editor
                    .adaptive
                    .slots
                    .get(&node)
                    .is_some_and(|slot| slot.columns == 3)
            );
            let original_slots = editor.adaptive.slots.clone();
            editor.selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
                node,
                2,
                Affinity::Downstream,
            )));
            let transaction = editor
                .apply_command(EditCommand::ReplaceSelection {
                    text: "longer edited text ".repeat(10),
                    typing: true,
                })
                .unwrap();
            let scope = diagnostics::MeasurementScope::new();
            editor.refresh_after_transaction(&transaction);
            let counts = scope.take_stage();
            drop(scope);
            assert!(
                counts.wrap_requests <= 12,
                "typing must not measure the 500 unrelated paragraphs: {counts:?}"
            );
            assert_eq!(editor.adaptive.slots, original_slots);
            assert_eq!(
                counts.wrap_requests, 1,
                "only the changed peer paragraph is wrapped"
            );
            assert_eq!(counts.geometry_requests, 6);
            assert_eq!(counts.geometry_cache_hits, 5);
            let expected = build_measured_visual_lines(
                &editor.projection,
                &HashMap::new(),
                1280.,
                &editor.adaptive,
                Some(&editor.measurement),
            );
            assert_eq!(editor.visual_lines.len(), expected.len());
            for (actual, expected) in editor.visual_lines.iter().zip(&expected) {
                assert_eq!(actual.projected_range(), expected.projected_range());
                assert_eq!(actual.slot, expected.slot);
                assert!(
                    (actual.y - expected.y).abs() < 0.1,
                    "row refresh y {} vs {}",
                    actual.y,
                    expected.y
                );
                assert!((actual.x_fraction - expected.x_fraction).abs() < 0.001);
                assert!((actual.width_fraction - expected.width_fraction).abs() < 0.001);
                assert!((actual.table_row_height - expected.table_row_height).abs() < 0.1);
            }
            editor.document.undo().unwrap();
            editor.refresh_projection();
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn retained_row_typing_matches_full_geometry_for_grids_leads_and_zoom(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = concat!(
            "# Title\n\nA lead paragraph.\n\n## Peers\n\n",
            "### Alpha\n\nFirst explanation.\n\n### Beta\n\nSecond explanation.\n\n",
            "### Gamma\n\nThird explanation.\n\n## Points\n\n",
            "- **North:** latitude\n- **South:** coast\n- **East:** sunrise\n",
            "- **West:** sunset\n- **Above:** sky\n- **Below:** earth\n\n",
            "## Example\n\nRead this example.\n\n```rust\nlet value = 1;\n```\n\n",
            "## Properties\n\nCheck these properties to understand the current document state. The reference beside this explanation gives each property's name and value. Keep the original values intact while editing the surrounding explanation so the pair retains its meaning.\n\n| Name | Value |\n| --- | --- |\n| State | Ready |\n\n",
            "## Formula\n\nFollowing formula $x^2$ remains attached.\n\nEnd.\n",
            "\n",
            include_str!("../../../performance/layout-fixtures/79-technical-sections.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/80-supporting-notes.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/81-supporting-note-pair.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/53-document-grammar.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/85-short-list-counts.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/86-independent-pair.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/87-balanced-list-rows.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/88-short-feature-columns.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/89-inline-enumerations.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/90-checklist-strips.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/91-nested-reading-measures.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/93-mixed-tree.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/94-enclosed-trees.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/95-code-first-tree.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/96-container-first-tree.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/109-unlabeled-tree.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/110-margin-note-cluster.md"),
            "\n",
            include_str!("../../../performance/layout-fixtures/112-technical-continuations.md"),
        );
        for width in [360., 1280.] {
            for zoom in [1., 1.5, 2.] {
                for target in [
                    "The history setting",
                    "The autosave option",
                    "Margin note: The instrument",
                    "Margin note: Weather observations",
                    "Margin note: Preserve the original",
                    "A lead",
                    "Keep evidence together:",
                    "Keep examples literal:",
                    "Sources checked",
                    "Notes saved",
                    "Calibration notes",
                    "Comparison notes",
                    "Keep this explanation",
                    "let reading",
                    "Temperature",
                    "Retain the original observation",
                    "Quoted evidence keeps",
                    "Enclosed guidance keeps",
                    "let accepted",
                    "Code-led explanation",
                    "Figure-led explanation",
                    "Evidence heading",
                    "The observer recorded",
                    "Keep the original readings",
                    "Reading",
                    "Note-led explanation",
                    "Table-led explanation",
                    "Unlabeled branches",
                    "Return to the first blank parent.",
                    "Field notes",
                    "Practical constraints",
                    "Review decisions",
                    "Compare the recorded decisions.",
                    "Talk through an open question.",
                    "A question for further research.",
                    "A decision with its explanation.",
                    "First explanation",
                    "Second explanation",
                    "North:",
                    "West:",
                    "Read this",
                    "let value",
                    "Check these",
                    "Name",
                    "State",
                    "Ready",
                    "The configuration keeps",
                    "Save these values",
                    "[document]",
                    "appearance",
                    "Follow the desktop",
                    "Start with the document",
                    "The document remains an ordinary",
                    "Read the explanation in context",
                    "Review one section at a time",
                    "The original files remain",
                    "Review one section at a time. Check the result",
                    "Establish a shared visual grammar.",
                    "Plain text remains authoritative.",
                    "Field notes from an observation.",
                    "Review notes identifying changes.",
                    "A collection of review decisions.",
                    "Field notes preserve details",
                ] {
                    let deep_target = matches!(
                        target,
                        "Calibration notes"
                            | "Comparison notes"
                            | "Keep this explanation"
                            | "let reading"
                            | "Temperature"
                            | "Retain the original observation"
                            | "Quoted evidence keeps"
                            | "Enclosed guidance keeps"
                            | "let accepted"
                            | "Code-led explanation"
                            | "Figure-led explanation"
                            | "Evidence heading"
                            | "The observer recorded"
                            | "Keep the original readings"
                            | "Reading"
                            | "Note-led explanation"
                            | "Table-led explanation"
                            | "Unlabeled branches"
                            | "Return to the first blank parent."
                    );
                    let margin_target = target.starts_with("Margin note:");
                    if width < 1280.
                        && !deep_target
                        && !margin_target
                        && !matches!(target, "The history setting" | "The autosave option")
                    {
                        continue;
                    }
                    let (editor, cx) = cx.add_window_view(|window, cx| {
                        RichDocumentEditor::new(
                            Document::from_markdown(source).unwrap(),
                            window,
                            cx,
                        )
                    });
                    editor.update(cx, |editor, cx| {
                        editor.layout_width = width * zoom;
                        editor.zoom_factor = zoom;
                        editor.measurement = Arc::new(FontMeasurement::new(
                            cx.text_system().clone(),
                            "Public Sans Tachyon".into(),
                            zoom,
                        ));
                        editor.measurement.measure_tables(&mut editor.projection);
                        editor.adaptive = build_measured_adaptive_plan(
                            &editor.projection,
                            width,
                            1000.,
                            None,
                            false,
                            &editor.measurement,
                        );
                        editor.visual_lines = Arc::new(build_measured_visual_lines(
                            &editor.projection,
                            &HashMap::new(),
                            width,
                            &editor.adaptive,
                            Some(&editor.measurement),
                        ));
                        scale_visual_lines(
                            Arc::make_mut(&mut editor.visual_lines).as_mut_slice(),
                            zoom,
                        );
                        editor.refresh_visual_index();
                        let node = editor
                            .projection
                            .segments()
                            .iter()
                            .find(|s| {
                                editor
                                    .projection
                                    .block(s.node_id)
                                    .is_some_and(|b| b.plain_text().starts_with(target))
                                    && (!deep_target || s.context.list_depth > 1)
                                    && (target != "Reading"
                                        || s.context.table_cell.is_some_and(|(table, _, _)| {
                                            s.context.list_item_container == Some(table)
                                        }))
                            })
                            .unwrap()
                            .node_id;
                        assert!(
                            editor.adaptive.slots.contains_key(&node)
                                || editor
                                    .projection
                                    .segment_for_node(node)
                                    .unwrap()
                                    .context
                                    .task_list_depth
                                    > 0
                                || (width < 1280.
                                    && matches!(
                                        target,
                                        "The history setting" | "The autosave option"
                                    ))
                                || (margin_target
                                    && editor
                                        .projection
                                        .segment_for_node(node)
                                        .unwrap()
                                        .context
                                        .margin_note_anchor
                                        .is_some())
                                || editor.adaptive.inline_lists.contains_key(&node)
                                || editor.adaptive.lead == Some(node)
                                || (deep_target
                                    && editor
                                        .projection
                                        .segment_for_node(node)
                                        .unwrap()
                                        .context
                                        .list_depth
                                        > 1),
                            "fixture must exercise an arranged node: {target}"
                        );
                        editor.selection = Selection::Text(TextSelection::caret(
                            DocumentPosition::new(node, 1, Affinity::Downstream),
                        ));
                        let original_lines = editor.visual_lines.clone();
                        let original_text = editor.projection.text().to_owned();
                        let result = editor
                            .apply_command(EditCommand::ReplaceSelection {
                                text: "growing text ".repeat(16),
                                typing: true,
                            })
                            .unwrap();
                        assert_eq!(result.text_changed_node, Some(node));
                        let scope = diagnostics::MeasurementScope::new();
                        editor.refresh_after_transaction(&result);
                        let counts = scope.take_stage();
                        drop(scope);
                        assert!(
                            counts.wrap_requests <= 8,
                            "bounded transaction path must handle {target} at {zoom}: {counts:?}"
                        );
                        let mut projection =
                            TextProjection::from_snapshot(&editor.document.snapshot());
                        projection.preview_edit_node = editor.projection.preview_edit_node;
                        projection.table_layout_lock = editor.projection.table_layout_lock.clone();
                        projection.reuse_table_measurements(&editor.projection);
                        projection.retain_quote_roles(&editor.adaptive.quote_roles, Some(node));
                        let mut expected = build_measured_visual_lines(
                            &projection,
                            &HashMap::new(),
                            width,
                            &editor.adaptive,
                            Some(&editor.measurement),
                        );
                        scale_visual_lines(&mut expected, zoom);
                        assert_eq!(*editor.paint_order, visual_line_paint_order(&expected));
                        assert_eq!(
                            editor.visual_lines.len(),
                            expected.len(),
                            "{target} at {zoom}"
                        );
                        for (actual, expected) in editor.visual_lines.iter().zip(&expected) {
                            assert_eq!(
                                actual.projected_range(),
                                expected.projected_range(),
                                "{target} at {zoom}"
                            );
                            assert_eq!(actual.compact_tree, expected.compact_tree);
                            assert_eq!(actual.inset, expected.inset);
                            assert_eq!(actual.slot, expected.slot);
                            for (a, b) in [
                                (actual.y, expected.y),
                                (actual.table_row_y, expected.table_row_y),
                                (actual.table_row_height, expected.table_row_height),
                                (actual.gap_before, expected.gap_before),
                                (actual.style.font_size, expected.style.font_size),
                                (actual.style.space_above, expected.style.space_above),
                                (actual.style.space_below, expected.style.space_below),
                            ] {
                                assert!((a - b).abs() < 0.1, "{target} at {zoom}: {a} != {b}");
                            }
                            assert_eq!(
                                actual.inline_math.as_ref().map(|m| (
                                    &m.range,
                                    m.attachments.iter().map(|a| &a.range).collect::<Vec<_>>()
                                )),
                                expected.inline_math.as_ref().map(|m| (
                                    &m.range,
                                    m.attachments.iter().map(|a| &a.range).collect::<Vec<_>>()
                                ))
                            );
                        }
                        editor.selection = Selection::Text(TextSelection {
                            anchor: DocumentPosition::new(node, 1, Affinity::Downstream),
                            head: DocumentPosition::new(
                                node,
                                1 + "growing text ".len() * 16,
                                Affinity::Upstream,
                            ),
                        });
                        let result = editor
                            .apply_command(EditCommand::ReplaceSelection {
                                text: String::new(),
                                typing: false,
                            })
                            .unwrap();
                        let scope = diagnostics::MeasurementScope::new();
                        editor.refresh_after_transaction(&result);
                        assert!(scope.take_stage().wrap_requests <= 8);
                        drop(scope);
                        assert_eq!(editor.projection.text(), original_text);
                        assert_eq!(editor.visual_lines.len(), original_lines.len());
                        for (actual, original) in
                            editor.visual_lines.iter().zip(original_lines.iter())
                        {
                            assert_eq!(actual.projected_range(), original.projected_range());
                            assert_eq!(actual.slot, original.slot);
                            assert!(
                                (actual.y - original.y).abs() < 0.1,
                                "shrink {target} at {zoom}"
                            );
                            assert_eq!(
                                actual.inline_math.as_ref().map(|m| &m.range),
                                original.inline_math.as_ref().map(|m| &m.range)
                            );
                        }
                        assert_eq!(
                            *editor.paint_order,
                            visual_line_paint_order(&original_lines)
                        );
                    });
                }
            }
        }
    }

    #[gpui::test]
    fn linked_figure_uses_the_existing_editor_link_target_route(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "[![Open figure](thumb.png)](https://example.test/full \"Full figure\")\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        editor.update(cx, |editor, _| {
            let segment = editor.projection.image_segments().next().unwrap();
            assert_eq!(
                editor.link_at_offset(segment.projection_start()).as_deref(),
                Some("https://example.test/full")
            );
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn structural_edit_does_not_measure_unvisited_chapters(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let original = include_str!("../../../performance/layout-fixtures/17-long-layout-spec.md");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(original).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                // A structural paste before the first paint: all later chapters
                // are legitimately unvisited and no cached widths can mask work.
                let counts = diagnostics::MeasurementScope::new();
                let result = editor
                    .apply_command(EditCommand::PasteMarkdown {
                        markdown: "Inserted paragraph.\n\nAnother paragraph.\n".into(),
                    })
                    .unwrap();
                assert!(result.text_changed_node.is_none());
                let after_command = editor.document.snapshot().serialize().unwrap();
                editor.refresh_after_transaction(&result);
                let measured = counts.take_stage();
                assert_eq!(
                    editor.document.snapshot().serialize().unwrap(),
                    after_command
                );
                eprintln!("structural refresh measurements: {measured:?}");
                assert!(
                    measured.tables_measured <= 4,
                    "unvisited tables must remain deferred: {measured:?}"
                );
                assert!(
                    measured.wrap_requests < 150,
                    "only nearby geometry should be shaped: {measured:?}"
                );
                assert!(measured.deferred_text_segments > 0);
                let key = editor.adaptive.geometry_key();
                let mut deferred = editor.adaptive.clone();
                deferred.edit_geometry_ranges.clear();
                assert!(
                    !key.matches(&deferred),
                    "geometry-only edit windows participate in cache identity"
                );
                assert!(editor.projection.text().contains("20. Synthetic section"));
                let rendered: HashSet<_> = editor
                    .visual_lines
                    .iter()
                    .filter_map(|line| {
                        editor
                            .projection
                            .segment_for_range(&line.projected_range())
                            .map(|segment| segment.node_id)
                    })
                    .collect();
                assert!(
                    editor
                        .projection
                        .segments()
                        .iter()
                        .all(|segment| rendered.contains(&segment.node_id)),
                    "deferred geometry must still render every canonical segment"
                );
                assert!(
                    editor.visual_lines.last().unwrap().projected_end()
                        > editor.projection.text().len() / 2
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
            });
        });
    }

    #[gpui::test]
    fn structural_edit_measures_a_new_offscreen_caret_without_intervening_chapters(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let original = include_str!("../../../performance/layout-fixtures/17-long-layout-spec.md");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(original).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let last = editor
                    .projection
                    .roots()
                    .filter(|block| matches!(block, BlockNode::Paragraph(_)))
                    .last()
                    .unwrap()
                    .id();
                editor.selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
                    last,
                    0,
                    Affinity::Downstream,
                )));
                let counts = diagnostics::MeasurementScope::new();
                let result = editor
                    .apply_command(EditCommand::PasteMarkdown {
                        markdown: "New distant paragraph.\n\nSecond distant paragraph.\n".into(),
                    })
                    .unwrap();
                editor.refresh_after_transaction(&result);
                let measured = counts.take_stage();
                assert!(
                    measured.tables_measured <= 6,
                    "only first and last windows: {measured:?}"
                );
                let Selection::Text(selection) = &editor.selection else {
                    panic!("text caret");
                };
                let target = editor
                    .projection
                    .segment_for_node(selection.head.node_id)
                    .unwrap();
                assert!(
                    editor
                        .adaptive
                        .has_measured_geometry(target.top_level_node_id)
                );
                let middle = editor
                    .projection
                    .roots()
                    .nth(editor.projection.roots().len() / 2)
                    .unwrap()
                    .id();
                assert!(!editor.adaptive.has_measured_geometry(middle));
                let actual: Vec<_> = editor
                    .visual_lines
                    .iter()
                    .filter(|line| {
                        target.projection_start() <= line.projected_start()
                            && line.projected_end() <= target.projection_end()
                    })
                    .map(|line| (line.projected_range(), line.style.line_height))
                    .collect();
                let mut full = editor.adaptive.clone();
                full.measurement_ranges = None;
                let oracle = build_measured_visual_lines(
                    &editor.projection,
                    &editor.image_layout_dimensions,
                    editor.layout_width,
                    &full,
                    Some(&editor.measurement),
                );
                let expected: Vec<_> = oracle
                    .iter()
                    .filter(|line| {
                        target.projection_start() <= line.projected_start()
                            && line.projected_end() <= target.projection_end()
                    })
                    .map(|line| (line.projected_range(), line.style.line_height))
                    .collect();
                assert!(!actual.is_empty());
                assert_eq!(
                    actual, expected,
                    "new caret uses exact native wrapping, not the deferred estimate"
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
            });
        });
    }

    #[gpui::test]
    fn rectangular_table_refresh_measures_its_offscreen_window(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let original = include_str!("../../../performance/layout-fixtures/17-long-layout-spec.md");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(original).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let table = editor
                    .projection
                    .roots()
                    .filter(|root| matches!(root, BlockNode::Table(_)))
                    .last()
                    .unwrap()
                    .id();
                editor.layout_focus = None;
                editor.selection = Selection::Table(RectangularSelection {
                    table_id: table,
                    anchor_row: 1,
                    anchor_column: 0,
                    head_row: 1,
                    head_column: 1,
                });
                let result = editor
                    .apply_command(EditCommand::InsertTableRow {
                        table_id: table,
                        index: 2,
                    })
                    .unwrap();
                let selection = editor.selection.clone();
                assert!(matches!(selection, Selection::Table(_)));
                let after_command = editor.document.snapshot().serialize().unwrap();
                let counts = diagnostics::MeasurementScope::new();
                editor.refresh_after_transaction(&result);
                let measured = counts.take_stage();
                assert!(
                    editor.projection.table_measurements(table).is_some(),
                    "rectangular selection must include its table even without a text focus: {measured:?}"
                );
                assert!(measured.tables_measured <= 6);
                assert!(editor.adaptive.has_measured_geometry(table));
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), after_command);
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
            });
        });
    }

    #[test]
    fn alert_borders_join_in_one_shape_with_fixed_equal_corners() {
        for zoom in [MIN_ZOOM, 1., 1.25, MAX_ZOOM] {
            for (width, height) in [(240., 80.), (960., 80.), (240., 320.)] {
                let bounds = Bounds::new(
                    point(px(20.), px(30.)),
                    size(px(width * zoom), px(height * zoom)),
                );
                let mut chrome = Vec::new();
                append_alert_chrome(
                    &mut chrome,
                    bounds,
                    TachyonPalette::LIGHT.signal(&AlertKind::Note),
                    zoom,
                );
                assert_eq!(
                    chrome.len(),
                    1,
                    "the accent rail must belong to the rounded border, not overlap it"
                );
                let quad = &chrome[0].quad;
                assert_eq!(quad.bounds, bounds);
                assert_eq!(quad.corner_radii, gpui::Corners::all(px(4. * zoom)));
                assert_eq!(quad.border_widths.left, px(zoom));
                assert_eq!(quad.border_widths.top, px(zoom));
                assert_eq!(quad.border_widths.right, px(zoom));
                assert_eq!(quad.border_widths.bottom, px(zoom));
            }
        }
    }

    #[test]
    fn a_table_ancestors_number_badge_stays_outside_its_first_cell() {
        for zoom in [MIN_ZOOM, 1., 1.25, MAX_ZOOM] {
            let table_left = px(32. * zoom);
            let text = Bounds::new(
                point(table_left + px(20. * zoom), px(50.)),
                size(px(300.), px(23. * zoom)),
            );
            let badge = list_marker_bounds(text, true, true, Some(table_left), zoom);
            assert!(badge.left() >= px(0.));
            assert!(
                badge.right() < table_left,
                "outer marker must not intersect the cell or be clipped by it: {badge:?}"
            );
            assert_eq!(badge.center().y, text.center().y);
        }
    }

    #[test]
    fn step_numbers_are_centered_on_the_first_text_line() {
        for zoom in [MIN_ZOOM, 1., 1.25, MAX_ZOOM] {
            for height in [23., 30., 42.] {
                let line = Bounds::new(point(px(80.), px(40.)), size(px(300.), px(height * zoom)));
                let badge = step_number_bounds(line, zoom);
                assert!(
                    f32::from(badge.center().y - line.center().y).abs() < 0.001,
                    "circle must be centered on the first line at height {height}, zoom {zoom}"
                );
                assert_eq!(badge.size, size(px(25. * zoom), px(25. * zoom)));
            }
        }
    }

    #[test]
    fn numbered_lists_reserve_gap_for_markers_and_continuation_text() {
        let source = "1. **Plan**\n\n   Clarify goals and scope.\n\n2. **Draft**\n\n   Turn ideas into structure.\n\n- A bullet\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let segments = projection.segments();
        assert_eq!(segments.len(), 5);
        for segment in &segments[..4] {
            assert_eq!(segment.context.ordered_list_depth, 1);
            assert_eq!(container_inset(segment), 32.);
        }
        assert_eq!(segments[4].context.ordered_list_depth, 0);
        assert_eq!(container_inset(&segments[4]), 24.);
        for zoom in [MIN_ZOOM, 1., 1.25, MAX_ZOOM] {
            let text = Bounds::new(
                point(px(40. * zoom), px(20.)),
                size(px(300.), px(30. * zoom)),
            );
            let badge = step_number_bounds(text, zoom);
            assert!((f32::from(text.left() - badge.right()) - 11. * zoom).abs() < 0.001);
            assert!(
                (f32::from(badge.left()) - 4. * zoom).abs() < 0.001,
                "the extra gap must not move the number rail or clip its circles"
            );
        }
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn checkbox_icons_stay_centered_and_clear_of_text_at_every_zoom() {
        for zoom in [MIN_ZOOM, 1., 1.25, MAX_ZOOM] {
            let line = Bounds::new(
                point(px(100.), px(40.)),
                size(px(400.), px(LINE_HEIGHT * zoom)),
            );
            let checkbox = task_checkbox_bounds(line, zoom);
            assert_eq!(checkbox.size, size(px(18. * zoom), px(18. * zoom)));
            assert!((f32::from(checkbox.center().y - line.center().y)).abs() < 0.001);
            assert_eq!(line.left() - checkbox.right(), px(13. * zoom));
            // The task list reserves a 32 px marker gutter; do not paint outside
            // its content mask (especially the unchecked box's left stroke).
            assert!(checkbox.left() >= line.left() - px(32. * zoom));
            assert!(checkbox.dilate(px(4. * zoom)).right() < line.left());
        }
    }

    #[test]
    fn rich_clipboard_wins_over_tabular_plain_text() {
        let metadata = RichClipboard::new("**left** | right", "left\tright")
            .to_json()
            .expect("rich clipboard JSON");
        let item = ClipboardItem::new_string_with_metadata("left\tright".into(), metadata);

        assert_eq!(
            clipboard_paste(&item),
            Some(ClipboardPaste::RichMarkdown("**left** | right".into()))
        );
    }

    #[test]
    fn invalid_rich_clipboard_falls_back_to_plain_text() {
        let item = ClipboardItem::new_string_with_metadata(
            "left\tright".into(),
            "{\"version\":99}".into(),
        );

        assert_eq!(
            clipboard_paste(&item),
            Some(ClipboardPaste::PlainText("left\tright".into()))
        );
    }
    use crate::init_editor;

    #[gpui::test]
    fn vertical_wheel_moves_the_document_viewport(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let markdown = (0..200)
            .map(|index| format!("Paragraph {index} with enough text to remain readable."))
            .collect::<Vec<_>>()
            .join("\n\n");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(markdown).expect("scroll fixture"),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let before = editor.read_with(cx, |editor, _| editor.scroll_metrics().0);
        let (document_height, viewport_height) = editor.read_with(cx, |editor, _| {
            (editor.document_height, editor.scroll_metrics().1)
        });

        cx.simulate_event(ScrollWheelEvent {
            position: point(px(200.), px(200.)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-180.))),
            ..Default::default()
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });

        let after = editor.read_with(cx, |editor, _| editor.scroll_metrics().0);
        assert!(
            after > before,
            "vertical wheel must advance the viewport (before={before}, after={after}, document={document_height}, viewport={viewport_height})"
        );
    }

    #[gpui::test]
    fn pixel_coast_advances_on_the_next_frame_without_a_release_timeout(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Paragraph.\n\n".repeat(200)).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                editor.on_scroll_wheel(
                    &ScrollWheelEvent {
                        position: point(px(200.), px(200.)),
                        delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-120.))),
                        touch_phase: gpui::TouchPhase::Moved,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                editor.momentum_frame_time = Some(Instant::now() - Duration::from_millis(16));
            });
            let before = editor.read(cx).scroll_metrics().0;
            // Deliver a native frame without pumping background timers. Input
            // must not require a release timeout to make forward progress.
            window.simulate_next_frame(cx);
            assert!(
                editor.read(cx).scroll_metrics().0 > before,
                "the frame following input must advance the coast"
            );
        });
    }

    #[gpui::test]
    fn repeated_pixel_input_keeps_one_frame_chain_and_credits_predicted_travel(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Paragraph.\n\n".repeat(200)).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            let event = ScrollWheelEvent {
                position: point(px(200.), px(200.)),
                delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-120.))),
                touch_phase: gpui::TouchPhase::Moved,
                ..Default::default()
            };
            editor.update(cx, |editor, cx| {
                editor.on_scroll_wheel(&event, window, cx);
                let previous = Instant::now() - Duration::from_millis(16);
                editor.momentum_frame_time = Some(previous);
                editor.momentum_last_input = Some(previous);
            });
            window.simulate_next_frame(cx);
            editor.update(cx, |editor, cx| {
                assert!(editor.scroll_metrics().0 > 120.);
                let clock = editor.momentum_frame_time;
                let generation = editor.momentum_generation;
                editor.on_scroll_wheel(&event, window, cx);
                assert_eq!(
                    editor.momentum_frame_time, clock,
                    "input must not restart the clock"
                );
                assert_eq!(editor.momentum_generation, generation);
                assert!(
                    (editor.scroll_metrics().0 - 240.).abs() < 0.01,
                    "predicted travel must not be added to finger displacement twice"
                );

                // A tiny delta following a prediction must not snap backwards.
                editor.momentum_extrapolated = Some(-30.);
                let before = editor.scroll_metrics().0;
                let small = ScrollWheelEvent {
                    delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-2.))),
                    ..event.clone()
                };
                editor.on_scroll_wheel(&small, window, cx);
                assert_eq!(editor.scroll_metrics().0, before);
                assert_eq!(editor.momentum_extrapolated, Some(-28.));
            });
        });
    }

    #[gpui::test]
    fn repeated_wheel_input_joins_the_frame_chain_and_cancelled_frames_stay_cancelled(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Paragraph.\n\n".repeat(200)).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            let event = ScrollWheelEvent {
                delta: gpui::ScrollDelta::Lines(point(0., -1.)),
                touch_phase: gpui::TouchPhase::Moved,
                ..Default::default()
            };
            editor.update(cx, |editor, cx| {
                editor.on_scroll_wheel(&event, window, cx);
                let before = editor.scroll_metrics().0;
                let remaining = editor.momentum_remaining;
                editor.on_scroll_wheel(&event, window, cx);
                assert_eq!(
                    editor.scroll_metrics().0,
                    before,
                    "repeated input must not insert extra integration steps between frames"
                );
                assert!((editor.momentum_remaining - (remaining - LINE_HEIGHT)).abs() < 0.01);
                editor.stop_momentum();
                editor.on_scroll_wheel(&event, window, cx);
                // Both the old cancelled callback and the new one are queued.
                editor.momentum_frame_time = Some(Instant::now() - Duration::from_millis(16));
            });
            let before = editor.read(cx).scroll_metrics().0;
            window.simulate_next_frame(cx);
            assert!(
                editor.read(cx).scroll_metrics().0 > before,
                "an old callback must not cancel a newly started gesture"
            );
            editor.update(cx, |editor, _| editor.stop_momentum());
            let stopped = editor.read(cx).scroll_metrics().0;
            window.simulate_next_frame(cx);
            assert_eq!(editor.read(cx).scroll_metrics().0, stopped);
        });
    }

    #[gpui::test]
    fn wayland_pixel_scrolling_coasts_with_a_visible_decelerating_tail(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Paragraph.\n\n".repeat(200)).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        // The pinned Wayland backend emits Pixels + Moved for continuous
        // devices, without generating any kinetic events after release.
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(200.), px(200.)),
            delta: gpui::ScrollDelta::Pixels(point(px(0.), px(-120.))),
            touch_phase: gpui::TouchPhase::Moved,
            ..Default::default()
        });
        editor.update(cx, |editor, cx| {
            assert!(
                editor.scroll_metrics().0 > 0.,
                "input must respond immediately"
            );
            assert!(
                editor.momentum_remaining < -10.,
                "continuous input must retain a visible coast after release"
            );
            let mut distances = Vec::new();
            for _ in 0..8 {
                let before = editor.scroll_metrics().0;
                editor.advance_momentum(0.1, cx);
                distances.push(editor.scroll_metrics().0 - before);
            }
            assert!(
                distances[3] > 2.,
                "motion must still be visible 400 ms after release: {distances:?}"
            );
            assert!(
                distances
                    .windows(2)
                    .all(|pair| pair[0] > pair[1] && pair[1] > 0.),
                "the coast must fade gradually: {distances:?}"
            );
        });
    }

    #[gpui::test]
    fn kinetic_input_handles_axis_noise_release_reversal_and_reduced_motion(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Paragraph.\n\n".repeat(200)).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let mut event = ScrollWheelEvent {
                    position: point(px(200.), px(200.)),
                    delta: gpui::ScrollDelta::Pixels(point(px(2.), px(-120.))),
                    ..Default::default()
                };
                editor.on_scroll_wheel(&event, window, cx);
                assert_eq!(
                    editor.scroll_metrics().0,
                    120.,
                    "sideways noise must not discard vertical input"
                );
                let pending = editor.momentum_remaining;
                let generation = editor.jump_generation;
                event.delta = gpui::ScrollDelta::Pixels(point(px(0.), px(0.)));
                event.touch_phase = gpui::TouchPhase::Ended;
                editor.on_scroll_wheel(&event, window, cx);
                assert_eq!(editor.momentum_remaining, pending);
                assert_eq!(
                    editor.jump_generation, generation,
                    "release must not cancel the coast task"
                );
                event.delta = gpui::ScrollDelta::Pixels(point(px(0.), px(24.)));
                event.touch_phase = gpui::TouchPhase::Moved;
                editor.on_scroll_wheel(&event, window, cx);
                assert!(
                    editor.momentum_remaining > 0.,
                    "reversal must discard the old direction"
                );
                let reversed = editor.scroll_metrics().0;
                editor.advance_momentum(0.1, cx);
                assert!(editor.scroll_metrics().0 < reversed);
                event.touch_phase = gpui::TouchPhase::Cancelled;
                editor.on_scroll_wheel(&event, window, cx);
                assert_eq!(editor.momentum_remaining, 0.);
                assert!(editor.momentum_frame_time.is_none());
                cx.set_reduce_motion(true);
                event.touch_phase = gpui::TouchPhase::Moved;
                event.delta = gpui::ScrollDelta::Pixels(point(px(0.), px(-60.)));
                let before = editor.scroll_metrics().0;
                editor.on_scroll_wheel(&event, window, cx);
                assert_eq!(editor.scroll_metrics().0, before + 60.);
                assert_eq!(editor.momentum_remaining, 0.);
                assert!(editor.momentum_frame_time.is_none());
            });
        });
    }

    #[gpui::test]
    fn kinetic_decay_is_refresh_independent_and_stops_at_document_edges(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Paragraph.\n\n".repeat(200)).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        editor.update(cx, |editor, cx| {
            let mut endpoints = Vec::new();
            for hz in [60, 120, 144] {
                editor.scroll_handle.set_offset(point(px(0.), px(-1000.)));
                editor.momentum_remaining = -500.;
                for _ in 0..hz {
                    editor.advance_momentum(1. / hz as f32, cx);
                }
                endpoints.push(editor.scroll_metrics().0);
            }
            assert!(
                endpoints
                    .windows(2)
                    .all(|pair| (pair[0] - pair[1]).abs() < 0.01),
                "{endpoints:?}"
            );
            let maximum = editor.scroll_handle.max_offset().y;
            editor.scroll_handle.set_offset(point(px(0.), -maximum));
            editor.momentum_remaining = -500.;
            assert!(!editor.advance_momentum(0.016, cx));
            assert_eq!(editor.scroll_handle.offset().y, -maximum);
            editor.scroll_handle.set_offset(point(px(0.), px(0.)));
            editor.momentum_remaining = 500.;
            assert!(!editor.advance_momentum(0.016, cx));
            assert_eq!(editor.scroll_handle.offset().y, px(0.));
        });
    }

    #[gpui::test]
    fn long_display_math_has_a_scrollbar_below_its_glyphs(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let expression = (1..=20)
            .map(|index| format!("a_{{{index}}}"))
            .collect::<Vec<_>>()
            .join("+");
        let source = format!("$$\n{expression}=S\n$$\n");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.simulate_resize(size(px(360.), px(600.)));
        for zoom in [0.75, 1., 2.] {
            editor.update(cx, |editor, cx| editor.set_zoom_factor(zoom, cx));
            for _ in 0..2 {
                cx.run_until_parked();
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
            }
            let viewport = cx.debug_bounds("display-math-viewport").unwrap();
            let image = cx.debug_bounds("display-math-image").unwrap();
            assert!(image.size.width > viewport.size.width);
            let scrollbar = cx
                .debug_bounds("display-math-scrollbar")
                .expect("overflow needs a visible pointer affordance");
            assert!(
                scrollbar.top() >= viewport.bottom(),
                "scrollbar must not cover formula glyphs"
            );
            assert_eq!(scrollbar.left(), viewport.left());
            assert_eq!(scrollbar.size.width, viewport.size.width);
            let (node_id, handle) = editor.read_with(cx, |editor, _| {
                let (&node_id, handle) = editor.math_scroll_handles.iter().next().unwrap();
                (node_id, handle.clone())
            });
            assert!(handle.max_offset().x > px(0.));
            editor.update(cx, |editor, cx| {
                editor.set_accessible_math_scroll(node_id, -1000., cx)
            });
            assert_eq!(handle.offset().x, px(0.));
            editor.update(cx, |editor, cx| {
                editor.scroll_math_accessibly(node_id, 1., cx)
            });
            assert!(handle.offset().x < px(0.));
            editor.update(cx, |editor, cx| {
                editor.set_accessible_math_scroll(node_id, f32::MAX, cx)
            });
            assert_eq!(handle.offset().x, -handle.max_offset().x);
            editor.update(cx, |editor, cx| {
                editor.scroll_math_accessibly(node_id, -1., cx)
            });
            assert!(handle.offset().x > -handle.max_offset().x);
            handle.set_offset(point(-handle.max_offset().x, px(0.)));
            cx.update(|window, cx| {
                window.refresh();
                _ = window.draw(cx);
            });
            cx.run_until_parked();
            let end_image = cx.debug_bounds("display-math-image").unwrap();
            assert!(
                (f32::from(end_image.right() - viewport.right())).abs() <= 1.,
                "the final glyph must be reachable without shrinking the formula"
            );
            let offset = handle.offset();
            editor.update(cx, |_, cx| cx.notify());
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            assert_eq!(
                handle.offset(),
                offset,
                "ordinary redraw must retain horizontal position"
            );
            assert_eq!(
                editor.read_with(cx, |editor, _| editor
                    .document
                    .snapshot()
                    .serialize()
                    .unwrap()),
                source
            );
        }
        cx.simulate_resize(size(px(4000.), px(600.)));
        for _ in 0..2 {
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        }
        assert!(
            cx.debug_bounds("display-math-scrollbar").is_none(),
            "fitting formulas need no scrollbar"
        );
        editor.update(cx, |editor, _| {
            assert!(matches!(
                editor.document.undo(),
                Err(DocumentError::NothingToUndo)
            ));
            let id = *editor.math_scroll_handles.keys().next().unwrap();
            editor
                .document
                .apply(EditCommand::DeleteBlock { node_id: id })
                .unwrap();
            editor.refresh_projection();
            assert!(
                editor.math_scroll_handles.is_empty(),
                "deleted formulas must release retained scroll state"
            );
        });
    }

    #[gpui::test]
    fn overflowing_display_math_is_keyboard_focusable_without_editing_source(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let expression = (1..=20)
            .map(|index| format!("a_{{{index}}}"))
            .collect::<Vec<_>>()
            .join("+");
        let source = format!("$$\n{expression}=S\n$$\n");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.simulate_resize(size(px(360.), px(600.)));
        for _ in 0..2 {
            cx.run_until_parked();
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
        }

        let viewport = cx.debug_bounds("display-math-viewport").unwrap();
        let (handle, revision) = editor.read_with(cx, |editor, _| {
            (
                editor.math_scroll_handles.values().next().unwrap().clone(),
                editor.document.snapshot().revision(),
            )
        });
        assert!(handle.max_offset().x > px(0.));
        cx.simulate_click(viewport.center(), gpui::Modifiers::default());
        cx.update(|window, cx| {
            assert_ne!(
                window.focused(cx),
                Some(editor.read(cx).focus_handle.clone())
            );
        });

        cx.simulate_keystrokes("right");
        assert!(handle.offset().x < px(0.));
        cx.simulate_keystrokes("end");
        assert_eq!(handle.offset().x, -handle.max_offset().x);
        cx.simulate_keystrokes("home");
        assert_eq!(handle.offset().x, px(0.));
        cx.simulate_keystrokes("escape");
        cx.update(|window, cx| {
            assert_eq!(
                window.focused(cx),
                Some(editor.read(cx).focus_handle.clone())
            );
        });
        cx.simulate_keystrokes("tab");
        cx.update(|window, cx| {
            assert_ne!(
                window.focused(cx),
                Some(editor.read(cx).focus_handle.clone())
            );
        });
        cx.simulate_keystrokes("right");
        assert!(handle.offset().x < px(0.));
        cx.simulate_keystrokes("escape");

        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.document.snapshot().revision(), revision);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            assert!(matches!(
                editor.document.undo(),
                Err(DocumentError::NothingToUndo)
            ));
        });
    }

    #[gpui::test]
    fn display_fraction_image_fits_its_scroll_viewport(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        for formula in [
            r"\frac{1}{2}",
            r"\frac{1}{\frac{2}{3}}",
            r"\sum_{i=1}^{n} i",
        ] {
            let source = format!("$$\n{formula}\n$$");
            let (editor, cx) = cx.add_window_view(|window, cx| {
                RichDocumentEditor::new(
                    Document::from_markdown(source.as_str()).unwrap(),
                    window,
                    cx,
                )
            });
            let cx: &mut gpui::VisualTestContext = cx;
            for zoom in [0.8, 1., 1.5, 2.] {
                editor.update(cx, |editor, cx| editor.set_zoom_factor(zoom, cx));
                cx.run_until_parked();
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                let viewport = cx.debug_bounds("display-math-viewport").unwrap();
                let image = cx.debug_bounds("display-math-image").unwrap();
                assert!(
                    image.top() >= viewport.top() && image.bottom() <= viewport.bottom(),
                    "{formula} at {zoom}: formula {image:?} is clipped by viewport {viewport:?}"
                );
                let expected_height = editor.read_with(cx, |editor, _| {
                    editor
                        .visual_lines
                        .iter()
                        .find_map(|line| line.display_math.as_ref().map(|math| math.light.height))
                        .unwrap()
                });
                assert!(
                    (f32::from(image.size.height) - expected_height * zoom).abs() <= 1.,
                    "{formula} at {zoom}: image {image:?}, expected height {expected_height}; formula height must not shrink to conceal overflow"
                );
                assert_eq!(
                    editor.read_with(cx, |editor, _| editor
                        .document
                        .snapshot()
                        .serialize()
                        .unwrap()),
                    source
                );
            }
        }
    }

    #[gpui::test]
    fn a_single_wheel_notch_fades_gradually_without_adding_travel(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Paragraph.\n\n".repeat(200)).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(200.), px(200.)),
            delta: gpui::ScrollDelta::Lines(point(0., -1.)),
            ..Default::default()
        });
        editor.update(cx, |editor, cx| {
            let initial = editor.scroll_metrics().0;
            assert!(initial > 0. && initial < LINE_HEIGHT);
            let mut distances = Vec::new();
            for _ in 0..8 {
                let before = editor.scroll_metrics().0;
                editor.advance_momentum(0.1, cx);
                distances.push(editor.scroll_metrics().0 - before);
            }
            assert!(distances.windows(2).all(|pair| pair[0] > pair[1]));
            assert!(
                distances[7] > 1.,
                "even one notch should retain visible motion at 800 ms: {distances:?}"
            );
            for _ in 0..50 {
                editor.advance_momentum(0.1, cx);
            }
            assert_eq!(editor.momentum_remaining, 0.);
            assert!((editor.scroll_metrics().0 - LINE_HEIGHT).abs() < 0.2);
        });
    }

    #[gpui::test]
    fn wheel_momentum_is_bounded_continues_and_cancels_on_edit(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Paragraph.\n\n".repeat(200)).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.simulate_event(ScrollWheelEvent {
            position: point(px(200.), px(200.)),
            delta: gpui::ScrollDelta::Lines(point(0., -3.)),
            ..Default::default()
        });
        editor.update(cx, |editor, cx| {
            let initial = editor.scroll_metrics().0;
            assert!(
                initial > 0. && initial < LINE_HEIGHT * 3.,
                "wheel is not applied twice: {initial}"
            );
            assert!(editor.momentum_remaining < 0.);
            editor.advance_momentum(0.1, cx);
            assert!(editor.scroll_metrics().0 > initial);
            for _ in 0..50 {
                editor.advance_momentum(0.1, cx);
            }
            assert!((editor.scroll_metrics().0 - LINE_HEIGHT * 3.).abs() < 0.2);
            assert_eq!(editor.momentum_remaining, 0.);
            editor.momentum_remaining = 120.;
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.move_to(0, window, cx);
                assert_eq!(editor.momentum_remaining, 0.);
                assert!(editor.momentum_frame_time.is_none());
            });
        });
    }

    #[test]
    fn chapter_prefixes_do_not_style_ordinary_titles() {
        assert_eq!(chapter_prefix_len("9.5 Calendar series"), 3);
        assert_eq!(chapter_prefix_len("12. Security"), 3);
        assert_eq!(chapter_prefix_len("RFC-123 Design"), 0);
        assert_eq!(chapter_prefix_len("2026"), 0);
    }

    #[gpui::test]
    fn wrapped_chapter_titles_hang_under_the_authored_title(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                "## 12. Information architecture and interactions across the complete workspace\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = AdaptivePlan::build(&projection, 420., None, false);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                420.,
                &plan,
                Some(&measurement),
            );
            assert!(lines.len() > 1);
            assert_eq!(lines[0].inset, 0.);
            let indent = lines[1].inset;
            assert!(indent > 20. && indent < 105.);
            assert!(
                lines[1..]
                    .iter()
                    .all(|line| (line.inset - indent).abs() < 0.01)
            );
            assert_eq!(
                lines
                    .iter()
                    .map(|line| &projection.text()[line.projected_range()])
                    .collect::<String>(),
                projection.text().trim_end_matches('\n')
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[test]
    fn status_chips_require_explicit_status_columns_and_keep_literal_labels() {
        let source = "Approved is ordinary prose.\n\n| Status | Description |\n| --- | --- |\n| Approved | Approved |\n| In review | Draft |\n| Unexpected value | Complete |\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let chips = projection
            .segments()
            .iter()
            .filter_map(|segment| {
                segment.context.badge.map(|badge| {
                    (
                        &projection.text()[segment.projection_range()],
                        badge.tone.style(TachyonPalette::LIGHT).paper,
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            chips,
            [
                ("Approved", 0xe8eee2),
                ("In review", 0xfff4df),
                ("Unexpected value", 0xf2f1ec)
            ]
        );
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[gpui::test]
    fn shared_views_keep_independent_selections(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let session =
            SharedDocumentSession::new(Document::from_markdown("one").expect("shared document"));
        let node_id = session.snapshot().blocks().get(0).expect("paragraph").id();
        let second_session = session.clone();
        let (first, cx) =
            cx.add_window_view(|window, cx| RichDocumentEditor::with_session(session, window, cx));
        let cx: &mut gpui::VisualTestContext = cx;
        let second = cx.update(|window, cx| {
            cx.new(|cx| RichDocumentEditor::with_session(second_session, window, cx))
        });

        cx.update(|_, cx| {
            second.update(cx, |editor, _| {
                editor.selection = Selection::Text(TextSelection::caret(DocumentPosition::new(
                    node_id,
                    3,
                    Affinity::Downstream,
                )));
                let result = editor
                    .apply_command(EditCommand::ReplaceSelection {
                        text: " two".into(),
                        typing: false,
                    })
                    .expect("edit from second view");
                editor.refresh_after_transaction(&result);
            });
            first.update(cx, |editor, cx| {
                assert!(editor.sync_shared_session(cx));
            });
        });

        assert_eq!(
            first.read_with(cx, |editor, _| editor.cursor_offset()),
            0,
            "the observing view's caret must not move"
        );
        assert_eq!(
            second.read_with(cx, |editor, _| editor.cursor_offset()),
            7,
            "the initiating view keeps its transformed caret"
        );
        assert_eq!(
            first.read_with(cx, |editor, _| {
                editor.document.snapshot().serialize().expect("serialize")
            }),
            "one two"
        );
    }

    #[gpui::test]
    fn another_view_cannot_overwrite_or_commit_owned_html_composition(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "<div>Editable text</div>\n";
        let session = SharedDocumentSession::new(Document::from_markdown(source).unwrap());
        let (first, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::with_session(session.clone(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        let second = cx.update(|window, cx| {
            cx.new(|cx| RichDocumentEditor::with_session(session.clone(), window, cx))
        });
        cx.update(|window, cx| {
            first.update(cx, |editor, cx| {
                click_html_text(editor, 0, 2, window, cx);
                editor.replace_and_mark_text_in_range(None, "仮", None, window, cx);
            });
            let provisional = session.snapshot().serialize().unwrap();
            let selection = session.snapshot().selection().clone();
            second.update(cx, |editor, cx| {
                editor.sync_shared_session(cx);
                editor.replace_text_in_range(None, "WRONG", window, cx);
                assert!(editor.last_error.is_some());
                editor.replace_and_mark_text_in_range(None, "WRONG", None, window, cx);
                assert!(editor.commit_pending_composition(cx).is_err());
                assert!(editor.cancel_pending_composition(cx).is_err());
                assert!(
                    editor
                        .apply_command(EditCommand::ReplaceSelection {
                            text: "WRONG".into(),
                            typing: false
                        })
                        .is_err()
                );
                assert!(editor.sync_selection_to_document().is_err());
                assert_eq!(session.snapshot().serialize().unwrap(), provisional);
                assert_eq!(session.snapshot().selection(), &selection);
            });
            first.update(cx, |editor, cx| {
                editor.replace_text_in_range(None, "確定", window, cx);
                assert!(
                    session
                        .snapshot()
                        .serialize()
                        .unwrap()
                        .contains("Ed確定itable text")
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(session.snapshot().serialize().unwrap(), source);
            });
        });
    }

    #[gpui::test]
    fn leaving_a_shared_session_cancels_only_this_views_preedit(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "<div>Editable text</div>\n";
        let session = SharedDocumentSession::new(Document::from_markdown(source).unwrap());
        let (root, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::with_session(session.clone(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        for drop_view in [false, true] {
            let owner = cx.update(|window, cx| {
                cx.new(|cx| RichDocumentEditor::with_session(session.clone(), window, cx))
            });
            cx.update(|window, cx| {
                owner.update(cx, |editor, cx| {
                    click_html_text(editor, 0, 2, window, cx);
                    editor.replace_and_mark_text_in_range(None, "仮", None, window, cx);
                    assert!(session.composition_active());
                });
                if !drop_view {
                    // A nonowner leaving cannot cancel the owner's preedit.
                    let other =
                        SharedDocumentSession::new(Document::from_markdown("Other").unwrap());
                    root.update(cx, |editor, cx| {
                        editor.attach_shared_session(other.clone(), cx)
                    });
                    assert!(session.composition_active());
                    owner.update(cx, |editor, cx| editor.attach_shared_session(other, cx));
                }
            });
            drop(owner);
            // GPUI releases zero-reference entities at the next effect flush.
            cx.update(|_, _| {});
            cx.run_until_parked();
            assert!(
                !session.composition_active(),
                "departed owner left a locked session (drop={drop_view})"
            );
            assert_eq!(session.snapshot().serialize().unwrap(), source);
            assert!(matches!(session.undo(), Err(DocumentError::NothingToUndo)));
        }
    }

    #[gpui::test]
    fn empty_cancelled_and_undone_html_preedit_restore_source_and_caret(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "<div>Editable text</div>\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                for mode in 0..3 {
                    click_html_text(editor, 0, 2, window, cx);
                    let range = editor.selected_byte_range();
                    editor.replace_and_mark_text_in_range(
                        None,
                        if mode == 0 { "" } else { "仮" },
                        None,
                        window,
                        cx,
                    );
                    assert!(editor.composition_active());
                    match mode {
                        0 => editor.unmark_text(window, cx),
                        1 => editor.replace_text_in_range(None, "", window, cx),
                        _ => editor.undo(&Undo, window, cx),
                    }
                    assert!(!editor.composition_active());
                    assert!(editor.composition_origin.is_none());
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                    assert_eq!(editor.selected_byte_range(), range);
                    assert!(editor.html_selection.is_some());
                    assert!(matches!(
                        editor.document.undo(),
                        Err(DocumentError::NothingToUndo)
                    ));
                }
            })
        });
    }

    #[gpui::test]
    fn complete_fixture_renders_with_scale_specific_shape_cache_entries(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let document =
            Document::from_markdown(include_str!("../../../performance/visual-fixture.md"))
                .expect("complete visual fixture");
        let (editor, cx) =
            cx.add_window_view(|window, cx| RichDocumentEditor::new(document, window, cx));
        let cx: &mut gpui::VisualTestContext = cx;

        for scale in [1.0_f32, 1.25, 1.5, 2.0] {
            cx.update(|window, cx| {
                window.set_scale_factor(scale);
                _ = window.draw(cx);
                assert_eq!(window.scale_factor(), scale);
                assert!(!window.painted_quads().is_empty());
            });
            let has_scale_key = editor.read_with(cx, |editor, _| {
                editor
                    .shaped_line_cache
                    .borrow()
                    .entries
                    .keys()
                    .any(|key| key.scale_bits == scale.to_bits())
            });
            assert!(has_scale_key, "shape cache must distinguish scale {scale}");
        }
    }

    #[gpui::test]
    fn selected_zoom_publishes_readable_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "## Executable sample\n\n```rust\n// Keep every syntax role readable\nlet value = (42, \"label\");\n```\n\n## Configuration sample\n\n```json\n{\"value\": 42, \"label\": \"retained\"}\n```\n";
        for select_before_dispatch in [true, false] {
            let (editor, cx) = cx.add_window_view(|window, cx| {
                RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
            });
            cx.simulate_resize(size(px(1006.), px(1666.)));
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            let (release, hold) = futures::channel::oneshot::channel();
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    assert!(editor.measured_layout);
                    assert!(
                        editor
                            .visual_lines
                            .iter()
                            .any(|line| line.width_fraction < 0.9),
                        "start with measured peers"
                    );
                    if select_before_dispatch {
                        editor.select_all(&SelectAll, window, cx);
                    } else {
                        editor.reflow.hold_next = Some(hold);
                    }
                    editor.set_zoom_factor(2., cx);
                })
            });
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
            if !select_before_dispatch {
                cx.update(|window, cx| {
                    editor.update(cx, |editor, cx| {
                        assert!(
                            editor.reflow.is_active(),
                            "hold a real in-flight zoom result"
                        );
                        editor.select_all(&SelectAll, window, cx);
                    })
                });
            }
            _ = release.send(());
            let selection = editor.read_with(cx, |editor, _| editor.selection.clone());
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                assert!(editor.measured_layout, "a settled selection must not strand zoom remeasurement; selected before dispatch={select_before_dispatch}");
                assert!(!editor.text_environment_pending);
                assert!(editor.visual_lines.iter().all(|line| line.width_fraction >= 0.99), "zoomed examples must stack at the reduced reading measure");
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(editor.scroll_metrics().0 < 1., "a selected endpoint is not a caret scroll anchor");
            });
        }
    }

    #[gpui::test]
    fn selected_reflow_distinguishes_environment_from_active_input(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("A selected paragraph.\n").unwrap(),
                window,
                cx,
            )
        });
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_selection(0..10, true, window, cx);
                let width = editor.layout_width;
                let height = editor.scroll_metrics().1;
                assert!(!editor.text_environment_pending);
                assert!(
                    editor.selection_defers_reflow(width, height),
                    "optional recomposition stays deferred"
                );
                assert!(
                    !editor.selection_defers_reflow(width - 200., height),
                    "resize must correct selected tracks"
                );
                assert!(
                    !editor.selection_defers_reflow(width, height - 200.),
                    "shorter viewport must reconsider selected columns"
                );
                editor.set_zoom_factor(2., cx);
                assert!(!editor.selection_defers_reflow(width, height));
                editor.is_selecting = true;
                assert!(
                    editor.selection_defers_reflow(width, height),
                    "never move targets during dragging"
                );
                editor.is_selecting = false;
                editor.marked_range = Some(0..1);
                assert!(
                    editor.selection_defers_reflow(width, height),
                    "composition remains protected"
                );
                editor.marked_range = None;
                assert_eq!(editor.selected_byte_range(), (0..10, true));
            })
        });
    }

    #[gpui::test]
    fn document_zoom_is_bounded_and_resettable(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let document = Document::from_markdown("# Zoom\n\nbody").expect("document");
        let (editor, cx) =
            cx.add_window_view(|window, cx| RichDocumentEditor::new(document, window, cx));
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                for _ in 0..20 {
                    editor.zoom_in(cx);
                }
                assert_eq!(editor.zoom_percent(), 200);
                for _ in 0..30 {
                    editor.zoom_out(cx);
                }
                assert_eq!(editor.zoom_percent(), 75);
                editor.reset_zoom(cx);
                assert_eq!(editor.zoom_percent(), 100);
            });
        });
    }

    #[gpui::test]
    fn layout_inspection_is_opt_in_and_does_not_edit_or_move_the_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "# Inspect\n\nThe original paragraph.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let generation = editor.geometry_generation;
                let selection = editor.selection.clone();
                let revision = editor.document.snapshot().revision();
                editor.request_layout_trace(&InspectLayout, window, cx);
                assert_eq!(editor.geometry_generation, generation);
                assert!(!editor.layout_trace_requested);
                editor.set_layout_trace_mode(LayoutTraceMode::Summary);
                editor.request_layout_trace(&InspectLayout, window, cx);
                assert!(editor.layout_trace_requested && editor.layout_replan_pending);
                assert_eq!(editor.geometry_generation, generation + 1);
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().revision(), revision);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            })
        });
    }

    #[gpui::test]
    fn html_edit_here_accounts_for_scroll_zoom_and_preserves_source_until_invoked(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "# Before\n\n<div style='padding:12px'><p>First paragraph</p><p>Second paragraph</p></div>\n\nAfter\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| editor.update(cx, |editor, cx| {
            let original_selection = editor.selection.clone();
            for zoom in [1., 1.5, 2.] {
                editor.set_zoom_factor(zoom, cx);
                let bounds = Bounds::new(point(px(75.), px(-140.)), size(px(800.), px(editor.document_height)));
                editor.element_bounds = Some(bounds);
                let line = editor.visual_lines.iter().find(|line| line.html_preview.is_some()).unwrap();
                let hit = line.html_preview.as_ref().unwrap().text_hits.iter().find(|hit| hit.left.text_node == 1 && hit.left.byte_offset == 2).unwrap();
                let [left, top, right, bottom] = hit.bounds;
                let point = point(bounds.left() + px(line.x_fraction * 800. + line.inset + (left + (right - left) * 0.1) * zoom),
                    bounds.top() + px(line.y + ((top + bottom) * 0.5) * zoom));
                let command = editor.html_edit_command_at(point).expect("scaled HTML target");
                assert!(matches!(&command, EditCommand::ConvertHtmlToMarkdownAt { position, .. } if position.text_node == 1 && position.byte_offset == 2));
                assert_eq!(editor.selection, original_selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                if zoom == 2. {
                    editor.apply_structural_command(command, window, cx);
                    let Selection::Text(selection) = &editor.selection else { panic!("text caret") };
                    assert_eq!(selection.head.text_offset, 2);
                    assert_eq!(editor.document.snapshot().node(selection.head.node_id).unwrap().text().unwrap().as_string(), "Second paragraph");
                }
            }
        }));
    }

    #[gpui::test]
    fn zoom_retains_html_until_background_remeasurement(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "# Zoom\n\n<div style='padding:12px'>An HTML fragment</div>\n";
        let document = Document::from_markdown(source).unwrap();
        let (editor, cx) =
            cx.add_window_view(|window, cx| RichDocumentEditor::new(document, window, cx));
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|_, cx| editor.update(cx, |editor, cx| {
            let before = editor.visual_lines.iter().find_map(|line| line.html_preview.clone()).unwrap();
            let selection = editor.selection.clone();
            editor.zoom_in(cx);
            let after = editor.visual_lines.iter().find_map(|line| line.html_preview.clone()).unwrap();
            assert!(Arc::ptr_eq(&before, &after), "zoom input must reuse the retained HTML image instead of laying out and rasterizing HTML synchronously");
            assert!(!editor.measured_layout, "exact-width remeasurement must be scheduled");
            assert_eq!(editor.selection, selection);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        }));
    }

    #[gpui::test]
    fn selection_toolbar_waits_for_selection_to_settle_before_fading_in(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("word").expect("document"),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_selection(0..4, false, window, cx);
                assert!(!editor.toolbar_visible);
            });
        });

        cx.executor()
            .advance_clock(TOOLBAR_SETTLE_DELAY - Duration::from_millis(1));
        cx.run_until_parked();
        assert!(!editor.read_with(cx, |editor, _| editor.toolbar_visible));

        cx.executor().advance_clock(Duration::from_millis(1));
        cx.run_until_parked();
        assert!(editor.read_with(cx, |editor, _| editor.toolbar_visible));
        assert_eq!(editor.read_with(cx, |editor, _| editor.toolbar_opacity), 0.);

        cx.executor()
            .advance_clock(TOOLBAR_FADE_FRAME * TOOLBAR_FADE_STEPS + Duration::from_millis(1));
        cx.run_until_parked();
        assert_eq!(editor.read_with(cx, |editor, _| editor.toolbar_opacity), 1.);
    }

    #[test]
    fn shaped_line_cache_is_lru_bounded() {
        let mut cache = BoundedLru::new(2);
        cache.insert("old", 1);
        cache.insert("hot", 2);
        assert_eq!(cache.get(&"old"), Some(1));
        cache.insert("new", 3);
        assert_eq!(cache.get(&"hot"), None);
        assert_eq!(cache.get(&"old"), Some(1));
        assert_eq!(cache.get(&"new"), Some(3));
    }

    #[test]
    fn line_ranges_include_empty_visual_lines() {
        assert_eq!(display_line_ranges("a\n\nb"), vec![0..1, 2..2, 3..4]);
    }

    #[test]
    fn retained_prose_ranges_follow_projection_without_suffix_line_rewrite() {
        assert_eq!(std::mem::size_of::<VisualLineSpec>(), 64);
        let mut document = Document::from_markdown("alpha\n\nbeta\n\ngamma").expect("document");
        let snapshot = document.snapshot();
        let target = snapshot.blocks().get(1).expect("middle paragraph").id();
        let mut prepared = PreparedDocumentView::prepare(&document);
        let retained = prepared
            .visual_lines
            .iter()
            .find(|line| prepared.projection.text()[line.projected_range()] == *"gamma")
            .expect("tail line");
        let old_range = retained.projected_range();
        let retained_payload = retained.payload.clone();
        let retained_source = retained.source.projection_start.clone();
        let retained_components = prepared.components.clone();

        let result = document
            .apply(EditCommand::ReplaceText {
                node_id: target,
                range: 4..4,
                text: "x".into(),
                selection_after: None,
                typing: true,
            })
            .expect("edit middle paragraph");
        assert!(prepared.refresh_text_node(&result.snapshot, target));
        let tail = prepared
            .visual_lines
            .iter()
            .find(|line| prepared.projection.text()[line.projected_range()] == *"gamma")
            .expect("retained tail line");
        assert!(Arc::ptr_eq(&tail.source.projection_start, &retained_source));
        assert_eq!(
            tail.projected_range(),
            old_range.start + 1..old_range.end + 1
        );
        assert!(Arc::ptr_eq(&tail.payload, &retained_payload));
        assert!(
            Arc::ptr_eq(&prepared.components, &retained_components),
            "a no-wrap prepared-view refresh retains the component index"
        );

        let mut sparse = tail.clone();
        sparse.flow_geometry = true;
        sparse.table_row_y = 18.;
        assert!(!Arc::ptr_eq(&sparse.payload, &retained_payload));
        let range = sparse.projected_range();
        let unrelated_chunk = Arc::new(crate::projection::ProjectionOffset::new(0));
        shift_retained_lines(std::slice::from_mut(&mut sparse), &unrelated_chunk, 1, -6.)
            .expect("valid vertical rebase");
        assert_eq!(sparse.projected_range(), range);
        assert_eq!(sparse.table_row_y, 12.);
    }

    #[test]
    fn viewport_slice_stays_bounded_for_huge_documents() {
        let lines = (0..100_000)
            .map(|index| VisualLineSpec {
                compact_tree: false,
                source: LineSourceRange::absolute(index..index + 1),
                payload: visual_line_payload(VisualLinePayload::default()),
                style: VisualLineStyle::BODY,
                inset: 0.,
                gap_before: 0.,
                y: index as f32 * LINE_HEIGHT,
                x_fraction: 0.,
                width_fraction: 1.,
                table_cell_first: false,
                flow_geometry: false,
            })
            .collect::<Vec<_>>();
        let order = visual_line_paint_order(&lines);
        let visible = visible_paint_order_range(&lines, &order, 1_000_000., 1_002_400.);
        assert!(visible.len() < 100, "one viewport must not scan 100k lines");
        assert!(visible.clone().all(|index| {
            let line = &lines[order[index]];
            line.y <= 1_002_400. && line.y + line.style.line_height >= 999_970.
        }));
    }

    #[gpui::test]
    fn display_math_paint_never_requests_formulas_after_cache_eviction(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "# Math\n\n```math\n\\frac{17}{29}\n```\n\n```math\n\\invalidCommand\n```\n\nFollowing text.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        // Fill beyond the cross-document LRU capacity with unrelated formulas.
        // Paint must retain its prepared result (including invalid fallback),
        // not depend on whether a global cache happens to contain this source.
        for n in 0..80 {
            crate::math::inline_formula(&format!("x+{n}"), 0x123456).unwrap();
        }
        for (index, mode) in [
            gpui_component::ThemeMode::Light,
            gpui_component::ThemeMode::Dark,
            gpui_component::ThemeMode::Light,
        ]
        .into_iter()
        .enumerate()
        {
            cx.update(|window, cx| {
                gpui_component::Theme::change(mode, Some(window), cx);
                editor.update(cx, |editor, cx| {
                    editor
                        .scroll_handle
                        .set_offset(point(px(0.), px(-(index as f32 * 10.))));
                    cx.notify();
                });
            });
            let before = crate::math::test_formula_requests();
            cx.update(|window, cx| {
                _ = window.draw(cx);
                assert!(!window.painted_quads().is_empty());
            });
            assert_eq!(
                crate::math::test_formula_requests(),
                before,
                "a paint frame must not request formula parsing/layout, even on cache hits"
            );
        }
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source)
        });
    }

    #[test]
    fn formula_preview_reserves_space_and_refreshes_only_its_source_node() {
        let source = "Before\n\n```math\nx\n```\n\nAfter\n";
        let mut document = Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let node_id = snapshot.blocks().get(1).unwrap().id();
        let mut prepared = PreparedDocumentView::prepare(&document);
        let original_height = prepared.document_height;
        let original_lines = prepared.line_count();
        assert_eq!(snapshot.serialize().unwrap(), source);
        let preview = crate::math::block_preview(snapshot.blocks().get(1).unwrap(), 0).unwrap();
        let first = prepared
            .visual_lines
            .iter()
            .find(|line| {
                prepared
                    .projection
                    .segment_for_range(&line.projected_range())
                    .is_some_and(|s| s.node_id == node_id)
            })
            .unwrap();
        let original_image = first.display_math.as_ref().unwrap().light.image.id;
        assert_ne!(
            original_image,
            first.display_math.as_ref().unwrap().dark.image.id
        );
        assert!(first.rendered_code_preview());
        assert_eq!(first.style.space_above, 0.);
        assert_eq!(first.gap_before, DocumentStyle::EQUATION_GAP);
        assert_eq!(first.style.line_height, preview.height);
        let replacement = r"\frac{1}{\sqrt{2}}";
        let result = document
            .apply(EditCommand::ReplaceText {
                node_id,
                range: 0..1,
                text: replacement.into(),
                selection_after: None,
                typing: true,
            })
            .unwrap();
        assert!(prepared.refresh_text_node(&result.snapshot, node_id));
        assert_eq!(prepared.line_count(), original_lines);
        assert!(prepared.document_height > original_height);
        let rebuilt = PreparedDocumentView::prepare(&document);
        let edited = prepared
            .visual_lines
            .iter()
            .find_map(|line| line.display_math.as_ref())
            .unwrap();
        let fresh = rebuilt
            .visual_lines
            .iter()
            .find_map(|line| line.display_math.as_ref())
            .unwrap();
        assert_ne!(edited.light.image.id, original_image);
        assert_eq!(edited.light.image.bytes, fresh.light.image.bytes);
        assert_eq!(edited.dark.image.bytes, fresh.dark.image.bytes);
        assert!((prepared.document_height - rebuilt.document_height).abs() < 0.01);
        assert_eq!(prepared.projection.text(), rebuilt.projection.text());
        assert_eq!(
            document.snapshot().serialize().unwrap(),
            source.replace("\nx\n", &format!("\n{replacement}\n"))
        );
        let result = document
            .apply(EditCommand::ReplaceText {
                node_id,
                range: 0..replacement.len(),
                text: "x".into(),
                selection_after: None,
                typing: true,
            })
            .unwrap();
        assert!(prepared.refresh_text_node(&result.snapshot, node_id));
        assert!((prepared.document_height - original_height).abs() < 0.01);
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn prepared_view_refreshes_only_one_text_node() {
        let mut document = Document::from_markdown("before\n\nshort\n\nafter").expect("document");
        let node_id = document.snapshot().blocks().get(1).expect("middle").id();
        let mut prepared = PreparedDocumentView::prepare(&document);
        let original_lines = prepared.line_count();
        let replacement = "word ".repeat(30);
        let result = document
            .apply(EditCommand::ReplaceText {
                node_id,
                range: 0..5,
                text: replacement.clone(),
                selection_after: None,
                typing: true,
            })
            .expect("edit");

        assert!(prepared.refresh_text_node(&result.snapshot, node_id));
        assert!(prepared.line_count() > original_lines);
        assert!(prepared.projection.text().contains(&replacement));
        assert_eq!(
            prepared.paint_order.len(),
            prepared.visual_lines.len(),
            "paint index remains complete"
        );
        assert_eq!(
            prepared.document_height,
            visual_document_height(&prepared.visual_lines)
        );
    }

    #[test]
    fn prose_wrap_ranges_cover_source_without_gaps() {
        let text = "one two three four five";
        let ranges = wrap_line_range(text, 0..text.len(), 8);
        assert_eq!(ranges, vec![0..8, 8..14, 14..19, 19..23]);
        assert_eq!(
            ranges
                .iter()
                .map(|range| &text[range.clone()])
                .collect::<String>(),
            text
        );
    }

    #[test]
    fn double_click_word_range_is_unicode_aware() {
        let text = "alpha naïve 日本語 omega";
        assert_eq!(word_range_at(text, 8), 6..12);
        assert_eq!(word_range_at(text, 15), 13..22);
        assert_eq!(word_range_at(text, text.len()), 23..28);
    }

    #[test]
    fn directional_collapse_uses_logical_edges_not_drag_direction() {
        let range = 4..11;
        assert_eq!(selection_collapse_offset(&range, true), 4);
        assert_eq!(selection_collapse_offset(&range, false), 11);
    }

    #[test]
    fn word_navigation_skips_whitespace_and_respects_unicode_graphemes() {
        let text = "alpha  naïve_日本語!  end";
        assert_eq!(previous_word_boundary(text, text.len()), 26);
        assert_eq!(previous_word_boundary(text, 13), 7);
        assert_eq!(next_word_boundary(text, 0), 5);
        assert_eq!(next_word_boundary(text, 7), 23);
        assert_eq!(next_word_boundary("a 👨‍👩‍👧‍👦 b", 2), 27);
    }

    #[test]
    fn vertical_geometry_skips_same_row_table_fragments() {
        let line = |range, y| VisualLineSpec {
            compact_tree: false,
            source: LineSourceRange::absolute(range),
            payload: visual_line_payload(VisualLinePayload {
                table_row_y: y,
                table_row_height: VisualLineStyle::BODY.line_height,
                ..VisualLinePayload::default()
            }),
            style: VisualLineStyle::BODY,
            inset: 0.,
            gap_before: 0.,
            y,
            x_fraction: 0.,
            width_fraction: 1.,
            table_cell_first: false,
            flow_geometry: false,
        };
        let lines = vec![line(0..3, 0.), line(3..6, 0.), line(6..9, 28.8)];
        assert_eq!(visual_vertical_neighbor(&lines, 0, 1), Some(2));
        assert_eq!(visual_vertical_neighbor(&lines, 2, -1), Some(1));
    }

    #[test]
    fn vertical_fallback_snaps_to_grapheme_boundaries() {
        let text = "a👨‍👩‍👧‍👦b";
        assert_eq!(snap_offset_to_grapheme(text, 1..26, 12), 1);
        assert_eq!(snap_offset_to_grapheme(text, 1..26, 26), 26);
    }

    #[gpui::test]
    fn consecutive_text_input_replacements_share_one_undo_group(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("base").expect("document"),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                assert!(editor.replace_range(4..4, "a", true, window, cx));
                assert!(editor.replace_range(5..5, "b", true, window, cx));
                assert_eq!(
                    editor.document.snapshot().serialize().expect("serialize"),
                    "baseab"
                );
                editor.document.undo().expect("one grouped undo");
                assert_eq!(
                    editor.document.snapshot().serialize().expect("serialize"),
                    "base"
                );
            });
        });
    }

    #[gpui::test]
    fn escape_cancels_provisional_composition_without_an_undo_entry(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("base").expect("document"),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_selection(4..4, false, window, cx);
                editor.replace_and_mark_text_in_range(None, "仮", Some(1..1), window, cx);
                assert!(editor.composition_active());
                assert_eq!(
                    editor.document.snapshot().serialize().expect("serialize"),
                    "base仮"
                );

                editor.dismiss(&Dismiss, window, cx);

                assert!(!editor.composition_active());
                assert!(editor.marked_range.is_none());
                assert_eq!(
                    editor.document.snapshot().serialize().expect("serialize"),
                    "base"
                );
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
            });
        });
    }

    #[gpui::test]
    fn planner_panic_publishes_current_width_stack_and_retries_only_changed_input(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "# Recovery\n\n- Alpha\n- Beta\n- Gamma\n- Delta\n- Epsilon\n- Zeta\n\n<div><strong>HTML survives.</strong></div>\n\n$$\n\\frac{1}{2}\n$$\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(1100.), px(900.)));
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        let selection = cx.update(|_, cx| {
            editor.update(cx, |editor, _| {
                assert!(
                    editor
                        .adaptive
                        .lists
                        .values()
                        .any(|list| matches!(list.layout, ListLayout::Grid(_)))
                );
                editor.reflow.fail_next_planner = true;
                editor.selection.clone()
            })
        });
        cx.simulate_resize(size(px(360.), px(900.)));
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                assert_eq!(
                    editor.layout_width, 360.,
                    "failure must publish a fresh narrow stack, not retain the wide grid"
                );
                assert!(
                    editor
                        .last_error()
                        .is_some_and(|message| message.starts_with("Automatic layout failed"))
                );
                assert!(
                    editor
                        .adaptive
                        .lists
                        .values()
                        .all(|list| !matches!(list.layout, ListLayout::Grid(_)))
                );
                assert!(
                    editor
                        .adaptive
                        .measured_rows
                        .chosen
                        .iter()
                        .all(|row| row.kind == crate::adaptive::rows::RowKind::Stack)
                );
                assert!(
                    editor
                        .visual_lines
                        .iter()
                        .any(|line| line.html_preview.is_some())
                );
                assert!(
                    editor
                        .visual_lines
                        .iter()
                        .any(|line| line.display_math.is_some())
                );
                assert!(
                    editor
                        .visual_lines
                        .windows(2)
                        .all(|lines| lines[0].y <= lines[1].y)
                );
                for label in ["Alpha", "Beta", "Gamma", "Delta", "Epsilon", "Zeta"] {
                    assert_eq!(
                        editor
                            .visual_lines
                            .iter()
                            .filter(|line| {
                                editor.projection.text()[line.projected_range()].contains(label)
                            })
                            .count(),
                        1,
                        "each item is rendered exactly once: {label}"
                    );
                }
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
                for _ in 0..100 {
                    editor.sync_image_dimensions(360., cx);
                    assert!(!editor.reflow.is_active());
                }
            })
        });
        cx.simulate_resize(size(px(1100.), px(900.)));
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.read_with(cx, |editor, _| {
            assert!(editor.last_error().is_none());
            assert!(
                editor
                    .adaptive
                    .lists
                    .values()
                    .any(|list| matches!(list.layout, ListLayout::Grid(_)))
            );
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn retained_recovery_stack_reuses_exact_environment_without_history_chain(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document = Document::from_markdown("# Recovery cache\n\n- Alpha\n- Beta\n- Gamma\n- Delta\n- Epsilon\n- Zeta\n\n<div><strong>HTML survives.</strong></div>\n\n$$\n\\frac{1}{2}\n$$\n").unwrap();
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let mut previous = AdaptivePlan::default();
            let mut published_geometry = None;
            let mut last_stack: Option<Arc<PublishedGeometry>> = None;
            for (index, width) in [1100., 1100., 700., 700.].into_iter().enumerate() {
                let recovery = reflow::Recovery::default();
                let (prepared, _, _) = PreparedDocumentView::try_prepare_snapshot_with_images(
                    &document.snapshot(), &HashMap::new(), None,
                    ReflowViewport { published_geometry, width, height: 900., zoom: 1.,
                        preview_edit_node: None, expanded_code_tail: None, editing_node: None, table_layout_lock: None,
                        html_disclosures: Arc::default(), html_loaded_images: Arc::default(),
                        trace_mode: LayoutTraceMode::Off, visible_roots: None, resource_generation: 0 },
                    &previous, &measurement, ReflowControl { deadline: &reflow::Deadline::unlimited(), recovery: Some(&recovery) },
                ).unwrap();
                let stack = recovery.take().unwrap().0.published_geometry.unwrap();
                assert!(recovery.take().is_none(), "only one fallback is retained per worker");
                assert!(stack.stack.is_none(), "retained stack must never link to old geometry generations");
                assert!(stack.lines.iter().any(|line| line.html_preview.is_some()));
                assert!(stack.lines.iter().any(|line| line.display_math.is_some()));
                if let Some(last) = &last_stack {
                    assert_eq!(Arc::ptr_eq(last, &stack), index != 2, "exact width must govern stack reuse");
                }
                assert!(prepared.adaptive.lists.values().any(|list| matches!(list.layout, ListLayout::Grid(_))));
                assert!(Arc::ptr_eq(prepared.published_geometry.as_ref().unwrap().stack.as_ref().unwrap(), &stack));
                last_stack = Some(stack);
                previous = prepared.adaptive;
                published_geometry = prepared.published_geometry;
            }
        });
    }

    #[gpui::test]
    fn planner_fallback_reports_recovery_but_respects_existing_edit_lock(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let mut document =
                Document::from_markdown("# Example\n\n- One\n- Two\n- Three\n").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let previous = AdaptivePlan::build(&projection, 900., None, false);
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let mut viewport = ReflowViewport {
                published_geometry: None,
                width: 900.,
                height: 800.,
                zoom: 1.,
                preview_edit_node: None,
                expanded_code_tail: None,
                editing_node: None,
                table_layout_lock: None,
                html_disclosures: Arc::default(),
                html_loaded_images: Arc::default(),
                trace_mode: LayoutTraceMode::Summary,
                visible_roots: None,
                resource_generation: 0,
            };
            let mut deadline = reflow::Deadline::unlimited();
            deadline.fail_planner = true;
            let (prepared, _, report) = PreparedDocumentView::try_prepare_snapshot_with_images(
                &document.snapshot(),
                &HashMap::new(),
                None,
                viewport.clone(),
                &previous,
                &measurement,
                ReflowControl {
                    deadline: &deadline,
                    recovery: None,
                },
            )
            .unwrap();
            assert_eq!(prepared.recovery, Some(reflow::Failed::PlannerPanicked));
            assert!(report.unwrap().planner_failed);
            viewport.editing_node = Some(projection.segments()[1].node_id);
            let result = PreparedDocumentView::try_prepare_snapshot_with_images(
                &document.snapshot(),
                &HashMap::new(),
                None,
                viewport,
                &previous,
                &measurement,
                ReflowControl {
                    deadline: &deadline,
                    recovery: None,
                },
            );
            assert!(
                matches!(result, Err(reflow::Failed::Panicked)),
                "a fallback cannot dismantle the focused layout"
            );
            assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
        });
    }

    #[gpui::test]
    fn definition_native_measurement_publishes_aligned_rows(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        cx.update(|cx| {
            let document = Document::from_markdown(include_str!(
                "../../../performance/layout-fixtures/62-definition-lists.md"
            ))
            .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let plan = build_measured_adaptive_plan(&projection, 1280., 1166., None, false, &fonts);
            let term = projection
                .segments()
                .iter()
                .find(|s| &projection.text()[s.projection_range()] == "Canvas")
                .unwrap();
            let description = projection
                .segments()
                .iter()
                .find(|s| {
                    projection.text()[s.projection_range()].starts_with("The available document")
                })
                .unwrap();
            assert!(
                plan.slots.contains_key(&term.node_id),
                "measure={:?}; term={:?}; roots={:?}",
                plan.prose_measures,
                term,
                projection.roots().collect::<Vec<_>>()
            );
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1280.,
                &plan,
                Some(&fonts),
            );
            let first = lines
                .iter()
                .find(|l| l.projected_start() == term.projection_start())
                .unwrap();
            let second = lines
                .iter()
                .find(|l| l.projected_start() == description.projection_start())
                .unwrap();
            assert_eq!(first.y, second.y);
            assert!(second.x_fraction > first.x_fraction);
        });
    }

    #[gpui::test]
    fn reflow_cancellation_at_each_stage_preserves_canonical_content(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "# Mixed preparation\n\n| Property | Value |\n| --- | --- |\n| Count | 3 |\n\n<div><strong>Retained HTML.</strong></div>\n\n$$\n\\frac{1}{2}\n$$\n";
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let previous = AdaptivePlan::build(&projection, 900., None, false);
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let viewport = ReflowViewport {
                published_geometry: None, width: 900., height: 800., zoom: 1.,
                preview_edit_node: None, expanded_code_tail: None, editing_node: None, table_layout_lock: None,
                html_disclosures: Arc::default(), html_loaded_images: Arc::default(),
                trace_mode: LayoutTraceMode::Off, visible_roots: None, resource_generation: 0,
            };
            for completed_checks in 0..=7 {
                let result = PreparedDocumentView::try_prepare_snapshot_with_images(
                    &document.snapshot(), &HashMap::new(), None, viewport.clone(), &previous,
                    &measurement, ReflowControl { deadline: &reflow::Deadline::after_checks(completed_checks), recovery: None },
                );
                if completed_checks < 7 {
                    assert!(matches!(result, Err(reflow::Failed::TimedOut)), "checkpoint {completed_checks}");
                } else {
                    let prepared = result.unwrap().0;
                    assert!(!prepared.visual_lines.is_empty());
                    assert!(prepared.visual_lines.iter().any(|line| line.html_preview.is_some()));
                    assert!(prepared.visual_lines.iter().any(|line| line.display_math.is_some()));
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
            }
        });
    }

    #[gpui::test]
    fn timed_out_reflow_keeps_ownership_and_discards_late_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "# Timeout fixture\n\n- Alpha\n- Beta\n- Gamma\n- Delta\n- Epsilon\n- Zeta\n\n<div><strong>HTML survives.</strong></div>\n\n$$\n\\frac{1}{2}\n$$\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(1100.), px(900.)));
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        let (release, hold) = futures::channel::oneshot::channel();
        let (lines, selection) = cx.update(|_, cx| {
            editor.update(cx, |editor, _| {
                assert!(
                    editor
                        .adaptive
                        .lists
                        .values()
                        .any(|list| matches!(list.layout, ListLayout::Grid(_)))
                );
                editor.reflow.hold_next = Some(hold);
                (editor.visual_lines.clone(), editor.selection.clone())
            })
        });
        cx.simulate_resize(size(px(360.), px(900.)));
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.background_executor
            .advance_clock(reflow::TIMEOUT + Duration::from_millis(1));
        cx.run_until_parked();
        let fallback_lines = cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                assert!(
                    editor
                        .last_error()
                        .is_some_and(|error| error.starts_with("Layout update timed out"))
                );
                assert!(
                    editor.reflow.is_active(),
                    "timeout is not proof that synchronous work stopped"
                );
                assert_eq!(
                    editor.layout_width, 360.,
                    "timeout must publish the ready narrow stack while the worker is still held"
                );
                assert!(!Arc::ptr_eq(&lines, &editor.visual_lines));
                assert!(
                    editor
                        .adaptive
                        .lists
                        .values()
                        .all(|list| !matches!(list.layout, ListLayout::Grid(_)))
                );
                assert!(
                    editor
                        .visual_lines
                        .iter()
                        .any(|line| line.html_preview.is_some())
                );
                assert!(
                    editor
                        .visual_lines
                        .iter()
                        .any(|line| line.display_math.is_some())
                );
                assert_eq!(editor.selection, selection);
                editor.reflow.fail_next = true;
                editor.sync_image_dimensions(editor.layout_width - 30., cx);
                assert!(
                    editor.reflow.fail_next,
                    "a second worker must not start while the first is held"
                );
                editor.reflow.fail_next = false;
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                editor.visual_lines.clone()
            })
        });
        release.send(()).unwrap();
        cx.run_until_parked();
        cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                assert!(!editor.reflow.is_active());
                assert!(
                    Arc::ptr_eq(&fallback_lines, &editor.visual_lines),
                    "late successful geometry must be discarded"
                );
                assert_eq!(editor.selection, selection);
                editor.sync_image_dimensions(editor.layout_width, cx);
                assert!(
                    !editor.reflow.is_active(),
                    "the timed-out request stays suppressed"
                );
                editor.sync_image_dimensions(editor.layout_width - 30., cx);
                assert!(editor.reflow.is_active());
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            editor.update(cx, |editor, _| {
                assert!(!editor.reflow.is_active());
                assert!(editor.last_error().is_none());
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
            })
        });
        cx.background_executor.advance_clock(reflow::TIMEOUT * 2);
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(!editor.reflow.is_active());
            assert!(
                editor.last_error().is_none(),
                "normal completion must cancel its watchdog"
            );
        });
    }

    #[gpui::test]
    fn failed_reflow_retains_content_suppresses_retries_and_recovers_on_resize(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "# Keep this document\n\nEvery paragraph must remain available.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        let (lines, selection) = cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                let retained = (editor.visual_lines.clone(), editor.selection.clone());
                editor.reflow.fail_next = true;
                editor.layout_replan_pending = true;
                editor.sync_image_dimensions(editor.layout_width, cx);
                assert!(editor.reflow.is_active());
                retained
            })
        });
        cx.run_until_parked();
        cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                assert!(!editor.reflow.is_active());
                assert!(
                    editor
                        .last_error()
                        .unwrap()
                        .starts_with("Layout update failed")
                );
                assert!(Arc::ptr_eq(&lines, &editor.visual_lines));
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
                for _ in 0..100 {
                    editor.sync_image_dimensions(editor.layout_width, cx);
                    assert!(!editor.reflow.is_active());
                }
                editor.sync_image_dimensions(editor.layout_width - 30., cx);
                assert!(editor.reflow.is_active());
            })
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(!editor.reflow.is_active());
            assert!(editor.last_error().is_none());
            assert!(!editor.visual_lines.is_empty());
            assert_eq!(editor.selection, selection);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn failed_or_fallback_reflow_does_not_overwrite_intervening_composition(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        for failure in 0..3 {
            let (editor, cx) = cx.add_window_view(|window, cx| {
                RichDocumentEditor::new(Document::from_markdown("base").unwrap(), window, cx)
            });
            for _ in 0..3 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            let (release, hold) = futures::channel::oneshot::channel();
            let (lines, selection, marked) = cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.reflow.fail_next = failure == 0;
                    editor.reflow.fail_next_planner = failure == 1;
                    editor.reflow.hold_next = (failure == 2).then_some(hold);
                    editor.layout_replan_pending = true;
                    editor.sync_image_dimensions(editor.layout_width, cx);
                    assert!(editor.reflow.is_active());
                    editor.set_selection(4..4, false, window, cx);
                    editor.replace_and_mark_text_in_range(None, "仮", Some(1..1), window, cx);
                    assert!(editor.composition_active());
                    (
                        editor.visual_lines.clone(),
                        editor.selection.clone(),
                        editor.marked_range.clone(),
                    )
                })
            });
            cx.run_until_parked();
            if failure == 2 {
                cx.background_executor
                    .advance_clock(reflow::TIMEOUT + Duration::from_millis(1));
                cx.run_until_parked();
                editor.read_with(cx, |editor, _| {
                    assert!(editor.reflow.is_active());
                    assert!(Arc::ptr_eq(&lines, &editor.visual_lines));
                    assert_eq!(editor.marked_range, marked);
                    assert!(editor.last_error().is_none());
                });
                release.send(()).unwrap();
                cx.run_until_parked();
            }
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    assert!(!editor.reflow.is_active());
                    assert!(
                        editor.last_error().is_none(),
                        "obsolete request must not publish an error into newer content"
                    );
                    assert!(Arc::ptr_eq(&lines, &editor.visual_lines));
                    assert_eq!(editor.selection, selection);
                    assert_eq!(editor.marked_range, marked);
                    assert!(editor.composition_active());
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), "base仮");
                    editor.dismiss(&Dismiss, window, cx);
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), "base");
                    assert!(matches!(
                        editor.document.undo(),
                        Err(DocumentError::NothingToUndo)
                    ));
                })
            });
        }
    }

    #[gpui::test]
    fn document_switch_rejects_held_geometry_from_another_session(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Old document.").unwrap(),
                window,
                cx,
            )
        });
        let (release, hold) = futures::channel::oneshot::channel();
        cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_layout_trace_mode(LayoutTraceMode::Summary);
                editor.reflow.hold_next = Some(hold);
                let old_session = editor.document.id();
                editor.sync_image_dimensions(900., cx);
                assert!(editor.reflow.is_active());
                let replacement = SharedDocumentSession::new(
                    Document::from_markdown("Replacement document.").unwrap(),
                );
                assert_eq!(editor.document.generation(), replacement.generation());
                assert_ne!(old_session, replacement.id());
                editor.attach_shared_session(replacement, cx);
                assert!(
                    editor.reflow.is_active(),
                    "switching documents must not release an unfinished worker"
                );
            })
        });
        release.send(()).expect("release held reflow");
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert_eq!(
                editor.document.snapshot().serialize().unwrap(),
                "Replacement document."
            );
            assert_eq!(editor.layout_trace_discarded, 1);
        });
    }

    #[gpui::test]
    fn image_resource_arrivals_wait_for_a_batch_before_reflow(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = format!(
            "# Images\n\n![First](first.png)\n\n![Second](second.png)\n\n{}",
            "After the images.\n\n".repeat(25)
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        let anchor = cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_scroll_y(500., cx);
                let anchor = capture_scroll_anchor(
                    &editor.document.snapshot(),
                    &editor.projection,
                    &editor.visual_lines,
                    editor.scroll_metrics().0,
                )
                .unwrap();
                assert!(editor.measured_layout);
                assert!(!editor.reflow.is_active());
                assert!(
                    editor
                        .visible_planning_roots()
                        .is_none_or(|scope| editor.adaptive.covers(&scope))
                );
                let now = Instant::now();
                let dimensions = Arc::new(Mutex::new((
                    1,
                    HashMap::from([(
                        hash(&resolved_image_resource("first.png", None)),
                        (800, 100),
                    )]),
                )));
                editor.set_image_dimensions(dimensions.clone(), cx);
                editor.sync_image_dimensions_at(editor.layout_width, now, cx);
                assert!(
                    !editor.reflow.is_active(),
                    "one image arrival must not immediately dispatch a whole reflow"
                );
                {
                    let mut latest = dimensions.lock().unwrap();
                    latest.0 = 2;
                    latest.1.insert(
                        hash(&resolved_image_resource("second.png", None)),
                        (200, 400),
                    );
                }
                editor.sync_image_dimensions_at(
                    editor.layout_width,
                    now + Duration::from_millis(40),
                    cx,
                );
                assert!(!editor.reflow.is_active());
                assert_eq!(editor.requested_image_dimensions_generation, 2);
                assert!(editor.image_resource_wake.is_some());
                editor.sync_image_dimensions_at(
                    editor.layout_width,
                    now + Duration::from_millis(119),
                    cx,
                );
                assert!(!editor.reflow.is_active());
                editor.sync_image_dimensions_at(
                    editor.layout_width,
                    now + Duration::from_millis(120),
                    cx,
                );
                assert!(
                    editor.reflow.is_active(),
                    "the quiet deadline must dispatch without another arrival"
                );
                assert!(editor.image_resource_wake.is_none());
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
                anchor
            })
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(!editor.reflow.is_active());
            assert_eq!(editor.image_dimensions_generation, 2);
            let anchor_y = resolve_scroll_anchor(
                &editor.document.snapshot(),
                &editor.projection,
                &editor.visual_lines,
                &anchor,
            )
            .unwrap();
            assert!(
                (anchor_y - editor.scroll_metrics().0).abs() < 0.1,
                "batched images must retain the reading anchor"
            );
            assert_eq!(editor.image_layout_dimensions.len(), 2);
            assert!(
                editor
                    .image_layout_dimensions
                    .values()
                    .any(|(_, size)| *size == (800, 100))
            );
            assert!(
                editor
                    .image_layout_dimensions
                    .values()
                    .any(|(_, size)| *size == (200, 400))
            );
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
        editor.update(cx, |editor, cx| {
            editor
                .shared_image_dimensions
                .as_ref()
                .unwrap()
                .lock()
                .unwrap()
                .0 = 3;
            editor.sync_image_dimensions(editor.layout_width, cx);
            assert!(editor.image_resource_wake.is_some());
            assert!(!editor.reflow.is_active());
            editor.sync_image_dimensions(editor.layout_width + 32., cx);
            assert!(
                editor.reflow.is_active(),
                "resize must bypass resource-only batching"
            );
            assert!(editor.image_resource_wake.is_none());
        });
    }

    #[gpui::test]
    fn image_resource_arrivals_discard_stale_initial_reflow(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("![First](first.png)\n").unwrap(),
                window,
                cx,
            )
        });
        let dimensions = Arc::new(Mutex::new((1, HashMap::new())));
        editor.update(cx, |editor, cx| {
            editor.set_layout_trace_mode(LayoutTraceMode::Summary);
            editor.set_image_dimensions(dimensions.clone(), cx);
            editor.sync_image_dimensions(760., cx);
            assert!(
                editor.reflow.is_active(),
                "initial layout must not wait for resources"
            );
            // Arrival after dispatch, before commit. The outdated result
            // must be discarded and a following frame may request the latest.
            dimensions.lock().unwrap().0 = 2;
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(!editor.reflow.is_active());
            assert!(
                editor.layout_trace_discarded >= 1,
                "the old resource result must not commit"
            );
            assert_eq!(editor.image_dimensions_generation, 2);
            assert_eq!(
                editor.document.snapshot().serialize().unwrap(),
                "![First](first.png)\n"
            );
        });
    }

    #[gpui::test]
    fn empty_ime_composition_blocks_background_topology_changes(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown("base").unwrap(), window, cx)
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_selection(4..4, false, window, cx);
                editor.replace_and_mark_text_in_range(None, "", Some(0..0), window, cx);
                assert!(editor.document.composition_active());
                assert!(editor.marked_range.is_none());
                let generation = editor.geometry_generation;
                editor.sync_image_dimensions(1200., cx);
                assert!(
                    !editor.reflow.is_active(),
                    "IME is active even with no marked text"
                );
                assert_eq!(editor.geometry_generation, generation);
                editor.dismiss(&Dismiss, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), "base");
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
            });
        });
    }

    #[gpui::test]
    fn platform_ime_empty_marked_replacement_cancels_without_rewriting_source(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = "System IME target marker.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_selection(0..0, false, window, cx);
                editor.replace_and_mark_text_in_range(None, "ni", None, window, cx);
                let marked = editor.marked_text_range(window, cx).unwrap();

                editor.replace_text_in_range(Some(marked), "", window, cx);

                assert!(!editor.composition_active());
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert_eq!(editor.selected_byte_range(), (0..0, false));
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));
            });
        });
    }

    #[gpui::test]
    fn committed_composition_is_one_undoable_edit(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("base").expect("document"),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_selection(4..4, false, window, cx);
                editor.replace_and_mark_text_in_range(None, "入力", Some(2..2), window, cx);
                assert!(editor.commit_pending_composition(cx).expect("commit"));
                assert!(!editor.composition_active());
                assert_eq!(
                    editor.document.snapshot().serialize().expect("serialize"),
                    "base入力"
                );

                editor.document.undo().expect("single composition undo");
                assert_eq!(
                    editor.document.snapshot().serialize().expect("serialize"),
                    "base"
                );
            });
        });
    }

    #[test]
    fn body_copy_keeps_a_readable_measure_on_a_wide_canvas() {
        let maximum_measure = PROSE_WIDTH * 84. / 72.;
        let text = "a".repeat(80);
        let document = Document::from_markdown(text.as_str()).expect("document");
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = build_visual_lines(&document, &projection);
        assert_eq!(
            lines.len(),
            2,
            "80 characters wrap at the comfortable prose measure"
        );
        assert!(
            lines
                .iter()
                .all(|line| line.width_fraction * 760. <= maximum_measure)
        );
        assert!(lines[0].width_fraction * 760. > PROSE_WIDTH);
        let wide = build_visual_lines_with_images(&projection, &HashMap::new(), 1120.);
        assert!(
            wide.len() == lines.len(),
            "prose keeps the same readable measure on a wider canvas"
        );
        assert!(
            wide.iter()
                .all(|line| line.width_fraction * 1120. <= maximum_measure + 0.1)
        );
    }

    #[test]
    fn markdown_components_reserve_space_for_their_visual_chrome() {
        let document = Document::from_markdown(concat!(
            "# Hero\n\n",
            "## Section\n\n",
            "> [!NOTE]\n> Read this.\n\n",
            "```rust\nlet answer = 42;\n```\n",
        ))
        .expect("component document");
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = build_visual_lines(&document, &projection);

        let hero = lines
            .iter()
            .find(|line| &projection.text()[line.projected_range()] == "Hero")
            .expect("hero line");
        let section = lines
            .iter()
            .find(|line| &projection.text()[line.projected_range()] == "Section")
            .expect("section line");
        let alert = lines
            .iter()
            .find(|line| &projection.text()[line.projected_range()] == "Read this.")
            .expect("alert line");
        let code = lines
            .iter()
            .find(|line| &projection.text()[line.projected_range()] == "let answer = 42;")
            .expect("code line");

        assert_eq!(hero.inset, 0., "the title shares the prose leading edge");
        assert_eq!(section.inset, 0., "sections do not carry decorative badges");
        assert!(alert.inset >= ALERT_CONTENT_INSET);
        assert!(alert.style.space_above >= 16.);
        assert!(code.style.space_above >= CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING);
    }

    #[test]
    fn active_heading_tracks_the_last_heading_at_or_above_viewport() {
        let document = Document::from_markdown("# Hero\n\nBody\n\n## Section\n\nMore body")
            .expect("heading document");
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = build_visual_lines(&document, &projection);
        let heading_ids = document
            .snapshot()
            .blocks()
            .iter()
            .filter_map(|block| {
                matches!(block.as_ref(), BlockNode::Heading(_)).then_some(block.id())
            })
            .collect::<Vec<_>>();
        assert_eq!(heading_ids.len(), 2);
        let heading_y = |node_id| {
            lines.iter().find_map(|line| {
                segment_for_line(&projection, &line.projected_range())
                    .filter(|segment| segment.node_id == node_id)
                    .map(|_| line.y)
            })
        };
        let first_y = heading_y(heading_ids[0]).expect("first heading line");
        let second_y = heading_y(heading_ids[1]).expect("second heading line");
        assert_eq!(
            active_heading_node_for_viewport(&projection, &lines, 0., 120.),
            Some(heading_ids[0]),
            "the first visible heading is active before it reaches the viewport top"
        );
        assert_eq!(
            active_heading_node_for_viewport(&projection, &lines, first_y + 1., 120.),
            Some(heading_ids[0])
        );
        assert_eq!(
            active_heading_node_for_viewport(&projection, &lines, second_y + 1., 120.),
            Some(heading_ids[1])
        );

        let introduction = Document::from_markdown(format!(
            "{}\n\n# Below the fold",
            "introductory prose ".repeat(40)
        ))
        .expect("introductory document");
        let introduction_projection = TextProjection::from_snapshot(&introduction.snapshot());
        let introduction_lines = build_visual_lines(&introduction, &introduction_projection);
        assert_eq!(
            active_heading_node_for_viewport(
                &introduction_projection,
                &introduction_lines,
                0.,
                80.
            ),
            None,
            "an off-screen heading must not be announced as active"
        );
    }

    #[test]
    fn fenced_code_highlighting_preserves_every_source_byte() {
        let spans = syntax_spans("let value = \"tachyon\"; // sample", Some("rust"));
        assert_eq!(spans.iter().map(|span| span.range.len()).sum::<usize>(), 32);
        assert!(spans.iter().any(|span| span.kind == SyntaxKind::Keyword));
        assert!(spans.iter().any(|span| span.kind == SyntaxKind::String));
        assert!(spans.iter().any(|span| span.kind == SyntaxKind::Comment));
    }

    #[test]
    fn composition_selection_is_utf16_relative() {
        assert_eq!(utf16_range_in_text("a🎉z", 1..3), 1.."a🎉".len());
    }

    #[test]
    fn outline_jump_is_exactly_120_ms_and_finishes_at_target() {
        assert_eq!(
            OUTLINE_JUMP_FRAME * OUTLINE_JUMP_STEPS,
            Duration::from_millis(120)
        );
        assert_eq!(outline_jump_position(20., 220., 0), 20.);
        assert_eq!(outline_jump_position(20., 220., OUTLINE_JUMP_STEPS), 220.);
        assert!(outline_jump_position(20., 220., 7) > 20.);
    }

    #[test]
    fn table_cells_share_rows_and_honor_stable_column_widths() {
        let document = Document::from_markdown(concat!(
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[100.0,300.0]} -->\n",
            "| a | b |\n| --- | --- |\n| c | d |\n"
        ))
        .expect("table document");
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = build_visual_lines(&document, &projection)
            .into_iter()
            .filter(|line| line.table_cell.is_some())
            .collect::<Vec<_>>();

        assert_eq!(lines.len(), 4);
        assert_eq!(lines[0].y, lines[1].y);
        assert_eq!(lines[0].x_fraction, 0.);
        assert!((lines[0].width_fraction * 760. - 100.).abs() < 0.01);
        assert!((lines[1].x_fraction * 760. - 100.).abs() < 0.01);
        assert!((lines[1].width_fraction * 760. - 300.).abs() < 0.01);
        assert!(lines[2].y > lines[0].y);
        assert_eq!(lines[2].y, lines[3].y);
    }

    #[test]
    fn pointer_distance_disambiguates_cells_in_the_same_row() {
        let left = Bounds::new(point(px(100.), px(80.)), size(px(120.), px(32.)));
        let right = Bounds::new(point(px(240.), px(80.)), size(px(120.), px(32.)));
        let pointer = point(px(300.), px(96.));

        assert_eq!(distance_to_vertical_bounds(pointer.y, &left), 0.);
        assert_eq!(distance_to_vertical_bounds(pointer.y, &right), 0.);
        assert!(distance_to_bounds(pointer, &right) < distance_to_bounds(pointer, &left));
    }

    #[gpui::test]
    fn table_context_and_ctrl_drag_selection_do_not_require_text_selection(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = concat!(
            "Before\n\n",
            "| A | B |\n| --- | --- |\n| one | two |\n",
            "| three | four |\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let before = editor.selection.clone();
                let cell = |row, column| {
                    editor
                        .painted_lines
                        .iter()
                        .find_map(|line| {
                            let (target, bounds) = editor.table_cell_geometry(line)?;
                            (target.row == row && target.column == column)
                                .then_some((target, bounds))
                        })
                        .expect("table cell")
                };
                let (target, start) = cell(1, 0);
                let (_, end) = cell(2, 1);

                editor.on_context_menu(
                    &MouseDownEvent {
                        position: start.center(),
                        button: MouseButton::Right,
                        click_count: 1,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    editor.selection, before,
                    "context click preserves the caret"
                );
                assert_eq!(editor.table_hover, Some(target));

                assert_eq!(
                    editor.table_edge_controls(cx).len(),
                    4,
                    "each hovered cell exposes four edge menus"
                );

                editor.on_mouse_down(
                    &MouseDownEvent {
                        position: start.center(),
                        button: MouseButton::Left,
                        click_count: 1,
                        modifiers: gpui::Modifiers {
                            control: true,
                            ..Default::default()
                        },
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                editor.on_mouse_move(
                    &MouseMoveEvent {
                        position: end.center(),
                        pressed_button: Some(MouseButton::Left),
                        modifiers: gpui::Modifiers {
                            control: true,
                            ..Default::default()
                        },
                    },
                    window,
                    cx,
                );
                editor.on_mouse_up(
                    &MouseUpEvent {
                        position: end.center(),
                        button: MouseButton::Left,
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                assert_eq!(
                    editor.selection,
                    Selection::Table(RectangularSelection {
                        table_id: target.table_id,
                        anchor_row: 1,
                        anchor_column: 0,
                        head_row: 2,
                        head_column: 1,
                    })
                );
                assert!(!editor.toolbar_visible);
                editor.copy(&Copy, window, cx);
                assert_eq!(
                    cx.read_from_clipboard().unwrap().text().as_deref(),
                    Some("one\ttwo\nthree\tfour")
                );
                assert_eq!(
                    editor.document.snapshot().serialize().unwrap(),
                    source,
                    "selection and copy do not mutate source"
                );
            });
        });
    }

    #[test]
    fn table_edge_hit_targets_are_centered_on_the_cell_edges() {
        let cell = Bounds::new(point(px(100.), px(60.)), size(px(240.), px(80.)));
        for edge in [
            TableEdge::Top,
            TableEdge::Right,
            TableEdge::Bottom,
            TableEdge::Left,
        ] {
            let hit = table_edge_hit_bounds(cell, edge);
            assert_eq!(
                hit.center(),
                match edge {
                    TableEdge::Top => point(cell.center().x, cell.top()),
                    TableEdge::Right => point(cell.right(), cell.center().y),
                    TableEdge::Bottom => point(cell.center().x, cell.bottom()),
                    TableEdge::Left => point(cell.left(), cell.center().y),
                }
            );
            assert!(hit.size.width == px(14.) || hit.size.height == px(14.));
        }
    }

    #[gpui::test]
    fn table_edge_trigger_opens_its_local_menu(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            init_editor(cx);
        });
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("| A | B |\n| --- | --- |\n| one | two |\n").unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                editor.table_hover = editor.painted_lines.iter().find_map(|line| {
                    let (target, _) = editor.table_cell_geometry(line)?;
                    (target.row == 1 && target.column == 0).then_some(target)
                });
                cx.notify();
            });
        });
        cx.update(|window, cx| _ = window.draw(cx));
        let trigger = cx
            .debug_bounds("table-edge-Bottom")
            .expect("bottom edge trigger");
        cx.simulate_event(MouseDownEvent {
            position: trigger.center(),
            button: MouseButton::Left,
            click_count: 1,
            ..Default::default()
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(
            editor.read_with(cx, |editor, _| editor.table_edge_menu.is_some()),
            "trigger bounds: {trigger:?}"
        );
        assert!(!editor.read_with(cx, |editor, _| editor.toolbar_visible));
    }

    #[test]
    fn table_rows_wrap_to_cell_width_and_share_the_tallest_height() {
        let document = Document::from_markdown(concat!(
            "| left | right |\n| --- | --- |\n",
            "| A long cell containing enough prose to wrap across several visual lines without entering its neighbor. | short |\n"
        ))
        .expect("table document");
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = build_visual_lines(&document, &projection)
            .into_iter()
            .filter(|line| matches!(line.table_cell, Some((_, 1, _, _))))
            .collect::<Vec<_>>();
        let left = lines
            .iter()
            .filter(|line| matches!(line.table_cell, Some((_, _, 0, _))))
            .collect::<Vec<_>>();
        let right = lines
            .iter()
            .find(|line| matches!(line.table_cell, Some((_, _, 1, _))))
            .expect("right cell");

        assert!(left.len() > 1, "long text must wrap inside its column");
        assert!(left[0].table_cell_first);
        assert!(right.table_cell_first);
        assert!(right.table_row_height > right.style.line_height);
        assert!(
            lines
                .iter()
                .all(|line| line.table_row_y == right.table_row_y
                    && line.table_row_height == right.table_row_height)
        );
    }

    #[test]
    fn table_text_clip_is_limited_to_the_cell_and_viewport() {
        let cell = Bounds::new(point(px(200.), px(40.)), size(px(180.), px(32.)));
        let viewport = Bounds::new(point(px(240.), px(40.)), size(px(100.), px(32.)));
        assert_eq!(
            intersect_bounds(cell, viewport),
            Some(Bounds::new(
                point(px(240.), px(40.)),
                size(px(100.), px(32.))
            ))
        );
    }

    #[gpui::test]
    fn oversized_html_table_scrolls_without_changing_source(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = "<table style='width:1500px'><tr><th colspan='2'>Group</th></tr><tr><td>First</td><td>Last</td></tr></table>";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        editor.update(cx, |editor, cx| editor.set_zoom_factor(2., cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let line = editor
                    .painted_lines
                    .iter()
                    .find(|line| line.horizontal_owner.is_some())
                    .unwrap();
                let owner = line.horizontal_owner.unwrap();
                let position =
                    line.content_mask.as_ref().unwrap().bounds.origin + point(px(20.), px(100.));
                let (viewport, content) = editor.horizontal_metrics[&owner];
                assert!(
                    content >= 1500. && content > viewport,
                    "viewport={viewport}, content={content}, previews={:?}",
                    editor
                        .visual_lines
                        .iter()
                        .map(|line| line.html_preview.as_ref().map(|p| (p.width, p.height)))
                        .collect::<Vec<_>>()
                );
                let before = editor.html_preview_at(position).unwrap().2;
                editor.on_scroll_wheel(
                    &ScrollWheelEvent {
                        position,
                        delta: gpui::ScrollDelta::Pixels(point(px(-10000.), px(0.))),
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                let offset = editor.horizontal_scrolls[&owner];
                assert_eq!(offset, content - viewport);
                let after = editor.html_preview_at(position).unwrap().2;
                assert!((after - before - offset / editor.zoom_factor).abs() < 1.);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        });
    }

    #[gpui::test]
    fn native_horizontal_delta_reveals_trailing_table_columns(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(concat!(
                "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[2000.0,2000.0]} -->\n",
                "| first | last |\n| --- | --- |\n| left | right |\n"
            )).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let line = editor
                    .painted_lines
                    .iter()
                    .find(|line| line.horizontal_owner.is_some())
                    .unwrap();
                let owner = line.horizontal_owner.unwrap();
                let position =
                    line.content_mask.as_ref().unwrap().bounds.origin + point(px(2.), px(2.));
                let (viewport, content) = editor.horizontal_metrics[&owner];
                assert!(content > viewport);
                let mut event = ScrollWheelEvent {
                    position,
                    delta: gpui::ScrollDelta::Pixels(point(px(-10000.), px(0.))),
                    ..Default::default()
                };
                editor.on_scroll_wheel(&event, window, cx);
                assert_eq!(
                    editor.horizontal_scrolls.get(&owner).copied().unwrap_or(0.),
                    content - viewport
                );
                event.delta = gpui::ScrollDelta::Pixels(point(px(50.), px(0.)));
                editor.on_scroll_wheel(&event, window, cx);
                assert_eq!(
                    editor.horizontal_scrolls[&owner],
                    (content - viewport - 50.).max(0.)
                );
            });
        });
    }

    #[test]
    fn wide_table_columns_keep_physical_width_and_scroll_locally() {
        let document = Document::from_markdown(concat!(
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[600.0,600.0]} -->\n",
            "| a | b |\n| --- | --- |\n"
        ))
        .expect("wide table");
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = build_visual_lines(&document, &projection)
            .into_iter()
            .filter(|line| line.table_cell.is_some())
            .collect::<Vec<_>>();
        assert!((lines[0].width_fraction * 760. - 600.).abs() < 0.01);
        assert!((lines[1].x_fraction * 760. - 600.).abs() < 0.01);
        assert_eq!(clamped_horizontal_scroll(0., 900., 760., 1200.), 440.);
        assert_eq!(clamped_horizontal_scroll(440., -100., 760., 1200.), 340.);
        assert_eq!(clamped_horizontal_scroll(0., 100., 760., 600.), 0.);
    }

    #[test]
    fn image_headers_replace_the_placeholder_with_aspect_ratio_layout() {
        let document = Document::from_markdown("![wide](wide.png)").expect("image document");
        let snapshot = document.snapshot();
        let image_id = snapshot.blocks().get(0).expect("image").id();
        let projection = TextProjection::from_snapshot(&snapshot);
        let placeholder = build_visual_lines(&document, &projection);
        assert_eq!(placeholder[0].style.line_height, 180.);

        let dimensions = HashMap::from([(image_id, ("wide.png".into(), (800, 400)))]);
        let measured = build_visual_lines_with_images(&projection, &dimensions, 600.);
        assert_eq!(measured[0].style.line_height, 300.);
    }

    #[test]
    fn rectangular_table_selection_exports_tsv() {
        let mut document = Document::from_markdown("| h1 | h2 |\n| --- | --- |\n| a | b |\n")
            .expect("table document");
        let snapshot = document.snapshot();
        let table_id = snapshot
            .blocks()
            .iter()
            .find_map(|block| matches!(block.as_ref(), BlockNode::Table(_)).then_some(block.id()))
            .expect("table");
        let selection = RectangularSelection {
            table_id,
            anchor_row: 0,
            anchor_column: 0,
            head_row: 1,
            head_column: 1,
        };
        document
            .apply(EditCommand::SetSelection(Selection::Table(selection)))
            .expect("selection");
        assert_eq!(
            document
                .snapshot()
                .clipboard_payload()
                .expect("payload")
                .and_then(|payload| payload.plain_text)
                .as_deref(),
            Some("h1\th2\na\tb")
        );
    }

    #[gpui::test]
    fn painted_resize_dispatches_reflow_without_another_input(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = include_str!("../../../performance/layout-fixtures/79-technical-sections.md");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        for width in [650., 1040., 650., 1040.] {
            cx.simulate_resize(size(px(width), px(680.)));
            cx.update(|_, cx| {
                editor.update(cx, |editor, cx| {
                    // Exercise the paint publication boundary directly: GPUI's
                    // test scheduler can otherwise deliver extra renders and
                    // hide the missing-dispatch bug observed on Wayland.
                    let mut bounds = editor.element_bounds.unwrap();
                    bounds.size.width = px(width);
                    editor.publish_painted_bounds(bounds, cx);
                    assert_eq!(editor.requested_layout_width, width);
                    assert!(editor.reflow.is_active());
                });
            });
            cx.run_until_parked();
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.layout_width, width);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        }
    }

    #[gpui::test]
    fn resize_burst_commits_only_current_viewport_after_in_flight_work(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = include_str!("../../../performance/layout-fixtures/79-technical-sections.md");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        let selection = editor.read_with(cx, |editor, _| editor.selection.clone());
        for final_size in [(1040., 1000.), (650., 420.)] {
            // Do not drain the executor between resize events. The old request
            // remains in flight as paint publishes successive viewport sizes.
            for (width, height) in [(650., 420.), (1040., 1000.), (790., 580.), final_size] {
                cx.simulate_resize(size(px(width), px(height)));
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                editor.read_with(cx, |editor, _| assert!(editor.reflow.is_active()));
            }
            cx.run_until_parked();
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.layout_width, final_size.0);
                assert_eq!(
                    editor.adaptive.measured_rows.viewport,
                    editor.scroll_metrics().1
                );
                assert!(!editor.reflow.is_active());
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        }
    }

    #[gpui::test]
    fn table_keyboard_navigation_reveals_overflowing_cells(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = concat!(
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[400.0,400.0,400.0]} -->\n",
            "| first | middle | last |\n| --- | --- | --- |\n| a | b | c |\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(500.), px(700.)));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let cursor = editor.projection.text().find("first").unwrap();
                editor.focus_handle.focus(window, cx);
                editor.set_selection(cursor..cursor, false, window, cx);
            });
        });
        for (reverse, expected) in [
            (false, "middle"),
            (false, "last"),
            (true, "middle"),
            (true, "first"),
        ] {
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    if reverse {
                        editor.previous_table_cell(&PreviousTableCell, window, cx);
                    } else {
                        editor.next_table_cell(&NextTableCell, window, cx);
                    }
                });
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                let cursor = editor.cursor_offset();
                assert_eq!(cursor, editor.projection.text().find(expected).unwrap());
                let line = editor
                    .painted_lines
                    .iter()
                    .find(|line| line.range.contains(&cursor) || line.range.end == cursor)
                    .expect("Tab must reveal the selected cell, not leave the caret offscreen");
                let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                    + shaped_x_for_index(&line.layout, cursor - line.range.start);
                let viewport = line.content_mask.unwrap().bounds;
                assert!(
                    x >= viewport.left() && x + px(1.5) <= viewport.right(),
                    "{expected} must be horizontally visible"
                );
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        }
    }

    #[gpui::test]
    fn document_boundary_navigation_uses_published_table_and_viewport_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        fn assert_caret_visible(editor: &RichDocumentEditor, expected: usize) {
            assert_eq!(editor.cursor_offset(), expected);
            let line = editor
                .painted_lines
                .iter()
                .find(|line| line.range.contains(&expected) || line.range.end == expected)
                .expect("document-boundary navigation must paint the destination caret");
            let viewport = line
                .content_mask
                .map(|mask| mask.bounds)
                .or(editor.element_bounds)
                .expect("painted editor has viewport bounds");
            assert!(
                line.bounds.top() >= viewport.top() && line.bounds.bottom() <= viewport.bottom(),
                "document-boundary caret must be vertically visible"
            );
            if line.content_mask.is_some() {
                let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                    + shaped_x_for_index(&line.layout, expected.saturating_sub(line.range.start));
                assert!(
                    x >= viewport.left() && x + px(1.5) <= viewport.right(),
                    "document-boundary caret must be visible inside the table viewport"
                );
            }
        }

        cx.update(init_editor);
        let source = format!(
            "start\n\n{}\n<!-- tachyon-table:v1 {{\"border\":\"Dotted\",\"widths\":[400.0,400.0,400.0]}} -->\n| first | middle | last |\n| --- | --- | --- |\n| a | b | finish |",
            (0..24)
                .map(|index| format!(
                    "paragraph {index} keeps the document taller than its viewport.\n\n"
                ))
                .collect::<String>()
        );
        let document = Document::from_markdown(source.as_str()).unwrap();
        let (editor, cx) =
            cx.add_window_view(|window, cx| RichDocumentEditor::new(document, window, cx));
        cx.simulate_resize(size(px(500.), px(260.)));

        for zoom in [1., 2.] {
            editor.update(cx, |editor, cx| editor.set_zoom_factor(zoom, cx));
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            let middle = editor.read_with(cx, |editor, _| {
                editor.projection.text().find("middle").unwrap()
            });
            let end = editor.read_with(cx, |editor, _| editor.editing_text().len());

            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.focus_handle.focus(window, cx);
                    editor.set_selection(middle..middle, false, window, cx);
                });
            });
            cx.simulate_keystrokes("ctrl-home");
            for _ in 0..3 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| assert_caret_visible(editor, 0));

            cx.simulate_keystrokes("ctrl-end");
            for _ in 0..3 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| assert_caret_visible(editor, end));

            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.set_selection(middle..middle, false, window, cx);
                });
            });
            cx.simulate_keystrokes("ctrl-shift-home");
            for _ in 0..3 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.selected_byte_range(), (0..middle, true));
                assert_caret_visible(editor, 0);
            });

            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.set_selection(middle..middle, false, window, cx);
                });
            });
            cx.simulate_keystrokes("ctrl-shift-end");
            for _ in 0..3 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.selected_byte_range(), (middle..end, false));
                assert_caret_visible(editor, end);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        }
    }

    #[gpui::test]
    fn inserting_table_rows_and_columns_preserves_the_visible_anchor(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = format!(
            "{}| Left | Right |\n| --- | --- |\n| Alpha | Beta |\n\n{}",
            "Preamble keeps the caret above the viewport.\n\n".repeat(24),
            "Trailing content leaves room below the table.\n\n".repeat(12),
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        cx.simulate_resize(size(px(700.), px(250.)));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }

        for insert_column in [false, true] {
            let (anchor, command) = cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.set_selection(0..0, false, window, cx);
                    let table_id = editor
                        .projection
                        .roots()
                        .find_map(|root| matches!(root, BlockNode::Table(_)).then_some(root.id()))
                        .expect("table");
                    let BlockNode::Table(table) = editor.projection.block(table_id).unwrap() else {
                        unreachable!()
                    };
                    let command = if insert_column {
                        EditCommand::InsertTableColumn {
                            table_id,
                            index: table.column_count(),
                        }
                    } else {
                        EditCommand::InsertTableRow {
                            table_id,
                            index: table.row_count(),
                        }
                    };
                    let table_top = editor
                        .visual_lines
                        .iter()
                        .find(|line| {
                            line.table_cell.is_some_and(|(id, row, column, _)| {
                                id == table_id && row == 0 && column == 0
                            })
                        })
                        .expect("table header line")
                        .y;
                    editor.set_scroll_y(table_top, cx);
                    let anchor = capture_scroll_anchor(
                        &editor.document.snapshot(),
                        &editor.projection,
                        &editor.visual_lines,
                        editor.scroll_metrics().0,
                    )
                    .expect("visible table anchor");
                    (anchor, command)
                })
            });

            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.apply_structural_command(command, window, cx);
                });
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                let resolved = resolve_scroll_anchor(
                    &editor.document.snapshot(),
                    &editor.projection,
                    &editor.visual_lines,
                    &anchor,
                )
                .expect("surviving visible table anchor");
                assert!(
                    (editor.scroll_metrics().0 - resolved).abs() < 0.1,
                    "{} insertion moved the visible anchor: scroll={}, anchor={resolved}",
                    if insert_column { "column" } else { "row" },
                    editor.scroll_metrics().0,
                );
            });
        }
    }

    #[gpui::test]
    fn table_row_menu_changes_preserve_selection_and_history_reveals_the_caret(
        cx: &mut gpui::TestAppContext,
    ) {
        fn assert_visible(editor: &RichDocumentEditor) {
            let cursor = editor.cursor_offset();
            let line = editor
                .painted_lines
                .iter()
                .find(|line| line.range.contains(&cursor) || line.range.end == cursor)
                .expect("structural edit history must reveal the caret");
            let viewport = line.content_mask.unwrap().bounds;
            let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                + shaped_x_for_index(&line.layout, cursor - line.range.start);
            assert!(x >= viewport.left() && x + px(1.5) <= viewport.right());
            assert!(
                line.bounds.top() >= viewport.top() && line.bounds.bottom() <= viewport.bottom(),
                "structural edit history caret must be vertically visible"
            );
        }
        cx.update(init_editor);
        let source = include_str!("../../../performance/layout-fixtures/122-paired-records.md");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(700.), px(250.)));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        for (zoom, delete) in [(1., false), (1., true), (2., false), (2., true)] {
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.set_zoom_factor(zoom, cx);
                    let marker = "Collects unresolved questions";
                    let cursor = editor.projection.text().find(marker).unwrap() + marker.len();
                    editor.focus_handle.focus(window, cx);
                    editor.set_selection(cursor..cursor, false, window, cx);
                    editor.keep_offset_visible(cursor);
                });
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            let original_selection = editor.read_with(cx, |editor, _| editor.selection.clone());
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    let Selection::Text(selection) = &editor.selection else {
                        panic!("text caret")
                    };
                    let (table_id, row, _) = editor
                        .document
                        .snapshot()
                        .table_cell_containing(selection.head.node_id)
                        .unwrap();
                    let command = if delete {
                        EditCommand::DeleteTableRow {
                            table_id,
                            index: row,
                        }
                    } else {
                        EditCommand::InsertTableRow {
                            table_id,
                            index: row,
                        }
                    };
                    editor.apply_structural_command(command, window, cx);
                });
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                let Selection::Text(selection) = &editor.selection else {
                    panic!("surviving text caret")
                };
                let snapshot = editor.document.snapshot();
                assert!(snapshot.validates_position(selection.head));
                let (_, row, column) = snapshot
                    .table_cell_containing(selection.head.node_id)
                    .unwrap();
                assert_eq!((row, column), (if delete { 2 } else { 3 }, 1));
                if delete {
                    let cursor = editor.cursor_offset();
                    let line = editor
                        .painted_lines
                        .iter()
                        .find(|line| line.range.contains(&cursor) || line.range.end == cursor)
                        .expect("row deletion must reveal the surviving caret");
                    let viewport = line.content_mask.unwrap().bounds;
                    assert!(
                        line.bounds.top() >= viewport.top()
                            && line.bounds.bottom() <= viewport.bottom(),
                        "surviving caret line must be visible after row deletion"
                    );
                }
            });
            let (changed_source, changed_selection) = editor.read_with(cx, |editor, _| {
                if delete {
                    assert_visible(editor);
                }
                (
                    editor.document.snapshot().serialize().unwrap(),
                    editor.selection.clone(),
                )
            });
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    // History navigation must reveal its restored selection
                    // even if the user has scrolled since the original edit.
                    editor
                        .scroll_handle
                        .set_offset(point(px(0.), px(-editor.document_height)));
                    editor.undo(&Undo, window, cx);
                    assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                });
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.selection, original_selection);
                assert_visible(editor);
            });
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| editor.redo(&Redo, window, cx));
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                assert_eq!(
                    editor.document.snapshot().serialize().unwrap(),
                    changed_source
                );
                assert_eq!(editor.selection, changed_selection);
                assert_visible(editor);
            });
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| editor.undo(&Undo, window, cx));
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert_eq!(editor.selection, original_selection);
                assert_visible(editor);
            });
        }
    }

    #[gpui::test]
    fn table_column_history_preserves_saved_widths_selection_and_view(
        cx: &mut gpui::TestAppContext,
    ) {
        fn assert_visible(editor: &RichDocumentEditor) {
            let cursor = editor.cursor_offset();
            let line = editor
                .painted_lines
                .iter()
                .find(|line| line.range.contains(&cursor) || line.range.end == cursor)
                .expect("column history must reveal the surviving caret");
            let viewport = line
                .content_mask
                .expect("overflowing table viewport")
                .bounds;
            let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                + shaped_x_for_index(&line.layout, cursor - line.range.start);
            assert!(x >= viewport.left() && x + px(1.5) <= viewport.right());
            assert!(
                line.bounds.top() >= viewport.top() && line.bounds.bottom() <= viewport.bottom()
            );
        }

        fn widths(editor: &RichDocumentEditor, table_id: NodeId) -> Vec<Option<f32>> {
            let snapshot = editor.document.snapshot();
            let BlockNode::Table(table) = snapshot.node(table_id).expect("table") else {
                panic!("table node")
            };
            table.columns.iter().map(|column| column.width).collect()
        }

        cx.update(init_editor);
        let source = concat!(
            "Before the table.\n\n",
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[400.0,400.0,400.0]} -->\n",
            "| left head | middle head | right head |\n",
            "| --- | --- | --- |\n",
            "| left body | middle body | right body |\n",
            "\nAfter the table.\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(500.), px(280.)));

        for (zoom, delete) in [(1., false), (1., true), (2., false), (2., true)] {
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.set_zoom_factor(zoom, cx);
                    let cursor = editor.projection.text().find("right body").unwrap();
                    editor.focus_handle.focus(window, cx);
                    editor.set_selection(cursor..cursor, false, window, cx);
                    editor.keep_offset_visible(cursor);
                });
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            let (table_id, original_selection, original_horizontal_scroll) =
                editor.read_with(cx, |editor, _| {
                    let Selection::Text(selection) = &editor.selection else {
                        panic!("text caret")
                    };
                    let (table_id, row, column) = editor
                        .document
                        .snapshot()
                        .table_cell_containing(selection.head.node_id)
                        .unwrap();
                    assert_eq!((row, column), (1, 2));
                    assert_eq!(widths(editor, table_id), vec![Some(400.); 3]);
                    (
                        table_id,
                        editor.selection.clone(),
                        editor.horizontal_scrolls.get(&table_id).copied(),
                    )
                });

            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    let command = if delete {
                        EditCommand::DeleteTableColumn { table_id, index: 0 }
                    } else {
                        EditCommand::InsertTableColumn { table_id, index: 0 }
                    };
                    editor.apply_structural_command(command, window, cx);
                });
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            let (changed_source, changed_selection) = editor.read_with(cx, |editor, _| {
                let Selection::Text(selection) = &editor.selection else {
                    panic!("surviving text caret")
                };
                let (_, row, column) = editor
                    .document
                    .snapshot()
                    .table_cell_containing(selection.head.node_id)
                    .unwrap();
                assert_eq!((row, column), (1, if delete { 1 } else { 3 }));
                assert_eq!(
                    widths(editor, table_id),
                    if delete {
                        vec![Some(400.); 2]
                    } else {
                        vec![None, Some(400.), Some(400.), Some(400.)]
                    }
                );
                if delete {
                    assert_visible(editor);
                } else {
                    assert_eq!(
                        editor.horizontal_scrolls.get(&table_id).copied(),
                        original_horizontal_scroll,
                        "column insertion must retain the local table scroll position"
                    );
                }
                (
                    editor.document.snapshot().serialize().unwrap(),
                    editor.selection.clone(),
                )
            });

            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    editor
                        .horizontal_scrolls
                        .insert(table_id, editor.document_height);
                    editor.undo(&Undo, window, cx);
                });
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert_eq!(editor.selection, original_selection);
                assert_eq!(widths(editor, table_id), vec![Some(400.); 3]);
                assert_visible(editor);
            });

            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| editor.redo(&Redo, window, cx));
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                assert_eq!(
                    editor.document.snapshot().serialize().unwrap(),
                    changed_source
                );
                assert_eq!(editor.selection, changed_selection);
                assert_visible(editor);
            });

            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| editor.undo(&Undo, window, cx));
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert_eq!(editor.selection, original_selection);
                assert_visible(editor);
            });
        }
    }

    #[gpui::test]
    fn growing_table_ime_preedit_keeps_its_measured_caret_in_view(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = concat!(
            "Before.\n\n",
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[400.0,400.0,400.0]} -->\n",
            "| left head | middle head | right head |\n",
            "| --- | --- | --- |\n",
            "| left body | middle body | tail |\n",
            "\nAfter.\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(500.), px(220.)));
        editor.update(cx, |editor, cx| editor.set_zoom_factor(2., cx));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }

        let preedit = "界".repeat(280);
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let cursor = editor.projection.text().find("tail").unwrap() + "tail".len();
                editor.focus_handle.focus(window, cx);
                editor.set_selection(cursor..cursor, false, window, cx);
                editor.keep_offset_visible(cursor);
                editor.replace_and_mark_text_in_range(None, &"界".repeat(40), None, window, cx);
                assert!(editor.composition_active());
                editor.replace_and_mark_text_in_range(None, &preedit, Some(279..280), window, cx);
            });
        });
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let cursor = editor.cursor_offset();
                let line = editor
                    .painted_lines
                    .iter()
                    .find(|line| line.range.contains(&cursor) || line.range.end == cursor)
                    .expect("the current IME caret must be painted");
                let viewport = line.content_mask.expect("table viewport").bounds;
                let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                    + shaped_x_for_index(&line.layout, cursor - line.range.start);
                assert!(x >= viewport.left() && x + px(1.5) <= viewport.right());
                assert!(
                    line.bounds.top() >= viewport.top()
                        && line.bounds.bottom() <= viewport.bottom(),
                    "the growing preedit caret must stay vertically visible"
                );
                let selected = editor.selected_text_range(false, window, cx).unwrap();
                let candidate = editor
                    .bounds_for_range(selected.range, viewport, window, cx)
                    .expect("IME candidate geometry must use the painted caret line");
                assert!(
                    candidate.top() >= viewport.top() && candidate.bottom() <= viewport.bottom()
                );

                assert!(editor.cancel_pending_composition(cx).unwrap());
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));

                let cursor = editor.projection.text().find("tail").unwrap() + "tail".len();
                editor.set_selection(cursor..cursor, false, window, cx);
                editor.replace_and_mark_text_in_range(None, "仮", None, window, cx);
                editor.replace_text_in_range(None, "確定", window, cx);
                assert!(!editor.composition_active());
                let committed = editor.document.snapshot().serialize().unwrap();
                assert!(committed.contains("tail確定"), "{committed}");
                assert!(committed.contains("\"widths\":[400.0,400.0,400.0]"));
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        });
    }

    #[gpui::test]
    fn resizing_during_table_ime_keeps_the_caret_in_the_painted_viewport(
        cx: &mut gpui::TestAppContext,
    ) {
        fn assert_caret_visible(editor: &RichDocumentEditor) {
            let cursor = editor.cursor_offset();
            let line = editor
                .painted_lines
                .iter()
                .find(|line| line.range.contains(&cursor) || line.range.end == cursor)
                .unwrap_or_else(|| {
                    panic!(
                        "the active IME caret at {cursor} must be painted; painted ranges: {:?}",
                        editor
                            .painted_lines
                            .iter()
                            .map(|line| line.range.clone())
                            .collect::<Vec<_>>()
                    )
                });
            let viewport = line.content_mask.expect("table viewport").bounds;
            let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                + shaped_x_for_index(&line.layout, cursor - line.range.start);
            assert!(
                x >= viewport.left() && x + px(1.5) <= viewport.right(),
                "IME caret {x:?} must remain inside resized viewport {viewport:?}"
            );
            assert!(
                line.bounds.top() >= viewport.top() && line.bounds.bottom() <= viewport.bottom(),
                "IME caret line must remain inside the resized viewport"
            );
        }

        fn widths(editor: &RichDocumentEditor, table_id: NodeId) -> Vec<Option<f32>> {
            let snapshot = editor.document.snapshot();
            let BlockNode::Table(table) = snapshot.node(table_id).expect("table") else {
                panic!("table node")
            };
            table.columns.iter().map(|column| column.width).collect()
        }

        cx.update(init_editor);
        let source = concat!(
            "Before.\n\n",
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[400.0,400.0,400.0]} -->\n",
            "| left head | middle head | right head |\n",
            "| --- | --- | --- |\n",
            "| left body | middle body | tail |\n",
            "\nAfter.\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(500.), px(220.)));
        editor.update(cx, |editor, cx| editor.set_zoom_factor(2., cx));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }

        let table_id = cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let cursor = editor.projection.text().find("tail").unwrap() + "tail".len();
                editor.focus_handle.focus(window, cx);
                editor.set_selection(cursor..cursor, false, window, cx);
                editor.keep_offset_visible(cursor);
                editor.replace_and_mark_text_in_range(None, &"界".repeat(40), None, window, cx);
                editor.replace_and_mark_text_in_range(
                    None,
                    &"界".repeat(280),
                    Some(279..280),
                    window,
                    cx,
                );
                let Selection::Text(selection) = &editor.selection else {
                    panic!("text composition")
                };
                editor
                    .document
                    .snapshot()
                    .table_cell_containing(selection.head.node_id)
                    .expect("table cell")
                    .0
            })
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.read_with(cx, |editor, _| {
            assert!(editor.composition_active());
            assert_caret_visible(editor);
            assert_eq!(widths(editor, table_id), vec![Some(400.); 3]);
        });

        cx.simulate_resize(size(px(360.), px(160.)));
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                assert!(editor.composition_active());
                assert_eq!(editor.layout_width, 500.);
                assert_eq!(editor.requested_layout_width, 360.);
                assert_caret_visible(editor);
                let selected = editor.selected_text_range(false, window, cx).unwrap();
                let line = editor
                    .painted_lines
                    .iter()
                    .find(|line| {
                        line.range.contains(&editor.cursor_offset())
                            || line.range.end == editor.cursor_offset()
                    })
                    .unwrap();
                let viewport = line.content_mask.unwrap().bounds;
                let candidate = editor
                    .bounds_for_range(selected.range, viewport, window, cx)
                    .expect("candidate bounds");
                assert!(
                    candidate.top() >= viewport.top() && candidate.bottom() <= viewport.bottom()
                );
                editor.replace_text_in_range(None, "確定", window, cx);
            });
        });
        for _ in 0..5 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                assert!(!editor.composition_active());
                assert_eq!(editor.layout_width, 360.);
                assert!(
                    (editor.adaptive.measured_rows.viewport
                        - editor.scroll_metrics().1 / editor.zoom_factor)
                        .abs()
                        < 0.5
                );
                assert_caret_visible(editor);
                assert_eq!(widths(editor, table_id), vec![Some(400.); 3]);
                let committed = editor.document.snapshot().serialize().unwrap();
                assert!(committed.contains("tail確定"), "{committed}");
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert_eq!(widths(editor, table_id), vec![Some(400.); 3]);
            });
        });
    }

    #[gpui::test]
    fn rtl_table_arrows_and_cell_navigation_share_painted_geometry(cx: &mut gpui::TestAppContext) {
        fn caret_x(editor: &RichDocumentEditor) -> Pixels {
            let cursor = editor.cursor_offset();
            let line = editor
                .painted_lines
                .iter()
                .find(|line| line.range.contains(&cursor) || line.range.end == cursor)
                .expect("RTL caret line must be painted");
            let viewport = line.content_mask.expect("table viewport").bounds;
            let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                + shaped_x_for_index(&line.layout, cursor - line.range.start);
            assert!(x >= viewport.left() && x + px(1.5) <= viewport.right());
            assert!(
                line.bounds.top() >= viewport.top() && line.bounds.bottom() <= viewport.bottom()
            );
            x
        }

        cx.update(init_editor);
        let source = concat!(
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[400.0,400.0,400.0]} -->\n",
            "| بداية عربية طويلة للاختبار | אמצע עברי לבדיקה | نهاية عربية |\n",
            "| --- | --- | --- |\n",
            "| أول | وسط | آخر |\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(500.), px(320.)));
        editor.update(cx, |editor, cx| editor.set_zoom_factor(2., cx));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }

        let start = editor.read_with(cx, |editor, _| {
            editor.projection.text().find("عربية").unwrap()
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.focus_handle.focus(window, cx);
                editor.set_selection(start..start, false, window, cx);
                editor.keep_offset_visible(start);
            });
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        let original_x = editor.read_with(cx, |editor, _| caret_x(editor));
        cx.simulate_keystrokes("left");
        for _ in 0..2 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.read_with(cx, |editor, _| {
            let moved = editor.cursor_offset();
            let moved_x = caret_x(editor);
            assert_ne!(
                moved, start,
                "visual Left must visit another RTL caret stop"
            );
            assert!(moved_x < original_x, "Left must move visually left");
        });
        cx.simulate_keystrokes("right");
        for _ in 0..2 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.cursor_offset(), start);
            assert!((caret_x(editor) - original_x).abs() < px(0.5));
        });

        for (keys, expected) in [
            ("tab", "אמצע"),
            ("tab", "نهاية"),
            ("shift-tab", "אמצע"),
            ("shift-tab", "بداية"),
        ] {
            cx.simulate_keystrokes(keys);
            for _ in 0..3 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.read_with(cx, |editor, _| {
                assert_eq!(
                    editor.cursor_offset(),
                    editor.projection.text().find(expected).unwrap()
                );
                _ = caret_x(editor);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                let snapshot = editor.document.snapshot();
                let table = snapshot
                    .blocks()
                    .iter()
                    .find_map(|block| match block.as_ref() {
                        BlockNode::Table(table) => Some(table),
                        _ => None,
                    })
                    .unwrap();
                assert_eq!(
                    table
                        .columns
                        .iter()
                        .map(|column| column.width)
                        .collect::<Vec<_>>(),
                    vec![Some(400.); 3]
                );
            });
        }
    }

    #[gpui::test]
    fn rtl_table_ime_preedit_uses_painted_caret_and_candidate_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        fn assert_visible(editor: &RichDocumentEditor) -> Bounds<Pixels> {
            let cursor = editor.cursor_offset();
            let line = editor
                .painted_lines
                .iter()
                .find(|line| line.range.contains(&cursor) || line.range.end == cursor)
                .expect("RTL preedit caret line must be painted");
            let viewport = line.content_mask.expect("table viewport").bounds;
            let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                + shaped_x_for_index(&line.layout, cursor - line.range.start);
            assert!(x >= viewport.left() && x + px(1.5) <= viewport.right());
            assert!(
                line.bounds.top() >= viewport.top() && line.bounds.bottom() <= viewport.bottom()
            );
            viewport
        }

        fn widths(editor: &RichDocumentEditor, table_id: NodeId) -> Vec<Option<f32>> {
            let snapshot = editor.document.snapshot();
            let BlockNode::Table(table) = snapshot.node(table_id).expect("table") else {
                panic!("table node")
            };
            table.columns.iter().map(|column| column.width).collect()
        }

        cx.update(init_editor);
        let source = concat!(
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[400.0,400.0,400.0]} -->\n",
            "| بداية عربية | אמצע עברי | نهاية عربية |\n",
            "| --- | --- | --- |\n",
            "| LTR אבג 123 عربية XYZ | وسط | ختام |\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(500.), px(220.)));
        editor.update(cx, |editor, cx| editor.set_zoom_factor(2., cx));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }

        let table_id = cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let cursor =
                    editor.projection.text().find("نهاية عربية").unwrap() + "نهاية عربية".len();
                editor.focus_handle.focus(window, cx);
                editor.set_selection(cursor..cursor, false, window, cx);
                editor.keep_offset_visible(cursor);
                let preedit = "إدخال־עברית ".repeat(80);
                let utf16 = preedit.encode_utf16().count();
                editor.replace_and_mark_text_in_range(
                    None,
                    &preedit,
                    Some(utf16 - 1..utf16),
                    window,
                    cx,
                );
                let Selection::Text(selection) = &editor.selection else {
                    panic!("text composition")
                };
                editor
                    .document
                    .snapshot()
                    .table_cell_containing(selection.head.node_id)
                    .expect("table cell")
                    .0
            })
        });
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                assert!(editor.composition_active());
                let viewport = assert_visible(editor);
                let selected = editor.selected_text_range(false, window, cx).unwrap();
                let candidate = editor
                    .bounds_for_range(selected.range, viewport, window, cx)
                    .expect("RTL candidate bounds");
                assert!(
                    candidate.top() >= viewport.top() && candidate.bottom() <= viewport.bottom()
                );
                assert_eq!(widths(editor, table_id), vec![Some(400.); 3]);
                assert!(editor.cancel_pending_composition(cx).unwrap());
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(matches!(
                    editor.document.undo(),
                    Err(DocumentError::NothingToUndo)
                ));

                let cursor =
                    editor.projection.text().find("نهاية عربية").unwrap() + "نهاية عربية".len();
                editor.set_selection(cursor..cursor, false, window, cx);
                editor.replace_and_mark_text_in_range(None, "קלט", None, window, cx);
                editor.replace_text_in_range(None, "تم", window, cx);
                assert!(!editor.composition_active());
                assert!(
                    editor
                        .document
                        .snapshot()
                        .serialize()
                        .unwrap()
                        .contains("نهاية عربيةتم")
                );
                assert_eq!(widths(editor, table_id), vec![Some(400.); 3]);
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert_eq!(widths(editor, table_id), vec![Some(400.); 3]);
            });
        });
    }

    #[gpui::test]
    fn mixed_direction_selection_spanning_table_cells_reveals_each_collapsed_edge(
        cx: &mut gpui::TestAppContext,
    ) {
        fn assert_visible(editor: &RichDocumentEditor, expected: usize) {
            assert_eq!(editor.cursor_offset(), expected);
            let line = editor
                .painted_lines
                .iter()
                .find(|line| line.range.contains(&expected) || line.range.end == expected)
                .expect("collapsed mixed-direction selection edge must be painted");
            let viewport = line.content_mask.expect("table viewport").bounds;
            let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                + shaped_x_for_index(&line.layout, expected - line.range.start);
            assert!(x >= viewport.left() && x + px(1.5) <= viewport.right());
        }

        cx.update(init_editor);
        let source = concat!(
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[400.0,400.0,400.0]} -->\n",
            "| LTR אבג 123 عربية XYZ | אמצע middle | نهاية tail |\n",
            "| --- | --- | --- |\n",
            "| first | second | third |\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(500.), px(240.)));
        editor.update(cx, |editor, cx| editor.set_zoom_factor(2., cx));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }

        let (start, end) = editor.read_with(cx, |editor, _| {
            (
                editor.projection.text().find("LTR").unwrap(),
                editor.projection.text().find("tail").unwrap() + "tail".len(),
            )
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.focus_handle.focus(window, cx);
                editor.set_selection(start..end, false, window, cx);
                editor.keep_offset_visible(end);
                assert_eq!(editor.selected_byte_range(), (start..end, false));
                let selected = &editor.editing_text()[start..end];
                assert!(selected.contains("אבג 123 عربية"));
                assert!(selected.contains("אמצע middle"));
                assert!(selected.ends_with("نهاية tail"));
            });
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.read_with(cx, |editor, _| assert_visible(editor, end));

        cx.simulate_keystrokes("left");
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.read_with(cx, |editor, _| {
            assert_visible(editor, start);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });

        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_selection(start..end, false, window, cx);
                editor.keep_offset_visible(start);
            });
        });
        cx.simulate_keystrokes("right");
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.read_with(cx, |editor, _| {
            assert_visible(editor, end);
            let snapshot = editor.document.snapshot();
            let table = snapshot
                .blocks()
                .iter()
                .find_map(|block| match block.as_ref() {
                    BlockNode::Table(table) => Some(table),
                    _ => None,
                })
                .unwrap();
            assert_eq!(
                table
                    .columns
                    .iter()
                    .map(|column| column.width)
                    .collect::<Vec<_>>(),
                vec![Some(400.); 3]
            );
            assert_eq!(snapshot.serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn table_row_append_reveals_the_new_empty_cell_and_undo_restores_source(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(init_editor);
        let source = include_str!("../../../performance/layout-fixtures/122-paired-records.md");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(1400.), px(250.)));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let marker = "completed_archive_retained_materials_primary";
                let cursor = editor.projection.text().find(marker).unwrap() + marker.len();
                editor.focus_handle.focus(window, cx);
                editor.set_selection(cursor..cursor, false, window, cx);
                editor.keep_offset_visible(cursor);
                // The final Steward value is intentionally empty. Tab first
                // enters it, then appends a complete new row from that cell.
                editor.next_table_cell(&NextTableCell, window, cx);
                editor.next_table_cell(&NextTableCell, window, cx);
            });
        });
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.read_with(cx, |editor, _| {
            let Selection::Text(selection) = &editor.selection else {
                panic!("new row needs a text caret")
            };
            let snapshot = editor.document.snapshot();
            let (_, row, column) = snapshot
                .table_cell_containing(selection.head.node_id)
                .unwrap();
            assert_eq!((row, column), (4, 0));
            let cursor = editor.cursor_offset();
            let range = &editor
                .projection
                .segment_for_node(selection.head.node_id)
                .unwrap()
                .projection_range();
            let line = editor
                .painted_lines
                .iter()
                .find(|line| &line.range == range)
                .expect("the newly appended empty cell must be painted");
            let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                + shaped_x_for_index(&line.layout, cursor - line.range.start);
            let viewport = line.content_mask.unwrap().bounds;
            assert!(x >= viewport.left() && x + px(1.5) <= viewport.right());
            assert!(
                line.bounds.top() >= viewport.top() && line.bounds.bottom() <= viewport.bottom(),
                "new row must be vertically visible"
            );
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        });
    }

    #[gpui::test]
    fn width_resize_reveals_the_active_trailing_table_header(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = concat!(
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[400.0,400.0,400.0]} -->\n",
            "| first | middle | last |\n| --- | --- | --- |\n| a | b | c |\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(1400.), px(700.)));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let cursor = editor.projection.text().find("last").unwrap() + 4;
                editor.focus_handle.focus(window, cx);
                editor.set_selection(cursor..cursor, false, window, cx);
                editor.replace_text_in_range(None, "x", window, cx);
                editor.last_edit_at = Some(Instant::now() - Duration::from_secs(2));
            });
        });
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        let (selection, edited) = editor.read_with(cx, |editor, _| {
            (
                editor.selection.clone(),
                editor.document.snapshot().serialize().unwrap(),
            )
        });
        cx.simulate_resize(size(px(500.), px(700.)));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.update(cx, |editor, _| {
            let cursor = editor.cursor_offset();
            let line = editor
                .painted_lines
                .iter()
                .find(|line| line.range.contains(&cursor) || line.range.end == cursor)
                .expect("the active header must be painted inside its horizontal viewport");
            let x = aligned_text_left(line.bounds, &line.layout, line.alignment)
                + shaped_x_for_index(&line.layout, cursor - line.range.start);
            let viewport = line.content_mask.unwrap().bounds;
            assert!(x >= viewport.left() && x + px(1.5) <= viewport.right());
            assert_eq!(editor.selection, selection);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), edited);
            editor.document.undo().unwrap();
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn width_resize_does_not_reveal_a_table_caret_scrolled_away(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = concat!(
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[400.0,400.0,400.0]} -->\n",
            "| first | middle | last |\n| --- | --- | --- |\n| a | b | c |\n"
        );
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(1000.), px(700.)));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let cursor = editor.projection.text().find("first").unwrap() + 5;
                editor.focus_handle.focus(window, cx);
                editor.set_selection(cursor..cursor, false, window, cx);
                editor.replace_text_in_range(None, "x", window, cx);
                editor.last_edit_at = Some(Instant::now() - Duration::from_secs(2));
            });
        });
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        let (owner, selection, edited) = editor.update(cx, |editor, cx| {
            let owner = editor
                .projection
                .segments()
                .iter()
                .find_map(|segment| segment.context.table_cell.map(|cell| cell.0))
                .unwrap();
            editor.horizontal_scrolls.insert(owner, 120.);
            cx.notify();
            (
                owner,
                editor.selection.clone(),
                editor.document.snapshot().serialize().unwrap(),
            )
        });
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.horizontal_scrolls[&owner], 120.);
            assert!(
                editor
                    .table_caret_scroll_target(editor.cursor_offset())
                    .unwrap()
                    .1
                    < 120.,
                "the edited first header is deliberately left of the table viewport"
            );
        });
        cx.simulate_resize(size(px(500.), px(700.)));
        for _ in 0..4 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        editor.update(cx, |editor, _| {
            assert_eq!(
                editor.horizontal_scrolls[&owner], 120.,
                "resizing must not undo intentional horizontal navigation"
            );
            assert_eq!(editor.selection, selection);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), edited);
            editor.document.undo().unwrap();
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn height_resize_keeps_a_previously_visible_edit_caret_in_view(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = format!(
            "{}\n{}",
            include_str!("../../../performance/layout-fixtures/119-rich-timelines.md"),
            "Retained trailing context.\n\n".repeat(40)
        );
        for scrolled_away in [false, true] {
            let (editor, cx) = cx.add_window_view(|window, cx| {
                RichDocumentEditor::new(
                    Document::from_markdown(source.as_str()).unwrap(),
                    window,
                    cx,
                )
            });
            cx.simulate_resize(size(px(1040.), px(1000.)));
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    let cursor = editor
                        .projection
                        .text()
                        .find("Preserve nested qualifications")
                        .unwrap();
                    editor.focus_handle.focus(window, cx);
                    editor.set_selection(cursor..cursor, false, window, cx);
                    editor.replace_text_in_range(None, "x", window, cx);
                    editor.last_edit_at = Some(Instant::now() - Duration::from_secs(2));
                    assert!(
                        editor.layout_focus.is_some(),
                        "exercise an active edit, not an unfocused caret"
                    );
                });
            });
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.update(cx, |editor, cx| {
                let cursor = editor.cursor_offset();
                let line = editor
                    .visual_lines
                    .iter()
                    .find(|line| line.projected_range().contains(&cursor))
                    .unwrap();
                let top = if scrolled_away {
                    line.y + 40.
                } else {
                    (line.y - 500.).max(0.)
                };
                assert!(
                    scrolled_away || line.y - top > 300.,
                    "caret begins in the lower part of the tall viewport"
                );
                editor.scroll_handle.set_offset(point(px(0.), px(-top)));
                cx.notify();
            });
            for _ in 0..3 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            let (selection, edited) = editor.read_with(cx, |editor, _| {
                (
                    editor.selection.clone(),
                    editor.document.snapshot().serialize().unwrap(),
                )
            });
            cx.simulate_resize(size(px(1040.), px(240.)));
            for _ in 0..4 {
                cx.update(|window, cx| {
                    _ = window.draw(cx);
                });
                cx.run_until_parked();
            }
            editor.update(cx, |editor, _| {
                let cursor = editor.cursor_offset();
                let line = editor.visual_lines.iter().find(|line| line.projected_range().contains(&cursor)).unwrap();
                let (top, height) = editor.scroll_metrics();
                if scrolled_away {
                    assert!(line.y + line.style.line_height < top, "reading away from an old edit must not jump back to its caret");
                } else {
                    assert!(line.y >= top && line.y + line.style.line_height <= top + height + 0.5,
                        "visible editing caret must remain inside the shorter viewport: line={}..{}, viewport={}..{}",
                        line.y, line.y + line.style.line_height, top, top + height);
                }
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), edited);
                editor.document.undo().unwrap();
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        }
    }

    #[gpui::test]
    fn height_only_resize_commits_current_viewport_without_input(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let source = include_str!("../../../performance/layout-fixtures/79-technical-sections.md");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.simulate_resize(size(px(1040.), px(1000.)));
        for _ in 0..3 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        let selection = editor.read_with(cx, |editor, _| editor.selection.clone());
        for height in [420., 1000., 420., 1000.] {
            cx.simulate_resize(size(px(1040.), px(height)));
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
            editor.read_with(cx, |editor, _| {
                let viewport = editor.scroll_metrics().1;
                assert_eq!(editor.layout_width, 1040.);
                assert_eq!(editor.painted_viewport_height, viewport);
                assert_eq!(editor.adaptive.measured_rows.viewport, viewport);
                assert_eq!(editor.selection, selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(!editor.reflow.is_active());
            });
        }
    }

    #[test]
    fn semantic_scroll_anchor_survives_insertions_above_it() {
        let original =
            Document::from_markdown("before\n\nAnchor target\n\nafter").expect("original document");
        let original_snapshot = original.snapshot();
        let original_projection = TextProjection::from_snapshot(&original_snapshot);
        let original_lines = build_visual_lines(&original, &original_projection);
        let anchor_line = original_lines
            .iter()
            .find(|line| {
                original_projection
                    .segment_for_range(&line.projected_range())
                    .and_then(|segment| original_snapshot.node(segment.node_id))
                    .is_some_and(|block| block.plain_text() == "Anchor target")
            })
            .expect("anchor line");
        let anchor = capture_scroll_anchor(
            &original_snapshot,
            &original_projection,
            &original_lines,
            anchor_line.y + 7.,
        )
        .expect("captured anchor");

        let reloaded = Document::from_markdown("inserted\n\nbefore\n\nAnchor target\n\nafter")
            .expect("reloaded document");
        let reloaded_snapshot = reloaded.snapshot();
        let reloaded_projection = TextProjection::from_snapshot(&reloaded_snapshot);
        let reloaded_lines = build_visual_lines(&reloaded, &reloaded_projection);
        let expected = reloaded_lines
            .iter()
            .find(|line| {
                reloaded_projection
                    .segment_for_range(&line.projected_range())
                    .and_then(|segment| reloaded_snapshot.node(segment.node_id))
                    .is_some_and(|block| block.plain_text() == "Anchor target")
            })
            .expect("reloaded anchor line")
            .y
            + 7.;

        assert_eq!(
            resolve_scroll_anchor(
                &reloaded_snapshot,
                &reloaded_projection,
                &reloaded_lines,
                &anchor,
            ),
            Some(expected)
        );
        assert_ne!(expected, anchor_line.y + 7.);
    }

    #[test]
    fn semantic_scroll_anchor_prefers_the_captured_line_at_a_wrap_boundary() {
        let document = Document::from_markdown("abcdefghij").expect("document");
        let snapshot = document.snapshot();
        let projection = TextProjection::from_snapshot(&snapshot);
        let segment = projection.segments().first().expect("paragraph segment");
        let split = segment.projection_start() + 5;
        let line = |range, y| VisualLineSpec {
            compact_tree: false,
            source: LineSourceRange::new(&projection, segment.node_id, range)
                .expect("test line belongs to segment"),
            payload: visual_line_payload(VisualLinePayload {
                table_row_y: y,
                table_row_height: VisualLineStyle::BODY.line_height,
                ..VisualLinePayload::default()
            }),
            style: VisualLineStyle::BODY,
            inset: 0.,
            gap_before: 0.,
            y,
            x_fraction: 0.,
            width_fraction: 1.,
            table_cell_first: false,
            flow_geometry: false,
        };
        let lines = vec![
            line(segment.projection_start()..split, 0.),
            line(split..segment.projection_end(), 57.),
        ];
        let anchor = scroll_anchor_for_line(&snapshot, &projection, &lines[1], 12.)
            .expect("wrapped-line anchor");

        assert_eq!(
            resolve_scroll_anchor(&snapshot, &projection, &lines, &anchor),
            Some(69.),
            "a boundary anchor must not resolve to the preceding wrapped line"
        );
    }

    #[test]
    fn semantic_scroll_anchor_uses_the_nearest_line_across_a_section_gap() {
        let document = Document::from_markdown("before\n\n## Nearby heading").expect("document");
        let snapshot = document.snapshot();
        let projection = TextProjection::from_snapshot(&snapshot);
        let mut lines = build_visual_lines(&document, &projection);
        assert_eq!(lines.len(), 2);
        lines[0].y = 0.;
        lines[1].y = 57.;

        let anchor =
            capture_scroll_anchor(&snapshot, &projection, &lines, 46.).expect("nearest anchor");
        assert_eq!(anchor.projection_offset, lines[1].projected_start());
        assert_eq!(anchor.intra_line_offset, -11.);
        assert_eq!(
            resolve_scroll_anchor(&snapshot, &projection, &lines, &anchor),
            Some(46.)
        );
    }

    #[test]
    fn semantic_roles_cover_rendered_document_components() {
        let document = Document::from_markdown(concat!(
            "# Heading\n\n",
            "[link](https://example.com)\n\n",
            "> quote\n\n",
            "- item\n\n",
            "![alt](image.png)\n\n",
            "| h |\n| --- |\n| c |\n"
        ))
        .expect("component document");
        let snapshot = document.snapshot();
        let projection = TextProjection::from_snapshot(&snapshot);
        let lines = build_visual_lines(&document, &projection);
        let bounds = semantic_bounds_for_lines(&projection, &lines, 0..lines.len());
        let tree = semantic_document_tree(snapshot.blocks(), &bounds);
        fn collect_roles(nodes: &[SemanticNodeSpec], roles: &mut Vec<Role>) {
            for node in nodes {
                roles.push(node.role);
                collect_roles(&node.children, roles);
            }
        }
        let mut roles = Vec::new();
        collect_roles(&tree, &mut roles);

        for expected in [
            Role::Heading,
            Role::Link,
            Role::Blockquote,
            Role::List,
            Role::ListItem,
            Role::Image,
            Role::Table,
            Role::Row,
            Role::ColumnHeader,
            Role::Cell,
        ] {
            assert!(
                roles.contains(&expected),
                "missing semantic role {expected:?}"
            );
        }
        let list = tree
            .iter()
            .find(|node| node.role == Role::List)
            .expect("list container");
        assert!(list.children.iter().all(|node| node.role == Role::ListItem));
        let table = tree
            .iter()
            .find(|node| node.role == Role::Table)
            .expect("table container");
        assert!(table.bounds.width_fraction > 0.);
        assert!(table.bounds.height > 1.);
        assert!(table.children.iter().all(|node| node.role == Role::Row));
        assert!(table.children.iter().all(|row| {
            row.children
                .iter()
                .all(|cell| matches!(cell.role, Role::ColumnHeader | Role::Cell))
        }));
        assert_eq!(table.label, "Heading table");
        assert_eq!(table.children[0].row_index, Some(1));
        assert_eq!(table.children[1].row_index, Some(2));
        assert_eq!(table.children[0].children[0].column_index, Some(1));
        assert_eq!(table.children[0].children[0].row_index, Some(1));

        let heading = tree
            .iter()
            .find(|node| node.role == Role::Heading)
            .expect("heading");
        let link = tree
            .iter()
            .find(|node| node.role == Role::Link)
            .expect("link");
        assert!(heading.bounds.height > 1.);
        assert!(link.bounds.y > heading.bounds.y);
        assert_ne!(heading.bounds, link.bounds);
    }

    #[test]
    fn semantic_task_items_expose_toggle_state_without_relabeling_plain_lists() {
        let tasks = Document::from_markdown("- [x] done\n- [ ] todo\n").expect("task list");
        let task_snapshot = tasks.snapshot();
        let task_projection = TextProjection::from_snapshot(&task_snapshot);
        let task_lines = build_visual_lines(&tasks, &task_projection);
        let task_bounds =
            semantic_bounds_for_lines(&task_projection, &task_lines, 0..task_lines.len());
        let task_tree = semantic_document_tree(task_snapshot.blocks(), &task_bounds);
        let task_list = task_tree
            .iter()
            .find(|node| node.role == Role::List)
            .expect("task list node");
        assert_eq!(task_list.children[0].role, Role::CheckBox);
        assert_eq!(task_list.children[0].toggled, Some(Toggled::True));
        assert_eq!(task_list.children[1].role, Role::CheckBox);
        assert_eq!(task_list.children[1].toggled, Some(Toggled::False));

        let plain = Document::from_markdown("- item\n").expect("plain list");
        let plain_snapshot = plain.snapshot();
        let plain_projection = TextProjection::from_snapshot(&plain_snapshot);
        let plain_lines = build_visual_lines(&plain, &plain_projection);
        let plain_bounds =
            semantic_bounds_for_lines(&plain_projection, &plain_lines, 0..plain_lines.len());
        let plain_tree = semantic_document_tree(plain_snapshot.blocks(), &plain_bounds);
        let plain_list = plain_tree
            .iter()
            .find(|node| node.role == Role::List)
            .expect("plain list node");
        assert_eq!(plain_list.children[0].role, Role::ListItem);
        assert_eq!(plain_list.children[0].toggled, None);
    }

    #[gpui::test]
    fn authoritative_geometry_and_outline_agree_across_required_width_scale_matrix(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = concat!(
                "# Measured layout agreement\n\n",
                "A compact opening paragraph uses ordinary words and wraps at every required canvas. ",
                "Its stable source identity makes it a useful reading anchor across reflow.\n\n",
                "## First section\n\n",
                "Caret points follow shaped clusters in this sentence, including café and 東京 text.\n\n",
                "- [x] Painting reads published lines\n",
                "- [ ] Hit testing reads the same lines\n",
                "- [ ] The outline uses their component positions\n\n",
                "## Second section\n\n",
                "> [!NOTE]\n",
                "> Accessibility and navigation share component bounds with the editor.\n\n",
                "The final paragraph leaves enough content below the second heading for viewport tests.\n",
            );
            let document = Document::from_markdown(source).expect("matrix document");
            let snapshot = document.snapshot();
            let outline = crate::project_outline(snapshot.blocks());
            let measurement = FontMeasurement::new(
                cx.text_system().clone(),
                "Public Sans Tachyon".into(),
                1.,
            );
            let mut previous = AdaptivePlan::default();
            let mut published_geometry = None;
            let mut previous_anchor: Option<(EditorScrollAnchor, f32)> = None;

            for width in [480., 640., 799., 800., 999., 1000., 1440.] {
                for zoom in [1., 1.25, 1.5, 2.] {
                    let (prepared, _, _) = PreparedDocumentView::prepare_snapshot_with_images(
                        &snapshot,
                        &HashMap::new(),
                        None,
                        ReflowViewport {
                            published_geometry,
                            width,
                            height: 900.,
                            zoom,
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
                        &previous,
                        &measurement,
                    );
                    let geometry = prepared
                        .published_geometry
                        .as_ref()
                        .expect("measured geometry must be published");
                    assert!(Arc::ptr_eq(&prepared.visual_lines, &geometry.lines));
                    assert!(Arc::ptr_eq(&prepared.paint_order, &geometry.paint_order));
                    assert!(Arc::ptr_eq(&prepared.components, &geometry.components));
                    assert_eq!(prepared.document_height, geometry.height);
                    assert_eq!(prepared.visual_lines.len(), prepared.paint_order.len());

                    for line in prepared.visual_lines.iter() {
                        assert!(
                            line.y.is_finite()
                                && line.style.line_height.is_finite()
                                && line.y >= 0.
                                && line.style.line_height > 0.,
                            "invalid vertical geometry at width={width} zoom={zoom}: y={} height={}",
                            line.y,
                            line.style.line_height
                        );
                        assert!(
                            line.x_fraction >= -0.001
                                && line.width_fraction > 0.
                                && line.x_fraction + line.width_fraction <= 1.001,
                            "line slot clips the canvas at width={width} zoom={zoom}: left={} width={}",
                            line.x_fraction,
                            line.width_fraction
                        );
                        let Some(segment) = segment_for_line(
                            &prepared.projection,
                            &line.projected_range(),
                        ) else {
                            continue;
                        };
                        let Some(block) = prepared.projection.block(segment.node_id) else {
                            continue;
                        };
                        if line.html_preview.is_some()
                            || line.display_math.is_some()
                            || line.diagram.is_some()
                            || segment.context.image_source.is_some()
                            || segment.context.table_cell.is_some()
                            || matches!(block, BlockNode::CodeBlock(_))
                        {
                            continue;
                        }
                        let shaped = measurement
                            .shape_unwrapped(
                                &prepared.projection,
                                line.projected_range(),
                                line.style.font_size,
                            )
                            .expect("published ordinary line can be shaped");
                        let trailing = line.command_trailing(zoom)
                            + line.slot.map_or(8. * zoom, |slot| slot.inset() * zoom);
                        let available = (width * line.width_fraction
                            - line.inset
                            - line.code_gutter()
                            - trailing)
                            .max(1.);
                        assert!(
                            f32::from(shaped.width()) <= available + 1.,
                            "measured line clips at width={width} zoom={zoom}: shaped={} available={available}",
                            f32::from(shaped.width())
                        );
                        for stop in shaped_caret_stops(&shaped) {
                            let x = shaped_x_for_index(&shaped, stop.offset);
                            assert_eq!(
                                shaped_index_for_x(&shaped, x),
                                stop.offset,
                                "caret point round trip failed at width={width} zoom={zoom}"
                            );
                        }
                    }

                    let heading_lines = outline
                        .iter()
                        .map(|entry| {
                            let line = prepared
                                .visual_lines
                                .iter()
                                .find(|line| {
                                    segment_for_line(
                                        &prepared.projection,
                                        &line.projected_range(),
                                    )
                                    .is_some_and(|segment| segment.node_id == entry.node_id)
                                })
                                .expect("outline heading has published geometry");
                            (line.y, entry.node_id)
                        })
                        .collect::<Vec<_>>();
                    for &(heading_y, _) in &heading_lines {
                        let expected = heading_lines
                            .iter()
                            .filter(|(candidate_y, _)| *candidate_y <= heading_y + 8.)
                            .max_by(|left, right| left.0.total_cmp(&right.0))
                            .and_then(|(row_y, _)| {
                                heading_lines
                                    .iter()
                                    .find(|(candidate_y, _)| candidate_y == row_y)
                            })
                            .map(|(_, id)| *id);
                        assert_eq!(
                            prepared.components.active_heading(heading_y, 1.),
                            expected,
                            "outline/component disagreement at width={width} zoom={zoom}"
                        );
                    }

                    if let Some((anchor, viewport_y)) = &previous_anchor {
                        let resolved_y = resolve_scroll_anchor(
                            &snapshot,
                            &prepared.projection,
                            &prepared.visual_lines,
                            anchor,
                        )
                        .expect("the semantic anchor survives every matrix reflow");
                        let restored_scroll = (resolved_y - viewport_y).max(0.);
                        let displacement = resolved_y - restored_scroll - viewport_y;
                        assert!(
                            displacement.abs() <= 1.,
                            "scroll anchor moved by {displacement}px at width={width} zoom={zoom}"
                        );
                    }
                    let anchor_line = prepared
                        .visual_lines
                        .iter()
                        .find(|line| {
                            prepared.projection.text()[line.projected_range()]
                                .contains("stable source identity")
                        })
                        .or_else(|| {
                            prepared.visual_lines.iter().find(|line| {
                                segment_for_line(
                                    &prepared.projection,
                                    &line.projected_range(),
                                )
                                .is_some_and(|segment| {
                                    snapshot.node(segment.node_id).is_some_and(|block| {
                                        block.plain_text().contains("stable source identity")
                                    })
                                })
                            })
                        })
                        .expect("stable anchor line");
                    let viewport_y = 7_f32.min(anchor_line.y);
                    previous_anchor = scroll_anchor_for_line(
                        &snapshot,
                        &prepared.projection,
                        anchor_line,
                        viewport_y,
                    )
                    .map(|anchor| (anchor, viewport_y));
                    previous = prepared.adaptive;
                    published_geometry = prepared.published_geometry;
                }
            }
        });
    }

    #[test]
    fn minimap_uses_editor_lines_and_retains_component_structure() {
        let document = Document::from_markdown(concat!(
            "# Repository guide\n\n",
            "A paragraph with enough words to leave a recognizable prose silhouette.\n\n",
            "- [x] inspect the geometry\n",
            "- [ ] verify the miniature\n\n",
            "```rust\n",
            "pub fn render() -> bool { true }\n",
            "```\n\n",
            "| Layer | Owner |\n",
            "| --- | --- |\n",
            "| View | Tachyon |\n\n",
            "> [!NOTE]\n",
            "> The minimap follows rendered geometry.\n",
        ))
        .expect("minimap document");
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let lines = build_visual_lines(&document, &projection);
        let document_height = visual_document_height(&lines);
        let mut minimap = Minimap::default();
        minimap.rebuild_rendered(
            lines
                .iter()
                .filter_map(|line| minimap_source_line(&projection, line, 760., 1.)),
            document_height,
            760.,
            300.,
        );

        for expected in [
            crate::MinimapPrimitiveKind::HeadingBadge,
            crate::MinimapPrimitiveKind::TaskFrame,
            crate::MinimapPrimitiveKind::CodeFrame,
            crate::MinimapPrimitiveKind::TableFrame,
            crate::MinimapPrimitiveKind::AlertFrame(MinimapAlertTone::Info),
        ] {
            assert!(
                minimap
                    .primitives()
                    .iter()
                    .any(|primitive| primitive.kind == expected),
                "missing {expected:?}"
            );
        }
        assert_eq!(
            minimap.document_offset_for_pointer(150., 300.),
            document_height / 2.
        );
    }
}
