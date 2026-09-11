use std::{
    ops::Range,
    sync::{
        Arc, OnceLock, RwLock,
        atomic::{AtomicIsize, AtomicUsize, Ordering},
    },
};

use document_core::{
    Affinity, AlertKind, BlockNode, BlockSequence, DocumentPosition, DocumentSnapshot, ListKind,
    NodeId, RichText, Selection, TableBorder,
};
use rustc_hash::FxHashMap;

const PROJECTION_CHUNK_SEGMENTS: usize = 256;
const PROJECTION_CHUNK_BYTES: usize = 64 * 1024;

/// Native document-unit constraints, never persisted into Markdown.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TableMeasurements {
    pub minimum: Vec<f32>,
    pub preferred: Vec<f32>,
    /// Local loaded-font prose boundary, in document units.
    pub reading_width: f32,
    /// Complete native header widths for an unambiguous, flat record table.
    /// None means that stacked records have not been proven semantically safe.
    pub record_headers: Option<Vec<f32>>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct RecordLayout {
    pub columns: usize,
    pub width: f32,
}

impl RecordLayout {
    pub const GAP: f32 = 16.;
}

/// Transient presentation constraints for the focused table. Never copied to
/// content, export or preference storage; font/width changes and blur release it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TableLayoutLock {
    id: NodeId,
    widths: Vec<f32>,
    measured: Option<TableMeasurements>,
}

impl TableMeasurements {
    fn available_width(&self, canvas: f32) -> f32 {
        let preferred = self.preferred.iter().sum::<f32>();
        // A modest adjustment makes near-width tables share the prose edge.
        // Compact tables and genuinely wide comparisons keep their own measure.
        if (0.8 * self.reading_width..=1.2 * self.reading_width).contains(&preferred) {
            canvas.min(self.reading_width)
        } else {
            canvas.min(preferred)
        }
    }

    pub fn fit(&self, available: f32) -> Vec<f32> {
        let available = self.available_width(available);
        let minimum = self.minimum.iter().sum::<f32>();
        let preferred = self.preferred.iter().sum::<f32>();
        if available <= minimum {
            return self.minimum.clone();
        }
        if available >= preferred {
            return self
                .preferred
                .iter()
                .map(|width| width * available / preferred)
                .collect();
        }
        let fraction = (available - minimum) / (preferred - minimum).max(f32::EPSILON);
        self.minimum
            .iter()
            .zip(&self.preferred)
            .map(|(low, high)| low + (high - low) * fraction)
            .collect()
    }
}

fn fit_columns(weights: &[f32], available: f32) -> Vec<f32> {
    let minimum = 96.;
    let remaining = (available - minimum * weights.len() as f32).max(0.);
    let flexible = weights.iter().map(|w| (w - minimum).max(1.)).sum::<f32>();
    weights
        .iter()
        .map(|w| minimum + remaining * (w - minimum).max(1.) / flexible.max(1.))
        .collect()
}

#[test]
fn near_width_tables_share_the_reading_edge_without_squeezing_minima() {
    for natural in [520., 610., 700., 760.] {
        let measured = TableMeasurements {
            minimum: vec![100., 160.],
            preferred: vec![natural * 0.3, natural * 0.7],
            reading_width: 640.,
            record_headers: None,
        };
        for canvas in [400., 640., 1000.] {
            let fitted = measured.fit(canvas);
            assert!((fitted.iter().sum::<f32>() - canvas.min(640.)).abs() < 0.01);
            assert!(fitted[0] >= 100. && fitted[1] >= 160.);
        }
        assert_eq!(measured.fit(200.), measured.minimum);
    }
    for natural in [300., 510., 770., 1200.] {
        let measured = TableMeasurements {
            minimum: vec![100., 160.],
            preferred: vec![natural * 0.4, natural * 0.6],
            reading_width: 640.,
            record_headers: None,
        };
        assert!((measured.fit(1600.).iter().sum::<f32>() - natural).abs() < 0.01);
    }
    let unbreakable = TableMeasurements {
        minimum: vec![350., 350.],
        preferred: vec![360., 360.],
        reading_width: 640.,
        record_headers: None,
    };
    assert_eq!(unbreakable.fit(1000.), unbreakable.minimum);
}

#[test]
fn automatic_columns_fit_without_squeezing_short_labels() {
    for width in [384., 640., 1100., 1600.] {
        let fitted = fit_columns(&[96., 400., 400., 200.], width);
        assert!((fitted.iter().sum::<f32>() - width).abs() < 0.01);
        assert!(fitted.iter().all(|width| *width >= 96.));
        assert!(fitted[1] >= fitted[3] && fitted[3] >= fitted[0]);
    }
    assert_eq!(fit_columns(&[400., 400., 400.], 200.), vec![96.; 3]);
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectionContext {
    pub(crate) metric: Option<crate::metrics::TextRole>,
    pub(crate) color_role: Option<crate::signals::ColorRole>,
    pub(crate) color_rgba: Option<u32>,
    pub(crate) badge: Option<crate::signals::Badge>,
    /// Explicit authored caption/credit attached to a canonical image. Never
    /// inferred from alt text; the paragraph remains the editable source.
    pub figure_text: Option<(NodeId, crate::FigureTextRole)>,
    /// Section-local reading mode, derived on projection rebuild. Never source.
    pub narrative: bool,
    /// Authored bibliography section owning this citation paragraph/list leaf.
    pub bibliography: Option<NodeId>,
    /// Authored resource-title boundary, prepared once rather than recognized
    /// while shaping/painting each visible run. None for ordinary linked prose.
    pub resource_title_end: Option<usize>,
    /// Authored document-property context. Metadata uses reference sans labels,
    /// not the serif lead-ins of an ordinary editorial feature list.
    pub metadata: bool,
    /// Nearest authored definition group; descendants keep their own semantic
    /// identity. Only the definition term's direct prose receives label styling.
    pub definition: Option<(NodeId, document_core::DefinitionKind)>,
    /// Canonical note owner; its marker/return action never enters source text.
    pub footnote: Option<NodeId>,
    pub footnote_first: bool,
    pub list_depth: usize,
    pub ordered_list_depth: usize,
    /// Task-item ancestry remains after the first paragraph's marker is cleared.
    pub task_list_depth: usize,
    /// Canonical list owners, outermost first. One shared allocation per list,
    /// not per line/item; used to paint hierarchy without scanning the source.
    pub list_ancestors: Arc<[NodeId]>,
    /// Complete labeled-outline footprint, derived once at its root, including
    /// enclosing quote/callout padding in the width-pressure estimate.
    pub(crate) outline_metrics: Option<(usize, usize)>,
    pub(crate) list_item_label: Option<NodeId>,
    /// First authored container, when the item begins with one. Its projected
    /// text supplies context; its outer bounds own the item's marker/caption.
    pub(crate) list_item_container: Option<NodeId>,
    pub(crate) list_parent_label: Option<NodeId>,
    pub(crate) list_branch_start: bool,
    /// Layout-local override on a segment clone, never canonical state.
    pub(crate) compact_outline: bool,
    pub list_marker: Option<String>,
    pub task_checked: Option<bool>,
    pub quote_depth: usize,
    pub quote: Option<NodeId>,
    pub quote_ancestors: Arc<[NodeId]>,
    pub quote_pull: bool,
    pub quote_attribution: bool,
    pub quote_before_attribution: bool,
    /// Explicit source-adjacent margin-note relationship; never a visual offset.
    pub margin_note_anchor: Option<NodeId>,
    pub quote_first: bool,
    pub quote_last: bool,
    pub alert: Option<(NodeId, AlertKind)>,
    pub alert_first: bool,
    pub alert_last: bool,
    pub table_cell: Option<(NodeId, usize, usize)>,
    pub table_header: bool,
    pub(crate) table_property_key: bool,
    pub table_border: Option<TableBorder>,
    pub image_source: Option<String>,
    pub preserved_source: bool,
}

#[derive(Debug, PartialEq, Eq)]
pub struct ProjectionSegment {
    pub node_id: NodeId,
    pub top_level_node_id: NodeId,
    pub(crate) projection_start: Arc<ProjectionOffset>,
    projection_local_start: u32,
    projection_len: u32,
    utf16_local_start: u32,
    utf16_len: u32,
    pub node_range: Range<usize>,
    // Keep the source-order range index compact. Local typing rebases every
    // following range; walking a cache line of coordinates is substantially
    // cheaper than dragging the large, mostly immutable presentation context
    // through cache for each segment.
    pub context: Box<ProjectionContext>,
}

impl Clone for ProjectionSegment {
    fn clone(&self) -> Self {
        Self {
            node_id: self.node_id,
            top_level_node_id: self.top_level_node_id,
            projection_start: self.projection_start.clone(),
            projection_local_start: self.projection_local_start,
            projection_len: self.projection_len,
            utf16_local_start: self.utf16_local_start,
            utf16_len: self.utf16_len,
            node_range: self.node_range.clone(),
            context: self.context.clone(),
        }
    }
}

#[derive(Debug)]
struct DeltaNode {
    index: usize,
    byte_delta: isize,
    utf16_delta: isize,
    byte_sum: isize,
    utf16_sum: isize,
    height: u8,
    left: Option<Box<DeltaNode>>,
    right: Option<Box<DeltaNode>>,
}

impl DeltaNode {
    fn new(index: usize, byte_delta: isize, utf16_delta: isize) -> Box<Self> {
        Box::new(Self {
            index,
            byte_delta,
            utf16_delta,
            byte_sum: byte_delta,
            utf16_sum: utf16_delta,
            height: 1,
            left: None,
            right: None,
        })
    }

    fn height(node: &Option<Box<Self>>) -> u8 {
        node.as_ref().map_or(0, |node| node.height)
    }

    fn sums(node: &Option<Box<Self>>) -> (isize, isize) {
        node.as_ref()
            .map_or((0, 0), |node| (node.byte_sum, node.utf16_sum))
    }

    fn refresh(&mut self) {
        let (left_byte, left_utf16) = Self::sums(&self.left);
        let (right_byte, right_utf16) = Self::sums(&self.right);
        self.byte_sum = left_byte + self.byte_delta + right_byte;
        self.utf16_sum = left_utf16 + self.utf16_delta + right_utf16;
        self.height = 1 + Self::height(&self.left).max(Self::height(&self.right));
    }

    fn rotate_left(mut root: Box<Self>) -> Box<Self> {
        let mut next = root.right.take().expect("left rotation has a right child");
        root.right = next.left.take();
        root.refresh();
        next.left = Some(root);
        next.refresh();
        next
    }

    fn rotate_right(mut root: Box<Self>) -> Box<Self> {
        let mut next = root.left.take().expect("right rotation has a left child");
        root.left = next.right.take();
        root.refresh();
        next.right = Some(root);
        next.refresh();
        next
    }

    fn balance(mut root: Box<Self>) -> Box<Self> {
        let balance = i16::from(Self::height(&root.left)) - i16::from(Self::height(&root.right));
        if balance > 1 {
            if root
                .left
                .as_ref()
                .is_some_and(|left| Self::height(&left.right) > Self::height(&left.left))
            {
                root.left = root.left.take().map(Self::rotate_left);
            }
            return Self::rotate_right(root);
        }
        if balance < -1 {
            if root
                .right
                .as_ref()
                .is_some_and(|right| Self::height(&right.left) > Self::height(&right.right))
            {
                root.right = root.right.take().map(Self::rotate_right);
            }
            return Self::rotate_left(root);
        }
        root.refresh();
        root
    }

    fn add(
        root: Option<Box<Self>>,
        index: usize,
        byte_delta: isize,
        utf16_delta: isize,
        visits: &mut usize,
    ) -> Box<Self> {
        let Some(mut root) = root else {
            return Self::new(index, byte_delta, utf16_delta);
        };
        *visits += 1;
        match index.cmp(&root.index) {
            std::cmp::Ordering::Less => {
                root.left = Some(Self::add(
                    root.left.take(),
                    index,
                    byte_delta,
                    utf16_delta,
                    visits,
                ));
            }
            std::cmp::Ordering::Greater => {
                root.right = Some(Self::add(
                    root.right.take(),
                    index,
                    byte_delta,
                    utf16_delta,
                    visits,
                ));
            }
            std::cmp::Ordering::Equal => {
                root.byte_delta += byte_delta;
                root.utf16_delta += utf16_delta;
            }
        }
        Self::balance(root)
    }

    fn prefix(root: &Option<Box<Self>>, index: usize) -> (isize, isize) {
        let Some(root) = root else {
            return (0, 0);
        };
        if index < root.index {
            return Self::prefix(&root.left, index);
        }
        let (left_byte, left_utf16) = Self::sums(&root.left);
        let (right_byte, right_utf16) = Self::prefix(&root.right, index);
        (
            left_byte + root.byte_delta + right_byte,
            left_utf16 + root.utf16_delta + right_utf16,
        )
    }
}

#[derive(Debug)]
struct OffsetDeltas {
    root: RwLock<Option<Box<DeltaNode>>>,
    version: AtomicUsize,
    first_changed: AtomicUsize,
    cache: OnceLock<OffsetCache>,
    #[cfg(test)]
    last_update_visits: AtomicUsize,
}

impl Default for OffsetDeltas {
    fn default() -> Self {
        Self {
            root: RwLock::new(None),
            version: AtomicUsize::new(0),
            first_changed: AtomicUsize::new(usize::MAX),
            cache: OnceLock::new(),
            #[cfg(test)]
            last_update_visits: AtomicUsize::new(0),
        }
    }
}

impl OffsetDeltas {
    fn seal(&self, len: usize) {
        self.cache
            .set(OffsetCache::new(len))
            .expect("projection coordinates are sealed once");
    }

    fn add_suffix(&self, start: usize, byte_delta: isize, utf16_delta: isize) {
        if byte_delta == 0 && utf16_delta == 0 {
            return;
        }
        let mut visits = 0;
        let mut root = self
            .root
            .write()
            .expect("projection coordinate lock is not poisoned");
        *root = Some(DeltaNode::add(
            root.take(),
            start,
            byte_delta,
            utf16_delta,
            &mut visits,
        ));
        self.first_changed.fetch_min(start, Ordering::Release);
        self.version.fetch_add(1, Ordering::Release);
        #[cfg(test)]
        self.last_update_visits.store(visits, Ordering::Relaxed);
    }

    fn deltas_at(&self, index: usize) -> (isize, isize) {
        let root = self
            .root
            .read()
            .expect("projection coordinate lock is not poisoned");
        DeltaNode::prefix(&root, index)
    }
}

#[derive(Debug)]
struct OffsetCache {
    versions: Box<[AtomicUsize]>,
    byte_deltas: Box<[AtomicIsize]>,
    utf16_deltas: Box<[AtomicIsize]>,
}

impl OffsetCache {
    fn new(len: usize) -> Self {
        Self {
            versions: (0..len).map(|_| AtomicUsize::new(0)).collect(),
            byte_deltas: (0..len).map(|_| AtomicIsize::new(0)).collect(),
            utf16_deltas: (0..len).map(|_| AtomicIsize::new(0)).collect(),
        }
    }
}

#[derive(Debug)]
pub(crate) struct ProjectionOffset {
    byte_base: usize,
    utf16_base: usize,
    index: usize,
    deltas: Option<Arc<OffsetDeltas>>,
}

impl ProjectionOffset {
    pub(crate) fn new(value: usize) -> Self {
        Self::new_pair(value, 0)
    }

    fn new_pair(byte: usize, utf16: usize) -> Self {
        Self {
            byte_base: byte,
            utf16_base: utf16,
            index: 0,
            deltas: None,
        }
    }

    fn indexed(
        byte_base: usize,
        utf16_base: usize,
        index: usize,
        deltas: Arc<OffsetDeltas>,
    ) -> Self {
        Self {
            byte_base,
            utf16_base,
            index,
            deltas: Some(deltas),
        }
    }

    #[inline(always)]
    fn resolved_deltas(&self) -> (isize, isize) {
        let Some(deltas) = &self.deltas else {
            return (0, 0);
        };
        let version = deltas.version.load(Ordering::Acquire);
        if version == 0 || self.index < deltas.first_changed.load(Ordering::Acquire) {
            return (0, 0);
        }
        let cache = deltas
            .cache
            .get()
            .expect("projection coordinates are sealed before use");
        if cache.versions[self.index].load(Ordering::Acquire) == version {
            return (
                cache.byte_deltas[self.index].load(Ordering::Relaxed),
                cache.utf16_deltas[self.index].load(Ordering::Relaxed),
            );
        }
        let (byte, utf16) = deltas.deltas_at(self.index);
        cache.byte_deltas[self.index].store(byte, Ordering::Relaxed);
        cache.utf16_deltas[self.index].store(utf16, Ordering::Relaxed);
        cache.versions[self.index].store(version, Ordering::Release);
        (byte, utf16)
    }

    #[inline(always)]
    pub(crate) fn get(&self) -> usize {
        self.byte_base
            .checked_add_signed(self.resolved_deltas().0)
            .expect("projection offset remains in usize range")
    }

    #[inline(always)]
    fn get_utf16(&self) -> usize {
        self.utf16_base
            .checked_add_signed(self.resolved_deltas().1)
            .expect("UTF-16 projection offset remains in usize range")
    }

    fn add_suffix(&self, start: usize, byte_delta: isize, utf16_delta: isize) -> Option<()> {
        let deltas = self.deltas.as_ref()?;
        deltas.add_suffix(start, byte_delta, utf16_delta);
        Some(())
    }

    #[cfg(test)]
    fn update_count(&self) -> usize {
        self.deltas.as_ref().map_or(0, |deltas| {
            deltas.last_update_visits.load(Ordering::Relaxed)
        })
    }
}

impl PartialEq for ProjectionOffset {
    fn eq(&self, other: &Self) -> bool {
        self.get() == other.get() && self.get_utf16() == other.get_utf16()
    }
}

impl Eq for ProjectionOffset {}

impl ProjectionSegment {
    #[must_use]
    #[inline(always)]
    pub fn projection_start(&self) -> usize {
        self.projection_start.get() + self.projection_local_start as usize
    }

    #[must_use]
    #[inline(always)]
    pub fn projection_end(&self) -> usize {
        self.projection_start() + self.projection_len as usize
    }

    #[must_use]
    #[inline(always)]
    pub fn projection_len(&self) -> usize {
        self.projection_len as usize
    }

    #[must_use]
    #[inline(always)]
    pub fn projection_range(&self) -> Range<usize> {
        let start = self.projection_start();
        start..start + self.projection_len as usize
    }

    #[inline(always)]
    pub(crate) fn projection_local_start(&self) -> usize {
        self.projection_local_start as usize
    }

    pub(crate) fn set_projection_range(&mut self, range: Range<usize>) {
        self.projection_len = u32::try_from(range.len()).expect("projection fragment fits u32");
        self.projection_start = Arc::new(ProjectionOffset::new(range.start));
        self.projection_local_start = 0;
    }

    fn utf16_start(&self) -> usize {
        self.projection_start.get_utf16() + self.utf16_local_start as usize
    }

    fn utf16_end(&self) -> usize {
        self.utf16_start() + self.utf16_len as usize
    }

    fn utf16_range(&self) -> Range<usize> {
        let start = self.utf16_start();
        start..start + self.utf16_len as usize
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct ContainerEdges {
    pub starts: Vec<NodeId>,
    pub ends: Vec<NodeId>,
}

#[derive(Clone, Debug)]
pub(crate) struct TableContext {
    pub outer: ProjectionContext,
    pub containers: Vec<NodeId>,
}

#[derive(Debug, Default)]
pub struct TextProjection {
    text: String,
    // Canonical roots, including non-text barriers. Deriving roots from text
    // segments loses thematic breaks and can incorrectly join distant groups.
    roots: Vec<NodeId>,
    revisions: FxHashMap<NodeId, document_core::Revision>,
    segments: Vec<ProjectionSegment>,
    by_node: FxHashMap<NodeId, usize>,
    blocks: FxHashMap<NodeId, Arc<BlockNode>>,
    image_segments: Vec<usize>,
    editable_segments: Vec<usize>,
    coordinate_deltas: Arc<OffsetDeltas>,
    coordinate_chunks: Vec<Arc<ProjectionOffset>>,
    coordinate_chunk_starts: Vec<usize>,
    table_widths: FxHashMap<NodeId, Vec<f32>>,
    table_contexts: FxHashMap<NodeId, TableContext>,
    container_cells: FxHashMap<NodeId, (NodeId, usize, usize)>,
    container_edges: FxHashMap<NodeId, ContainerEdges>,
    measured_tables: FxHashMap<NodeId, TableMeasurements>,
    pub(crate) table_layout_lock: Option<TableLayoutLock>,
    pub(crate) preview_edit_node: Option<NodeId>,
    pub(crate) expanded_code_tail: Option<NodeId>,
    pub(crate) command_strip_lock: Option<(NodeId, Option<u32>)>,
    pub(crate) footnotes: Arc<crate::footnotes::Index>,
    pub(crate) html_disclosures: Arc<FxHashMap<NodeId, crate::html::DisclosureState>>,
    pub(crate) html_image_references: FxHashMap<NodeId, Vec<document_core::InertHtmlImage>>,
    pub(crate) html_images: Arc<FxHashMap<NodeId, crate::html::images::BoundImages>>,
}

impl Clone for TextProjection {
    fn clone(&self) -> Self {
        let mut projection = Self {
            text: self.text.clone(),
            roots: self.roots.clone(),
            revisions: self.revisions.clone(),
            segments: self.segments.clone(),
            by_node: self.by_node.clone(),
            blocks: self.blocks.clone(),
            image_segments: self.image_segments.clone(),
            editable_segments: self.editable_segments.clone(),
            coordinate_deltas: Arc::new(OffsetDeltas::default()),
            coordinate_chunks: Vec::new(),
            coordinate_chunk_starts: Vec::new(),
            table_widths: self.table_widths.clone(),
            table_contexts: self.table_contexts.clone(),
            container_cells: self.container_cells.clone(),
            container_edges: self.container_edges.clone(),
            measured_tables: self.measured_tables.clone(),
            table_layout_lock: self.table_layout_lock.clone(),
            preview_edit_node: self.preview_edit_node,
            expanded_code_tail: self.expanded_code_tail,
            command_strip_lock: self.command_strip_lock,
            footnotes: self.footnotes.clone(),
            html_disclosures: self.html_disclosures.clone(),
            html_image_references: self.html_image_references.clone(),
            html_images: self.html_images.clone(),
        };
        projection.reindex_coordinates();
        projection
    }
}

/// Exact inputs to renderer geometry, without copying the projected text or
/// every leaf's metadata. Immutable root allocation identity includes all
/// descendants and their order; view-only constraints are compared separately.
pub(crate) struct ProjectionGeometryKey {
    roots: Vec<Arc<BlockNode>>,
    measured_tables: FxHashMap<NodeId, TableMeasurements>,
    table_layout_lock: Option<TableLayoutLock>,
    preview_edit_node: Option<NodeId>,
    expanded_code_tail: Option<NodeId>,
    command_strip_lock: Option<(NodeId, Option<u32>)>,
    html_disclosures: Arc<FxHashMap<NodeId, crate::html::DisclosureState>>,
    html_images: Arc<FxHashMap<NodeId, crate::html::images::BoundImages>>,
}

impl ProjectionGeometryKey {
    pub(crate) fn matches(&self, projection: &TextProjection) -> bool {
        self.preview_edit_node == projection.preview_edit_node
            && self.expanded_code_tail == projection.expanded_code_tail
            && self.command_strip_lock == projection.command_strip_lock
            && self.table_layout_lock == projection.table_layout_lock
            && self.measured_tables == projection.measured_tables
            && self.html_disclosures == projection.html_disclosures
            && self.html_images == projection.html_images
            && self.roots.len() == projection.roots.len()
            && self.roots.iter().zip(&projection.roots).all(|(root, id)| {
                projection
                    .block_handle(*id)
                    .is_some_and(|current| Arc::ptr_eq(root, current))
            })
    }
}

impl TextProjection {
    /// The terminal newline stays in source; its empty editing row is only
    /// needed when the active endpoint actually occupies that row.
    pub(crate) fn code_tail_for_selection(
        snapshot: &DocumentSnapshot,
        selection: &Selection,
    ) -> Option<NodeId> {
        let Selection::Text(selection) = selection else {
            return None;
        };
        let head = selection.head;
        let block = snapshot.node(head.node_id)?;
        let BlockNode::CodeBlock(code) = block else {
            return None;
        };
        (head.text_offset == code.content.len()
            && code.content.as_cow().ends_with('\n')
            && !crate::math::is_math(block))
        .then_some(head.node_id)
    }

    pub(crate) fn geometry_key(&self) -> ProjectionGeometryKey {
        ProjectionGeometryKey {
            roots: self
                .roots
                .iter()
                .map(|id| self.blocks[id].clone())
                .collect(),
            measured_tables: self.measured_tables.clone(),
            table_layout_lock: self.table_layout_lock.clone(),
            preview_edit_node: self.preview_edit_node,
            expanded_code_tail: self.expanded_code_tail,
            command_strip_lock: self.command_strip_lock,
            html_disclosures: self.html_disclosures.clone(),
            html_images: self.html_images.clone(),
        }
    }

    pub(crate) fn html_disclosure_overrides(
        &self,
        node: NodeId,
    ) -> Option<&crate::html::DisclosureOverrides> {
        let state = self.html_disclosures.get(&node)?;
        matches!(self.block(node), Some(BlockNode::PreservedSource { source, .. }) if *source == state.source)
            .then_some(&state.overrides)
    }

    pub(crate) fn html_images(&self, node: NodeId) -> Option<&crate::html::images::BoundImages> {
        let images = self.html_images.get(&node)?;
        matches!(self.block(node), Some(BlockNode::PreservedSource { source, .. }) if *source == images.source)
            .then_some(images)
    }

    pub(crate) fn retain_html_disclosures(&mut self) {
        let mut states = self.html_disclosures.as_ref().clone();
        states.retain(|node, state| matches!(self.block(*node), Some(BlockNode::PreservedSource { source, .. }) if *source == state.source));
        self.html_disclosures = Arc::new(states);
    }

    fn coordinate_chunk(
        &mut self,
        segment_index: usize,
        byte_start: usize,
        utf16_start: usize,
    ) -> (Arc<ProjectionOffset>, usize, usize) {
        let needs_chunk = self.coordinate_chunks.last().is_none_or(|chunk| {
            segment_index - self.coordinate_chunk_starts.last().copied().unwrap_or(0)
                >= PROJECTION_CHUNK_SEGMENTS
                || byte_start.saturating_sub(chunk.byte_base) >= PROJECTION_CHUNK_BYTES
        });
        if needs_chunk {
            let chunk_index = self.coordinate_chunks.len();
            self.coordinate_chunks
                .push(Arc::new(ProjectionOffset::indexed(
                    byte_start,
                    utf16_start,
                    chunk_index,
                    self.coordinate_deltas.clone(),
                )));
            self.coordinate_chunk_starts.push(segment_index);
        }
        let chunk = self.coordinate_chunks.last().unwrap().clone();
        let local_byte = byte_start - chunk.byte_base;
        let local_utf16 = utf16_start - chunk.utf16_base;
        (chunk, local_byte, local_utf16)
    }

    fn reindex_coordinates(&mut self) {
        let starts = self
            .segments
            .iter()
            .map(|segment| (segment.projection_start(), segment.utf16_start()))
            .collect::<Vec<_>>();
        self.coordinate_deltas = Arc::new(OffsetDeltas::default());
        self.coordinate_chunks.clear();
        self.coordinate_chunk_starts.clear();
        for (index, (byte_start, utf16_start)) in starts.into_iter().enumerate() {
            let (chunk, local_byte, local_utf16) =
                self.coordinate_chunk(index, byte_start, utf16_start);
            let segment = &mut self.segments[index];
            segment.projection_start = chunk;
            segment.projection_local_start =
                u32::try_from(local_byte).expect("projection chunk span fits u32");
            segment.utf16_local_start =
                u32::try_from(local_utf16).expect("UTF-16 projection chunk span fits u32");
        }
        self.coordinate_deltas.seal(self.coordinate_chunks.len());
        self.editable_segments.clear();
        self.editable_segments.extend(
            self.segments
                .iter()
                .enumerate()
                .filter_map(|(index, segment)| {
                    (!segment.context.preserved_source).then_some(index)
                }),
        );
    }

    #[must_use]
    pub fn from_snapshot(snapshot: &DocumentSnapshot) -> Self {
        let source_bytes = snapshot.source_spine().original().len();
        // A single linear reserve estimate is cheaper than a complete tree
        // census. Markdown leaves average far more than 32 source bytes in
        // practice; undersized pathological inputs still grow normally.
        let segment_capacity = source_bytes / 32;
        let mut by_node = FxHashMap::default();
        by_node.reserve(segment_capacity);
        let mut blocks = FxHashMap::default();
        blocks.reserve(segment_capacity);
        let mut projection = Self {
            text: String::with_capacity(source_bytes),
            roots: snapshot.blocks().iter().map(|block| block.id()).collect(),
            revisions: FxHashMap::default(),
            segments: Vec::with_capacity(segment_capacity),
            by_node,
            blocks,
            image_segments: Vec::new(),
            editable_segments: Vec::new(),
            coordinate_deltas: Arc::new(OffsetDeltas::default()),
            coordinate_chunks: Vec::new(),
            coordinate_chunk_starts: Vec::new(),
            table_widths: FxHashMap::default(),
            table_contexts: FxHashMap::default(),
            container_cells: FxHashMap::default(),
            container_edges: FxHashMap::default(),
            measured_tables: FxHashMap::default(),
            footnotes: Arc::default(),
            table_layout_lock: None,
            html_disclosures: Arc::default(),
            html_image_references: FxHashMap::default(),
            html_images: Arc::default(),
            preview_edit_node: match snapshot.selection() {
                Selection::Text(selection) => Some(selection.head.node_id),
                _ => None,
            },
            expanded_code_tail: Self::code_tail_for_selection(snapshot, snapshot.selection()),
            command_strip_lock: None,
        };
        append_sequence(
            snapshot.blocks(),
            &mut projection,
            &ProjectionContext::default(),
            None,
            &mut Vec::new(),
        );
        projection
            .coordinate_deltas
            .seal(projection.coordinate_chunks.len());
        projection.assign_reading_modes();
        projection.footnotes = Arc::new(crate::footnotes::Index::build(&projection));
        projection.preview_edit_node = projection.preview_edit_node.filter(|id| {
            projection
                .block(*id)
                .and_then(BlockNode::text)
                .is_some_and(|text| {
                    text.runs().iter().any(|run| {
                        run.styles
                            .iter()
                            .any(|style| matches!(style, document_core::InlineStyle::Math { .. }))
                    })
                })
        });
        projection.revisions = projection
            .segments
            .iter()
            .filter_map(|segment| {
                snapshot
                    .node_revision(segment.node_id)
                    .map(|revision| (segment.node_id, revision))
            })
            .collect();
        projection
    }

    fn assign_reading_modes(&mut self) {
        // A sustained prose section gets the reading face; reference material
        // keeps a coherent sans mode. Work is linear at source publication,
        // never a document scan in painting or scrolling.
        let editorials = crate::adaptive::editorial::analyze(&self.roots().collect::<Vec<_>>());
        let metadata = crate::adaptive::metadata::analyze(&self.roots().collect::<Vec<_>>());
        let figures = crate::figures::associations(&self.roots().collect::<Vec<_>>());
        let bibliography = crate::bibliography::entries(&self.roots().collect::<Vec<_>>());
        let mut narrative = std::collections::HashSet::new();
        let mut start = 0;
        while start < self.roots.len() {
            let mut end = start + 1;
            while end < self.roots.len()
                && !matches!(
                    self.blocks[&self.roots[end]].as_ref(),
                    BlockNode::Heading(_)
                )
            {
                end += 1;
            }
            let mut paragraphs = 0;
            let mut characters = 0;
            let mut reference = false;
            for id in &self.roots[start..end] {
                match self.blocks[id].as_ref() {
                    BlockNode::Paragraph(p) => {
                        if figures.contains_key(id) {
                            continue;
                        }
                        paragraphs += 1;
                        characters += p.content.len().min(4096);
                    }
                    BlockNode::Heading(_)
                    | BlockNode::BlockQuote { .. }
                    | BlockNode::ThematicBreak { .. } => {}
                    BlockNode::Image(image) if crate::adaptive::prose::supporting_image(image) => {}
                    _ => reference = true,
                }
            }
            if !reference && paragraphs >= 2 && characters >= 280 {
                narrative.extend(self.roots[start..end].iter().copied());
            }
            start = end;
        }
        let resource_lists = self
            .roots
            .iter()
            .filter_map(|id| {
                matches!(self.blocks[id].as_ref(), BlockNode::List(list)
                if crate::adaptive::resource::is_resource_list(list))
                .then_some(*id)
            })
            .collect::<std::collections::HashSet<_>>();
        for segment in &mut self.segments {
            segment.context.metric = editorials
                .get(&segment.top_level_node_id)
                .and_then(|member| member.metric_role(segment.node_id));
            segment.context.color_role = editorials
                .get(&segment.top_level_node_id)
                .and_then(|member| member.color_role(segment.node_id));
            segment.context.color_rgba = segment
                .context
                .color_role
                .filter(|role| *role == crate::signals::ColorRole::Literal)
                .and_then(|_| crate::signals::color_literal(&self.blocks[&segment.node_id]));
            segment.context.bibliography = bibliography.get(&segment.node_id).copied();
            segment.context.figure_text = figures.get(&segment.node_id).copied();
            let resource = if segment.context.figure_text.is_none()
                && segment.context.bibliography.is_none()
                && segment.context.table_cell.is_none()
                && segment.context.quote_depth == 0
                && segment.context.alert.is_none()
                && (segment.top_level_node_id == segment.node_id
                    || resource_lists.contains(&segment.top_level_node_id))
            {
                match self.blocks[&segment.node_id].as_ref() {
                    BlockNode::Paragraph(p) => crate::adaptive::resource::classify(p),
                    _ => None,
                }
            } else {
                None
            };
            segment.context.resource_title_end = resource.and_then(|r| r.body_start);
            segment.context.metadata = metadata.contains(&segment.top_level_node_id);
            segment.context.narrative = narrative.contains(&segment.top_level_node_id)
                && segment.context.bibliography.is_none()
                && segment.context.figure_text.is_none()
                && !editorials.contains_key(&segment.top_level_node_id)
                && resource.is_none()
                && segment.context.table_cell.is_none()
                && segment.context.list_depth == 0;
        }
        for index in 0..self.segments.len() {
            let badge = crate::signals::badge(self, &self.segments[index]);
            self.segments[index].context.badge = badge;
        }
    }

    pub(crate) fn retain_reading_modes(&mut self, modes: &std::collections::HashMap<NodeId, bool>) {
        for segment in &mut self.segments {
            if let Some(mode) = modes.get(&segment.top_level_node_id) {
                segment.context.narrative = *mode
                    && segment.context.bibliography.is_none()
                    && segment.context.resource_title_end.is_none()
                    && segment.context.list_depth == 0
                    && segment.context.table_cell.is_none();
            }
        }
    }

    pub(crate) fn retain_value_roles(
        &mut self,
        members: &std::collections::HashMap<NodeId, crate::adaptive::editorial::Member>,
        editing: Option<NodeId>,
    ) {
        let Some(member) = editing
            .and_then(|id| members.get(&id))
            .filter(|m| m.metric_value.is_some() || m.color_value.is_some())
        else {
            return;
        };
        // Preserve roles while the quantity or label is temporarily incomplete.
        // A structural edit releases the object instead of assigning roles to
        // unrelated new content or headings.
        let owned = self
            .roots
            .iter()
            .filter(|id| members.get(id) == Some(member))
            .copied()
            .collect::<Vec<_>>();
        let Some(start) = self.roots.iter().position(|id| *id == member.owner) else {
            return;
        };
        if owned.len() != members.values().filter(|m| *m == member).count()
            || self.roots.get(start..start + owned.len()) != Some(owned.as_slice())
            || owned.iter().any(|id| {
                if *id == member.owner {
                    !matches!(self.blocks[id].as_ref(), BlockNode::Heading(_))
                } else {
                    !matches!(self.blocks[id].as_ref(), BlockNode::Paragraph(_))
                }
            })
        {
            return;
        }
        for segment in &mut self.segments {
            if members.get(&segment.node_id) == Some(member) {
                segment.context.metric = member.metric_role(segment.node_id);
                segment.context.color_role = member.color_role(segment.node_id);
                segment.context.color_rgba = segment
                    .context
                    .color_role
                    .filter(|role| *role == crate::signals::ColorRole::Literal)
                    .and_then(|_| crate::signals::color_literal(&self.blocks[&segment.node_id]));
                segment.context.narrative = false;
            }
        }
    }

    pub(crate) fn retain_quote_roles(
        &mut self,
        roles: &std::collections::HashMap<NodeId, crate::quotes::TextRole>,
        editing: Option<NodeId>,
    ) {
        let owner = editing
            .and_then(|id| self.segment_for_node(id))
            .map(|s| s.top_level_node_id);
        let Some(owner) = owner else {
            return;
        };
        let adjacent = self
            .roots()
            .scan(None, |previous, root| {
                let pair = (root.id(), *previous);
                *previous = Some(root.id());
                Some(pair)
            })
            .find(|(id, _)| *id == owner)
            .and_then(|(_, previous)| previous)
            .and_then(|id| margin_anchor_after(self.block(id), self));
        for segment in &mut self.segments {
            if segment.top_level_node_id == owner
                && let Some(role) = roles.get(&segment.node_id)
                && segment.context.quote == Some(role.owner)
            {
                segment.context.quote_pull = role.pull;
                segment.context.quote_attribution = role.attribution;
                segment.context.quote_before_attribution = role.before_attribution;
                segment.context.margin_note_anchor =
                    role.margin_anchor.filter(|id| Some(*id) == adjacent);
            }
        }
        // An incomplete label in the focused note must not release otherwise
        // unchanged notes that follow it. Reconnect only the contiguous,
        // source-valid suffix, through the retained canonical paragraph anchor.
        let Some(anchor) = editing
            .and_then(|id| self.segment_for_node(id))
            .and_then(|s| s.context.margin_note_anchor)
        else {
            return;
        };
        let following = self
            .roots()
            .skip_while(|root| root.id() != owner)
            .skip(1)
            .take_while(|root| {
                matches!(root, BlockNode::BlockQuote { blocks, .. }
                if crate::quotes::margin_note_anchor(self.block(anchor), blocks).is_some())
            })
            .map(BlockNode::id)
            .collect::<std::collections::HashSet<_>>();
        for segment in &mut self.segments {
            if following.contains(&segment.top_level_node_id) {
                segment.context.margin_note_anchor = Some(anchor);
                segment.context.quote_attribution = false;
                segment.context.quote_before_attribution = false;
            }
        }
    }

    pub(crate) fn retain_bibliography(
        &mut self,
        roots: &std::collections::HashMap<NodeId, NodeId>,
        editing: Option<NodeId>,
    ) {
        let Some(root) = editing
            .and_then(|id| self.segment_for_node(id))
            .map(|s| s.top_level_node_id)
        else {
            return;
        };
        let Some(&owner) = roots.get(&root) else {
            return;
        };
        for segment in &mut self.segments {
            if segment.top_level_node_id == root
                && segment.context.list_depth <= 1
                && segment.context.task_checked.is_none()
                && segment.context.quote.is_none()
                && segment.context.alert.is_none()
                && segment.context.table_cell.is_none()
                && matches!(
                    self.blocks[&segment.node_id].as_ref(),
                    BlockNode::Paragraph(_)
                )
            {
                segment.context.bibliography = Some(owner);
                segment.context.narrative = false;
                segment.context.resource_title_end = None;
            }
        }
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn segments(&self) -> &[ProjectionSegment] {
        &self.segments
    }

    #[must_use]
    pub fn block(&self, node_id: NodeId) -> Option<&BlockNode> {
        self.blocks.get(&node_id).map(Arc::as_ref)
    }

    pub(crate) fn roots(&self) -> impl ExactSizeIterator<Item = &BlockNode> {
        self.roots.iter().map(|id| self.blocks[id].as_ref())
    }

    /// Immutable canonical allocation identity for renderer-owned caches.
    pub(crate) fn block_handle(&self, id: NodeId) -> Option<&Arc<BlockNode>> {
        self.blocks.get(&id)
    }

    pub(crate) fn node_revision(&self, id: NodeId) -> document_core::Revision {
        self.revisions.get(&id).copied().unwrap_or_default()
    }

    pub(crate) fn table_widths(&self, node_id: NodeId) -> Option<&[f32]> {
        if let Some(lock) = &self.table_layout_lock
            && lock.id == node_id
        {
            return Some(&lock.widths);
        }
        self.measured_tables
            .get(&node_id)
            .map(|m| m.preferred.as_slice())
            .or_else(|| self.table_widths.get(&node_id).map(Vec::as_slice))
    }

    pub(crate) fn table_measurements(&self, id: NodeId) -> Option<&TableMeasurements> {
        if let Some(lock) = &self.table_layout_lock
            && lock.id == id
        {
            return lock.measured.as_ref();
        }
        self.measured_tables.get(&id)
    }

    pub(crate) fn uses_record_layout(&self, id: NodeId, width: f32) -> bool {
        self.record_layout(id, width).is_some()
    }

    pub(crate) fn record_layout(&self, id: NodeId, width: f32) -> Option<RecordLayout> {
        let context = self.table_context(id)?;
        let measured = self.table_measurements(id)?;
        let headers = measured.record_headers.as_ref()?;
        let entities = headers.len() > 2;
        let overflowing = measured.minimum.iter().sum::<f32>() > width;
        if !context.containers.is_empty()
            || (width >= crate::theme::DocumentStyle::SINGLE_COLUMN_WIDTH
                && !(entities && overflowing))
            || headers.iter().sum::<f32>() > width
            || measured.preferred.iter().sum::<f32>() <= width * 1.2
            // Record insets are 24 rather than ordinary table cells' 12.
            || measured.minimum.iter().any(|w| *w + 24. > width)
        {
            return None;
        }
        let paired_width = (width - RecordLayout::GAP) / 2.;
        let paired = entities
            && paired_width >= crate::theme::DocumentStyle::SINGLE_COLUMN_WIDTH
            && measured.minimum.iter().all(|w| *w + 24. <= paired_width)
            && matches!(self.block(id), Some(BlockNode::Table(table)) if table.row_count() >= 3);
        Some(RecordLayout {
            columns: if paired { 2 } else { 1 },
            width: if paired { paired_width } else { width },
        })
    }

    pub(crate) fn table_layout_is_locked(&self, id: NodeId) -> bool {
        self.table_layout_lock
            .as_ref()
            .is_some_and(|lock| lock.id == id)
    }

    pub(crate) fn lock_table_for_node(&mut self, node: Option<NodeId>) {
        let table = node
            .and_then(|node| self.segment_for_node(node))
            .and_then(|segment| segment.context.table_cell)
            .map(|(id, _, _)| id);
        self.table_layout_lock = table.and_then(|id| {
            Some(TableLayoutLock {
                id,
                widths: self.table_widths(id)?.to_vec(),
                measured: self.table_measurements(id).cloned(),
            })
        });
    }

    /// A structural edit can replace the focused leaf without replacing its
    /// table. Retain that table's presentation constraints, but never carry a
    /// lock across a column command or into an unrelated editing target.
    pub(crate) fn retain_table_layout_lock(&mut self, previous: &Self, node: Option<NodeId>) {
        let lock = previous.table_layout_lock.as_ref().and_then(|lock| {
            let table_id = self.segment_for_node(node?)?.context.table_cell?.0;
            if table_id != lock.id {
                return None;
            }
            let BlockNode::Table(before) = previous.block(table_id)? else {
                return None;
            };
            let BlockNode::Table(after) = self.block(table_id)? else {
                return None;
            };
            (before.columns == after.columns).then(|| lock.clone())
        });
        self.install_table_layout_lock(lock);
    }

    /// Both immediate refresh and background publication restore the visual
    /// roles together with their frozen measured constraints.
    pub(crate) fn install_table_layout_lock(&mut self, lock: Option<TableLayoutLock>) {
        self.table_layout_lock = lock;
        if let Some(lock) = &self.table_layout_lock
            && lock
                .measured
                .as_ref()
                .is_some_and(|m| m.record_headers.is_some())
        {
            for segment in &mut self.segments {
                if segment
                    .context
                    .table_cell
                    .is_some_and(|(id, row, column)| id == lock.id && row > 0 && column == 0)
                {
                    segment.context.table_property_key = true;
                }
            }
        }
    }

    pub(crate) fn install_table_measurements(&mut self, id: NodeId, measured: TableMeasurements) {
        if measured.minimum.len() == measured.preferred.len()
            && measured
                .minimum
                .iter()
                .zip(&measured.preferred)
                .all(|(low, high)| low.is_finite() && *low > 0. && high.is_finite() && high >= low)
        {
            self.measured_tables.insert(id, measured);
        }
    }

    /// Untouched canonical table Arcs survive transactions. Reuse only under
    /// the same font environment (edit refresh, never font/zoom reflow).
    pub(crate) fn reuse_table_measurements(&mut self, previous: &Self) {
        if self.footnotes.numbering != previous.footnotes.numbering {
            return;
        }
        for (id, measured) in &previous.measured_tables {
            if self
                .blocks
                .get(id)
                .zip(previous.blocks.get(id))
                .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
            {
                self.measured_tables.insert(*id, measured.clone());
            }
        }
    }

    pub(crate) fn table_context(&self, id: NodeId) -> Option<&TableContext> {
        self.table_contexts.get(&id)
    }

    pub(crate) fn container_cell(&self, id: NodeId) -> Option<(NodeId, usize, usize)> {
        self.container_cells.get(&id).copied()
    }

    pub(crate) fn container_edges(&self, leaf: NodeId) -> Option<&ContainerEdges> {
        self.container_edges.get(&leaf)
    }

    pub(crate) fn table_available_width(&self, id: NodeId, canvas: f32) -> f32 {
        let Some(BlockNode::Table(table)) = self.block(id) else {
            return canvas;
        };
        if table.columns.iter().any(|column| column.width.is_some()) {
            return canvas;
        }
        if let Some(measured) = self.table_measurements(id) {
            return measured.available_width(canvas);
        }
        if table.columns.len() == 2 {
            canvas.min(crate::adaptive::PROSE_WIDTH)
        } else {
            canvas
        }
    }

    /// Water-fill intrinsic column weights, preserving a readable minimum.
    /// Evaluated during geometry preparation, never in the scroll paint loop.
    pub(crate) fn fitted_table_widths(&self, id: NodeId, available: f32) -> Option<Vec<f32>> {
        let widths = self.table_widths(id)?;
        let explicit = matches!(self.block(id), Some(BlockNode::Table(table))
            if table.columns.iter().any(|column| column.width.is_some()));
        if explicit {
            return Some(widths.to_vec());
        }
        if let Some(measured) = self.table_measurements(id) {
            return Some(measured.fit(available));
        }
        Some(fit_columns(widths, available))
    }

    /// A record layout retains an editable schema row. Its measured headers
    /// are independent of data-column minima; ordinary rows keep the table fit.
    pub(crate) fn fitted_table_row_widths(
        &self,
        id: NodeId,
        row: usize,
        available: f32,
    ) -> Option<Vec<f32>> {
        if row == 0 && self.uses_record_layout(id, available) {
            return self.table_measurements(id)?.record_headers.clone();
        }
        self.fitted_table_widths(id, available)
    }

    #[must_use]
    pub fn segment_for_node(&self, node_id: NodeId) -> Option<&ProjectionSegment> {
        self.by_node
            .get(&node_id)
            .and_then(|index| self.segments.get(*index))
    }

    pub(crate) fn segment_index_for_node(&self, node_id: NodeId) -> Option<usize> {
        self.by_node.get(&node_id).copied()
    }

    #[must_use]
    pub fn utf16_len(&self) -> usize {
        self.segments.last().map_or(0, ProjectionSegment::utf16_end)
    }

    pub(crate) fn image_segments(&self) -> impl Iterator<Item = &ProjectionSegment> {
        self.image_segments
            .iter()
            .filter_map(|index| self.segments.get(*index))
    }

    /// Refreshes one existing editable segment without walking or copying the
    /// rest of the document. Returns the old projected range and its byte-size
    /// delta. Structural edits deliberately fall back to a full projection.
    pub(crate) fn refresh_text_node(
        &mut self,
        snapshot: &DocumentSnapshot,
        node_id: NodeId,
    ) -> Option<(Range<usize>, isize)> {
        let index = *self.by_node.get(&node_id)?;
        // A note's aggregate semantic label and its descendants must come from
        // one snapshot, including when an edit changes only its first leaf.
        if self.segments[index].context.footnote.is_some() {
            return None;
        }
        let block = snapshot.node(node_id)?.clone();
        let table = match self.segments[index].context.table_cell {
            Some((id, _, _)) => {
                let table = snapshot.node(id)?.clone();
                // Header/property-label edits change the meaning of sibling
                // values; republish their signals together, not one stale leaf.
                if crate::signals::affects_badge_peers(self, &self.segments[index], &block) {
                    return None;
                }
                // Entity records repeat these authored headers beside values.
                // Republish dependent label geometry from the same snapshot.
                if self.segments[index].context.table_header
                    && self
                        .table_measurements(id)
                        .and_then(|m| m.record_headers.as_ref())
                        .is_some_and(|h| h.len() > 2)
                {
                    return None;
                }
                Some((id, table))
            }
            None => None,
        };
        let rich_text = block.text()?;
        // Reference edits can renumber notes in unrelated leaves. Publish a
        // complete source-order index and invalidate their geometry together.
        if [self.block(node_id)?.text()?, rich_text]
            .iter()
            .any(|text| {
                text.runs().iter().any(|run| {
                    run.styles.iter().any(|style| {
                        matches!(style, document_core::InlineStyle::FootnoteReference(_))
                    })
                })
            })
        {
            return None;
        }
        let mut replacement = String::with_capacity(rich_text.len());
        rich_text.append_to(&mut replacement);
        let old_range = self.segments.get(index)?.projection_range();
        let old_utf16_len = self.segments.get(index)?.utf16_len as usize;
        let replacement_utf16_len = replacement.encode_utf16().count();
        let delta =
            isize::try_from(replacement.len()).ok()? - isize::try_from(old_range.len()).ok()?;
        let utf16_delta =
            isize::try_from(replacement_utf16_len).ok()? - isize::try_from(old_utf16_len).ok()?;
        self.text.replace_range(old_range.clone(), &replacement);

        let segment = self.segments.get_mut(index)?;
        let coordinate_chunk = segment.projection_start.clone();
        segment.projection_len = u32::try_from(replacement.len()).ok()?;
        segment.utf16_len = u32::try_from(replacement_utf16_len).ok()?;
        segment.node_range = 0..replacement.len();
        if segment.context.color_role == Some(crate::signals::ColorRole::Literal) {
            segment.context.color_rgba = crate::signals::color_literal(&block);
        }
        if let Some(before) = segment.context.resource_title_end {
            segment.context.resource_title_end = Some(
                match &block {
                    BlockNode::Paragraph(p) => crate::adaptive::resource::classify(p)
                        .and_then(|resource| resource.body_start),
                    _ => None,
                }
                .unwrap_or_else(|| replacement.floor_char_boundary(before.min(replacement.len()))),
            );
        }
        for following in self.segments[index + 1..]
            .iter_mut()
            .take_while(|following| Arc::ptr_eq(&following.projection_start, &coordinate_chunk))
        {
            following.projection_local_start = u32::try_from(
                (following.projection_local_start as usize).checked_add_signed(delta)?,
            )
            .ok()?;
            following.utf16_local_start = u32::try_from(
                (following.utf16_local_start as usize).checked_add_signed(utf16_delta)?,
            )
            .ok()?;
        }
        coordinate_chunk.add_suffix(coordinate_chunk.index + 1, delta, utf16_delta)?;
        self.blocks.insert(node_id, Arc::new(block));
        if let Some((id, table)) = table {
            self.blocks.insert(id, Arc::new(table));
            // The lock retains the visible widths, but these measurements no
            // longer describe the canonical content. Do not reuse them on blur.
            self.measured_tables.remove(&id);
        }
        self.revisions
            .insert(node_id, snapshot.node_revision(node_id)?);
        let badge = crate::signals::badge(self, &self.segments[index]);
        self.segments[index].context.badge = badge;
        Some((old_range, delta))
    }

    /// Finds the segment containing a projected visual-line range in
    /// logarithmic time. Segment starts are monotonic, including empty nodes.
    #[must_use]
    pub fn segment_for_range(&self, range: &Range<usize>) -> Option<&ProjectionSegment> {
        let split = self
            .segments
            .partition_point(|segment| segment.projection_start() <= range.start);
        let candidate = self.segments.get(split.checked_sub(1)?)?;
        (candidate.projection_range().contains(&range.start)
            || candidate.projection_range() == *range
            || candidate.projection_end() == range.end)
            .then_some(candidate)
    }

    #[must_use]
    pub fn position_at(&self, offset: usize, affinity: Affinity) -> Option<DocumentPosition> {
        if offset > self.text.len() || !self.text.is_char_boundary(offset) {
            return None;
        }
        let split = self
            .editable_segments
            .partition_point(|index| self.segments[*index].projection_start() <= offset);
        let before = split
            .checked_sub(1)
            .and_then(|index| self.editable_segments.get(index))
            .copied();
        let after = self.editable_segments.get(split).copied();
        let direct = [before, after].into_iter().flatten().find(|index| {
            let segment = &self.segments[*index];
            segment.projection_range().contains(&offset)
                || (offset == self.text.len() && segment.projection_end() == self.text.len())
                || (affinity == Affinity::Upstream && segment.projection_end() == offset)
                || (affinity == Affinity::Downstream && segment.projection_start() == offset)
        });
        if let Some(index) = direct {
            let segment = &self.segments[index];
            let local = offset
                .saturating_sub(segment.projection_start())
                .min(segment.node_range.len());
            return Some(DocumentPosition::new(
                segment.node_id,
                segment.node_range.start + local,
                affinity,
            ));
        }

        let segment = [before, after]
            .into_iter()
            .flatten()
            .map(|index| &self.segments[index])
            .min_by_key(|segment| {
                if offset < segment.projection_start() {
                    segment.projection_start() - offset
                } else {
                    offset.saturating_sub(segment.projection_end())
                }
            })?;
        let text_offset = if offset <= segment.projection_start() {
            segment.node_range.start
        } else {
            segment.node_range.end
        };
        Some(DocumentPosition::new(
            segment.node_id,
            text_offset,
            affinity,
        ))
    }

    #[must_use]
    pub fn offset_of(&self, position: DocumentPosition) -> Option<usize> {
        let segment = self
            .by_node
            .get(&position.node_id)
            .and_then(|index| self.segments.get(*index))?;
        if !segment.node_range.contains(&position.text_offset)
            && position.text_offset != segment.node_range.end
        {
            return None;
        }
        Some(segment.projection_start() + position.text_offset - segment.node_range.start)
    }

    #[must_use]
    pub fn selection_range(&self, selection: &Selection) -> Option<(Range<usize>, bool)> {
        let Selection::Text(selection) = selection else {
            return None;
        };
        let anchor = self.offset_of(selection.anchor)?;
        let head = self.offset_of(selection.head)?;
        Some((anchor.min(head)..anchor.max(head), head < anchor))
    }

    #[must_use]
    pub fn utf16_offset_for_byte(&self, byte_offset: usize) -> Option<usize> {
        if byte_offset > self.text.len() || !self.text.is_char_boundary(byte_offset) {
            return None;
        }
        let split = self
            .segments
            .partition_point(|segment| segment.projection_start() <= byte_offset);
        let index = split.checked_sub(1)?;
        let segment = self.segments.get(index)?;
        let utf16_range = segment.utf16_range();
        if byte_offset <= segment.projection_end() {
            let local = &self.text[segment.projection_start()..byte_offset];
            Some(utf16_range.start + local.encode_utf16().count())
        } else {
            Some(utf16_range.end + byte_offset - segment.projection_end())
        }
    }

    #[must_use]
    pub fn byte_offset_for_utf16(&self, utf16_offset: usize) -> usize {
        let Some(index) = self
            .segments
            .partition_point(|segment| segment.utf16_start() <= utf16_offset)
            .checked_sub(1)
        else {
            return 0;
        };
        let Some(segment) = self.segments.get(index) else {
            return self.text.len();
        };
        let utf16_range = segment.utf16_range();
        if utf16_offset > utf16_range.end {
            return (segment.projection_end() + utf16_offset - utf16_range.end)
                .min(self.text.len());
        }
        let target = utf16_offset.saturating_sub(utf16_range.start);
        let text = &self.text[segment.projection_range()];
        let mut units = 0;
        for (byte, character) in text.char_indices() {
            if units >= target {
                return segment.projection_start() + byte;
            }
            let next = units + character.len_utf16();
            if next > target {
                return segment.projection_start() + byte;
            }
            units = next;
        }
        segment.projection_end()
    }

    #[must_use]
    pub fn range_from_utf16(&self, range: Range<usize>) -> Range<usize> {
        self.byte_offset_for_utf16(range.start)..self.byte_offset_for_utf16(range.end)
    }

    #[must_use]
    pub fn range_to_utf16(&self, range: Range<usize>) -> Option<Range<usize>> {
        Some(self.utf16_offset_for_byte(range.start)?..self.utf16_offset_for_byte(range.end)?)
    }

    fn push_text(
        &mut self,
        node_id: NodeId,
        top_level_node_id: NodeId,
        value: &str,
        context: ProjectionContext,
    ) {
        if !self.segments.is_empty() {
            self.text.push('\n');
        }
        let start = self.text.len();
        let utf16_start = self.segments.last().map_or(0, |segment| {
            segment.projection_start.utf16_base
                + segment.utf16_local_start as usize
                + segment.utf16_len as usize
                + 1
        });
        self.text.push_str(value);
        let index = self.segments.len();
        let (projection_start, projection_local_start, utf16_local_start) =
            self.coordinate_chunk(index, start, utf16_start);
        self.segments.push(ProjectionSegment {
            node_id,
            top_level_node_id,
            projection_start,
            projection_local_start: u32::try_from(projection_local_start)
                .expect("projection chunk span fits u32"),
            projection_len: u32::try_from(self.text.len() - start)
                .expect("projection segment fits u32"),
            utf16_local_start: u32::try_from(utf16_local_start)
                .expect("UTF-16 projection chunk span fits u32"),
            utf16_len: u32::try_from(value.encode_utf16().count())
                .expect("UTF-16 projection segment fits u32"),
            node_range: 0..value.len(),
            context: Box::new(context),
        });
        if self.segments[index].context.image_source.is_some() {
            self.image_segments.push(index);
        }
        if !self.segments[index].context.preserved_source {
            self.editable_segments.push(index);
        }
        self.by_node.insert(node_id, index);
    }

    fn push_rich_text(
        &mut self,
        node_id: NodeId,
        top_level_node_id: NodeId,
        value: &RichText,
        context: ProjectionContext,
    ) {
        if !self.segments.is_empty() {
            self.text.push('\n');
        }
        let start = self.text.len();
        let utf16_start = self.segments.last().map_or(0, |segment| {
            segment.projection_start.utf16_base
                + segment.utf16_local_start as usize
                + segment.utf16_len as usize
                + 1
        });
        self.text.reserve(value.len());
        let previous_len = self.text.len();
        value.append_to(&mut self.text);
        let utf16_len = self.text[previous_len..].encode_utf16().count();
        let index = self.segments.len();
        let (projection_start, projection_local_start, utf16_local_start) =
            self.coordinate_chunk(index, start, utf16_start);
        self.segments.push(ProjectionSegment {
            node_id,
            top_level_node_id,
            projection_start,
            projection_local_start: u32::try_from(projection_local_start)
                .expect("projection chunk span fits u32"),
            projection_len: u32::try_from(self.text.len() - start)
                .expect("projection segment fits u32"),
            utf16_local_start: u32::try_from(utf16_local_start)
                .expect("UTF-16 projection chunk span fits u32"),
            utf16_len: u32::try_from(utf16_len).expect("UTF-16 projection segment fits u32"),
            node_range: 0..value.len(),
            context: Box::new(context),
        });
        if self.segments[index].context.image_source.is_some() {
            self.image_segments.push(index);
        }
        if !self.segments[index].context.preserved_source {
            self.editable_segments.push(index);
        }
        self.by_node.insert(node_id, index);
    }
}

fn record_container(
    projection: &mut TextProjection,
    id: NodeId,
    start: usize,
    end: usize,
    cell: Option<(NodeId, usize, usize)>,
) {
    if let Some(cell) = cell {
        projection.container_cells.insert(id, cell);
    }
    let first = projection.segments[start].node_id;
    let last = projection.segments[end - 1].node_id;
    projection
        .container_edges
        .entry(first)
        .or_default()
        .starts
        .push(id);
    projection
        .container_edges
        .entry(last)
        .or_default()
        .ends
        .push(id);
}

fn outline_item_label(blocks: &BlockSequence) -> Option<NodeId> {
    let block = blocks.get(0)?;
    match block.as_ref() {
        BlockNode::Paragraph(_)
        | BlockNode::Heading(_)
        | BlockNode::CodeBlock(_)
        | BlockNode::Image(_) => Some(block.id()),
        BlockNode::BlockQuote { blocks, .. } | BlockNode::Alert { blocks, .. } => {
            outline_item_label(blocks)
        }
        BlockNode::Table(table) => outline_item_label(&table.rows.first()?.cells.first()?.blocks),
        _ => None,
    }
}

/// A new explicit note can continue the preceding note's canonical anchor.
/// Ordinary quotes/alerts have no such context and therefore end the chain.
fn margin_anchor_after(
    previous: Option<&BlockNode>,
    projection: &TextProjection,
) -> Option<NodeId> {
    match previous? {
        BlockNode::Paragraph(paragraph) => Some(paragraph.id),
        BlockNode::BlockQuote { blocks, .. } => {
            projection
                .segment_for_node(blocks.get(0)?.id())?
                .context
                .margin_note_anchor
        }
        _ => None,
    }
}

fn outline_metrics(
    list: &document_core::ListBlock,
    depth: usize,
    inset: usize,
) -> Option<(usize, usize)> {
    let depth = depth.checked_add(1)?;
    let inset = inset.checked_add(if matches!(list.kind, ListKind::Ordered { .. }) {
        32
    } else {
        24
    })?;
    let mut maximum = (depth, inset);
    for item in list.items.iter() {
        outline_item_label(&item.blocks)?;
        let child = outline_block_metrics(&item.blocks, depth, inset)?;
        maximum.0 = maximum.0.max(child.0);
        maximum.1 = maximum.1.max(child.1);
    }
    Some(maximum)
}

fn outline_block_metrics(
    blocks: &BlockSequence,
    depth: usize,
    inset: usize,
) -> Option<(usize, usize)> {
    let mut maximum = (depth, inset);
    for block in blocks.iter() {
        let child = match block.as_ref() {
            BlockNode::List(list) => outline_metrics(list, depth, inset)?,
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::Definition { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                outline_block_metrics(blocks, depth, inset)?
            }
            BlockNode::Table(table) => {
                let mut inner = (depth, inset);
                for cell in table.rows.iter().flat_map(|row| row.cells.iter()) {
                    let child = outline_block_metrics(&cell.blocks, depth, inset)?;
                    inner.0 = inner.0.max(child.0);
                    inner.1 = inner.1.max(child.1);
                }
                inner
            }
            _ => (depth, inset),
        };
        maximum.0 = maximum.0.max(child.0);
        maximum.1 = maximum.1.max(child.1);
    }
    Some(maximum)
}

fn append_sequence(
    blocks: &BlockSequence,
    projection: &mut TextProjection,
    context: &ProjectionContext,
    top_level_node_id: Option<NodeId>,
    containers: &mut Vec<NodeId>,
) {
    for (ordinal, block) in blocks.iter().enumerate() {
        let previous = ordinal
            .checked_sub(1)
            .and_then(|i| blocks.get(i))
            .map(AsRef::as_ref);
        let top_level_node_id = top_level_node_id.unwrap_or_else(|| block.id());
        projection.blocks.insert(block.id(), block.clone());
        match block.as_ref() {
            BlockNode::Paragraph(paragraph) => {
                projection.push_rich_text(
                    paragraph.id,
                    top_level_node_id,
                    &paragraph.content,
                    context.clone(),
                );
            }
            BlockNode::Heading(heading) => {
                projection.push_rich_text(
                    heading.id,
                    top_level_node_id,
                    &heading.content,
                    context.clone(),
                );
            }
            BlockNode::CodeBlock(code) => {
                projection.push_rich_text(
                    code.id,
                    top_level_node_id,
                    &code.content,
                    context.clone(),
                );
            }
            BlockNode::Image(image) => {
                let mut image_context = context.clone();
                image_context.image_source = Some(image.source.clone());
                projection.push_rich_text(image.id, top_level_node_id, &image.alt, image_context);
            }
            BlockNode::List(list) => {
                let outline_metrics = if context.list_depth == 0 {
                    (context.table_cell.is_none()
                        && !context.metadata
                        && context.bibliography.is_none()
                        && context.definition.is_none())
                    .then(|| {
                        // Resolve outer pressure once, so supporting quotes
                        // deeper in a branch cannot switch only half the tree.
                        let outer = context
                            .quote_depth
                            .checked_mul(48)?
                            .checked_add(if context.alert.is_some() { 72 } else { 0 })?;
                        outline_metrics(list, 0, outer)
                    })
                    .flatten()
                } else {
                    context.outline_metrics
                };
                let mut ancestors = context.list_ancestors.to_vec();
                ancestors.push(list.id);
                let ancestors: Arc<[NodeId]> = ancestors.into();
                if let Some(cell) = context.table_cell {
                    projection.container_cells.insert(list.id, cell);
                }
                for (index, item) in list.items.iter().enumerate() {
                    let start = projection.segments.len();
                    let mut list_context = context.clone();
                    list_context.outline_metrics = outline_metrics;
                    list_context.list_parent_label = context.list_item_label;
                    list_context.list_branch_start = index == 0
                        || list.items[index - 1]
                            .blocks
                            .iter()
                            .any(|block| matches!(block.as_ref(), BlockNode::List(_)));
                    list_context.list_item_label = outline_item_label(&item.blocks);
                    list_context.list_item_container = item
                        .blocks
                        .get(0)
                        .filter(|block| {
                            matches!(
                                block.as_ref(),
                                BlockNode::BlockQuote { .. }
                                    | BlockNode::Alert { .. }
                                    | BlockNode::Table(_)
                            )
                        })
                        .map(|block| block.id());
                    list_context.list_depth += 1;
                    list_context.list_ancestors = ancestors.clone();
                    list_context.ordered_list_depth +=
                        usize::from(matches!(list.kind, ListKind::Ordered { .. }));
                    list_context.task_list_depth += usize::from(item.checked.is_some());
                    list_context.list_marker = Some(match list.kind {
                        ListKind::Unordered | ListKind::Task => "•".into(),
                        ListKind::Ordered { start } => format!("{}.", start + index as u64),
                    });
                    list_context.task_checked = item.checked;
                    append_sequence(
                        &item.blocks,
                        projection,
                        &list_context,
                        Some(top_level_node_id),
                        containers,
                    );
                    for segment in projection.segments.iter_mut().skip(start + 1) {
                        if segment.context.list_depth == list_context.list_depth {
                            segment.context.list_marker = None;
                            segment.context.task_checked = None;
                        }
                    }
                }
            }
            BlockNode::BlockQuote { id, blocks, .. } => {
                let start = projection.segments.len();
                let mut quote_context = context.clone();
                quote_context.quote_depth += 1;
                quote_context.quote = Some(*id);
                let mut ancestors = context.quote_ancestors.to_vec();
                ancestors.push(*id);
                quote_context.quote_ancestors = ancestors.into();
                quote_context.quote_pull = context.quote_depth == 0
                    && context.list_depth == 0
                    && context.alert.is_none()
                    && context.table_cell.is_none()
                    && crate::quotes::is_pull_quote(previous, blocks);
                quote_context.margin_note_anchor = (context.quote_depth == 0
                    && context.list_depth == 0
                    && context.alert.is_none()
                    && context.table_cell.is_none())
                .then(|| {
                    let anchor = margin_anchor_after(previous, projection)?;
                    crate::quotes::margin_note_anchor(projection.block(anchor), blocks)
                })
                .flatten();
                containers.push(*id);
                append_sequence(
                    blocks,
                    projection,
                    &quote_context,
                    Some(top_level_node_id),
                    containers,
                );
                let _ = containers.pop();
                let end = projection.segments.len();
                if start < end {
                    projection.segments[start].context.quote_first = true;
                    projection.segments[end - 1].context.quote_last = true;
                    if quote_context.margin_note_anchor.is_none()
                        && let Some(attribution) = crate::quotes::attribution(blocks)
                        && let Some(index) = projection.by_node.get(&attribution).copied()
                    {
                        projection.segments[index].context.quote_attribution = true;
                        projection.segments[index - 1]
                            .context
                            .quote_before_attribution = true;
                    }
                    record_container(projection, *id, start, end, context.table_cell);
                }
            }
            BlockNode::Alert {
                id, kind, blocks, ..
            } => {
                let start = projection.segments.len();
                let mut alert_context = context.clone();
                alert_context.alert = Some((*id, kind.clone()));
                containers.push(*id);
                append_sequence(
                    blocks,
                    projection,
                    &alert_context,
                    Some(top_level_node_id),
                    containers,
                );
                let _ = containers.pop();
                let end = projection.segments.len();
                if start < end {
                    projection.segments[start].context.alert_first = true;
                    projection.segments[end - 1].context.alert_last = true;
                    record_container(projection, *id, start, end, context.table_cell);
                }
            }
            BlockNode::FootnoteDefinition { id, blocks, .. } => {
                let start = projection.segments.len();
                let mut note_context = context.clone();
                note_context.footnote = Some(*id);
                note_context.narrative = false;
                containers.push(*id);
                append_sequence(
                    blocks,
                    projection,
                    &note_context,
                    Some(top_level_node_id),
                    containers,
                );
                let _ = containers.pop();
                let end = projection.segments.len();
                if start < end {
                    projection.segments[start].context.footnote_first = true;
                    record_container(projection, *id, start, end, context.table_cell);
                }
            }
            BlockNode::Definition { id, kind, blocks } => {
                let start = projection.segments.len();
                let mut definition_context = context.clone();
                definition_context.narrative = false;
                definition_context.definition = Some((*id, *kind));
                containers.push(*id);
                append_sequence(
                    blocks,
                    projection,
                    &definition_context,
                    Some(top_level_node_id),
                    containers,
                );
                let _ = containers.pop();
                record_container(
                    projection,
                    *id,
                    start,
                    projection.segments.len(),
                    context.table_cell,
                );
            }
            BlockNode::Table(table) => {
                let properties = property_header_labels(table) || entity_header_labels(table);
                projection.table_contexts.insert(
                    table.id,
                    TableContext {
                        outer: context.clone(),
                        containers: containers.clone(),
                    },
                );
                let mut widths = vec![96_f32; table.columns.len()];
                // Analyze once per projection, not once per cell or frame.
                // Long descriptive columns can negotiate more room while
                // authored column widths continue to take precedence.
                for row in table.rows.iter() {
                    for (column, cell) in row.cells.iter().enumerate() {
                        if let Some(width) = widths.get_mut(column) {
                            let length = cell
                                .blocks
                                .iter()
                                .filter_map(|block| block.text())
                                .map(|text| text.len().min(72))
                                .sum::<usize>()
                                .min(72);
                            *width = width.max((length as f32 * 7.7 + 24.).clamp(96., 400.));
                        }
                    }
                }
                for (width, column) in widths.iter_mut().zip(table.columns.iter()) {
                    if let Some(explicit) = column.width {
                        *width = explicit.max(32.);
                    }
                }
                projection.table_widths.insert(table.id, widths);
                for (row_index, row) in table.rows.iter().enumerate() {
                    for (column_index, cell) in row.cells.iter().enumerate() {
                        let mut cell_context = context.clone();
                        cell_context.table_cell = Some((table.id, row_index, column_index));
                        cell_context.table_header = row_index < table.header_rows;
                        cell_context.table_property_key =
                            properties && row_index >= table.header_rows && column_index == 0;
                        cell_context.table_border = Some(table.border);
                        append_sequence(
                            &cell.blocks,
                            projection,
                            &cell_context,
                            Some(top_level_node_id),
                            containers,
                        );
                    }
                }
            }
            BlockNode::PreservedSource {
                id,
                description,
                source,
                ..
            } => {
                if let Some(fragment) = document_core::inert_html_fragment(source)
                    && !fragment.images().is_empty()
                {
                    projection
                        .html_image_references
                        .insert(*id, fragment.images().to_vec());
                }
                let mut preserved_context = context.clone();
                preserved_context.preserved_source = true;
                projection.push_text(*id, top_level_node_id, description, preserved_context);
            }
            BlockNode::ThematicBreak { id } => {
                let mut rule_context = context.clone();
                // A source-order geometry/semantic anchor, never an editable
                // text node. Empty text adds no invented document content.
                rule_context.preserved_source = true;
                projection.push_text(*id, top_level_node_id, "", rule_context);
            }
        }
    }
}

pub(crate) fn property_header_labels(table: &document_core::Table) -> bool {
    if table.column_count() != 2 || table.header_rows != 1 {
        return false;
    }
    let Some(header) = table.rows.first() else {
        return false;
    };
    header.cells.len() == 2
        && header.cells.iter().enumerate().all(|(column, cell)| {
            if cell.blocks.len() != 1 {
                return false;
            }
            let Some(BlockNode::Paragraph(p)) = cell.blocks.get(0).map(AsRef::as_ref) else {
                return false;
            };
            let text = p.content.as_string();
            let label = text.trim().to_ascii_lowercase();
            if column == 0 {
                matches!(
                    label.as_str(),
                    "property" | "field" | "key" | "setting" | "parameter" | "attribute" | "term"
                )
            } else {
                matches!(
                    label.as_str(),
                    "value" | "description" | "definition" | "meaning"
                )
            }
        })
}

/// A directory row names an independent entity and describes it. Numeric
/// option/criterion comparisons are intentionally not nominated by this rule.
pub(crate) fn entity_header_labels(table: &document_core::Table) -> bool {
    if !(3..=8).contains(&table.column_count()) || table.header_rows != 1 {
        return false;
    }
    let Some(row) = table.rows.first() else {
        return false;
    };
    let labels = row
        .cells
        .iter()
        .map(|cell| {
            if cell.blocks.len() != 1 {
                return None;
            }
            let BlockNode::Paragraph(p) = cell.blocks.get(0)?.as_ref() else {
                return None;
            };
            let text = p.content.as_string().trim().to_ascii_lowercase();
            (!text.is_empty() && !text.contains(['\n', '\r'])).then_some(text)
        })
        .collect::<Option<Vec<_>>>();
    let Some(labels) = labels else {
        return false;
    };
    labels.len() == table.column_count()
        && labels.first().is_some_and(|label| {
            matches!(
                label.as_str(),
                "name"
                    | "service"
                    | "component"
                    | "resource"
                    | "document"
                    | "package"
                    | "endpoint"
                    | "dataset"
            )
        })
        && labels
            .iter()
            .collect::<std::collections::HashSet<_>>()
            .len()
            == labels.len()
        && labels.iter().skip(1).any(|label| {
            matches!(
                label.as_str(),
                "purpose" | "description" | "summary" | "responsibility" | "details" | "notes"
            )
        })
        && table
            .columns
            .iter()
            .filter(|column| column.alignment == document_core::ColumnAlignment::Right)
            .count()
            < 2
}

#[cfg(test)]
mod tests {
    use document_core::{Document, EditCommand};

    use super::*;

    #[test]
    fn reading_modes_follow_sections_without_rewriting_reference_content() {
        let prose = "A sustained passage gives an idea enough room to develop, keeping examples and qualifications close to the argument. ";
        let source = format!(
            "## Reading\n\n{}\n\n{}\n\n## Reference\n\nShort instructions.\n\n- One\n- Two\n",
            prose.repeat(2),
            prose.repeat(2)
        );
        let document = Document::from_markdown(source.as_str()).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let mut reading = true;
        for segment in projection.segments() {
            if projection.text()[segment.projection_range()] == *"Reference" {
                reading = false;
            }
            assert_eq!(segment.context.narrative, reading);
        }
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn focused_edits_do_not_switch_the_published_reading_face() {
        let mut document =
            Document::from_markdown("## Notes\n\nFirst thought.\n\nSecond thought.\n").unwrap();
        let previous = TextProjection::from_snapshot(&document.snapshot());
        let modes = previous
            .segments()
            .iter()
            .map(|s| (s.top_level_node_id, s.context.narrative))
            .collect();
        let paragraph = previous.segments()[1].node_id;
        document
            .apply(EditCommand::ReplaceText {
                node_id: paragraph,
                range: 0..0,
                text: "A longer idea with supporting context. ".repeat(10),
                selection_after: None,
                typing: true,
            })
            .unwrap();
        let mut focused = TextProjection::from_snapshot(&document.snapshot());
        assert!(focused.segments().iter().all(|s| s.context.narrative));
        focused.retain_reading_modes(&modes);
        assert!(focused.segments().iter().all(|s| !s.context.narrative));
        assert!(
            TextProjection::from_snapshot(&document.snapshot())
                .segments()
                .iter()
                .all(|s| s.context.narrative),
            "releasing editing may reconsider typography without changing source"
        );
    }

    #[test]
    fn plain_tables_default_to_sharp_rules_without_losing_authored_borders() {
        for (prefix, expected) in [
            ("", TableBorder::LogicalPixel),
            (
                "<!-- tachyon-table:v1 {\"border\":\"PhysicalPixel\",\"widths\":[null,null]} -->\n",
                TableBorder::PhysicalPixel,
            ),
            (
                "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[null,null]} -->\n",
                TableBorder::Dotted,
            ),
        ] {
            let source = format!("{prefix}| A | B |\n| --- | --- |\n| One | Two |\n");
            let document = Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            assert!(
                projection
                    .segments()
                    .iter()
                    .filter(|s| s.context.table_cell.is_some())
                    .all(|s| s.context.table_border == Some(expected))
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn table_lock_survives_only_the_same_table_and_column_definition() {
        let mut document =
            Document::from_markdown("| A | B |\n| --- | --- |\n| one | two |\n\nOutside\n")
                .unwrap();
        let mut previous = TextProjection::from_snapshot(&document.snapshot());
        let cell = previous
            .segments()
            .iter()
            .find(|segment| segment.context.table_cell.is_some())
            .unwrap();
        let node = cell.node_id;
        let table = cell.context.table_cell.unwrap().0;
        let outside = previous
            .segments()
            .iter()
            .find(|segment| segment.context.table_cell.is_none())
            .unwrap()
            .node_id;
        previous.lock_table_for_node(Some(node));
        assert!(previous.table_layout_is_locked(table));
        let mut next = TextProjection::from_snapshot(&document.snapshot());
        next.retain_table_layout_lock(&previous, Some(node));
        assert!(next.table_layout_is_locked(table));
        next.retain_table_layout_lock(&previous, Some(outside));
        assert!(!next.table_layout_is_locked(table));
        next.retain_table_layout_lock(&previous, None);
        assert!(!next.table_layout_is_locked(table));
        document
            .apply(document_core::EditCommand::SetTableColumnWidth {
                table_id: table,
                column: 0,
                width: 180.,
            })
            .unwrap();
        let mut resized = TextProjection::from_snapshot(&document.snapshot());
        resized.retain_table_layout_lock(&previous, Some(node));
        assert!(
            !resized.table_layout_is_locked(table),
            "explicit column commands must remain effective"
        );
    }

    #[test]
    fn maps_utf16_and_stable_positions_across_blocks() {
        let document = Document::from_markdown("# A🎉\n\nSecond").expect("document");
        let snapshot = document.snapshot();
        let projection = TextProjection::from_snapshot(&snapshot);
        assert_eq!(projection.text(), "A🎉\nSecond");
        let second = snapshot.blocks().get(1).expect("second block").id();
        let position = projection
            .position_at("A🎉\nSe".len(), Affinity::Downstream)
            .expect("mapped");
        assert_eq!(position.node_id, second);
        assert_eq!(position.text_offset, 2);
        assert_eq!(projection.byte_offset_for_utf16(3), "A🎉".len());
    }

    #[test]
    fn utf16_byte_round_trips_include_inter_block_separators() {
        let document = Document::from_markdown("A🎉\n\nBé\n\n第三").expect("document");
        let projection = TextProjection::from_snapshot(&document.snapshot());
        assert_eq!(projection.text(), "A🎉\nBé\n第三");

        for byte in projection
            .text()
            .char_indices()
            .map(|(byte, _)| byte)
            .chain(std::iter::once(projection.text().len()))
        {
            let utf16 = projection
                .utf16_offset_for_byte(byte)
                .expect("character boundary has a UTF-16 offset");
            assert_eq!(
                projection.byte_offset_for_utf16(utf16),
                byte,
                "round trip failed at byte {byte}"
            );
        }

        for separator in projection.text().match_indices('\n').map(|(byte, _)| byte) {
            let utf16 = projection
                .utf16_offset_for_byte(separator)
                .expect("separator offset");
            assert_eq!(projection.byte_offset_for_utf16(utf16), separator);
        }
    }

    #[test]
    fn one_text_node_refresh_shifts_only_following_projection_ranges() {
        assert_eq!(std::mem::size_of::<ProjectionSegment>(), 64);
        let mut document = Document::from_markdown("before\n\ntarget\n\nafter").expect("document");
        let target = document.snapshot().blocks().get(1).expect("target").id();
        let mut projection = TextProjection::from_snapshot(&document.snapshot());
        let detached = projection.clone();
        let before_range = projection.segments()[0].projection_range();
        let old_after_start = projection.segments()[2].projection_start();
        let old_after_local_start = projection.segments()[2].projection_local_start();
        let retained_after_chunk = projection.segments()[2].projection_start.clone();

        let result = document
            .apply(EditCommand::ReplaceText {
                node_id: target,
                range: 6..6,
                text: " 🎉".into(),
                selection_after: None,
                typing: true,
            })
            .expect("edit");
        let (_, delta) = projection
            .refresh_text_node(&result.snapshot, target)
            .expect("local refresh");

        assert_eq!(delta, " 🎉".len() as isize);
        assert_eq!(projection.text(), "before\ntarget 🎉\nafter");
        assert_eq!(projection.segments()[0].projection_range(), before_range);
        assert_eq!(
            projection.segments()[2].projection_start(),
            old_after_start + " 🎉".len()
        );
        assert!(Arc::ptr_eq(
            &retained_after_chunk,
            &projection.segments()[2].projection_start
        ));
        assert_eq!(
            projection.segments()[2].projection_local_start(),
            old_after_local_start + " 🎉".len(),
            "the bounded remainder of the edited chunk is rebased locally"
        );
        assert_eq!(
            detached.segments()[2].projection_start(),
            old_after_start,
            "cloned projections own independent chunk coordinates"
        );
        assert!(!Arc::ptr_eq(
            &projection.segments()[2].projection_start,
            &detached.segments()[2].projection_start
        ));
        assert_eq!(
            projection
                .block(target)
                .expect("refreshed block")
                .plain_text(),
            "target 🎉"
        );
    }

    #[test]
    fn large_suffix_coordinates_update_logarithmically_and_match_rebuild() {
        let source = (0..4_096)
            .map(|index| format!("paragraph {index} 🎉"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut document = Document::from_markdown(source).expect("document");
        let mut projection = TextProjection::from_snapshot(&document.snapshot());
        let old_tail_start = projection.segments().last().unwrap().projection_start();
        let old_tail_utf16 = projection.segments().last().unwrap().utf16_start();
        let logarithmic_bound =
            usize::BITS as usize - projection.coordinate_chunks.len().leading_zeros() as usize + 1;
        let targets = [512, 1_024, 1_536, 2_048, 2_560, 3_072]
            .map(|index| projection.segments()[index].node_id);
        let mut snapshot = document.snapshot();
        for target in targets {
            let target_len = snapshot
                .node(target)
                .and_then(BlockNode::text)
                .expect("target text")
                .len();
            let result = document
                .apply(EditCommand::ReplaceText {
                    node_id: target,
                    range: target_len..target_len,
                    text: "é".into(),
                    selection_after: None,
                    typing: true,
                })
                .expect("edit");
            projection
                .refresh_text_node(&result.snapshot, target)
                .expect("local refresh");
            let updated = projection.segment_for_node(target).unwrap();
            assert!(updated.projection_start.update_count() <= logarithmic_bound);
            snapshot = result.snapshot;
        }
        assert!(
            projection.segments()[3_072].projection_start.update_count() > 0,
            "distinct chunk-boundary edits must exercise the balanced suffix index"
        );
        assert_eq!(
            projection.segments().last().unwrap().projection_start(),
            old_tail_start + targets.len() * "é".len()
        );
        assert_eq!(
            projection.segments().last().unwrap().utf16_start(),
            old_tail_utf16 + targets.len() * "é".encode_utf16().count()
        );

        let rebuilt = TextProjection::from_snapshot(&snapshot);
        assert_eq!(projection.text(), rebuilt.text());
        for (retained, complete) in projection.segments().iter().zip(rebuilt.segments()) {
            assert_eq!(retained.projection_range(), complete.projection_range());
            assert_eq!(retained.utf16_range(), complete.utf16_range());
        }
        let tail = projection.segments().last().unwrap();
        assert_eq!(
            projection
                .position_at(tail.projection_start(), Affinity::Downstream)
                .unwrap()
                .node_id,
            tail.node_id
        );
    }

    #[test]
    fn chunk_boundary_edits_rebase_local_and_indexed_coordinates_exactly() {
        let source = (0..600)
            .map(|index| format!("paragraph {index}"))
            .collect::<Vec<_>>()
            .join("\n\n");
        let mut document = Document::from_markdown(source).expect("document");
        let mut projection = TextProjection::from_snapshot(&document.snapshot());
        let target = projection.segments()[250].node_id;
        let same_chunk = 251;
        let next_chunk = 256;
        assert!(Arc::ptr_eq(
            &projection.segments()[250].projection_start,
            &projection.segments()[same_chunk].projection_start
        ));
        assert!(!Arc::ptr_eq(
            &projection.segments()[250].projection_start,
            &projection.segments()[next_chunk].projection_start
        ));
        let same_handle = projection.segments()[same_chunk].projection_start.clone();
        let next_handle = projection.segments()[next_chunk].projection_start.clone();
        let same_local = projection.segments()[same_chunk].projection_local_start();
        let next_local = projection.segments()[next_chunk].projection_local_start();
        let same_start = projection.segments()[same_chunk].projection_start();
        let next_start = projection.segments()[next_chunk].projection_start();
        let target_len = projection.segments()[250].projection_len();

        let result = document
            .apply(EditCommand::ReplaceText {
                node_id: target,
                range: target_len..target_len,
                text: "é".into(),
                selection_after: None,
                typing: true,
            })
            .expect("edit");
        projection
            .refresh_text_node(&result.snapshot, target)
            .expect("local refresh");

        assert!(Arc::ptr_eq(
            &same_handle,
            &projection.segments()[same_chunk].projection_start
        ));
        assert_eq!(
            projection.segments()[same_chunk].projection_local_start(),
            same_local + "é".len()
        );
        assert_eq!(
            projection.segments()[same_chunk].projection_start(),
            same_start + "é".len()
        );
        assert!(Arc::ptr_eq(
            &next_handle,
            &projection.segments()[next_chunk].projection_start
        ));
        assert_eq!(
            projection.segments()[next_chunk].projection_local_start(),
            next_local
        );
        assert_eq!(
            projection.segments()[next_chunk].projection_start(),
            next_start + "é".len()
        );

        let rebuilt = TextProjection::from_snapshot(&result.snapshot);
        for (retained, complete) in projection.segments().iter().zip(rebuilt.segments()) {
            assert_eq!(retained.projection_range(), complete.projection_range());
            assert_eq!(retained.utf16_range(), complete.utf16_range());
        }
    }

    #[test]
    fn empty_blocks_keep_distinct_projection_boundaries() {
        let document = Document::from_markdown("#\n\nSecond").expect("document");
        let snapshot = document.snapshot();
        let projection = TextProjection::from_snapshot(&snapshot);
        assert_eq!(projection.segments().len(), 2);
        assert_eq!(projection.text(), "\nSecond");
        assert_eq!(projection.segments()[0].projection_range(), 0..0);
        assert_eq!(projection.segments()[1].projection_range(), 1..7);
        assert_eq!(
            projection
                .segment_for_range(&(0..0))
                .map(|segment| segment.node_id),
            projection.segments().first().map(|segment| segment.node_id)
        );
        assert_eq!(
            projection
                .segment_for_range(&(1..7))
                .map(|segment| segment.node_id),
            projection.segments().get(1).map(|segment| segment.node_id)
        );
    }

    #[test]
    fn alert_segments_retain_semantic_kind_and_edges() {
        let document =
            Document::from_markdown("> [!TIP]\n> First paragraph.\n>\n> Second paragraph.")
                .expect("alert document");
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let segments = projection
            .segments()
            .iter()
            .filter(|segment| segment.context.alert.is_some())
            .collect::<Vec<_>>();

        assert_eq!(segments.len(), 2);
        assert!(matches!(
            segments[0].context.alert,
            Some((_, AlertKind::Tip))
        ));
        assert!(segments[0].context.alert_first);
        assert!(!segments[0].context.alert_last);
        assert!(!segments[1].context.alert_first);
        assert!(segments[1].context.alert_last);
    }

    #[test]
    fn nested_list_ownership_is_shared_and_preserves_canonical_order() {
        let source =
            "- Parent\n\n  4. Child one\n  5. Child two\n     - Grandchild\n- Next parent\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let segments = projection.segments();
        assert_eq!(segments.len(), 5);
        assert_eq!(
            segments
                .iter()
                .map(|s| s.context.list_ancestors.len())
                .collect::<Vec<_>>(),
            [1, 2, 2, 3, 1]
        );
        assert!(Arc::ptr_eq(
            &segments[1].context.list_ancestors,
            &segments[2].context.list_ancestors
        ));
        assert!(Arc::ptr_eq(
            &segments[0].context.list_ancestors,
            &segments[4].context.list_ancestors
        ));
        for segment in segments {
            assert_eq!(
                segment.context.list_ancestors.len(),
                segment.context.list_depth
            );
            assert!(
                segment
                    .context
                    .list_ancestors
                    .iter()
                    .all(|id| matches!(projection.block(*id), Some(BlockNode::List(_))))
            );
            let position = projection
                .position_at(segment.projection_start(), Affinity::Downstream)
                .unwrap();
            assert_eq!(position.node_id, segment.node_id);
        }
        assert_eq!(segments[1].context.list_marker.as_deref(), Some("4."));
        assert_eq!(segments[2].context.list_marker.as_deref(), Some("5."));
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn preserved_placeholders_hit_test_to_the_nearest_editable_node() {
        let document = Document::from_markdown(concat!(
            "before\n\n",
            "<!-- tachyon-table:v1 {\"border\":\"Dotted\",\"widths\":[-1]} -->\n",
            "| a |\n| --- |\n| b |\n\n",
            "after"
        ))
        .expect("document");
        let snapshot = document.snapshot();
        let projection = TextProjection::from_snapshot(&snapshot);
        let placeholder = projection
            .segments()
            .iter()
            .find(|segment| segment.context.preserved_source)
            .expect("preserved placeholder");
        let hit = projection
            .position_at(placeholder.projection_start() + 1, Affinity::Downstream)
            .expect("nearest editable position");

        assert_ne!(hit.node_id, placeholder.node_id);
        assert!(
            snapshot
                .node(hit.node_id)
                .and_then(BlockNode::text)
                .is_some()
        );
    }
}
