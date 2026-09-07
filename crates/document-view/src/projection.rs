use std::{ops::Range, sync::Arc};

use document_core::{
    Affinity, AlertKind, BlockNode, BlockSequence, DocumentPosition, DocumentSnapshot, ListKind,
    NodeId, RichText, Selection, TableBorder,
};
use rustc_hash::FxHashMap;

/// Native document-unit constraints, never persisted into Markdown.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct TableMeasurements {
    pub minimum: Vec<f32>,
    pub preferred: Vec<f32>,
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
    pub fn fit(&self, available: f32) -> Vec<f32> {
        let minimum = self.minimum.iter().sum::<f32>();
        let preferred = self.preferred.iter().sum::<f32>();
        if available <= minimum {
            return self.minimum.clone();
        }
        if available >= preferred {
            return self.preferred.clone();
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
    pub list_depth: usize,
    pub ordered_list_depth: usize,
    pub list_marker: Option<String>,
    pub task_checked: Option<bool>,
    pub quote_depth: usize,
    pub quote: Option<NodeId>,
    pub quote_first: bool,
    pub quote_last: bool,
    pub alert: Option<(NodeId, AlertKind)>,
    pub alert_first: bool,
    pub alert_last: bool,
    pub table_cell: Option<(NodeId, usize, usize)>,
    pub table_header: bool,
    pub table_border: Option<TableBorder>,
    pub image_source: Option<String>,
    pub preserved_source: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectionSegment {
    pub node_id: NodeId,
    pub top_level_node_id: NodeId,
    pub projection_range: Range<usize>,
    pub node_range: Range<usize>,
    pub context: ProjectionContext,
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

#[derive(Clone, Debug, Default)]
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
    utf16_ranges: Vec<Range<usize>>,
    table_widths: FxHashMap<NodeId, Vec<f32>>,
    table_contexts: FxHashMap<NodeId, TableContext>,
    container_cells: FxHashMap<NodeId, (NodeId, usize, usize)>,
    container_edges: FxHashMap<NodeId, ContainerEdges>,
    measured_tables: FxHashMap<NodeId, TableMeasurements>,
    pub(crate) table_layout_lock: Option<TableLayoutLock>,
    pub(crate) math_edit_node: Option<NodeId>,
    pub(crate) html_disclosures: Arc<FxHashMap<NodeId, crate::html::DisclosureState>>,
    pub(crate) html_image_references: FxHashMap<NodeId, Vec<document_core::InertHtmlImage>>,
    pub(crate) html_images: Arc<FxHashMap<NodeId, crate::html::images::BoundImages>>,
}

/// Exact inputs to renderer geometry, without copying the projected text or
/// every leaf's metadata. Immutable root allocation identity includes all
/// descendants and their order; view-only constraints are compared separately.
pub(crate) struct ProjectionGeometryKey {
    roots: Vec<Arc<BlockNode>>,
    measured_tables: FxHashMap<NodeId, TableMeasurements>,
    table_layout_lock: Option<TableLayoutLock>,
    math_edit_node: Option<NodeId>,
    html_disclosures: Arc<FxHashMap<NodeId, crate::html::DisclosureState>>,
    html_images: Arc<FxHashMap<NodeId, crate::html::images::BoundImages>>,
}

impl ProjectionGeometryKey {
    pub(crate) fn matches(&self, projection: &TextProjection) -> bool {
        self.math_edit_node == projection.math_edit_node
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
    pub(crate) fn geometry_key(&self) -> ProjectionGeometryKey {
        ProjectionGeometryKey {
            roots: self
                .roots
                .iter()
                .map(|id| self.blocks[id].clone())
                .collect(),
            measured_tables: self.measured_tables.clone(),
            table_layout_lock: self.table_layout_lock.clone(),
            math_edit_node: self.math_edit_node,
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
            utf16_ranges: Vec::with_capacity(segment_capacity),
            table_widths: FxHashMap::default(),
            table_contexts: FxHashMap::default(),
            container_cells: FxHashMap::default(),
            container_edges: FxHashMap::default(),
            measured_tables: FxHashMap::default(),
            table_layout_lock: None,
            html_disclosures: Arc::default(),
            html_image_references: FxHashMap::default(),
            html_images: Arc::default(),
            math_edit_node: match snapshot.selection() {
                Selection::Text(selection) => Some(selection.head.node_id),
                _ => None,
            },
        };
        append_sequence(
            snapshot.blocks(),
            &mut projection,
            &ProjectionContext::default(),
            None,
            &mut Vec::new(),
        );
        projection.math_edit_node = projection.math_edit_node.filter(|id| {
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
        self.table_layout_lock = previous.table_layout_lock.as_ref().and_then(|lock| {
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
            return canvas.min(measured.preferred.iter().sum::<f32>());
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

    #[must_use]
    pub fn segment_for_node(&self, node_id: NodeId) -> Option<&ProjectionSegment> {
        self.by_node
            .get(&node_id)
            .and_then(|index| self.segments.get(*index))
    }

    #[must_use]
    pub fn utf16_len(&self) -> usize {
        self.utf16_ranges.last().map_or(0, |range| range.end)
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
        let block = snapshot.node(node_id)?.clone();
        let table = match self.segments[index].context.table_cell {
            Some((id, _, _)) => Some((id, snapshot.node(id)?.clone())),
            None => None,
        };
        let rich_text = block.text()?;
        let mut replacement = String::with_capacity(rich_text.len());
        rich_text.append_to(&mut replacement);
        let old_range = self.segments.get(index)?.projection_range.clone();
        let old_utf16_range = self.utf16_ranges.get(index)?.clone();
        let replacement_utf16_len = replacement.encode_utf16().count();
        let delta =
            isize::try_from(replacement.len()).ok()? - isize::try_from(old_range.len()).ok()?;
        let utf16_delta = isize::try_from(replacement_utf16_len).ok()?
            - isize::try_from(old_utf16_range.len()).ok()?;
        self.text.replace_range(old_range.clone(), &replacement);

        let segment = self.segments.get_mut(index)?;
        segment.projection_range.end = segment.projection_range.start + replacement.len();
        segment.node_range = 0..replacement.len();
        self.utf16_ranges[index].end = self.utf16_ranges[index].start + replacement_utf16_len;
        for following in &mut self.segments[index + 1..] {
            following.projection_range.start =
                following.projection_range.start.checked_add_signed(delta)?;
            following.projection_range.end =
                following.projection_range.end.checked_add_signed(delta)?;
        }
        for following in &mut self.utf16_ranges[index + 1..] {
            following.start = following.start.checked_add_signed(utf16_delta)?;
            following.end = following.end.checked_add_signed(utf16_delta)?;
        }
        self.blocks.insert(node_id, Arc::new(block));
        if let Some((id, table)) = table {
            self.blocks.insert(id, Arc::new(table));
            // The lock retains the visible widths, but these measurements no
            // longer describe the canonical content. Do not reuse them on blur.
            self.measured_tables.remove(&id);
        }
        self.revisions
            .insert(node_id, snapshot.node_revision(node_id)?);
        Some((old_range, delta))
    }

    /// Finds the segment containing a projected visual-line range in
    /// logarithmic time. Segment starts are monotonic, including empty nodes.
    #[must_use]
    pub fn segment_for_range(&self, range: &Range<usize>) -> Option<&ProjectionSegment> {
        let split = self
            .segments
            .partition_point(|segment| segment.projection_range.start <= range.start);
        let candidate = self.segments.get(split.checked_sub(1)?)?;
        (candidate.projection_range.contains(&range.start)
            || candidate.projection_range == *range
            || candidate.projection_range.end == range.end)
            .then_some(candidate)
    }

    #[must_use]
    pub fn position_at(&self, offset: usize, affinity: Affinity) -> Option<DocumentPosition> {
        if offset > self.text.len() || !self.text.is_char_boundary(offset) {
            return None;
        }
        let segment = self.segments.iter().find(|segment| {
            !segment.context.preserved_source
                && (segment.projection_range.contains(&offset)
                    || (offset == self.text.len()
                        && segment.projection_range.end == self.text.len())
                    || (affinity == Affinity::Upstream && segment.projection_range.end == offset)
                    || (affinity == Affinity::Downstream
                        && segment.projection_range.start == offset))
        });
        if let Some(segment) = segment {
            let local = offset
                .saturating_sub(segment.projection_range.start)
                .min(segment.node_range.len());
            return Some(DocumentPosition::new(
                segment.node_id,
                segment.node_range.start + local,
                affinity,
            ));
        }

        let segment = self
            .segments
            .iter()
            .filter(|segment| !segment.context.preserved_source)
            .min_by_key(|segment| {
                if offset < segment.projection_range.start {
                    segment.projection_range.start - offset
                } else {
                    offset.saturating_sub(segment.projection_range.end)
                }
            })?;
        let text_offset = if offset <= segment.projection_range.start {
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
        Some(segment.projection_range.start + position.text_offset - segment.node_range.start)
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
            .partition_point(|segment| segment.projection_range.start <= byte_offset);
        let index = split.checked_sub(1)?;
        let segment = self.segments.get(index)?;
        let utf16_range = self.utf16_ranges.get(index)?;
        if byte_offset <= segment.projection_range.end {
            let local = &self.text[segment.projection_range.start..byte_offset];
            Some(utf16_range.start + local.encode_utf16().count())
        } else {
            Some(utf16_range.end + byte_offset - segment.projection_range.end)
        }
    }

    #[must_use]
    pub fn byte_offset_for_utf16(&self, utf16_offset: usize) -> usize {
        let Some(index) = self
            .utf16_ranges
            .partition_point(|range| range.start <= utf16_offset)
            .checked_sub(1)
        else {
            return 0;
        };
        let Some(utf16_range) = self.utf16_ranges.get(index) else {
            return self.text.len();
        };
        let Some(segment) = self.segments.get(index) else {
            return self.text.len();
        };
        if utf16_offset > utf16_range.end {
            return (segment.projection_range.end + utf16_offset - utf16_range.end)
                .min(self.text.len());
        }
        let target = utf16_offset.saturating_sub(utf16_range.start);
        let text = &self.text[segment.projection_range.clone()];
        let mut units = 0;
        for (byte, character) in text.char_indices() {
            if units >= target {
                return segment.projection_range.start + byte;
            }
            let next = units + character.len_utf16();
            if next > target {
                return segment.projection_range.start + byte;
            }
            units = next;
        }
        segment.projection_range.end
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
        let utf16_start = self.utf16_ranges.last().map_or(0, |range| range.end + 1);
        self.text.push_str(value);
        let index = self.segments.len();
        self.segments.push(ProjectionSegment {
            node_id,
            top_level_node_id,
            projection_range: start..self.text.len(),
            node_range: 0..value.len(),
            context,
        });
        if self.segments[index].context.image_source.is_some() {
            self.image_segments.push(index);
        }
        self.utf16_ranges
            .push(utf16_start..utf16_start + value.encode_utf16().count());
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
        let utf16_start = self.utf16_ranges.last().map_or(0, |range| range.end + 1);
        self.text.reserve(value.len());
        let previous_len = self.text.len();
        value.append_to(&mut self.text);
        let utf16_len = self.text[previous_len..].encode_utf16().count();
        let index = self.segments.len();
        self.segments.push(ProjectionSegment {
            node_id,
            top_level_node_id,
            projection_range: start..self.text.len(),
            node_range: 0..value.len(),
            context,
        });
        if self.segments[index].context.image_source.is_some() {
            self.image_segments.push(index);
        }
        self.utf16_ranges.push(utf16_start..utf16_start + utf16_len);
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

fn append_sequence(
    blocks: &BlockSequence,
    projection: &mut TextProjection,
    context: &ProjectionContext,
    top_level_node_id: Option<NodeId>,
    containers: &mut Vec<NodeId>,
) {
    for block in blocks {
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
                for (index, item) in list.items.iter().enumerate() {
                    let start = projection.segments.len();
                    let mut list_context = context.clone();
                    list_context.list_depth += 1;
                    list_context.ordered_list_depth +=
                        usize::from(matches!(list.kind, ListKind::Ordered { .. }));
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
            BlockNode::FootnoteDefinition { blocks, .. } => {
                append_sequence(
                    blocks,
                    projection,
                    context,
                    Some(top_level_node_id),
                    containers,
                );
            }
            BlockNode::Table(table) => {
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

#[cfg(test)]
mod tests {
    use document_core::{Document, EditCommand};

    use super::*;

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
        let mut document = Document::from_markdown("before\n\ntarget\n\nafter").expect("document");
        let target = document.snapshot().blocks().get(1).expect("target").id();
        let mut projection = TextProjection::from_snapshot(&document.snapshot());
        let before_range = projection.segments()[0].projection_range.clone();
        let old_after_start = projection.segments()[2].projection_range.start;

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
        assert_eq!(projection.segments()[0].projection_range, before_range);
        assert_eq!(
            projection.segments()[2].projection_range.start,
            old_after_start + " 🎉".len()
        );
        assert_eq!(
            projection
                .block(target)
                .expect("refreshed block")
                .plain_text(),
            "target 🎉"
        );
    }

    #[test]
    fn empty_blocks_keep_distinct_projection_boundaries() {
        let document = Document::from_markdown("#\n\nSecond").expect("document");
        let snapshot = document.snapshot();
        let projection = TextProjection::from_snapshot(&snapshot);
        assert_eq!(projection.segments().len(), 2);
        assert_eq!(projection.text(), "\nSecond");
        assert_eq!(projection.segments()[0].projection_range, 0..0);
        assert_eq!(projection.segments()[1].projection_range, 1..7);
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
    fn preserved_placeholders_hit_test_to_the_nearest_editable_node() {
        let document = Document::from_markdown(concat!(
            "before\n\n",
            "<!-- mineral-table:v1 {\"border\":\"Dotted\",\"widths\":[-1]} -->\n",
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
            .position_at(placeholder.projection_range.start + 1, Affinity::Downstream)
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
