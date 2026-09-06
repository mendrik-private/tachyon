use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    hash::{DefaultHasher, Hash as _, Hasher as _},
    ops::Range,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};

use document_core::{
    Affinity, BlockNode, BlockStyle, ColumnAlignment, Document, DocumentError, DocumentPosition,
    DocumentSnapshot, EditCommand, InlineFormat, InlineStyle, InsertBlockKind, NodeId,
    RectangularSelection, Revision, RichClipboard, Selection, TableBorder, TextSelection,
};
use gpui::{
    AnyElement, App, BorderStyle, Bounds, ClipboardItem, ContentMask, Context, CursorStyle,
    Element, ElementId, ElementInputHandler, Entity, EntityInputHandler, FocusHandle, Focusable,
    FontStyle, FontWeight, GlobalElementId, ImageSource, KeyBinding, LayoutId, MouseButton,
    MouseDownEvent, MouseMoveEvent, MouseUpEvent, ObjectFit, PaintQuad, Pixels, Point, Resource,
    Role, ScrollHandle, ScrollWheelEvent, ShapedLine, StrikethroughStyle, Style, StyledImage as _,
    TextRun, UTF16Selection, UnderlineStyle, Window, actions, div, fill, hash, img, outline, point,
    prelude::*, px, relative, rgb, rgba, size,
};
use gpui_component::{
    ActiveTheme as _, Sizable as _, Theme,
    input::{Input, InputState},
};
use unicode_segmentation::UnicodeSegmentation as _;

use crate::{MineralPalette, SharedDocumentSession, TextProjection};

const KEY_CONTEXT: &str = "RichDocumentEditor";
const LINE_HEIGHT: f32 = 28.8;
const OUTLINE_JUMP_STEPS: u32 = 15;
const OUTLINE_JUMP_FRAME: Duration = Duration::from_millis(8);
const SHAPED_LINE_CACHE_CAPACITY: usize = 2_048;
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
    visual_lines: Vec<VisualLineSpec>,
    paint_order: Vec<usize>,
    document_height: f32,
}

impl PreparedDocumentView {
    #[must_use]
    pub fn prepare(document: &Document) -> Self {
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let visual_lines = build_visual_lines(document, &projection);
        let paint_order = visual_line_paint_order(&visual_lines);
        let document_height = visual_document_height(&visual_lines);
        Self {
            projection,
            visual_lines,
            paint_order,
            document_height,
        }
    }

    fn prepare_snapshot_with_images(
        snapshot: &DocumentSnapshot,
        source_dimensions: &SourceImageDimensions,
        document_directory: Option<&std::path::Path>,
        layout_width: f32,
    ) -> (Self, NodeImageDimensions) {
        let projection = TextProjection::from_snapshot(snapshot);
        let image_dimensions = projection
            .image_segments()
            .filter_map(|segment| {
                let source = segment.context.image_source.as_ref()?;
                let resource = resolved_image_resource(source, document_directory);
                source_dimensions
                    .get(&hash(&resource))
                    .copied()
                    .map(|size| (segment.node_id, (source.clone(), size)))
            })
            .collect::<HashMap<_, _>>();
        let visual_lines =
            build_visual_lines_with_images(&projection, &image_dimensions, layout_width);
        let paint_order = visual_line_paint_order(&visual_lines);
        let document_height = visual_document_height(&visual_lines);
        (
            Self {
                projection,
                visual_lines,
                paint_order,
                document_height,
            },
            image_dimensions,
        )
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
        if refresh_text_node_geometry(
            &mut self.projection,
            &mut self.visual_lines,
            &mut self.paint_order,
            &mut self.document_height,
            TextRefreshRequest {
                snapshot,
                node_id,
                image_dimensions: &HashMap::new(),
                layout_width: 760.,
            },
        )
        .is_none()
        {
            return false;
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
struct VisualLineSpec {
    range: Range<usize>,
    style: VisualLineStyle,
    inset: f32,
    y: f32,
    x_fraction: f32,
    width_fraction: f32,
    table_cell: Option<(NodeId, usize, usize, usize)>,
    table_cell_first: bool,
    table_row_y: f32,
    table_row_height: f32,
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
        Up,
        Down,
        SelectLeft,
        SelectRight,
        SelectUp,
        SelectDown,
        SelectAll,
        Home,
        End,
        Copy,
        Cut,
        Paste,
        PasteAsMarkdown,
        Undo,
        Redo,
        Save,
        FormatBold,
        FormatItalic,
        FormatStrike,
        FormatCode,
        FormatLink,
        Enter,
        HardBreak,
        NextTableCell,
        PreviousTableCell,
        ExitTable,
        Dismiss,
    ]
);

pub fn init(cx: &mut App) {
    if !cx.has_global::<Theme>() {
        gpui_component::init(cx);
    }
    cx.bind_keys([
        KeyBinding::new("backspace", Backspace, Some(KEY_CONTEXT)),
        KeyBinding::new("delete", Delete, Some(KEY_CONTEXT)),
        KeyBinding::new("left", Left, Some(KEY_CONTEXT)),
        KeyBinding::new("right", Right, Some(KEY_CONTEXT)),
        KeyBinding::new("up", Up, Some(KEY_CONTEXT)),
        KeyBinding::new("down", Down, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-left", SelectLeft, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-right", SelectRight, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-up", SelectUp, Some(KEY_CONTEXT)),
        KeyBinding::new("shift-down", SelectDown, Some(KEY_CONTEXT)),
        KeyBinding::new("home", Home, Some(KEY_CONTEXT)),
        KeyBinding::new("end", End, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-a", SelectAll, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-c", Copy, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-x", Cut, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-v", Paste, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-v", PasteAsMarkdown, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-z", Undo, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-z", Redo, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-s", Save, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-b", FormatBold, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-i", FormatItalic, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-shift-x", FormatStrike, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-e", FormatCode, Some(KEY_CONTEXT)),
        KeyBinding::new("ctrl-k", FormatLink, Some(KEY_CONTEXT)),
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
    level: Option<usize>,
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
    document: SharedDocumentSession,
    selection: Selection,
    projection: TextProjection,
    projected_generation: u64,
    focus_handle: FocusHandle,
    scroll_handle: ScrollHandle,
    link_input: Entity<InputState>,
    link_popover_visible: bool,
    image_source_input: Entity<InputState>,
    image_alt_input: Entity<InputState>,
    image_popover_node: Option<NodeId>,
    document_directory: Option<PathBuf>,
    shared_image_dimensions: Option<SharedImageDimensions>,
    image_layout_dimensions: NodeImageDimensions,
    image_dimensions_generation: u64,
    requested_image_dimensions_generation: u64,
    layout_width: f32,
    requested_layout_width: f32,
    reflow_in_flight: bool,
    marked_range: Option<Range<usize>>,
    visual_lines: Vec<VisualLineSpec>,
    paint_order: Vec<usize>,
    document_height: f32,
    painted_lines: Vec<PaintedLine>,
    preferred_x: Option<Pixels>,
    element_bounds: Option<Bounds<Pixels>>,
    is_selecting: bool,
    table_resize_drag: Option<TableResizeDrag>,
    toolbar_visible: bool,
    toolbar_generation: u64,
    jump_generation: u64,
    horizontal_scrolls: HashMap<NodeId, f32>,
    horizontal_metrics: HashMap<NodeId, (f32, f32)>,
    shaped_line_cache: RefCell<BoundedLru<ShapeCacheKey, ShapedLine>>,
    context_menu_position: Option<Point<Pixels>>,
    last_error: Option<String>,
    has_painted: bool,
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
        let image_source_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Image source"));
        let image_alt_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Alternative text"));
        Self {
            document,
            selection,
            projection: prepared.projection,
            projected_generation,
            focus_handle: cx.focus_handle(),
            scroll_handle: ScrollHandle::new(),
            link_input,
            link_popover_visible: false,
            image_source_input,
            image_alt_input,
            image_popover_node: None,
            document_directory: None,
            shared_image_dimensions: None,
            image_layout_dimensions: HashMap::new(),
            image_dimensions_generation: 0,
            requested_image_dimensions_generation: 0,
            layout_width: 760.,
            requested_layout_width: 760.,
            reflow_in_flight: false,
            marked_range: None,
            visual_lines: prepared.visual_lines,
            paint_order: prepared.paint_order,
            document_height: prepared.document_height,
            painted_lines: Vec::new(),
            preferred_x: None,
            element_bounds: None,
            is_selecting: false,
            table_resize_drag: None,
            toolbar_visible: false,
            toolbar_generation: 0,
            jump_generation: 0,
            horizontal_scrolls: HashMap::new(),
            horizontal_metrics: HashMap::new(),
            shaped_line_cache: RefCell::new(BoundedLru::new(SHAPED_LINE_CACHE_CAPACITY)),
            context_menu_position: None,
            last_error: None,
            has_painted: false,
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
        self.last_error.as_deref()
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
        prepared: PreparedDocumentView,
        cx: &mut Context<Self>,
    ) {
        self.document = document;
        self.selection = self.document.snapshot().selection().clone();
        self.install_prepared(prepared);
        self.projected_generation = self.document.generation();
        self.reset_document_view_caches();
        cx.notify();
    }

    fn install_prepared(&mut self, prepared: PreparedDocumentView) {
        self.projection = prepared.projection;
        self.visual_lines = prepared.visual_lines;
        self.paint_order = prepared.paint_order;
        self.document_height = prepared.document_height;
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
        self.link_popover_visible = false;
        self.image_popover_node = None;
        cx.notify();
        true
    }

    fn reset_document_view_caches(&mut self) {
        self.marked_range = None;
        self.toolbar_visible = false;
        self.context_menu_position = None;
        self.link_popover_visible = false;
        self.image_popover_node = None;
        self.image_layout_dimensions.clear();
        self.image_dimensions_generation = 0;
        self.requested_image_dimensions_generation = 0;
        self.requested_layout_width = self.layout_width;
        self.reflow_in_flight = false;
        self.horizontal_scrolls.clear();
        self.horizontal_metrics.clear();
        self.shaped_line_cache.borrow_mut().clear();
        self.last_error = None;
        self.has_painted = false;
    }

    pub fn set_document_directory(&mut self, directory: Option<PathBuf>, cx: &mut Context<Self>) {
        self.document_directory = directory;
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

    fn sync_image_dimensions(&mut self, width: f32, cx: &mut Context<Self>) {
        let Some(dimensions) = self.shared_image_dimensions.as_ref() else {
            return;
        };
        let Ok(dimensions) = dimensions.lock() else {
            return;
        };
        let dimensions_generation = dimensions.0;
        let width = width.max(1.);
        if dimensions_generation == self.image_dimensions_generation
            && (width - self.layout_width).abs() < 0.5
        {
            return;
        }
        self.requested_image_dimensions_generation = dimensions_generation;
        self.requested_layout_width = width;
        if self.reflow_in_flight {
            return;
        }
        let source_dimensions = dimensions.1.clone();
        drop(dimensions);

        self.reflow_in_flight = true;
        let snapshot = self.document.snapshot();
        let document_generation = self.document.generation();
        let document_directory = self.document_directory.clone();
        let reflow = cx
            .background_executor()
            .spawn_dedicated(move |_| async move {
                PreparedDocumentView::prepare_snapshot_with_images(
                    &snapshot,
                    &source_dimensions,
                    document_directory.as_deref(),
                    width,
                )
            });
        cx.spawn(async move |this, cx| {
            let (prepared, image_dimensions) = reflow.await;
            let _ = this.update(cx, |this, cx| {
                this.reflow_in_flight = false;
                if this.document.generation() == document_generation
                    && this.requested_image_dimensions_generation == dimensions_generation
                    && (this.requested_layout_width - width).abs() < 0.5
                {
                    let scroll_y = this.scroll_metrics().0;
                    let anchor = this
                        .visual_lines
                        .iter()
                        .rev()
                        .find(|line| line.y <= scroll_y)
                        .and_then(|line| {
                            segment_for_line(&this.projection, &line.range)
                                .map(|segment| (segment.node_id, scroll_y - line.y))
                        });
                    this.image_layout_dimensions = image_dimensions;
                    this.layout_width = width;
                    this.image_dimensions_generation = dimensions_generation;
                    this.jump_generation = this.jump_generation.saturating_add(1);
                    this.install_prepared(prepared);
                    if let Some((node_id, offset)) = anchor
                        && let Some(line) = this.visual_lines.iter().find(|line| {
                            segment_for_line(&this.projection, &line.range)
                                .is_some_and(|segment| segment.node_id == node_id)
                        })
                    {
                        let x = this.scroll_handle.offset().x;
                        this.scroll_handle
                            .set_offset(point(x, px(-(line.y + offset).max(0.))));
                    }
                }
                cx.notify();
            });
        })
        .detach();
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
    pub fn scroll_metrics(&self) -> (f32, f32) {
        let scroll_y: f32 = (-self.scroll_handle.offset().y).into();
        let viewport_height: f32 = self.scroll_handle.bounds().size.height.into();
        (scroll_y.max(0.), viewport_height.max(0.))
    }

    pub fn set_scroll_y(&mut self, scroll_y: f32, cx: &mut Context<Self>) {
        self.jump_generation = self.jump_generation.saturating_add(1);
        let x = self.scroll_handle.offset().x;
        self.scroll_handle
            .set_offset(point(x, px(-scroll_y.max(0.))));
        self.toolbar_visible = false;
        self.link_popover_visible = false;
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
    }

    #[must_use]
    pub fn view_state(&self) -> EditorViewState {
        let (selection, reversed) = self.selected_byte_range();
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
        let snapshot = self.document.snapshot();
        self.projected_generation = self.document.generation();
        self.projection = TextProjection::from_snapshot(&snapshot);
        self.visual_lines = build_visual_lines_with_images(
            &self.projection,
            &self.image_layout_dimensions,
            self.layout_width,
        );
        self.refresh_visual_index();
    }

    fn refresh_visual_index(&mut self) {
        self.paint_order = visual_line_paint_order(&self.visual_lines);
        self.document_height = visual_document_height(&self.visual_lines);
    }

    fn refresh_after_transaction(&mut self, result: &document_core::TransactionResult) {
        if result.dirty_node_ids.len() == 1
            && let Some(node_id) = result.dirty_node_ids.first().copied()
            && self.refresh_text_node(node_id)
        {
            return;
        }
        self.refresh_projection();
        self.shaped_line_cache.borrow_mut().clear();
    }

    /// Updates one non-table text segment and its visual-line slice. Topology
    /// changes and table row-height coupling intentionally use the full path.
    fn refresh_text_node(&mut self, node_id: NodeId) -> bool {
        let scroll_y = self.scroll_metrics().0;
        let snapshot = self.document.snapshot();
        let Some(refresh) = refresh_text_node_geometry(
            &mut self.projection,
            &mut self.visual_lines,
            &mut self.paint_order,
            &mut self.document_height,
            TextRefreshRequest {
                snapshot: &snapshot,
                node_id,
                image_dimensions: &self.image_layout_dimensions,
                layout_width: self.layout_width,
            },
        ) else {
            return false;
        };
        if scroll_y >= refresh.old_y_after && refresh.y_delta.abs() > f32::EPSILON {
            let x = self.scroll_handle.offset().x;
            self.scroll_handle
                .set_offset(point(x, px(-(scroll_y + refresh.y_delta).max(0.))));
        }
        self.projected_generation = self.document.generation();
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
        self.sync_selection_to_document()?;
        let result = self.document.apply(command)?;
        self.selection = result.selection.clone();
        Ok(result)
    }

    fn selected_byte_range(&self) -> (Range<usize>, bool) {
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
        self.schedule_selection_toolbar(cx);
        cx.emit(EditorEvent::ViewChanged);
        cx.notify();
    }

    fn schedule_selection_toolbar(&mut self, cx: &mut Context<Self>) {
        self.toolbar_generation = self.toolbar_generation.saturating_add(1);
        let generation = self.toolbar_generation;
        let selected = !self.selected_byte_range().0.is_empty();
        self.toolbar_visible = false;
        if !selected {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let _ = this.update(cx, |editor, cx| {
                if editor.toolbar_generation == generation
                    && !editor.selected_byte_range().0.is_empty()
                {
                    editor.toolbar_visible = true;
                    cx.notify();
                }
            });
        })
        .detach();
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
        let result = self
            .apply_command(EditCommand::SetSelection(selection))
            .and_then(|_| {
                self.apply_command(EditCommand::ReplaceSelection {
                    text: text.to_owned(),
                    typing,
                })
            });
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
        self.set_selection(offset..offset, false, window, cx);
    }

    fn select_to(&mut self, offset: usize, window: &mut Window, cx: &mut Context<Self>) {
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
        let text = self.projection.text();
        let offset = offset.min(text.len());
        text[..offset]
            .grapheme_indices(true)
            .next_back()
            .map_or(0, |(index, _)| index)
    }

    fn next_boundary(&self, offset: usize) -> usize {
        let text = self.projection.text();
        let offset = offset.min(text.len());
        text[offset..]
            .grapheme_indices(true)
            .nth(1)
            .map_or(text.len(), |(index, _)| offset + index)
    }

    fn left(&mut self, _: &Left, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        let (range, reversed) = self.selected_byte_range();
        if range.is_empty() {
            self.move_to(self.previous_boundary(self.cursor_offset()), window, cx);
        } else {
            self.move_to(if reversed { range.end } else { range.start }, window, cx);
        }
    }

    fn right(&mut self, _: &Right, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        let (range, reversed) = self.selected_byte_range();
        if range.is_empty() {
            self.move_to(self.next_boundary(self.cursor_offset()), window, cx);
        } else {
            self.move_to(if reversed { range.start } else { range.end }, window, cx);
        }
    }

    fn select_left(&mut self, _: &SelectLeft, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.select_to(self.previous_boundary(self.cursor_offset()), window, cx);
    }

    fn select_right(&mut self, _: &SelectRight, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        self.select_to(self.next_boundary(self.cursor_offset()), window, cx);
    }

    fn vertical_target(&mut self, direction: isize) -> Option<usize> {
        let offset = self.cursor_offset();
        let current_index = self
            .painted_lines
            .iter()
            .position(|line| line.range.contains(&offset))
            .or_else(|| {
                self.painted_lines
                    .iter()
                    .position(|line| line.range.end == offset)
            })?;
        let current = &self.painted_lines[current_index];
        let local = offset
            .saturating_sub(current.range.start)
            .min(current.layout.len());
        let preferred_x = self.preferred_x.unwrap_or(
            aligned_text_left(current.bounds, &current.layout, current.alignment)
                + current.layout.x_for_index(local),
        );
        self.preferred_x = Some(preferred_x);
        let target_index = current_index.checked_add_signed(direction)?;
        let target = self.painted_lines.get(target_index)?;
        let relative_x = (preferred_x
            - aligned_text_left(target.bounds, &target.layout, target.alignment))
        .max(px(0.));
        Some(
            target.range.start
                + target
                    .layout
                    .closest_index_for_x(relative_x)
                    .min(target.range.len()),
        )
    }

    fn keep_offset_visible(&mut self, offset: usize) {
        let Some(line) = self
            .visual_lines
            .iter()
            .find(|line| line.range.contains(&offset))
            .or_else(|| {
                self.visual_lines
                    .iter()
                    .find(|line| line.range.end == offset)
            })
        else {
            return;
        };
        let (scroll_y, viewport_height) = self.scroll_metrics();
        if viewport_height <= 0. {
            return;
        }
        let line_top = line.y;
        let line_bottom = line.y + line.style.line_height;
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
        if let Some(offset) = self.vertical_target(-1) {
            self.move_to(offset, window, cx);
            self.keep_offset_visible(offset);
        }
    }

    fn down(&mut self, _: &Down, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(offset) = self.vertical_target(1) {
            self.move_to(offset, window, cx);
            self.keep_offset_visible(offset);
        }
    }

    fn select_up(&mut self, _: &SelectUp, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(offset) = self.vertical_target(-1) {
            self.select_to(offset, window, cx);
            self.keep_offset_visible(offset);
        }
    }

    fn select_down(&mut self, _: &SelectDown, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(offset) = self.vertical_target(1) {
            self.select_to(offset, window, cx);
            self.keep_offset_visible(offset);
        }
    }

    fn select_all(&mut self, _: &SelectAll, window: &mut Window, cx: &mut Context<Self>) {
        self.set_selection(0..self.projection.text().len(), false, window, cx);
    }

    fn home(&mut self, _: &Home, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        let offset = visual_line_range_at(self, self.cursor_offset()).start;
        self.move_to(offset, window, cx);
    }

    fn end(&mut self, _: &End, window: &mut Window, cx: &mut Context<Self>) {
        self.preferred_x = None;
        let offset = visual_line_range_at(self, self.cursor_offset()).end;
        self.move_to(offset, window, cx);
    }

    fn backspace(&mut self, _: &Backspace, window: &mut Window, cx: &mut Context<Self>) {
        let (mut range, _) = self.selected_byte_range();
        if range.is_empty()
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
        if matches!(self.selection, Selection::Table(_)) {
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
                if text.contains('\t') && self.paste_tsv(&text, window, cx) {
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
                self.toolbar_visible = true;
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
                self.toolbar_visible = !self.selected_byte_range().0.is_empty();
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
            .link_at_offset(selection.start)
            .unwrap_or_else(|| "https://".into());
        self.link_input
            .update(cx, |input, cx| input.set_value(value, window, cx));
        self.link_input.read(cx).focus_handle(cx).focus(window, cx);
        self.link_popover_visible = true;
        self.toolbar_visible = false;
        cx.notify();
    }

    fn apply_link(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let value = self.link_input.read(cx).value().trim().to_owned();
        let target = (!value.is_empty()).then_some(value);
        match self.apply_command(EditCommand::SetLinkSelection { target }) {
            Ok(result) if !result.dirty_node_ids.is_empty() => {
                self.refresh_after_transaction(&result);
                self.link_popover_visible = false;
                self.toolbar_visible = true;
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
        self.toolbar_visible = false;
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
                self.toolbar_visible = true;
                self.last_error = None;
                self.focus_handle.focus(window, cx);
                cx.emit(EditorEvent::Changed);
                cx.notify();
            }
            Ok(_) => {
                self.image_popover_node = None;
                self.toolbar_visible = true;
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
        self.toolbar_visible = false;
        if append_row {
            cx.emit(EditorEvent::Changed);
        }
        cx.notify();
    }

    fn next_table_cell(&mut self, _: &NextTableCell, window: &mut Window, cx: &mut Context<Self>) {
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

    fn dismiss(&mut self, _: &Dismiss, _: &mut Window, cx: &mut Context<Self>) {
        self.context_menu_position = None;
        self.link_popover_visible = false;
        self.image_popover_node = None;
        self.toolbar_visible = false;
        cx.notify();
    }

    fn insert_block(&mut self, kind: InsertBlockKind, window: &mut Window, cx: &mut Context<Self>) {
        match self.apply_command(EditCommand::InsertBlockAfterSelection { kind }) {
            Ok(result) => {
                self.refresh_after_transaction(&result);
                self.context_menu_position = None;
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

    fn selection_toolbar_origin(&self) -> (f32, f32) {
        let (selection, _) = self.selected_byte_range();
        let Some(element) = self.element_bounds else {
            return (8., 0.);
        };
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

    fn on_mouse_down(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.focus_handle.focus(window, cx);
        self.context_menu_position = None;
        if let Some(drag) = self.table_resize_at(event.position) {
            self.table_resize_drag = Some(drag);
            self.toolbar_visible = false;
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
            self.toolbar_visible = true;
            cx.emit(EditorEvent::ViewChanged);
            cx.notify();
            cx.stop_propagation();
            return;
        }
        if let Some(item_id) = self.task_item_at_marker(event.position) {
            match self.apply_command(EditCommand::ToggleTask { item_id }) {
                Ok(result) => {
                    self.refresh_after_transaction(&result);
                    cx.emit(EditorEvent::Changed);
                    cx.notify();
                }
                Err(error) => self.record_error(error, window),
            }
            return;
        }
        let offset = self.index_for_mouse_position(event.position);
        if event.modifiers.control
            && let Some(target) = self.link_at_offset(offset)
        {
            cx.open_url(&target);
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
        if position.x < line.bounds.left() - px(26.) || position.x > line.bounds.left() {
            return None;
        }
        let segment = segment_for_line(&self.projection, &line.range)?;
        segment.context.task_checked?;
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
            let edge = line.bounds.right() + px(8.);
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
                initial_width: table.columns.get(column)?.width.unwrap_or(160.),
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
        let text = self.projection.block(position.node_id)?.text()?;
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
        cx.stop_propagation();
        self.focus_handle.focus(window, cx);
        self.move_to(self.index_for_mouse_position(event.position), window, cx);
        let origin = self
            .element_bounds
            .map_or(point(px(0.), px(0.)), |bounds| bounds.origin);
        self.context_menu_position = Some(point(
            event.position.x - origin.x,
            event.position.y - origin.y,
        ));
        self.toolbar_visible = false;
        cx.notify();
    }

    fn on_mouse_up(&mut self, event: &MouseUpEvent, window: &mut Window, cx: &mut Context<Self>) {
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
        self.toolbar_visible = matches!(self.selection, Selection::Table(_))
            || !self.selected_byte_range().0.is_empty()
            || self.current_image().is_some();
        cx.notify();
    }

    fn on_mouse_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.table_resize_drag.is_some() {
            return;
        }
        if self.is_selecting {
            if let Some(bounds) = self.element_bounds {
                let scroll_y = self.scroll_metrics().0;
                let next_scroll = if event.position.y < bounds.top() {
                    let distance: f32 = (bounds.top() - event.position.y).into();
                    Some((scroll_y - distance.min(48.)).max(0.))
                } else if event.position.y > bounds.bottom() {
                    let distance: f32 = (event.position.y - bounds.bottom()).into();
                    Some(scroll_y + distance.min(48.))
                } else {
                    None
                };
                if let Some(next_scroll) = next_scroll {
                    self.jump_generation = self.jump_generation.saturating_add(1);
                    let x = self.scroll_handle.offset().x;
                    self.scroll_handle.set_offset(point(x, px(-next_scroll)));
                    self.toolbar_visible = false;
                }
            }
            self.select_to(self.index_for_mouse_position(event.position), window, cx);
        }
    }

    fn on_scroll_wheel(
        &mut self,
        event: &ScrollWheelEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.jump_generation = self.jump_generation.saturating_add(1);
        self.toolbar_visible = false;
        self.link_popover_visible = false;
        let delta = event.delta.pixel_delta(px(LINE_HEIGHT));
        let delta_x: f32 = if event.modifiers.shift && delta.x == px(0.) {
            delta.y.into()
        } else {
            delta.x.into()
        };
        if delta_x.abs() < f32::EPSILON {
            cx.emit(EditorEvent::ViewChanged);
            return;
        }
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
        let next = clamped_horizontal_scroll(current, delta_x, viewport, content);
        if (next - current).abs() >= f32::EPSILON {
            self.horizontal_scrolls.insert(owner, next);
            cx.stop_propagation();
            cx.notify();
        }
    }
}

impl gpui::EventEmitter<EditorEvent> for RichDocumentEditor {}

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

    fn unmark_text(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        if self.document.composition_active()
            && let Ok(snapshot) = self.document.commit_composition()
        {
            self.selection = snapshot.selection().clone();
            cx.emit(EditorEvent::Changed);
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
            if let Err(error) = self
                .document
                .begin_composition(TextSelection { anchor, head })
            {
                self.record_error(error, window);
                return;
            }
        }

        let edited_node = self
            .projection
            .position_at(replacement.start, Affinity::Downstream)
            .map(|position| position.node_id);
        match self.document.update_composition(new_text.to_owned()) {
            Ok(snapshot) => {
                self.selection = snapshot.selection().clone();
                if edited_node.is_none_or(|node_id| !self.refresh_text_node(node_id)) {
                    self.refresh_projection();
                    self.shaped_line_cache.borrow_mut().clear();
                }
                self.marked_range = (!new_text.is_empty())
                    .then_some(replacement.start..replacement.start + new_text.len());
                let selected = new_selected_range_utf16.map_or_else(
                    || {
                        let end = replacement.start + new_text.len();
                        end..end
                    },
                    |range| {
                        let relative = utf16_range_in_text(new_text, range);
                        replacement.start + relative.start..replacement.start + relative.end
                    },
                );
                self.set_selection(selected, false, window, cx);
                cx.notify();
            }
            Err(error) => self.record_error(error, window),
        }
    }

    fn bounds_for_range(
        &mut self,
        range_utf16: Range<usize>,
        _: Bounds<Pixels>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) -> Option<Bounds<Pixels>> {
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
        self.projection
            .utf16_offset_for_byte(self.index_for_mouse_position(point))
    }

    fn set_selected_text_range(
        &mut self,
        range_utf16: Range<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.set_selection(
            self.projection.range_from_utf16(range_utf16),
            false,
            window,
            cx,
        );
    }

    fn text_length_utf16(&mut self, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        Some(self.projection.utf16_len())
    }
}

impl gpui::Render for RichDocumentEditor {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let palette = MineralPalette::for_dark(cx.theme().is_dark());
        let layout_width = self
            .element_bounds
            .map_or(self.layout_width, |bounds| f32::from(bounds.size.width));
        self.sync_image_dimensions(layout_width, cx);
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
                    TableBorder::Dotted => TableBorder::PhysicalPixel,
                    TableBorder::PhysicalPixel => TableBorder::None,
                    TableBorder::None => TableBorder::Dotted,
                };
                div()
                    .flex()
                    .gap(px(2.))
                    .child(
                        div()
                            .id("table-add-row")
                            .px(px(7.))
                            .py(px(4.))
                            .rounded(px(4.))
                            .hover(|button| button.bg(rgb(palette.surface)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_structural_command(
                                    EditCommand::InsertTableRow {
                                        table_id,
                                        index: row + 1,
                                    },
                                    window,
                                    cx,
                                );
                            }))
                            .child("+R"),
                    )
                    .child(
                        div()
                            .id("table-add-column")
                            .px(px(7.))
                            .py(px(4.))
                            .rounded(px(4.))
                            .hover(|button| button.bg(rgb(palette.surface)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_structural_command(
                                    EditCommand::InsertTableColumn {
                                        table_id,
                                        index: column + 1,
                                    },
                                    window,
                                    cx,
                                );
                            }))
                            .child("+C"),
                    )
                    .when(row_count > 1, |tools| {
                        tools.child(
                            div()
                                .id("table-delete-row")
                                .px(px(7.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::DeleteTableRow {
                                            table_id,
                                            index: row,
                                        },
                                        window,
                                        cx,
                                    );
                                }))
                                .child("−R"),
                        )
                    })
                    .when(column_count > 1, |tools| {
                        tools.child(
                            div()
                                .id("table-delete-column")
                                .px(px(7.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::DeleteTableColumn {
                                            table_id,
                                            index: column,
                                        },
                                        window,
                                        cx,
                                    );
                                }))
                                .child("−C"),
                        )
                    })
                    .when(row > 0, |tools| {
                        tools.child(
                            div()
                                .id("table-move-row-up")
                                .px(px(7.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::MoveTableRow {
                                            table_id,
                                            from: row,
                                            to: row - 1,
                                        },
                                        window,
                                        cx,
                                    );
                                }))
                                .child("R↑"),
                        )
                    })
                    .when(row + 1 < row_count, |tools| {
                        tools.child(
                            div()
                                .id("table-move-row-down")
                                .px(px(7.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::MoveTableRow {
                                            table_id,
                                            from: row,
                                            to: row + 1,
                                        },
                                        window,
                                        cx,
                                    );
                                }))
                                .child("R↓"),
                        )
                    })
                    .when(column > 0, |tools| {
                        tools.child(
                            div()
                                .id("table-move-column-left")
                                .px(px(7.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::MoveTableColumn {
                                            table_id,
                                            from: column,
                                            to: column - 1,
                                        },
                                        window,
                                        cx,
                                    );
                                }))
                                .child("C←"),
                        )
                    })
                    .when(column + 1 < column_count, |tools| {
                        tools.child(
                            div()
                                .id("table-move-column-right")
                                .px(px(7.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(move |this, _, window, cx| {
                                    this.apply_structural_command(
                                        EditCommand::MoveTableColumn {
                                            table_id,
                                            from: column,
                                            to: column + 1,
                                        },
                                        window,
                                        cx,
                                    );
                                }))
                                .child("C→"),
                        )
                    })
                    .child(
                        div()
                            .id("table-align-column")
                            .px(px(7.))
                            .py(px(4.))
                            .rounded(px(4.))
                            .hover(|button| button.bg(rgb(palette.surface)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_structural_command(
                                    EditCommand::SetTableColumnAlignment {
                                        table_id,
                                        column,
                                        alignment: next_alignment,
                                    },
                                    window,
                                    cx,
                                );
                            }))
                            .child("Align"),
                    )
                    .child(
                        div()
                            .id("table-border")
                            .px(px(7.))
                            .py(px(4.))
                            .rounded(px(4.))
                            .hover(|button| button.bg(rgb(palette.surface)))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.apply_structural_command(
                                    EditCommand::SetTableBorder {
                                        table_id,
                                        border: next_border,
                                    },
                                    window,
                                    cx,
                                );
                            }))
                            .child("Border"),
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
        let context_menu = self.context_menu_position.map(|position| {
            let options = [
                ("insert-paragraph", "Paragraph", InsertBlockKind::Paragraph),
                ("insert-heading", "Heading", InsertBlockKind::Heading(2)),
                ("insert-list", "List", InsertBlockKind::UnorderedList),
                ("insert-task-list", "Task list", InsertBlockKind::TaskList),
                ("insert-quote", "Quote", InsertBlockKind::BlockQuote),
                ("insert-code", "Code block", InsertBlockKind::CodeBlock),
                ("insert-image", "Image", InsertBlockKind::Image),
                ("insert-table", "Table", InsertBlockKind::Table),
                (
                    "insert-thematic-break",
                    "Thematic break",
                    InsertBlockKind::ThematicBreak,
                ),
            ];
            let paste_entity = cx.entity();
            let menu = div()
                .id("insertion-context-menu")
                .absolute()
                .left(position.x)
                .top(position.y)
                .w(px(180.))
                .p(px(4.))
                .rounded(px(8.))
                .bg(rgb(palette.floating))
                .text_size(px(13.))
                .on_mouse_down(MouseButton::Left, |_, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                })
                .child(
                    div()
                        .id("paste-as-markdown")
                        .w_full()
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(4.))
                        .hover(|item| item.bg(rgb(palette.surface)))
                        .on_click(move |_, window, cx| {
                            paste_entity.update(cx, |editor, cx| {
                                editor.paste_as_markdown(&PasteAsMarkdown, window, cx);
                                editor.context_menu_position = None;
                            });
                        })
                        .child("Paste as Markdown"),
                );
            options.into_iter().fold(menu, |menu, (id, label, kind)| {
                let entity = cx.entity();
                menu.child(
                    div()
                        .id(id)
                        .w_full()
                        .px(px(8.))
                        .py(px(6.))
                        .rounded(px(4.))
                        .hover(|item| item.bg(rgb(palette.surface)))
                        .on_click(move |_, window, cx| {
                            entity.update(cx, |editor, cx| {
                                editor.insert_block(kind, window, cx);
                            });
                        })
                        .child(label),
                )
            })
        });
        let scroll_y: f32 = (-self.scroll_handle.offset().y).into();
        let viewport_height: f32 = self.scroll_handle.bounds().size.height.into();
        let viewport_height = viewport_height.max(800.);
        let image_top = (scroll_y - viewport_height).max(0.);
        let image_bottom = scroll_y + viewport_height * 2.;
        let visible_order = visible_paint_order_range(
            &self.visual_lines,
            &self.paint_order,
            image_top,
            image_bottom,
        )
        .map(|order_index| self.paint_order[order_index])
        .collect::<Vec<_>>();
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
                        .absolute()
                        .top(px(line.y))
                        .left(relative(line.x_fraction))
                        .ml(px(-horizontal_offset))
                        .w(relative(line.width_fraction))
                        .h(px(line.style.line_height))
                        .px(px(8.))
                        .child(img(image_source).size_full().object_fit(ObjectFit::Contain)),
                )
            })
            .collect::<Vec<_>>();
        let semantic_order = visible_paint_order_range(
            &self.visual_lines,
            &self.paint_order,
            scroll_y,
            scroll_y + viewport_height,
        )
        .map(|order_index| self.paint_order[order_index])
        .collect::<Vec<_>>();
        let semantic_bounds = semantic_bounds_for_lines(
            &self.projection,
            &self.visual_lines,
            semantic_order.iter().copied(),
        );
        let mut semantic_roots = HashSet::new();
        let semantic_elements = semantic_order
            .iter()
            .filter_map(|index| {
                segment_for_line(&self.projection, &self.visual_lines[*index].range)
            })
            .filter(|segment| semantic_roots.insert(segment.top_level_node_id))
            .filter_map(|segment| self.projection.block(segment.top_level_node_id))
            .filter_map(|block| semantic_block(block, &semantic_bounds))
            .map(|node| {
                semantic_element(
                    node,
                    SemanticBounds {
                        x_fraction: 0.,
                        width_fraction: 1.,
                        y: 0.,
                        height: self.document_height,
                    },
                )
            })
            .collect::<Vec<_>>();
        div()
            .id("rich-document-editor")
            .role(Role::MultilineTextInput)
            .aria_label("Markdown document editor")
            .aria_description("Rendered rich Markdown; use arrow keys to move and typing to edit")
            .relative()
            .flex()
            .flex_col()
            .size_full()
            .overflow_x_hidden()
            .overflow_y_scroll()
            .track_scroll(&self.scroll_handle)
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::backspace))
            .on_action(cx.listener(Self::delete))
            .on_action(cx.listener(Self::left))
            .on_action(cx.listener(Self::right))
            .on_action(cx.listener(Self::up))
            .on_action(cx.listener(Self::down))
            .on_action(cx.listener(Self::select_left))
            .on_action(cx.listener(Self::select_right))
            .on_action(cx.listener(Self::select_up))
            .on_action(cx.listener(Self::select_down))
            .on_action(cx.listener(Self::select_all))
            .on_action(cx.listener(Self::home))
            .on_action(cx.listener(Self::end))
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
            .on_action(cx.listener(Self::enter))
            .on_action(cx.listener(Self::hard_break))
            .on_action(cx.listener(Self::next_table_cell))
            .on_action(cx.listener(Self::previous_table_cell))
            .on_action(cx.listener(Self::exit_table))
            .on_action(cx.listener(Self::dismiss))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_down(MouseButton::Right, cx.listener(Self::on_context_menu))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_scroll_wheel(cx.listener(Self::on_scroll_wheel))
            .child(DocumentTextElement {
                editor: cx.entity(),
            })
            .children(semantic_elements)
            .children(image_elements)
            .when(self.toolbar_visible, |editor| {
                editor.child(
                    div()
                        .id("selection-toolbar")
                        .absolute()
                        .top(px(toolbar_top))
                        .left(px(toolbar_left))
                        .flex()
                        .items_center()
                        .h(px(36.))
                        .px(px(4.))
                        .gap(px(2.))
                        .rounded(px(8.))
                        .bg(rgb(palette.floating))
                        .text_size(px(13.))
                        .on_mouse_down(MouseButton::Left, |_, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        })
                        .child(
                            div()
                                .id("format-paragraph")
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_block_style(BlockStyle::Paragraph, window, cx);
                                }))
                                .child("¶"),
                        )
                        .child(
                            div()
                                .id("format-heading")
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .font_weight(FontWeight::SEMIBOLD)
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_block_style(BlockStyle::Heading(2), window, cx);
                                }))
                                .child("H2"),
                        )
                        .child(
                            div()
                                .id("format-quote")
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_block_style(BlockStyle::BlockQuote, window, cx);
                                }))
                                .child("❝"),
                        )
                        .child(
                            div()
                                .id("format-bold")
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .font_weight(FontWeight::BOLD)
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_selection_format(InlineFormat::Bold, window, cx);
                                }))
                                .child("B"),
                        )
                        .child(
                            div()
                                .id("format-italic")
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .italic()
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_selection_format(InlineFormat::Italic, window, cx);
                                }))
                                .child("I"),
                        )
                        .child(
                            div()
                                .id("format-strike")
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .line_through()
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_selection_format(
                                        InlineFormat::Strikethrough,
                                        window,
                                        cx,
                                    );
                                }))
                                .child("S"),
                        )
                        .child(
                            div()
                                .id("format-code")
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .font_family("Spline Sans Mono Mineral")
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.apply_selection_format(InlineFormat::Code, window, cx);
                                }))
                                .child("</>"),
                        )
                        .child(
                            div()
                                .id("format-link")
                                .px(px(8.))
                                .py(px(4.))
                                .rounded(px(4.))
                                .text_color(rgb(palette.accent))
                                .hover(|button| button.bg(rgb(palette.surface)))
                                .on_click(cx.listener(|this, _, window, cx| {
                                    this.show_link_editor(window, cx);
                                }))
                                .child("Link"),
                        )
                        .children(table_tools)
                        .children(image_tools),
                )
            })
            .children(link_popover)
            .children(image_popover)
            .children(context_menu)
    }
}

struct DocumentTextElement {
    editor: Entity<RichDocumentEditor>,
}

struct TextPrepaintState {
    lines: Vec<PaintedLine>,
    decorations: Vec<PaintedLine>,
    chrome: Vec<MaskedQuad>,
    overlays: Vec<MaskedQuad>,
    caret: Option<MaskedQuad>,
    horizontal_metrics: HashMap<NodeId, (f32, f32)>,
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
        None
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
        let (selected, _) = editor.selected_byte_range();
        let snapshot = editor.document.snapshot();
        let table_selection = match &editor.selection {
            Selection::Table(selection) => Some(*selection),
            Selection::Text(_) => None,
        };
        let marked = editor.marked_range.clone();
        let text_style = window.text_style();
        let mut lines = Vec::new();
        let mut decorations = Vec::new();
        let mut chrome = Vec::new();
        let mut overlays = Vec::new();
        let mut caret = None;
        let mut horizontal_metrics = HashMap::<NodeId, (f32, f32)>::new();
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
        let visible_lines = visible_paint_order_range(
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
            let horizontal_owner =
                segment.and_then(|segment| horizontal_scroll_owner(&editor.projection, segment));
            let horizontal_offset = horizontal_owner
                .and_then(|owner| editor.horizontal_scrolls.get(&owner).copied())
                .unwrap_or(0.);
            let line_bounds = Bounds::new(
                point(
                    bounds.left() + px(width * spec.x_fraction + spec.inset - horizontal_offset),
                    bounds.top() + px(spec.y),
                ),
                size(
                    px((width * spec.width_fraction - spec.inset - 8.).max(1.)),
                    px(visual_style.line_height),
                ),
            );
            let horizontal_viewport = Bounds::new(
                point(bounds.left(), line_bounds.top()),
                size(bounds.size.width, line_bounds.size.height),
            );
            let content_mask = if spec.table_cell.is_some() {
                let Some(clipped) = intersect_bounds(line_bounds, horizontal_viewport) else {
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
                    point(bounds.left(), cell.top()),
                    size(bounds.size.width, cell.size.height),
                );
                intersect_bounds(cell, row_viewport).map(|bounds| ContentMask { bounds })
            });
            if line_bounds.bottom() < overscan_top || line_bounds.top() > overscan_bottom {
                continue;
            }
            if let (Some(owner), Some(segment)) = (horizontal_owner, segment)
                && segment.context.table_cell.is_some()
            {
                let content_width = width * (spec.x_fraction + spec.width_fraction);
                record_horizontal_metrics(&mut horizontal_metrics, owner, width, content_width);
            }
            if let Some(segment) = segment {
                if segment.context.quote_depth > 0 {
                    chrome.push(MaskedQuad {
                        quad: fill(line_bounds, rgb(palette.surface_quiet)),
                        content_mask,
                    });
                    chrome.push(MaskedQuad {
                        quad: fill(
                            Bounds::new(
                                point(line_bounds.left() - px(12.), line_bounds.top()),
                                size(px(2.), line_bounds.size.height),
                            ),
                            rgb(palette.border),
                        ),
                        content_mask,
                    });
                }
                if segment.context.table_cell.is_some()
                    && spec.table_cell_first
                    && let (Some(cell_bounds), Some(cell_mask)) =
                        (table_cell_bounds, table_content_mask)
                {
                    if segment.context.table_header {
                        chrome.push(MaskedQuad {
                            quad: fill(cell_bounds, rgb(palette.surface_quiet)),
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
                    let marker = match segment.context.task_checked {
                        Some(true) => "☑",
                        Some(false) => "☐",
                        None => marker,
                    };
                    let font = text_style.font();
                    let marker_layout = window.text_system().shape_line(
                        marker.to_owned().into(),
                        px(15.),
                        &[TextRun {
                            len: marker.len(),
                            font,
                            color: rgb(palette.secondary).into(),
                            background_color: None,
                            underline: None,
                            strikethrough: None,
                        }],
                        None,
                    );
                    decorations.push(PaintedLine {
                        range: range.start..range.start,
                        layout: marker_layout,
                        bounds: Bounds::new(
                            point(line_bounds.left() - px(22.), line_bounds.top()),
                            size(px(20.), line_bounds.size.height),
                        ),
                        line_height: px(visual_style.line_height),
                        horizontal_owner,
                        content_mask,
                        alignment: ColumnAlignment::None,
                    });
                }
            }
            if matches!(
                segment_and_block(editor, &range),
                Some((_, BlockNode::CodeBlock(_)))
            ) {
                chrome.push(MaskedQuad {
                    quad: fill(
                        content_mask.map_or(line_bounds, |mask| mask.bounds),
                        rgb(palette.surface),
                    ),
                    content_mask,
                });
            }
            let line_text = if text.is_empty() {
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
            let alignment = table_column_alignment(&editor.projection, segment);
            let text_left = aligned_text_left(line_bounds, &layout, alignment);
            if let Some(owner) = horizontal_owner
                && segment.is_some_and(|segment| segment.context.table_cell.is_none())
            {
                record_horizontal_metrics(
                    &mut horizontal_metrics,
                    owner,
                    width,
                    f32::from(layout.width()) + spec.inset + 8.,
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
            lines,
            decorations,
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
                    gpui::TextAlign::Left,
                    None,
                    window,
                    cx,
                );
            });
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
            editor.element_bounds = Some(bounds);
            if !editor.has_painted {
                editor.has_painted = true;
                cx.emit(EditorEvent::Ready);
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
) {
    match border {
        TableBorder::None => {}
        TableBorder::PhysicalPixel => {
            output.push(MaskedQuad {
                quad: outline(bounds, rgb(border_color), BorderStyle::Solid),
                content_mask,
            });
        }
        TableBorder::Dotted => {
            // GPUI's dashed border shader produces a dotted appearance at this
            // one-pixel width without allocating one quad per individual dot.
            output.push(MaskedQuad {
                quad: outline(bounds, rgb(border_color), BorderStyle::Dashed),
                content_mask,
            });
        }
    }
}

fn build_visual_lines(document: &Document, projection: &TextProjection) -> Vec<VisualLineSpec> {
    let _ = document;
    build_visual_lines_with_images(projection, &HashMap::new(), 760.)
}

fn capture_scroll_anchor(
    snapshot: &document_core::DocumentSnapshot,
    projection: &TextProjection,
    lines: &[VisualLineSpec],
    scroll_y: f32,
) -> Option<EditorScrollAnchor> {
    let line = lines
        .iter()
        .rev()
        .find(|line| line.y <= scroll_y)
        .or_else(|| lines.first())?;
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
        intra_line_offset: scroll_y - line.y,
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

    let y_before = first.y - first.style.space_above;
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
    let mut replacement =
        build_visual_lines_for_segment(projection, &segment, image_dimensions, layout_width);
    let new_y_after = position_visual_lines(&mut replacement, projection, layout_width, y_before);
    let y_delta = new_y_after - old_y_after;
    let replacement_len = replacement.len();

    for line in &mut visual_lines[after_lines..] {
        line.range.start = line.range.start.checked_add_signed(byte_delta)?;
        line.range.end = line.range.end.checked_add_signed(byte_delta)?;
        line.y += y_delta;
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
    *document_height = (*document_height + y_delta).max(LINE_HEIGHT);
    Some(VisualTextRefresh {
        old_y_after,
        y_delta,
    })
}

fn build_visual_lines_with_images(
    projection: &TextProjection,
    image_dimensions: &NodeImageDimensions,
    layout_width: f32,
) -> Vec<VisualLineSpec> {
    let mut lines = Vec::with_capacity(projection.segments().len());
    for segment in projection.segments() {
        lines.extend(build_visual_lines_for_segment(
            projection,
            segment,
            image_dimensions,
            layout_width,
        ));
    }
    position_visual_lines(&mut lines, projection, layout_width, 0.);
    lines
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
) -> Vec<VisualLineSpec> {
    let Some(block) = projection.block(segment.node_id) else {
        return Vec::new();
    };
    let text = projection.text();
    let columns = estimated_wrap_columns(block, segment, projection, layout_width);
    let segment_text = &text[segment.projection_range.clone()];
    let mut lines = Vec::new();
    let image_height =
        image_reserved_height(projection, block, segment, image_dimensions, layout_width);
    let table_cell = segment
        .context
        .table_cell
        .and_then(|(table_id, row, column)| {
            let columns = match projection.block(table_id)? {
                BlockNode::Table(table) => table.columns.len().max(1),
                _ => return None,
            };
            Some((table_id, row, column, columns))
        });
    for_each_display_line_range(segment_text, |local_line| {
        let logical_line = segment.projection_range.start + local_line.start
            ..segment.projection_range.start + local_line.end;
        for_each_wrap_line_range(text, logical_line, columns, |range| {
            let style = visual_line_style_for(block, segment, &range, image_height);
            let inset = segment.context.list_depth as f32 * 24.
                + segment.context.quote_depth as f32 * 24.
                + f32::from(segment.context.table_cell.is_some()) * 12.;
            lines.push(VisualLineSpec {
                range,
                style,
                inset,
                y: 0.,
                x_fraction: 0.,
                width_fraction: 1.,
                table_cell,
                table_cell_first: false,
                table_row_y: 0.,
                table_row_height: 0.,
            });
        });
    });
    lines
}

fn estimated_wrap_columns(
    block: &BlockNode,
    segment: &crate::ProjectionSegment,
    projection: &TextProjection,
    layout_width: f32,
) -> usize {
    let reference_columns = match block {
        BlockNode::Heading(heading) => match heading.level {
            1 => 28,
            2 => 38,
            _ => 48,
        },
        BlockNode::CodeBlock(_) => return usize::MAX,
        _ => 68,
    };
    let available_width = segment
        .context
        .table_cell
        .and_then(|(table_id, _, column)| {
            let BlockNode::Table(table) = projection.block(table_id)? else {
                return None;
            };
            let total_width = table
                .columns
                .iter()
                .map(|column| column.width.unwrap_or(160.).max(32.))
                .sum::<f32>()
                .max(1.);
            let table_scale = (layout_width.max(1.) / total_width).max(1.);
            table
                .columns
                .get(column)
                .map(|column| column.width.unwrap_or(160.).max(32.) * table_scale)
        })
        .unwrap_or(layout_width);
    let inset = segment.context.list_depth as f32 * 24.
        + segment.context.quote_depth as f32 * 24.
        + f32::from(segment.context.table_cell.is_some()) * 12.
        + 8.;
    ((reference_columns as f32 * (available_width - inset).max(1.) / 760.)
        .floor()
        .max(1.)) as usize
}

fn position_visual_lines(
    lines: &mut [VisualLineSpec],
    projection: &TextProjection,
    layout_width: f32,
    mut y: f32,
) -> f32 {
    let mut index = 0;
    while index < lines.len() {
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
        let table = match projection.block(table_id) {
            Some(BlockNode::Table(table)) => Some(table),
            _ => None,
        };
        let total_width = table.map_or(160. * columns as f32, |table| {
            table
                .columns
                .iter()
                .map(|column| column.width.unwrap_or(160.).max(32.))
                .sum()
        });
        let table_scale = (layout_width.max(1.) / total_width).max(1.);
        let mut column_heights = vec![0.; columns];
        for line in &mut lines[index..end] {
            let Some((_, _, column, columns)) = line.table_cell else {
                continue;
            };
            let column = column.min(columns.saturating_sub(1));
            column_heights[column] += line.style.space_above;
            line.y = y + column_heights[column];
            let column_width = table
                .and_then(|table| table.columns.get(column))
                .map_or(160., |column| column.width.unwrap_or(160.).max(32.));
            let preceding_width = table.map_or(160. * column as f32, |table| {
                table
                    .columns
                    .iter()
                    .take(column)
                    .map(|column| column.width.unwrap_or(160.).max(32.))
                    .sum()
            });
            line.x_fraction = preceding_width * table_scale / layout_width.max(1.);
            line.width_fraction = column_width * table_scale / layout_width.max(1.);
            column_heights[column] += line.style.line_height + line.style.space_below;
        }
        let row_height = column_heights.into_iter().fold(44., f32::max);
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
        y += row_height;
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
        let end = preferred
            .filter(|boundary| *boundary > start)
            .unwrap_or(candidate);
        visit(start..end);
        start = end;
    }
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
        Some(BlockNode::CodeBlock(_))
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
    block: &BlockNode,
    segment: &crate::ProjectionSegment,
    line: &Range<usize>,
    image_height: Option<f32>,
) -> VisualLineStyle {
    let first_line = line.start == segment.projection_range.start;
    let last_line = line.end == segment.projection_range.end;
    if segment.context.image_source.is_some() {
        return VisualLineStyle {
            font_size: 15.,
            line_height: image_height.unwrap_or(180.),
            space_above: if first_line { 8. } else { 0. },
            space_below: if last_line { 16. } else { 0. },
        };
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
            _ => (18., LINE_HEIGHT),
        };
        return VisualLineStyle {
            font_size,
            line_height,
            space_above: if first_line { 8. } else { 0. },
            space_below: if last_line { 8. } else { 0. },
        };
    }
    match block {
        BlockNode::Heading(heading) => {
            let (font_size, line_height) = match heading.level {
                1 => (42., 42.84),
                2 => (32., 35.84),
                3 => (26., 31.2),
                4 => (22., 26.4),
                5 => (19., 22.8),
                _ => (17., 20.4),
            };
            VisualLineStyle {
                font_size,
                line_height,
                space_above: if first_line { 32. } else { 0. },
                space_below: if last_line { 12. } else { 0. },
            }
        }
        BlockNode::CodeBlock(_) => VisualLineStyle {
            font_size: 15.,
            line_height: 22.5,
            space_above: if first_line { 8. } else { 0. },
            space_below: if last_line { 16. } else { 0. },
        },
        _ => VisualLineStyle {
            space_below: if last_line { 16. } else { 0. },
            ..VisualLineStyle::BODY
        },
    }
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
    let runtime = dimensions
        .get(&image.id)
        .filter(|(source, _)| source == &image.source)
        .map(|(_, dimensions)| *dimensions);
    let (width, height) = runtime.or(image.intrinsic_size)?;
    let width_fraction = segment
        .context
        .table_cell
        .and_then(|(table_id, _, column)| {
            let BlockNode::Table(table) = projection.block(table_id)? else {
                return None;
            };
            let total = table
                .columns
                .iter()
                .map(|column| column.width.unwrap_or(160.).max(32.))
                .sum::<f32>()
                .max(1.);
            table
                .columns
                .get(column)
                .map(|column| column.width.unwrap_or(160.).max(32.) / total)
        })
        .unwrap_or(1.);
    let available = (layout_width * width_fraction - 16.).max(1.);
    Some(available.min(width.max(1) as f32) * height as f32 / width.max(1) as f32)
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

fn segment_and_block<'a>(
    editor: &'a RichDocumentEditor,
    line: &Range<usize>,
) -> Option<(&'a crate::ProjectionSegment, &'a BlockNode)> {
    let segment = segment_for_line(&editor.projection, line)?;
    let block = editor.projection.block(segment.node_id)?;
    Some((segment, block))
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
                    Some(SemanticNodeSpec {
                        node_id: item.id,
                        role: Role::ListItem,
                        label: sequence_plain_text(&item.blocks),
                        level: None,
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
                        .filter_map(|cell| {
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
                                level: None,
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
                        level: None,
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
        BlockNode::CodeBlock(code) => (Role::Code, code.content.as_string(), None, Vec::new()),
        BlockNode::Image(image) => (Role::Image, image.alt.as_string(), None, Vec::new()),
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
        level,
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

fn semantic_element(node: SemanticNodeSpec, parent: SemanticBounds) -> AnyElement {
    let parent_width = parent.width_fraction.max(f32::EPSILON);
    let left = ((node.bounds.x_fraction - parent.x_fraction) / parent_width).clamp(0., 1.);
    let width = (node.bounds.width_fraction / parent_width).clamp(0., 1. - left);
    let children = node
        .children
        .into_iter()
        .map(|child| semantic_element(child, node.bounds))
        .collect::<Vec<_>>();
    div()
        .id(("semantic-document-node", node.node_id.get() as usize))
        .absolute()
        .top(px((node.bounds.y - parent.y).max(0.)))
        .left(relative(left))
        .w(relative(width.max(f32::EPSILON)))
        .h(px(node.bounds.height.max(1.)))
        .role(node.role)
        .aria_label(node.label)
        .when_some(node.level, |element, level| element.aria_level(level))
        .children(children)
        .into_any_element()
}

fn styled_runs(
    editor: &RichDocumentEditor,
    line: &Range<usize>,
    line_len: usize,
    text_style: &gpui::TextStyle,
    placeholder: bool,
    palette: MineralPalette,
) -> Vec<TextRun> {
    let mut base_font = text_style.font();
    let mut base_color = rgb(palette.text).into();
    let mut base_background = None;
    let Some((segment, block)) = segment_and_block(editor, line) else {
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
        }
        BlockNode::CodeBlock(_) => {
            base_font.family = "Spline Sans Mono Mineral".into();
            base_background = Some(rgb(palette.surface).into());
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
                InlineStyle::Code => {
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

#[cfg(test)]
mod tests {
    use super::*;

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
                range: index..index + 1,
                style: VisualLineStyle::BODY,
                inset: 0.,
                y: index as f32 * LINE_HEIGHT,
                x_fraction: 0.,
                width_fraction: 1.,
                table_cell: None,
                table_cell_first: false,
                table_row_y: 0.,
                table_row_height: 0.,
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
}
