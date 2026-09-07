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
    AnyElement, App, BorderStyle, Bounds, ClipboardItem, ContentMask, Context, CursorStyle,
    Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable,
    FontStyle, FontWeight, GlobalElementId, ImageSource, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, PaintQuad, Pixels, Point, Resource,
    Role, ScrollHandle, ScrollWheelEvent, ShapedLine, StrikethroughStyle, Style, StyledImage as _,
    Task, TextRun, Toggled, UTF16Selection, UnderlineStyle, Window, actions, div, fill, hash, img,
    outline, point, prelude::*, px, relative, rgb, rgba, size,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, IconNamed as _, Sizable as _, Theme,
    button::{Button, ButtonVariants as _},
    input::{Input, InputState},
    menu::{ContextMenuExt as _, DropdownMenu as _, PopupMenuItem},
    scroll::{Scrollbar, ScrollbarMode},
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::adaptive::{
    AdaptivePlan, CARD_PADDING, LAYOUT_GAP, LAYOUT_HEADER, LayoutSlot, ListLayout, PROSE_WIDTH,
};
use crate::{
    MineralPalette, Minimap, MinimapAlertTone, MinimapCodeTone, SharedDocumentSession,
    TextProjection,
    minimap::{MinimapSourceKind, MinimapSourceLine},
};

mod arrangement;
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
mod drag_scroll;
mod html_disclosure;
mod html_edit;
mod html_images;
mod preview_navigation;
mod preview_selection;
use html_edit::{CompositionOrigin, HtmlSelection};
mod accessibility;
mod diagnostics;
mod inline_math;
mod reflow;
mod resource_batch;
mod search;
#[cfg(feature = "layout-validation")]
mod validation;
pub use diagnostics::{LayoutDiagnosticsReport, LayoutTraceMode};

const KEY_CONTEXT: &str = "RichDocumentEditor";

fn editor_button(
    id: &'static str,
    icon: &'static str,
    label: &'static str,
    palette: MineralPalette,
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
        .icon(Icon::default().path(format!("mineral/{icon}.svg")))
        .tooltip(label)
}
const LINE_HEIGHT: f32 = 28.8;
const OUTLINE_JUMP_STEPS: u32 = 15;
const OUTLINE_JUMP_FRAME: Duration = Duration::from_millis(8);
const SHAPED_LINE_CACHE_CAPACITY: usize = 2_048;
const BODY_REFERENCE_COLUMNS: usize = 78;
const CODE_BLOCK_PADDING: f32 = 16.;
const CODE_HEADER_HEIGHT: f32 = 36.;
const ALERT_CONTENT_INSET: f32 = 48.;
const NUMBERED_LIST_EXTRA_GAP: f32 = 8.;
const ALERT_HEADER_HEIGHT: f32 = 38.;
const ALERT_BOTTOM_PADDING: f32 = 14.;
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
    math_edit_node: Option<NodeId>,
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
        let components = component_geometry(&projection, &visual_lines, 760., &paint_order);
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
            math_edit_node,
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
        projection.math_edit_node = math_edit_node;
        projection.table_layout_lock = table_layout_lock;
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
        if self.adaptive.slots.contains_key(&node_id) || self.adaptive.lead == Some(node_id) {
            return false;
        }
        self.published_geometry = None;
        if refresh_text_node_geometry(
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
        )
        .is_none()
        {
            return false;
        }
        self.components = Arc::new(component_geometry(
            &self.projection,
            &self.visual_lines,
            760.,
            &self.paint_order,
        ));
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
struct VisualLineSpec {
    range: Range<usize>,
    html_preview: Option<Arc<crate::html::HtmlPreview>>,
    display_math: Option<Arc<crate::math::BlockFormula>>,
    inline_math: Option<Arc<inline_math::InlineLine>>,
    style: VisualLineStyle,
    inset: f32,
    /// External group separation, excluded from table/card background bounds.
    gap_before: f32,
    y: f32,
    x_fraction: f32,
    width_fraction: f32,
    table_cell: Option<(NodeId, usize, usize, usize)>,
    table_cell_first: bool,
    table_row_y: f32,
    table_row_height: f32,
    slot: Option<LayoutSlot>,
}

impl VisualLineStyle {
    const BODY: Self = Self {
        font_size: 18.,
        line_height: LINE_HEIGHT,
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
        Dismiss,
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
    status: u32,
    zoom: f32,
) {
    // One native rounded quad owns the fill and all four edges. A separate
    // full-height accent rail protrudes past the circular corner silhouette.
    chrome.push(MaskedQuad {
        quad: fill(bounds, rgba(MineralPalette::with_alpha(status, 0x16)))
            .corner_radii(px(10. * zoom))
            .border_color(rgb(status))
            .border_widths(gpui::Edges {
                top: px(zoom),
                right: px(zoom),
                bottom: px(zoom),
                left: px(3. * zoom),
            }),
        content_mask: None,
    });
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct ShapeCacheKey {
    shadow: bool,
    node_id: Option<NodeId>,
    node_revision: Revision,
    fragment: Range<usize>,
    font_fingerprint: u64,
    font_size_bits: u32,
    width_bits: u32,
    scale_bits: u32,
    marked_fragment: Option<Range<usize>>,
}

struct ShapeCacheInput<'a> {
    snapshot: &'a document_core::DocumentSnapshot,
    segment: Option<&'a crate::ProjectionSegment>,
    range: &'a Range<usize>,
    runs: &'a [TextRun],
    palette: MineralPalette,
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
    initial_width: f32,
}

pub struct RichDocumentEditor {
    semantic_cache: accessibility::SemanticCache,
    document: SharedDocumentSession,
    selection: Selection,
    projection: TextProjection,
    measurement: Arc<FontMeasurement>,
    measured_layout: bool,
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
    is_selecting: bool,
    drag_scroll: Option<drag_scroll::DragScroll>,
    drag_scroll_generation: u64,
    table_resize_drag: Option<TableResizeDrag>,
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
    last_error: Option<String>,
    has_painted: bool,
    zoom_factor: f32,
    requested_zoom_factor: f32,
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
            is_selecting: false,
            drag_scroll: None,
            drag_scroll_generation: 0,
            table_resize_drag: None,
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
            last_error: None,
            has_painted: false,
            zoom_factor: 1.,
            requested_zoom_factor: 1.,
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
            self.reflow
                .error(self.document.generation(), self.geometry_generation)
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
        self.math_scroll_handles
            .retain(|id, _| self.projection.block(*id).is_some_and(crate::math::is_math));
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
                .segment_for_range(&self.visual_lines[index].range)
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
        let viewport_height = self.scroll_metrics().1;
        let visible_roots = self.visible_planning_roots();
        let environment_current = self.measured_layout
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
        if self.is_selecting
            || self.marked_range.is_some()
            || self.document.composition_active()
            || !self.selected_byte_range().0.is_empty()
        {
            return;
        }
        let key = reflow::Key {
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
        let math_edit_node = self.projection.math_edit_node;
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
                                math_edit_node,
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
                    eprintln!("MINERAL_LAYOUT_VALIDATION holding-timeout");
                    hold.await;
                    eprintln!("MINERAL_LAYOUT_VALIDATION released-timeout");
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
                    .error(this.document.generation(), this.geometry_generation)
                    .is_some();
                if !this.reflow.finish(ticket, outcome.as_ref().err().copied()) {
                    #[cfg(feature = "layout-validation")]
                    if native_fault == Some(validation::Fault::Timeout) {
                        eprintln!("MINERAL_LAYOUT_VALIDATION discarded-late-result");
                    }
                    cx.notify();
                    return;
                }
                if had_error
                    != this
                        .reflow
                        .error(this.document.generation(), this.geometry_generation)
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
        let (prepared, image_dimensions, mut report) = output;
        let reflow::Key {
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
        if self.document.generation() == document_generation
            && self.geometry_generation == geometry_generation
            && self.layout_focus == editing_node
            && !self.is_selecting
            && self.selected_byte_range().0.is_empty()
            && self.marked_range.is_none()
            && !self.document.composition_active()
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
            let caret = self.layout_focus.and_then(|_| {
                self.visual_lines.iter().find(|line| {
                    line.range.start <= cursor
                        && cursor <= line.range.end
                        && self
                            .projection
                            .segment_for_range(&line.range)
                            .is_some_and(|segment| Some(segment.node_id) == self.layout_focus)
                        && line.y >= scroll_y
                        && line.y < scroll_y + viewport_height
                })
            });
            let anchor = caret
                .and_then(|line| {
                    scroll_anchor_for_line(&snapshot, &self.projection, line, 0.).map(
                        |mut anchor| {
                            anchor.node_text_offset += cursor - line.range.start;
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
            report.discard_reason = Some(if self.document.generation() != document_generation {
                "document_changed"
            } else if self.geometry_generation != geometry_generation {
                "geometry_changed"
            } else if self.layout_focus != editing_node {
                "focus_changed"
            } else if self.is_selecting
                || !self.selected_byte_range().0.is_empty()
                || self.marked_range.is_some()
                || self.document.composition_active()
            {
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
        let offset = segment.projection_range.start;
        let y = self
            .visual_lines
            .iter()
            .find(|line| line.range.contains(&offset) || line.range.end == offset)
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

    #[must_use]
    pub fn zoom_percent(&self) -> u16 {
        (self.zoom_factor * 100.).round() as u16
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
        self.refresh_inline_math_focus();
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
        projection.math_edit_node = match &self.selection {
            Selection::Text(selection)
                if inline_math::has_math(&projection, selection.head.node_id) =>
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
            &self.paint_order,
        ));
        self.geometry_generation = self.geometry_generation.wrapping_add(1);
    }

    fn refresh_after_transaction(&mut self, result: &document_core::TransactionResult) {
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
        let refresh = if self.adaptive.slots.contains_key(&node_id)
            || self.adaptive.lead == Some(node_id)
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
            if result.is_some() {
                self.document_height = visual_document_height(&self.visual_lines);
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
        if scroll_y >= refresh.old_y_after && refresh.y_delta.abs() > f32::EPSILON {
            let x = self.scroll_handle.offset().x;
            self.scroll_handle
                .set_offset(point(x, px(-(scroll_y + refresh.y_delta).max(0.))));
        }
        self.projected_generation = self.document.generation();
        self.geometry_generation = self.geometry_generation.wrapping_add(1);
        self.components = Arc::new(component_geometry(
            &self.projection,
            &self.visual_lines,
            self.layout_width,
            &self.paint_order,
        ));
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
        self.refresh_inline_math_focus();
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

    fn refresh_inline_math_focus(&mut self) {
        if self.is_selecting
            || self.marked_range.is_some()
            || !self.selected_byte_range().0.is_empty()
        {
            return;
        }
        let next = match &self.selection {
            Selection::Text(selection)
                if inline_math::has_math(&self.projection, selection.head.node_id) =>
            {
                Some(selection.head.node_id)
            }
            _ => None,
        };
        if next == self.projection.math_edit_node {
            return;
        }
        let scroll = self.scroll_metrics().0;
        let snapshot = self.document.snapshot();
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
        let text = self.editing_text();
        let offset = offset.min(text.len());
        text[..offset]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next_boundary(&self, offset: usize) -> usize {
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
            self.move_to(self.previous_boundary(self.cursor_offset()), window, cx);
        } else {
            self.move_to(selection_collapse_offset(&range, true), window, cx);
        }
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.navigate_preview_horizontal(1, false, false, window, cx) {
            return;
        }
        let (range, _) = self.selected_byte_range();
        if range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), window, cx);
        } else {
            self.move_to(selection_collapse_offset(&range, false), window, cx);
        }
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
        self.select_to(self.previous_boundary(self.cursor_offset()), window, cx);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        if self.navigate_preview_horizontal(1, false, true, window, cx) {
            return;
        }
        self.select_to(self.next_boundary(self.cursor_offset()), window, cx);
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
        let target_index = visual_vertical_neighbor(&self.visual_lines, current_index, direction)?;
        let target = self.visual_lines.get(target_index)?;

        // Use exact shaping whenever both lines are currently painted. The
        // visual-line index remains authoritative for off-screen movement,
        // with a grapheme-safe proportional fallback until the target enters
        // the painted viewport.
        if let Some(current_painted) = painted_line_for_offset(&self.painted_lines, offset)
            && let Some(target_painted) = self
                .painted_lines
                .iter()
                .find(|line| line.range == target.range)
        {
            let local = offset
                .saturating_sub(current_painted.range.start)
                .min(current_painted.layout.len());
            let preferred_x = self.preferred_x.unwrap_or(
                aligned_text_left(
                    current_painted.bounds,
                    &current_painted.layout,
                    current_painted.alignment,
                ) + current_painted.layout.x_for_index(local),
            );
            self.preferred_x = Some(preferred_x);
            let relative_x = (preferred_x
                - aligned_text_left(
                    target_painted.bounds,
                    &target_painted.layout,
                    target_painted.alignment,
                ))
            .max(px(0.));
            return Some(
                target.range.start
                    + target_painted
                        .layout
                        .closest_index_for_x(relative_x)
                        .min(target.range.len()),
            );
        }

        let local = offset.saturating_sub(current.range.start);
        let ratio = if current.range.is_empty() {
            0.
        } else {
            local as f32 / current.range.len() as f32
        };
        let candidate = target.range.start
            + (target.range.len() as f32 * ratio)
                .round()
                .clamp(0., target.range.len() as f32) as usize;
        Some(snap_offset_to_grapheme(
            self.projection.text(),
            target.range.clone(),
            candidate,
        ))
    }

    fn keep_offset_visible(&mut self, offset: usize) {
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
                .find(|line| line.range.contains(&offset))
                .or_else(|| {
                    self.visual_lines
                        .iter()
                        .find(|line| line.range.end == offset)
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
            if let Some(cross) = &selection.cross {
                let payload = cross
                    .selection(selection.anchor, selection.head)
                    .ok_or_else(|| DocumentError::Html("Choose the preview text again".into()))
                    .and_then(|range| self.document.snapshot().preview_clipboard_payload(&range));
                match payload {
                    Ok(Some(payload)) => {
                        if let Some(text) = payload.plain_text {
                            cx.write_to_clipboard(if let Some(metadata) = payload.rich_json {
                                ClipboardItem::new_string_with_metadata(text, metadata)
                            } else {
                                ClipboardItem::new_string(text)
                            });
                        }
                    }
                    Ok(None) => {}
                    Err(error) => self.record_error(error, window),
                }
                return;
            }
            if !selection.range().is_empty() {
                cx.write_to_clipboard(ClipboardItem::new_string(
                    selection.preview.editable_text[selection.range()].to_owned(),
                ));
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
        let Some((_, source, _)) = image else {
            window.play_system_bell();
            return;
        };
        cx.emit(EditorEvent::RetryImage {
            source,
            document_directory: self.document_directory.clone(),
        });
        cx.notify();
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
            window.play_system_bell();
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
            window.play_system_bell();
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
        self.is_selecting = false;
        self.stop_drag_scroll();
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
        match self.apply_command(command) {
            Ok(result) if !result.dirty_node_ids.is_empty() => {
                self.refresh_after_transaction(&result);
                self.last_error = None;
                self.focus_handle.focus(window, cx);
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
            let local = line.layout.closest_index_for_x(
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
        let local = line.layout.closest_index_for_x(
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
                    .find(|candidate| candidate.range == line.range)?;

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
            + line.layout.x_for_index(local_index)
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
        self.is_selecting = false;
        self.stop_drag_scroll();
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
            self.animate_toolbar(false, cx);
            cx.stop_propagation();
            return;
        }
        if let Some(selection) = self.table_selection_at(event.position) {
            if let Err(error) =
                self.apply_command(EditCommand::SetSelection(Selection::Table(selection)))
            {
                self.record_error(error, window);
                return;
            }
            self.is_selecting = false;
            self.animate_toolbar(true, cx);
            cx.emit(EditorEvent::ViewChanged);
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
        if line.range.start != segment.projection_range.start {
            return None;
        }
        self.document
            .snapshot()
            .list_item_containing(segment.node_id)
            .map(|(_, item_id, _)| item_id)
    }

    fn table_resize_at(&self, position: Point<Pixels>) -> Option<TableResizeDrag> {
        self.painted_lines.iter().find_map(|line| {
            if position.y < line.bounds.top() || position.y > line.bounds.bottom() {
                return None;
            }
            let edge = line.bounds.right() + px(12. * self.zoom_factor);
            let distance: f32 = (position.x - edge).abs().into();
            if distance > 6. {
                return None;
            }
            let segment = segment_for_line(&self.projection, &line.range)?;
            let (table_id, _, column) = segment.context.table_cell?;
            let BlockNode::Table(table) = self.projection.block(table_id)? else {
                return None;
            };
            Some(TableResizeDrag {
                table_id,
                column,
                pointer_x: position.x,
                initial_width: table.columns.get(column)?.width.unwrap_or_else(|| {
                    if let Some(spec) = self
                        .visual_lines
                        .iter()
                        .find(|spec| spec.range == line.range)
                    {
                        return spec.width_fraction * self.layout_width / self.zoom_factor;
                    }
                    self.projection
                        .table_widths(table_id)
                        .and_then(|widths| widths.get(column))
                        .copied()
                        .unwrap_or(160.)
                }),
            })
        })
    }

    fn table_selection_at(&self, position: Point<Pixels>) -> Option<RectangularSelection> {
        self.painted_lines.iter().find_map(|line| {
            let segment = segment_for_line(&self.projection, &line.range)?;
            let (table_id, row, column) = segment.context.table_cell?;
            let row_handle = column == 0
                && position.y >= line.bounds.top()
                && position.y <= line.bounds.bottom()
                && position.x >= line.bounds.left() - px(16.)
                && position.x < line.bounds.left();
            let column_handle = row == 0
                && position.x >= line.bounds.left()
                && position.x <= line.bounds.right()
                && position.y >= line.bounds.top() - px(12.)
                && position.y < line.bounds.top();
            if !row_handle && !column_handle {
                return None;
            }
            let BlockNode::Table(table) = self.projection.block(table_id)? else {
                return None;
            };
            if row_handle {
                Some(RectangularSelection {
                    table_id,
                    anchor_row: row,
                    anchor_column: 0,
                    head_row: row,
                    head_column: table.column_count().saturating_sub(1),
                })
            } else {
                Some(RectangularSelection {
                    table_id,
                    anchor_row: 0,
                    anchor_column: column,
                    head_row: table.row_count().saturating_sub(1),
                    head_column: column,
                })
            }
        })
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
                .map(|segment| segment.projection_range.start)
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
                .or_else(|| segment_for_line(&self.projection, &line.range).map(|s| s.node_id))
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
            let local_y = (y - line.y) / self.zoom_factor - 32.;
            if !(local_x >= 0.
                && local_x <= preview.width
                && local_y >= 0.
                && local_y <= preview.height)
            {
                continue;
            }
            let segment = self.projection.segment_for_range(&line.range)?;
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
            let delta: f32 = (event.position.x - drag.pointer_x).into();
            self.apply_structural_command(
                EditCommand::SetTableColumnWidth {
                    table_id: drag.table_id,
                    column: drag.column,
                    width: (drag.initial_width + delta).max(32.),
                },
                window,
                cx,
            );
            return;
        }
        self.is_selecting = false;
        self.animate_toolbar(
            matches!(self.selection, Selection::Table(_))
                || !self.selected_byte_range().0.is_empty()
                || self.current_image().is_some(),
            cx,
        );
        self.refresh_inline_math_focus();
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_selecting && event.pressed_button != Some(MouseButton::Left) {
            self.is_selecting = false;
            self.stop_drag_scroll();
            return;
        }
        if self.table_resize_drag.is_some() {
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
        if matches!(self.composition_origin, Some(CompositionOrigin::Html(_)))
            && text.is_empty()
            && self.document.composition_active()
        {
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
                    + line.layout.x_for_index(start),
                line.bounds.top(),
            ),
            point(
                aligned_text_left(line.bounds, &line.layout, line.alignment)
                    + line.layout.x_for_index(end),
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
        let palette = MineralPalette::for_dark(cx.theme().is_dark());
        if !self
            .measurement
            .matches(&cx.theme().font_family, self.zoom_factor)
        {
            self.measurement = Arc::new(FontMeasurement::new(
                cx.text_system().clone(),
                cx.theme().font_family.clone(),
                self.zoom_factor,
            ));
            self.projection.table_layout_lock = None;
            self.measured_layout = false;
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
        let active_table = {
            let location = match &self.selection {
                Selection::Text(selection) => self
                    .projection
                    .segment_for_node(selection.head.node_id)
                    .and_then(|segment| segment.context.table_cell),
                Selection::Table(selection) => Some((
                    selection.table_id,
                    selection.head_row,
                    selection.head_column,
                )),
            };
            location.and_then(|(table_id, row, column)| {
                let BlockNode::Table(table) = self.projection.block(table_id)? else {
                    return None;
                };
                Some((
                    table_id,
                    row,
                    column,
                    table.columns.get(column)?.alignment,
                    table.border,
                    table.row_count(),
                    table.column_count(),
                ))
            })
        };
        let table_tools = active_table.map(
            |(table_id, row, column, alignment, border, row_count, column_count)| {
                let next_alignment = match alignment {
                    ColumnAlignment::None | ColumnAlignment::Right => ColumnAlignment::Left,
                    ColumnAlignment::Left => ColumnAlignment::Center,
                    ColumnAlignment::Center => ColumnAlignment::Right,
                };
                let next_border = match border {
                    TableBorder::Dotted | TableBorder::PhysicalPixel => TableBorder::None,
                    TableBorder::None => TableBorder::PhysicalPixel,
                };
                div()
                    .flex()
                    .gap(px(2.))
                    .child(
                        editor_button("table-add-row", "row-add", "Insert row below", palette, cx)
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_structural_command(
                                    EditCommand::InsertTableRow {
                                        table_id,
                                        index: row + 1,
                                    },
                                    window,
                                    cx,
                                );
                            })),
                    )
                    .child(
                        editor_button(
                            "table-add-column",
                            "column-add",
                            "Insert column after",
                            palette,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.apply_structural_command(
                                    EditCommand::InsertTableColumn {
                                        table_id,
                                        index: column + 1,
                                    },
                                    window,
                                    cx,
                                );
                            },
                        )),
                    )
                    .when(row_count > 1, |tools| {
                        tools.child(
                            editor_button(
                                "table-delete-row",
                                "row-delete",
                                "Delete row",
                                palette,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::DeleteTableRow {
                                            table_id,
                                            index: row,
                                        },
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                    })
                    .when(column_count > 1, |tools| {
                        tools.child(
                            editor_button(
                                "table-delete-column",
                                "column-delete",
                                "Delete column",
                                palette,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::DeleteTableColumn {
                                            table_id,
                                            index: column,
                                        },
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                    })
                    .when(row > 0, |tools| {
                        tools.child(
                            editor_button(
                                "table-move-row-up",
                                "row-up",
                                "Move row up",
                                palette,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::MoveTableRow {
                                            table_id,
                                            from: row,
                                            to: row - 1,
                                        },
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                    })
                    .when(row + 1 < row_count, |tools| {
                        tools.child(
                            editor_button(
                                "table-move-row-down",
                                "row-down",
                                "Move row down",
                                palette,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::MoveTableRow {
                                            table_id,
                                            from: row,
                                            to: row + 1,
                                        },
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                    })
                    .when(column > 0, |tools| {
                        tools.child(
                            editor_button(
                                "table-move-column-left",
                                "column-left",
                                "Move column left",
                                palette,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::MoveTableColumn {
                                            table_id,
                                            from: column,
                                            to: column - 1,
                                        },
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                    })
                    .when(column + 1 < column_count, |tools| {
                        tools.child(
                            editor_button(
                                "table-move-column-right",
                                "column-right",
                                "Move column right",
                                palette,
                                cx,
                            )
                            .on_click(cx.listener(
                                move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::MoveTableColumn {
                                            table_id,
                                            from: column,
                                            to: column + 1,
                                        },
                                        window,
                                        cx,
                                    );
                                },
                            )),
                        )
                    })
                    .child(
                        editor_button(
                            "table-align-column",
                            "align",
                            "Cycle column alignment: left, center, right",
                            palette,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.apply_structural_command(
                                    EditCommand::SetTableColumnAlignment {
                                        table_id,
                                        column,
                                        alignment: next_alignment,
                                    },
                                    window,
                                    cx,
                                );
                            },
                        )),
                    )
                    .child(
                        editor_button(
                            "table-border",
                            "border",
                            "Toggle table borders",
                            palette,
                            cx,
                        )
                        .on_click(cx.listener(
                            move |this, _, window, cx| {
                                this.apply_structural_command(
                                    EditCommand::SetTableBorder {
                                        table_id,
                                        border: next_border,
                                    },
                                    window,
                                    cx,
                                );
                            },
                        )),
                    )
            },
        );
        let image_tools = active_image.as_ref().map(|_| {
            div()
                .flex()
                .gap(px(2.))
                .child(
                    div()
                        .id("edit-image")
                        .px(px(7.))
                        .py(px(4.))
                        .rounded(px(4.))
                        .hover(|button| button.bg(rgb(palette.surface)))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.show_image_editor(window, cx);
                        }))
                        .child("Image…"),
                )
                .child(
                    div()
                        .id("retry-image")
                        .px(px(7.))
                        .py(px(4.))
                        .rounded(px(4.))
                        .hover(|button| button.bg(rgb(palette.surface)))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.retry_current_image(window, cx);
                        }))
                        .child("Retry"),
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
            let segment = segment_for_line(&self.projection, &line.range);
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
                let formula = if cx.theme().is_dark() {
                    &attachment.dark
                } else {
                    &attachment.light
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
                let segment = segment_for_line(&self.projection, &line.range)?;
                if line.range.start != segment.projection_range.start {
                    return None;
                }
                let source = segment.context.image_source.as_deref()?;
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
                Some(
                    div()
                        .id(("document-image", segment.node_id.get() as usize))
                        .when(linked, |this| this.cursor_pointer())
                        .absolute()
                        .top(px(line.y))
                        .left(relative(line.x_fraction))
                        .ml(px(-horizontal_offset))
                        .w(relative(line.width_fraction))
                        .h(px(line.style.line_height))
                        .px(px(8.))
                        .child(
                            img(image_source)
                                .w_full()
                                // A percentage height plus the image's intrinsic
                                // aspect ratio can exceed the reserved row. Give
                                // the image the same definite height as geometry.
                                .h(px(line.style.line_height))
                                .min_h_0()
                                .max_h(px(line.style.line_height))
                                .object_fit(ObjectFit::Contain),
                        ),
                )
            })
            .collect::<Vec<_>>();
        let mut component_chrome = Vec::<AnyElement>::new();
        let mut outer_container_headers = Vec::<AnyElement>::new();
        let mut rendered_alert_headers = HashSet::new();
        let mut rendered_code_headers = HashSet::new();
        for index in &visible_order {
            let Some(line) = self.visual_lines.get(*index) else {
                continue;
            };
            let Some(segment) = segment_for_line(&self.projection, &line.range) else {
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
                        .child(self.html_toolbar(node_id, preview, viewport, cx))
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
                        .top(px(component.top - ALERT_HEADER_HEIGHT * self.zoom_factor
                            + 9. * self.zoom_factor))
                        .left(relative(component.left_fraction))
                        .ml(px(
                            -ALERT_CONTENT_INSET * self.zoom_factor + 14. * self.zoom_factor
                        ))
                        .flex()
                        .items_center()
                        .gap(px(9. * self.zoom_factor))
                        .text_size(px(15. * self.zoom_factor))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(color))
                        .child(
                            Icon::new(alert_icon(kind))
                                .with_size(px(18. * self.zoom_factor))
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
                let preview = first
                    .display_math
                    .as_ref()
                    .map(|math| math.for_dark(cx.theme().is_dark()));
                let preview_extent = first.display_math.as_ref().map_or(0., |math| math.extent());
                let header_top = first.y
                    - (CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING + preview_extent) * self.zoom_factor;
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
                if let Some(preview) = preview {
                    let scroll_handle = self
                        .math_scroll_handles
                        .entry(segment.node_id)
                        .or_default()
                        .clone();
                    let available = (self.layout_width * first.width_fraction
                        - first.inset
                        - CODE_BLOCK_PADDING * self.zoom_factor)
                        .max(1.);
                    component_chrome.push(
                        div()
                            .id(("math-preview", segment.node_id.get() as usize))
                            .debug_selector(|| "display-math-viewport".into())
                            // Block layout can enlarge an image to its rounded
                            // intrinsic aspect ratio, clipping the denominator
                            // against this exact-height scrolling viewport.
                            .flex()
                            .absolute()
                            .top(px(header_top
                                + (CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING)
                                    * self.zoom_factor))
                            .left(relative(first.x_fraction))
                            .ml(px(first.inset))
                            .w(px(available))
                            .h(px(preview.height * self.zoom_factor))
                            .overflow_x_scroll()
                            .track_scroll(&scroll_handle)
                            .child(
                                img(ImageSource::Image(preview.image.clone()))
                                    .debug_selector(|| "display-math-image".into())
                                    .w(px(preview.width * self.zoom_factor))
                                    .h(px(preview.height * self.zoom_factor))
                                    .flex_shrink_0()
                                    .object_fit(ObjectFit::Contain),
                            )
                            .into_any_element(),
                    );
                    if preview.width * self.zoom_factor > available {
                        // Use the existing preview/source gap, not the image's
                        // viewport: an overlay would obscure low glyphs and
                        // fraction denominators. This is view state only.
                        component_chrome.push(
                            div()
                                .debug_selector(|| "display-math-scrollbar".into())
                                .absolute()
                                .top(px(header_top
                                    + (CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING + preview.height)
                                        * self.zoom_factor))
                                .left(relative(first.x_fraction))
                                .ml(px(first.inset))
                                .w(px(available))
                                .h(px(16.))
                                .child(
                                    Scrollbar::horizontal(&scroll_handle)
                                        .id(("math-scrollbar", segment.node_id.get() as usize))
                                        .viewport_from_layout()
                                        .mode(ScrollbarMode::Always),
                                )
                                .into_any_element(),
                        );
                    }
                    debug_assert!(preview.baseline <= preview.height);
                }
                component_chrome.push(
                    div()
                        .absolute()
                        .top(px(header_top))
                        .left(relative(first.x_fraction))
                        .ml(px(header_left + 12. * self.zoom_factor))
                        .h(px(CODE_HEADER_HEIGHT * self.zoom_factor))
                        .flex()
                        .items_center()
                        .font_family("Spline Sans Mono Mineral")
                        .text_size(px(12. * self.zoom_factor))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgb(palette.secondary))
                        .child(language)
                        .into_any_element(),
                );
                component_chrome.push(
                    div()
                        .absolute()
                        .top(px(header_top))
                        .h(px(CODE_HEADER_HEIGHT * self.zoom_factor))
                        .flex()
                        .items_center()
                        .right(px(self.layout_width
                            * (1. - first.x_fraction - first.width_fraction)
                            + (9. + if first.table_cell.is_some() { 12. } else { 0. })
                                * self.zoom_factor))
                        .child(
                            Button::new(("copy-code", segment.node_id.get() as usize))
                                .ghost()
                                .small()
                                .icon(IconName::Copy)
                                .label("Copy")
                                .tooltip(if math {
                                    "Copy formula source"
                                } else {
                                    "Copy code block"
                                })
                                .on_click(move |_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        code_text.clone(),
                                    ));
                                }),
                        )
                        .into_any_element(),
                );
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
        let semantics = window.is_a11y_active().then(|| {
            self.semantic_cache
                .get(&self.projection, &self.visual_lines, self.layout_width)
        });
        let context_editor = cx.entity();
        let context_focus = self.focus_handle.clone();
        let toolbar_focus = self.focus_handle.clone();
        let has_text_selection = !self.selected_byte_range().0.is_empty();
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
            .cursor(CursorStyle::IBeam)
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
            })
            .children(component_chrome)
            .children(self.render_task_summaries(&visible_order, palette))
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
                        .children(table_tools)
                        .children(image_tools),
                )
            })
            .children(link_popover)
            .children(image_popover)
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
                        !has_text_selection,
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
}

struct TextPrepaintState {
    semantic_bounds: Bounds<Pixels>,
    semantic_scale: f32,
    semantic_actions: Vec<accessibility::ActionTarget>,
    lines: Vec<PaintedLine>,
    decorations: Vec<PaintedLine>,
    checkboxes: Vec<PaintedCheckbox>,
    chrome: Vec<MaskedQuad>,
    overlays: Vec<MaskedQuad>,
    caret: Option<MaskedQuad>,
    horizontal_metrics: HashMap<NodeId, (f32, f32)>,
}

struct PaintedCheckbox {
    bounds: Bounds<Pixels>,
    checked: bool,
    content_mask: Option<ContentMask<Pixels>>,
}

fn task_checkbox_bounds(line: Bounds<Pixels>, zoom: f32) -> Bounds<Pixels> {
    let side = px(18. * zoom);
    Bounds::new(
        point(
            line.left() - px(23. * zoom),
            line.top() + (line.size.height - side) / 2.,
        ),
        size(side, side),
    )
}

impl PaintedCheckbox {
    fn paint(&self, palette: MineralPalette, window: &mut Window, cx: &App) {
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
        Some(Role::Document)
    }

    fn a11y_synthetic_children(
        &mut self,
        state: &mut Self::PrepaintState,
        builder: &mut gpui::A11ySubtreeBuilder,
    ) {
        if let Some(semantics) = &self.semantics {
            state.semantic_actions =
                semantics.publish(builder, state.semantic_bounds, state.semantic_scale);
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
        let palette = MineralPalette::for_dark(cx.theme().is_dark());
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
        let mut chrome = Vec::new();
        let mut overlays = Vec::new();
        let mut caret = None;
        let mut horizontal_metrics = HashMap::<NodeId, (f32, f32)>::new();
        let mut painted_alerts = HashSet::new();
        let mut painted_code_blocks = HashSet::new();
        let mut painted_lists = HashSet::new();
        let mut painted_quotes = HashSet::new();
        let mut painted_cards = HashSet::new();
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
            let range = spec.range.clone();
            let visual_style = spec.style;
            let width: f32 = bounds.size.width.into();
            let segment = segment_for_line(&editor.projection, &range);
            let block = segment.and_then(|segment| editor.projection.block(segment.node_id));
            let is_code = matches!(block, Some(BlockNode::CodeBlock(_)));
            let horizontal_owner =
                segment.and_then(|segment| horizontal_scroll_owner(&editor.projection, segment));
            let horizontal_offset = horizontal_owner
                .and_then(|owner| editor.horizontal_scrolls.get(&owner).copied())
                .unwrap_or(0.);
            let line_bounds = visual_line_bounds(editor, spec, bounds, is_code, horizontal_offset);
            let (viewport_left, viewport_width) = if spec.table_cell.is_some() {
                table_viewport_geometry(spec, &editor.projection, width, editor.zoom_factor)
            } else if is_code || spec.html_preview.is_some() {
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
                if spec
                    .slot
                    .is_some_and(|slot| slot.cards && painted_cards.insert((slot.group, slot.item)))
                {
                    let card = Bounds::new(
                        point(
                            bounds.left() + px(width * spec.x_fraction),
                            bounds.top() + px(spec.table_row_y),
                        ),
                        size(px(width * spec.width_fraction), px(spec.table_row_height)),
                    );
                    chrome.push(MaskedQuad {
                        quad: fill(card, rgb(palette.surface_quiet)).corner_radii(px(4.)),
                        content_mask: None,
                    });
                    chrome.push(MaskedQuad {
                        quad: outline(card, rgb(palette.border), BorderStyle::Solid)
                            .corner_radii(px(4.)),
                        content_mask: None,
                    });
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
                        ALERT_CONTENT_INSET * editor.zoom_factor,
                        8. * editor.zoom_factor,
                        ALERT_HEADER_HEIGHT * editor.zoom_factor,
                        ALERT_BOTTOM_PADDING * editor.zoom_factor,
                    )
                {
                    let status = alert_color(palette, alert_kind);
                    let mut alert_bounds = alert_bounds;
                    let cell_local = editor.projection.container_cell(*alert_id).is_some();
                    if cell_local {
                        alert_bounds.origin.x -= px(horizontal_offset);
                    }
                    let start = chrome.len();
                    append_alert_chrome(&mut chrome, alert_bounds, status, editor.zoom_factor);
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
                            0.
                        },
                        (CODE_HEADER_HEIGHT
                            + CODE_BLOCK_PADDING
                            + editor
                                .components
                                .get(&segment.node_id)
                                .and_then(|component| editor.visual_lines.get(component.first_line))
                                .and_then(|line| line.display_math.as_ref())
                                .map_or(0., |math| math.extent()))
                            * editor.zoom_factor,
                        CODE_BLOCK_PADDING * editor.zoom_factor,
                    )
                {
                    let mut code_bounds = code_bounds;
                    let code_mask = if segment.context.table_cell.is_some() {
                        code_bounds.origin.x -= px(horizontal_offset);
                        table_content_mask
                    } else {
                        None
                    };
                    chrome.push(MaskedQuad {
                        quad: fill(code_bounds, rgb(palette.surface_quiet))
                            .corner_radii(px(8. * editor.zoom_factor)),
                        content_mask: code_mask,
                    });
                    chrome.push(MaskedQuad {
                        quad: outline(code_bounds, rgb(palette.border), BorderStyle::Solid)
                            .corner_radii(px(8. * editor.zoom_factor)),
                        content_mask: code_mask,
                    });
                    chrome.push(MaskedQuad {
                        quad: fill(
                            Bounds::new(
                                point(
                                    code_bounds.left(),
                                    code_bounds.top() + px(CODE_HEADER_HEIGHT * editor.zoom_factor),
                                ),
                                size(code_bounds.size.width, px(1.)),
                            ),
                            rgb(palette.border),
                        ),
                        content_mask: code_mask,
                    });
                }
                if let Some(quote) = segment.context.quote
                    && painted_quotes.insert(quote)
                    && let Some(panel) = visual_component_bounds(
                        editor,
                        bounds,
                        quote,
                        20. * editor.zoom_factor,
                        0.,
                        16. * editor.zoom_factor,
                        16. * editor.zoom_factor,
                    )
                {
                    let mut panel = panel;
                    let quote_mask = if editor.projection.container_cell(quote).is_some() {
                        panel.origin.x -= px(horizontal_offset);
                        table_content_mask
                    } else {
                        None
                    };
                    chrome.push(MaskedQuad {
                        quad: fill(panel, rgb(palette.surface_quiet)).corner_radii(px(4.)),
                        content_mask: quote_mask,
                    });
                    chrome.push(MaskedQuad {
                        quad: fill(
                            Bounds::new(
                                point(panel.left(), panel.top()),
                                size(px(2.), panel.size.height),
                            ),
                            rgb(palette.accent),
                        ),
                        content_mask: quote_mask,
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
                    if segment.context.table_header || alternate_row {
                        chrome.push(MaskedQuad {
                            quad: fill(
                                cell_bounds,
                                if segment.context.table_header {
                                    rgb(palette.surface_quiet)
                                } else {
                                    rgba(MineralPalette::with_alpha(palette.surface, 0x70))
                                },
                            ),
                            content_mask: Some(cell_mask),
                        });
                    }
                    if let (Some(selection), Some((table_id, row, column))) =
                        (table_selection, segment.context.table_cell)
                        && selection.table_id == table_id
                    {
                        let (rows, columns) = selection.normalized();
                        if rows.contains(&row) && columns.contains(&column) {
                            overlays.push(MaskedQuad {
                                quad: fill(cell_bounds, rgb(palette.selection)),
                                content_mask: Some(cell_mask),
                            });
                        }
                    }
                    push_table_border(
                        &mut chrome,
                        cell_bounds,
                        segment.context.table_border.unwrap_or(TableBorder::Dotted),
                        Some(cell_mask),
                        palette.border,
                        window.scale_factor(),
                        segment
                            .context
                            .table_cell
                            .map(|(_, row, column)| (row == 0, column == 0))
                            .unwrap_or_default(),
                    );
                }
                if segment.context.image_source.is_some() {
                    chrome.push(MaskedQuad {
                        quad: fill(line_bounds, rgb(palette.surface)),
                        content_mask,
                    });
                }
                if range.start == segment.projection_range.start
                    && let Some(marker) = segment.context.list_marker.as_deref()
                {
                    let numbered = marker.ends_with('.');
                    let table_leading_edge = segment
                        .context
                        .table_cell
                        .and_then(|(table, _, _)| editor.projection.table_context(table))
                        .filter(|table| segment.context.list_depth == table.outer.list_depth)
                        .map(|_| bounds.left() + px(viewport_left));
                    let marker_mask = if table_leading_edge.is_some() {
                        None
                    } else {
                        content_mask
                    };
                    let marker_bounds = list_marker_bounds(
                        line_bounds,
                        list_layout == Some(ListLayout::Steps),
                        numbered,
                        table_leading_edge,
                        editor.zoom_factor,
                    );
                    if list_layout == Some(ListLayout::Steps) && marker.ends_with('.') {
                        chrome.push(MaskedQuad {
                            quad: fill(marker_bounds, rgb(palette.accent))
                                .corner_radii(px(13. * editor.zoom_factor)),
                            content_mask: marker_mask,
                        });
                    }
                    let task = segment.context.task_checked;
                    let marker = if list_layout == Some(ListLayout::Steps) {
                        marker.trim_end_matches('.')
                    } else {
                        marker
                    };
                    if let Some(checked) = task {
                        checkboxes.push(PaintedCheckbox {
                            bounds: task_checkbox_bounds(line_bounds, editor.zoom_factor),
                            checked,
                            content_mask,
                        });
                    }
                    if task.is_none() {
                        let font = text_style.font();
                        let marker_layout = window.text_system().shape_line(
                            marker.to_owned().into(),
                            px(15. * editor.zoom_factor),
                            &[TextRun {
                                len: marker.len(),
                                font,
                                color: rgb(if list_layout == Some(ListLayout::Steps) {
                                    palette.page
                                } else if marker.ends_with('.') {
                                    palette.accent
                                } else {
                                    palette.secondary
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
                            line_height: px(if list_layout == Some(ListLayout::Steps) {
                                25. * editor.zoom_factor
                            } else {
                                visual_style.line_height
                            }),
                            horizontal_owner,
                            content_mask: marker_mask,
                            alignment: ColumnAlignment::Center,
                        });
                    }
                }
            }
            let line_text = if spec.html_preview.is_some()
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
            let cache_key = shape_cache_key(ShapeCacheInput {
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
            let cached = editor.shaped_line_cache.borrow_mut().get(&cache_key);
            if spec.inline_math.is_none() && segment.is_some_and(|segment| matches!(editor.projection.block(segment.node_id), Some(BlockNode::Heading(heading)) if heading.level <= 2)) {
                let mut shadow_key = cache_key.clone();
                shadow_key.shadow = true;
                let shadow = editor.shaped_line_cache.borrow_mut().get(&shadow_key);
                let shadow = shadow.unwrap_or_else(|| {
                    let mut runs = runs.clone();
                    for run in &mut runs { run.color = rgba(0x62665c30).into(); run.background_color = None; }
                    let shaped = window.text_system().shape_line(line_text.to_owned().into(), px(visual_style.font_size), &runs, None);
                    editor.shaped_line_cache.borrow_mut().insert(shadow_key, shaped.clone());
                    shaped
                });
                decorations.push(PaintedLine {
                    range: range.start..range.start, layout: shadow,
                    bounds: Bounds::new(line_bounds.origin + point(px(0.), px(1. * editor.zoom_factor)), line_bounds.size),
                    line_height: px(visual_style.line_height), horizontal_owner, content_mask,
                    alignment: ColumnAlignment::None,
                });
            }
            let layout = cached.unwrap_or_else(|| {
                let layout = window.text_system().shape_line(
                    line_text.to_owned().into(),
                    px(visual_style.font_size),
                    &runs,
                    None,
                );
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
            if let Some(owner) = horizontal_owner
                && segment.is_some_and(|segment| segment.context.table_cell.is_none())
            {
                record_horizontal_metrics(
                    &mut horizontal_metrics,
                    owner,
                    width * spec.width_fraction,
                    spec.html_preview
                        .as_ref()
                        .map_or(f32::from(layout.width()), |preview| {
                            preview.width * editor.zoom_factor
                        })
                        + spec.inset
                        + 8. * editor.zoom_factor,
                );
            }

            let overlap = selected.start.max(range.start)..selected.end.min(range.end);
            if overlap.start < overlap.end {
                overlays.push(MaskedQuad {
                    quad: fill(
                        Bounds::from_corners(
                            point(
                                text_left + layout.x_for_index(overlap.start - range.start),
                                line_bounds.top(),
                            ),
                            point(
                                text_left + layout.x_for_index(overlap.end - range.start),
                                line_bounds.bottom(),
                            ),
                        ),
                        rgb(palette.selection),
                    ),
                    content_mask,
                });
            }
            if selected.is_empty()
                && selected.start >= range.start
                && selected.start <= range.end
                && caret.is_none()
            {
                caret = Some(MaskedQuad {
                    quad: fill(
                        Bounds::new(
                            point(
                                text_left + layout.x_for_index(selected.start - range.start),
                                line_bounds.top() + px(2.),
                            ),
                            size(px(1.5), px(visual_style.line_height - 4.)),
                        ),
                        rgb(palette.accent),
                    ),
                    content_mask,
                });
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

        TextPrepaintState {
            semantic_bounds: bounds,
            semantic_scale: window.scale_factor(),
            semantic_actions: Vec::new(),
            lines,
            decorations,
            checkboxes,
            chrome,
            overlays,
            caret,
            horizontal_metrics,
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
        let palette = MineralPalette::for_dark(cx.theme().is_dark());
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
            editor.horizontal_scrolls.retain(|owner, offset| {
                let Some((viewport, content)) = state.horizontal_metrics.get(owner) else {
                    return false;
                };
                *offset = clamped_horizontal_scroll(*offset, 0., *viewport, *content);
                true
            });
            let width_changed = editor.element_bounds.is_none_or(|old| {
                (f32::from(old.size.width) - f32::from(bounds.size.width)).abs() >= 0.5
            });
            editor.element_bounds = Some(bounds);
            if !editor.has_painted {
                editor.has_painted = true;
                cx.emit(EditorEvent::Ready);
                cx.notify();
            } else if width_changed {
                // Wayland can configure a new size between render and paint.
                // Publish it now instead of waiting for an unrelated pointer
                // event to start the measured reflow at the actual width.
                cx.notify();
            }
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
            let start = segment.node_range.start
                + range.start.saturating_sub(segment.projection_range.start);
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
        shadow: false,
        node_id,
        node_revision,
        fragment,
        font_fingerprint: font_hasher.finish(),
        font_size_bits: font_size.to_bits(),
        width_bits: width.to_bits(),
        scale_bits: scale.to_bits(),
        marked_fragment,
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

fn push_table_border(
    output: &mut Vec<MaskedQuad>,
    bounds: Bounds<Pixels>,
    border: TableBorder,
    content_mask: Option<ContentMask<Pixels>>,
    border_color: u32,
    scale: f32,
    outer_edges: (bool, bool),
) {
    if border != TableBorder::None {
        // Each shared edge is painted once, aligned to physical pixels even
        // at fractional desktop scaling. No overlapping/dashed cell outlines.
        let snap = |value: Pixels| px((f32::from(value) * scale).round() / scale);
        let left = snap(bounds.left());
        let right = snap(bounds.right());
        let top = snap(bounds.top());
        let bottom = snap(bounds.bottom());
        let stroke = px(2. / scale);
        for edge in [
            Some(Bounds::new(
                point(left, bottom - stroke),
                size(right - left, stroke),
            )),
            Some(Bounds::new(
                point(right - stroke, top),
                size(stroke, bottom - top),
            )),
            outer_edges
                .0
                .then(|| Bounds::new(point(left, top), size(right - left, stroke))),
            outer_edges
                .1
                .then(|| Bounds::new(point(left, top), size(stroke, bottom - top))),
        ]
        .into_iter()
        .flatten()
        {
            output.push(MaskedQuad {
                quad: fill(edge, rgb(border_color)),
                content_mask,
            });
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
    let segment = segment_for_line(projection, &line.range)?;
    let block = projection.block(segment.node_id)?;
    let text = projection
        .text()
        .get(line.range.clone())
        .unwrap_or_default();
    let first_visual_line = line.range.start == segment.projection_range.start;
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
    } else if line.range.is_empty() {
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
        line.gap_before *= scale;
        line.y *= scale;
        line.table_row_y *= scale;
        line.table_row_height *= scale;
    }
}

fn capture_scroll_anchor(
    snapshot: &document_core::DocumentSnapshot,
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    scroll_y: f32,
) -> Option<EditorScrollAnchor> {
    let line = lines
        .iter()
        .filter(|line| line.y <= scroll_y)
        .max_by(|a, b| a.y.total_cmp(&b.y))
        .or_else(|| lines.first())?;
    scroll_anchor_for_line(snapshot, projection, line, scroll_y - line.y)
}

fn scroll_anchor_for_line(
    snapshot: &document_core::DocumentSnapshot,
    projection: &TextProjection,
    line: &VisualLineSpec,
    intra_line_offset: f32,
) -> Option<EditorScrollAnchor> {
    let segment = projection.segment_for_range(&line.range)?;
    let node_text_hint = snapshot
        .node(segment.node_id)
        .map(BlockNode::plain_text)
        .map(|text| text.chars().take(96).collect())?;
    Some(EditorScrollAnchor {
        node_id: segment.node_id,
        node_text_hint,
        node_text_offset: line
            .range
            .start
            .saturating_sub(segment.projection_range.start)
            .min(segment.node_range.len()),
        projection_offset: line.range.start,
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
                        .projection_range
                        .start
                        .abs_diff(anchor.projection_offset)
                })
        })?;
    let target = segment.projection_range.start
        + anchor.node_text_offset.min(segment.projection_range.len());
    let line = lines.iter().find(|line| {
        line.range.start <= target
            && target <= line.range.end
            && projection
                .segment_for_range(&line.range)
                .is_some_and(|candidate| candidate.node_id == segment.node_id)
    })?;
    Some((line.y + anchor.intra_line_offset).max(0.))
}

struct VisualTextRefresh {
    old_y_after: f32,
    y_delta: f32,
}

struct TextRefreshRequest<'a> {
    snapshot: &'a document_core::DocumentSnapshot,
    node_id: NodeId,
    image_dimensions: &'a NodeImageDimensions,
    layout_width: f32,
    zoom_factor: f32,
    measurement: Option<&'a FontMeasurement>,
}

fn shift_retained_line(line: &mut VisualLineSpec, byte_delta: isize, y_delta: f32) -> Option<()> {
    line.range.start = line.range.start.checked_add_signed(byte_delta)?;
    line.range.end = line.range.end.checked_add_signed(byte_delta)?;
    if byte_delta != 0
        && let Some(math) = &mut line.inline_math
    {
        // Formula rasters and line-local attachment ranges stay valid. Only
        // the containing formula line's absolute source range follows the edit.
        let math = Arc::make_mut(math);
        math.range.start = math.range.start.checked_add_signed(byte_delta)?;
        math.range.end = math.range.end.checked_add_signed(byte_delta)?;
    }
    line.y += y_delta;
    if line.slot.is_some() || line.table_cell.is_some() {
        line.table_row_y += y_delta;
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
        visual_lines.partition_point(|line| line.range.start < segment.projection_range.start);
    let slot = visual_lines.get(node_first)?.slot;
    let in_row = |line: &VisualLineSpec| {
        if let Some(slot) = slot {
            line.slot.is_some_and(|candidate| {
                candidate.group == slot.group
                    && candidate.item / candidate.columns == slot.item / slot.columns
            })
        } else if let Some((table, row, _)) = table_cell {
            line.table_cell
                .is_some_and(|(id, candidate_row, _, _)| id == table && candidate_row == row)
        } else {
            segment.projection_range.start <= line.range.start
                && line.range.end <= segment.projection_range.end
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
    let range_start = visual_lines[first].range.start;
    let range_end = visual_lines.get(end.checked_sub(1)?)?.range.end;
    let segment_start = projection
        .segments()
        .partition_point(|s| s.projection_range.end < range_start);
    let segment_end = projection
        .segments()
        .partition_point(|s| s.projection_range.start <= range_end);
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
    let (_, byte_delta) = projection.refresh_text_node(snapshot, node_id)?;
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
    let y_delta = new_y_after - old_y_after;
    for line in &mut visual_lines[end..] {
        shift_retained_line(line, byte_delta, y_delta)?;
    }
    let index_delta = replacement.len() as isize - (end - first) as isize;
    let replacement_order = visual_line_paint_order(&replacement);
    for index in &mut paint_order[paint_end..] {
        *index = index.checked_add_signed(index_delta)?;
    }
    paint_order.splice(
        paint_start..paint_end,
        replacement_order.into_iter().map(|i| first + i),
    );
    visual_lines.splice(first..end, replacement);
    Some(VisualTextRefresh {
        old_y_after,
        y_delta,
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
    let first_line =
        visual_lines.partition_point(|line| line.range.start < old_segment.projection_range.start);
    let after_lines =
        visual_lines.partition_point(|line| line.range.start <= old_segment.projection_range.end);
    let first = visual_lines.get(first_line)?;
    let old_math_extent = first.display_math.as_ref().map_or(0., |math| math.extent());
    let last = visual_lines.get(after_lines.checked_sub(1)?)?;
    if projection
        .segment_for_range(&first.range)
        .is_none_or(|segment| segment.node_id != node_id)
        || projection
            .segment_for_range(&last.range)
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
    let (_, byte_delta) = projection.refresh_text_node(snapshot, node_id)?;
    let segment = projection.segment_for_node(node_id)?.clone();
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
    for line in &mut replacement {
        line.x_fraction = placement.0;
        line.width_fraction = placement.1;
        line.inset = placement.4;
    }
    if let Some(first) = replacement.first_mut() {
        let new_math_extent = first.display_math.as_ref().map_or(0., |math| math.extent());
        first.style.space_above = placement.2 + (new_math_extent - old_math_extent) * zoom_factor;
        first.gap_before = placement.5;
    }
    if let Some(last) = replacement.last_mut() {
        last.style.space_below = placement.3;
    }
    let new_y_after = position_visual_lines(&mut replacement, projection, layout_width, y_before);
    let y_delta = new_y_after - old_y_after;
    let replacement_len = replacement.len();

    for line in &mut visual_lines[after_lines..] {
        shift_retained_line(line, byte_delta, y_delta)?;
    }
    visual_lines.splice(first_line..after_lines, replacement);
    paint_order.drain(paint_start..paint_start + old_line_count);
    let index_delta =
        isize::try_from(replacement_len).ok()? - isize::try_from(old_line_count).ok()?;
    for index in &mut paint_order[paint_start..] {
        if *index >= after_lines {
            *index = index.checked_add_signed(index_delta)?;
        }
    }
    paint_order.splice(
        paint_start..paint_start,
        first_line..first_line + replacement_len,
    );
    *document_height = visual_document_height(visual_lines);
    Some(VisualTextRefresh {
        old_y_after,
        y_delta,
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
    let segment_text = &text[segment.projection_range.clone()];
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
        return vec![VisualLineSpec {
            range: segment.projection_range.clone(),
            style: with_component_spacing(
                projection,
                VisualLineStyle {
                    line_height: preview.height + 32.,
                    space_above: if table_cell.is_some() { 10. } else { 8. },
                    space_below: if table_cell.is_some() { 10. } else { 24. },
                    ..VisualLineStyle::BODY
                },
                segment,
                true,
                true,
            ),
            html_preview: Some(preview),
            display_math: None,
            inline_math: None,
            inset: html_inset,
            gap_before: 0.,
            y: 0.,
            x_fraction: 0.,
            width_fraction: 1.,
            table_cell,
            table_cell_first: false,
            table_row_y: 0.,
            table_row_height: 0.,
            slot: None,
        }];
    }
    let image_height =
        image_reserved_height(projection, block, segment, image_dimensions, layout_width);
    for_each_display_line_range(segment_text, |local_line| {
        let logical_line = segment.projection_range.start + local_line.start
            ..segment.projection_range.start + local_line.end;
        let mut push_line = |range, inline: Option<inline_math::InlineLine>| {
            let mut style = visual_line_style_for(projection, block, segment, &range, image_height);
            if let Some(inline) = &inline {
                style.line_height = style.line_height.max(inline.ascent + inline.descent + 4.);
            }
            let inset = if table_cell.is_some() {
                12. + table_insets(segment, projection).1
            } else {
                container_inset(segment)
            } + f32::from(matches!(block, BlockNode::CodeBlock(_)))
                * CODE_BLOCK_PADDING;
            lines.push(VisualLineSpec {
                range,
                html_preview: None,
                display_math: None,
                inline_math: inline.map(Arc::new),
                style,
                inset,
                gap_before: 0.,
                y: 0.,
                x_fraction: 0.,
                width_fraction: 1.,
                table_cell,
                table_cell_first: false,
                table_row_y: 0.,
                table_row_height: 0.,
                slot: None,
            });
        };
        let mut wrap = |range: Range<usize>| {
            if let Some(measurement) = measurement
                && inline_math::has_math(projection, segment.node_id)
                && let Some(lines) = inline_math::layout(
                    projection,
                    segment,
                    range.clone(),
                    segment_text_width(segment, projection, layout_width),
                    font_size_override.unwrap_or_else(|| {
                        visual_line_style_for(projection, block, segment, &range, image_height)
                            .font_size
                    }),
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
                    segment_text_width(segment, projection, layout_width),
                    font_size_override.unwrap_or_else(|| {
                        visual_line_style_for(projection, block, segment, &range, image_height)
                            .font_size
                    }),
                )
            {
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
    if let Some(preview) = crate::math::prepare_block(block)
        && let Some(first) = lines.first_mut()
    {
        first.style.space_above += preview.extent();
        first.display_math = Some(preview);
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
    let (outer, inner) = table_insets(segment, projection);
    let available_width = segment
        .context
        .table_cell
        .and_then(|(table_id, _, column)| {
            let widths = projection.fitted_table_widths(
                table_id,
                projection
                    .table_available_width(table_id, table_container_width(layout_width, outer)),
            )?;
            widths.get(column).copied()
        })
        .unwrap_or(layout_width);
    let inset = if segment.context.table_cell.is_some() {
        24. + inner
    } else {
        container_inset(segment) + 8.
    };
    (available_width - inset).max(1.)
}

/// Ancestor indentation belongs to the whole table, never to every column.
fn container_inset(segment: &crate::ProjectionSegment) -> f32 {
    segment.context.list_depth as f32 * 24.
        + segment.context.ordered_list_depth as f32 * NUMBERED_LIST_EXTRA_GAP
        + segment.context.quote_depth as f32 * 24.
        + f32::from(segment.context.alert.is_some()) * ALERT_CONTENT_INSET
}

fn table_insets(segment: &crate::ProjectionSegment, projection: &TextProjection) -> (f32, f32) {
    let Some(outer) = segment
        .context
        .table_cell
        .and_then(|(id, _, _)| projection.table_context(id))
        .map(|context| &context.outer)
    else {
        return (0., container_inset(segment));
    };
    let inherited = outer.list_depth as f32 * 24.
        + outer.ordered_list_depth as f32 * NUMBERED_LIST_EXTRA_GAP
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

fn step_number_bounds(line: Bounds<Pixels>, zoom: f32) -> Bounds<Pixels> {
    Bounds::new(
        point(
            line.left() - px((28. + NUMBERED_LIST_EXTRA_GAP) * zoom),
            line.center().y - px(12.5 * zoom),
        ),
        size(px(25. * zoom), px(25. * zoom)),
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
    let inset = segment_for_line(projection, &line.range)
        .map_or(0., |segment| table_insets(segment, projection).0);
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
            let row = slot.item / slot.columns;
            let end = lines[index..]
                .iter()
                .position(|line| {
                    line.slot.is_none_or(|candidate| {
                        candidate.group != slot.group || candidate.item / candidate.columns != row
                    })
                })
                .map_or(lines.len(), |offset| index + offset);
            let mut row_height = 0_f32;
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
                row_height = row_height.max(height);
                first = last;
            }
            for line in &mut lines[index..end] {
                if line.table_cell.is_none() {
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
                y += LAYOUT_GAP;
            }
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
        let columns = lines[index].table_cell.map_or(1, |cell| cell.3);
        let inset = segment_for_line(projection, &lines[index].range)
            .map_or(0., |segment| table_insets(segment, projection).0);
        let available =
            projection.table_available_width(table_id, table_container_width(region.width, inset));
        let fitted = projection.fitted_table_widths(table_id, available);
        let widths = fitted.as_deref();
        let total_width = widths.map_or(160. * columns as f32, |widths| widths.iter().sum());
        let table_scale = (available.max(1.) / total_width).max(1.);
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
            line.x_fraction =
                (region.left + inset + preceding_width * table_scale) / layout_width.max(1.);
            line.width_fraction = column_width * table_scale / layout_width.max(1.);
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
        .partition_point(|line| line.range.start <= offset);
    split
        .checked_sub(1)
        .and_then(|index| editor.visual_lines.get(index))
        .filter(|line| line.range.contains(&offset) || line.range.end == offset)
        .map(|line| line.range.clone())
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
            container.left() + px(width * spec.x_fraction + spec.inset - horizontal_offset),
            container.top() + px(spec.y),
        ),
        size(
            px((width * spec.width_fraction
                - spec.inset
                - if is_code {
                    CODE_BLOCK_PADDING * editor.zoom_factor
                } else if spec.table_cell.is_some() {
                    12. * editor.zoom_factor
                } else if spec.slot.is_some_and(|slot| slot.cards) {
                    CARD_PADDING * editor.zoom_factor
                } else {
                    8. * editor.zoom_factor
                })
            .max(1.)),
            px(spec.style.line_height),
        ),
    )
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

fn alert_color(palette: MineralPalette, kind: &AlertKind) -> u32 {
    match kind {
        AlertKind::Note => palette.info,
        AlertKind::Tip => palette.success,
        AlertKind::Important => palette.syntax_number,
        AlertKind::Warning => palette.warning,
        AlertKind::Caution => palette.error,
        AlertKind::Other(_) => palette.secondary,
    }
}

fn alert_icon(kind: &AlertKind) -> IconName {
    match kind {
        AlertKind::Note => IconName::Info,
        AlertKind::Tip => IconName::CircleCheck,
        AlertKind::Important => IconName::Star,
        AlertKind::Warning | AlertKind::Caution => IconName::TriangleAlert,
        AlertKind::Other(_) => IconName::Info,
    }
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
    let first_line = line.start == segment.projection_range.start;
    let last_line = line.end == segment.projection_range.end;
    if segment.context.image_source.is_some() {
        return with_component_spacing(
            projection,
            VisualLineStyle {
                font_size: 15.,
                line_height: image_height.unwrap_or(180.),
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
            BlockNode::Heading(heading) => match heading.level {
                1 => (42., 42.84),
                2 => (32., 35.84),
                3 => (26., 31.2),
                4 => (22., 26.4),
                5 => (19., 22.8),
                _ => (17., 20.4),
            },
            BlockNode::CodeBlock(_) => (15., 22.5),
            _ => (15.5, 23.),
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
    let style = match block {
        BlockNode::ThematicBreak { .. } => VisualLineStyle {
            font_size: VisualLineStyle::BODY.font_size,
            line_height: 1.,
            space_above: 0.,
            space_below: 0.,
        },
        BlockNode::Heading(heading) => {
            let (font_size, line_height) = match heading.level {
                1 => (44., 50.),
                2 => (28., 34.),
                3 => (23., 30.),
                4 => (22., 26.4),
                5 => (19., 22.8),
                _ => (17., 20.4),
            };
            VisualLineStyle {
                font_size,
                line_height,
                space_above: if first_line {
                    if segment.projection_range.start == 0 {
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
            font_size: 15.,
            line_height: 22.5,
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
            space_below: if last_line { 16. } else { 0. },
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
    let Some(segment) = segment_for_line(projection, &line.range) else {
        return (0., 0.);
    };
    let Some(context) = segment
        .context
        .table_cell
        .and_then(|(id, _, _)| projection.table_context(id))
    else {
        return (0., 0.);
    };
    component_spacing(
        projection,
        segment,
        line.range.start == segment.projection_range.start,
        line.range.end == segment.projection_range.end,
        Some(&context.containers),
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
                Some(BlockNode::BlockQuote { .. }) => 16.,
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
    let width_fraction = segment
        .context
        .table_cell
        .and_then(|(table_id, _, column)| {
            let BlockNode::Table(table) = projection.block(table_id)? else {
                return None;
            };
            let widths = projection.table_widths(table.id)?;
            let total = widths.iter().sum::<f32>().max(1.);
            widths.get(column).map(|width| width / total)
        })
        .unwrap_or(1.);
    let available = (layout_width * width_fraction - 16.).max(1.);
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
            let segment = segment_for_line(projection, &line.range)?;
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
    blocks
        .iter()
        .filter_map(|block| semantic_block(block, bounds))
        .collect()
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
    for index in indices {
        let Some(line) = lines.get(index) else {
            continue;
        };
        let Some(segment) = segment_for_line(projection, &line.range) else {
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

fn styled_runs(
    editor: &RichDocumentEditor,
    line: &Range<usize>,
    line_len: usize,
    text_style: &gpui::TextStyle,
    placeholder: bool,
    palette: MineralPalette,
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
    palette: MineralPalette,
) -> Vec<TextRun> {
    let mut base_font = text_style.font();
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
                rgba(MineralPalette::with_alpha(palette.secondary, 0x80)).into()
            } else {
                base_color
            },
            background_color: None,
            underline: None,
            strikethrough: None,
        }];
    };
    match block {
        BlockNode::Heading(heading) => {
            base_font.family = match heading.level {
                1 => "Fraunces Mineral H1",
                2 => "Fraunces Mineral H2",
                _ => "Fraunces Mineral H3",
            }
            .into();
            base_font.weight = FontWeight::SEMIBOLD;
            base_color = rgb(palette.heading).into();
        }
        BlockNode::CodeBlock(_) => {
            base_font.family = "Spline Sans Mono Mineral".into();
        }
        BlockNode::PreservedSource { .. } => {
            base_font.family = "Spline Sans Mono Mineral".into();
            base_color = rgb(palette.secondary).into();
            base_background = Some(rgb(palette.surface).into());
        }
        _ => {}
    }
    if segment.context.table_header {
        base_font.weight = FontWeight::SEMIBOLD;
    }
    if let BlockNode::CodeBlock(code) = block {
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

    let node_start = segment.node_range.start + line.start - segment.projection_range.start;
    let node_end = node_start + line_len;
    let mut result = Vec::new();
    for inline in rich_text.runs() {
        let overlap = inline.range.start.max(node_start)..inline.range.end.min(node_end);
        if overlap.start >= overlap.end {
            continue;
        }
        let mut font = base_font.clone();
        let mut color = base_color;
        let mut background_color = base_background;
        let mut underline = None;
        let mut strikethrough = None;
        for style in &inline.styles {
            match style {
                InlineStyle::Bold => font.weight = FontWeight::BOLD,
                InlineStyle::Italic => {
                    font.style = if font.family.as_ref() == "Spline Sans Mineral" {
                        FontStyle::Oblique
                    } else {
                        FontStyle::Italic
                    };
                }
                InlineStyle::Strikethrough => {
                    strikethrough = Some(StrikethroughStyle {
                        color: Some(color),
                        thickness: px(1.),
                    });
                }
                InlineStyle::Code | InlineStyle::Math { .. } => {
                    font.family = "Spline Sans Mono Mineral".into();
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
        result.push(TextRun {
            len: overlap.len(),
            font,
            color,
            background_color,
            underline,
            strikethrough,
        });
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
        let prefix = chapter_prefix_len(&projection.text()[segment.projection_range.clone()]);
        let mut styled = Vec::with_capacity(result.len() + 1);
        let mut offset = node_start;
        for mut run in result {
            let length = run.len;
            if offset < prefix {
                let mut number = run.clone();
                number.len = length.min(prefix - offset);
                number.color = rgb(palette.accent).into();
                number.font.family = "Spline Sans Mineral".into();
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
    palette: MineralPalette,
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
    lines
        .iter()
        .position(|line| line.range.contains(&offset) || line.range.end == offset)
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
            bounds.top() + px(line.y + (32. + (top + bottom) * 0.5) * editor.zoom_factor),
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
                    origin.top() + px(line.y + 32. + (b[1] + b[3]) * 0.5),
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
                    .projection_range
                    .start;
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
                        math_edit_node: None,
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
                    math_edit_node: None,
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
                assert!(editor.html_selection_chrome(node, MineralPalette::for_dark(false), false).is_empty());
                assert_eq!(editor.html_selection_chrome(node, MineralPalette::for_dark(false), true).len(), 1);
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
                    "One café\nTwo 😀 words"
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
                let palette = MineralPalette::for_dark(false);
                for spec in editor.visual_lines.iter() {
                    let text = &editor.projection.text()[spec.range.clone()];
                    if text.is_empty() {
                        continue;
                    }
                    let runs = styled_runs(
                        editor, &spec.range, text.len(), &window.text_style(), false, palette,
                    );
                    let original_lengths: Vec<_> = runs.iter().map(|run| run.len).collect();
                    let marked = apply_marked_runs(runs, editor.marked_range.as_ref(), &spec.range, palette);
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
                    .find(|s| &editor.projection.text()[s.projection_range.clone()] == "Short")
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
                    assert_eq!(actual.range, expected.range);
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
                            && &editor.projection.text()[segment.projection_range.clone()] == marker
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
                    assert_eq!(actual.range, expected.range);
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
                .find(|s| &editor.projection.text()[s.projection_range.clone()] == "Short")
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
                        math_edit_node: None,
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
                assert_eq!(actual.range, expected.range);
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
            "## Properties\n\nCheck these properties.\n\n| Name | Value |\n| --- | --- |\n| State | Ready |\n\n",
            "## Formula\n\nFollowing formula $x^2$ remains attached.\n\nEnd.\n",
        );
        for zoom in [1., 1.5, 2.] {
            for target in [
                "A lead",
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
            ] {
                let (editor, cx) = cx.add_window_view(|window, cx| {
                    RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
                });
                editor.update(cx, |editor, cx| {
                    editor.layout_width = 1280. * zoom;
                    editor.zoom_factor = zoom;
                    editor.measurement = Arc::new(FontMeasurement::new(
                        cx.text_system().clone(),
                        "Spline Sans Mineral".into(),
                        zoom,
                    ));
                    editor.measurement.measure_tables(&mut editor.projection);
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
                        })
                        .unwrap()
                        .node_id;
                    assert!(
                        editor.adaptive.slots.contains_key(&node)
                            || editor.adaptive.lead == Some(node),
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
                    let mut projection = TextProjection::from_snapshot(&editor.document.snapshot());
                    projection.math_edit_node = editor.projection.math_edit_node;
                    projection.table_layout_lock = editor.projection.table_layout_lock.clone();
                    projection.reuse_table_measurements(&editor.projection);
                    let mut expected = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        1280.,
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
                        assert_eq!(actual.range, expected.range, "{target} at {zoom}");
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
                    for (actual, original) in editor.visual_lines.iter().zip(original_lines.iter())
                    {
                        assert_eq!(actual.range, original.range);
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
                editor
                    .link_at_offset(segment.projection_range.start)
                    .as_deref(),
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
                            .segment_for_range(&line.range)
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
                    editor.visual_lines.last().unwrap().range.end
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
                        target.projection_range.start <= line.range.start
                            && line.range.end <= target.projection_range.end
                    })
                    .map(|line| (line.range.clone(), line.style.line_height))
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
                        target.projection_range.start <= line.range.start
                            && line.range.end <= target.projection_range.end
                    })
                    .map(|line| (line.range.clone(), line.style.line_height))
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
                append_alert_chrome(&mut chrome, bounds, 0x2274cc, zoom);
                assert_eq!(
                    chrome.len(),
                    1,
                    "the accent rail must belong to the rounded border, not overlap it"
                );
                let quad = &chrome[0].quad;
                assert_eq!(quad.bounds, bounds);
                assert_eq!(quad.corner_radii, gpui::Corners::all(px(10. * zoom)));
                assert_eq!(quad.border_widths.left, px(3. * zoom));
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
            assert_eq!(line.left() - checkbox.right(), px(5. * zoom));
            // The list reserves a 24 px marker gutter; do not paint outside
            // its content mask (especially the unchecked box's left stroke).
            assert!(checkbox.left() >= line.left() - px(24. * zoom));
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
            let handle = editor.read_with(cx, |editor, _| {
                editor.math_scroll_handles.values().next().unwrap().clone()
            });
            assert!(handle.max_offset().x > px(0.));
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
                    bounds.top() + px(line.y + (32. + (top + bottom) * 0.5) * zoom));
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
    fn viewport_slice_stays_bounded_for_huge_documents() {
        let lines = (0..100_000)
            .map(|index| VisualLineSpec {
                html_preview: None,
                display_math: None,
                inline_math: None,
                range: index..index + 1,
                style: VisualLineStyle::BODY,
                inset: 0.,
                gap_before: 0.,
                y: index as f32 * LINE_HEIGHT,
                x_fraction: 0.,
                width_fraction: 1.,
                table_cell: None,
                table_cell_first: false,
                table_row_y: 0.,
                table_row_height: 0.,
                slot: None,
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
                    .segment_for_range(&line.range)
                    .is_some_and(|s| s.node_id == node_id)
            })
            .unwrap();
        let original_image = first.display_math.as_ref().unwrap().light.image.id;
        assert_ne!(
            original_image,
            first.display_math.as_ref().unwrap().dark.image.id
        );
        assert!(
            first.style.space_above
                >= CODE_HEADER_HEIGHT
                    + CODE_BLOCK_PADDING
                    + preview.height
                    + crate::math::PREVIEW_GAP
        );
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
            range,
            html_preview: None,
            display_math: None,
            inline_math: None,
            style: VisualLineStyle::BODY,
            inset: 0.,
            gap_before: 0.,
            y,
            x_fraction: 0.,
            width_fraction: 1.,
            table_cell: None,
            table_cell_first: false,
            table_row_y: y,
            table_row_height: VisualLineStyle::BODY.line_height,
            slot: None,
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
                                editor.projection.text()[line.range.clone()].contains(label)
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let mut previous = AdaptivePlan::default();
            let mut published_geometry = None;
            let mut last_stack: Option<Arc<PublishedGeometry>> = None;
            for (index, width) in [1100., 1100., 700., 700.].into_iter().enumerate() {
                let recovery = reflow::Recovery::default();
                let (prepared, _, _) = PreparedDocumentView::try_prepare_snapshot_with_images(
                    &document.snapshot(), &HashMap::new(), None,
                    ReflowViewport { published_geometry, width, height: 900., zoom: 1.,
                        math_edit_node: None, editing_node: None, table_layout_lock: None,
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
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let mut viewport = ReflowViewport {
                published_geometry: None,
                width: 900.,
                height: 800.,
                zoom: 1.,
                math_edit_node: None,
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
    fn reflow_cancellation_at_each_stage_preserves_canonical_content(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "# Mixed preparation\n\n| Property | Value |\n| --- | --- |\n| Count | 3 |\n\n<div><strong>Retained HTML.</strong></div>\n\n$$\n\\frac{1}{2}\n$$\n";
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let previous = AdaptivePlan::build(&projection, 900., None, false);
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let viewport = ReflowViewport {
                published_geometry: None, width: 900., height: 800., zoom: 1.,
                math_edit_node: None, editing_node: None, table_layout_lock: None,
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
    fn document_switch_keeps_ownership_of_the_running_reflow(cx: &mut gpui::TestAppContext) {
        cx.update(init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown("Old document.").unwrap(),
                window,
                cx,
            )
        });
        cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                editor.sync_image_dimensions(900., cx);
                assert!(editor.reflow.is_active());
                let document = Document::from_markdown("Replacement document.").unwrap();
                let prepared = PreparedDocumentView::prepare(&document);
                editor.replace_document_prepared(document, prepared, cx);
                assert!(
                    editor.reflow.is_active(),
                    "switching documents must not release an unfinished worker"
                );
            })
        });
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert_eq!(
                editor.document.snapshot().serialize().unwrap(),
                "Replacement document."
            );
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
                .all(|line| line.width_fraction * 760. <= PROSE_WIDTH)
        );
        let wide = build_visual_lines_with_images(&projection, &HashMap::new(), 1120.);
        assert!(
            wide.len() < lines.len(),
            "wide windows use the expanded prose measure"
        );
        assert!(
            wide.iter()
                .all(|line| line.width_fraction * 1120. <= PROSE_WIDTH + 0.1)
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
            .find(|line| &projection.text()[line.range.clone()] == "Hero")
            .expect("hero line");
        let section = lines
            .iter()
            .find(|line| &projection.text()[line.range.clone()] == "Section")
            .expect("section line");
        let alert = lines
            .iter()
            .find(|line| &projection.text()[line.range.clone()] == "Read this.")
            .expect("alert line");
        let code = lines
            .iter()
            .find(|line| &projection.text()[line.range.clone()] == "let answer = 42;")
            .expect("code line");

        assert_eq!(hero.inset, 0., "the title shares the prose leading edge");
        assert_eq!(section.inset, 0., "sections do not carry decorative badges");
        assert!(alert.inset >= ALERT_CONTENT_INSET);
        assert!(alert.style.space_above >= ALERT_HEADER_HEIGHT);
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
                segment_for_line(&projection, &line.range)
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
        let spans = syntax_spans("let value = \"mineral\"; // sample", Some("rust"));
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
            "<!-- mineral-table:v1 {\"border\":\"Dotted\",\"widths\":[100.0,300.0]} -->\n",
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
        assert_eq!(lines[0].width_fraction, 0.25);
        assert_eq!(lines[1].x_fraction, 0.25);
        assert_eq!(lines[1].width_fraction, 0.75);
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
                "<!-- mineral-table:v1 {\"border\":\"Dotted\",\"widths\":[2000.0,2000.0]} -->\n",
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
            "<!-- mineral-table:v1 {\"border\":\"Dotted\",\"widths\":[600.0,600.0]} -->\n",
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
        assert_eq!(measured[0].style.line_height, 292.);
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
                    .segment_for_range(&line.range)
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
                    .segment_for_range(&line.range)
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
            "| View | Mineral |\n\n",
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
