use std::{ops::Range, sync::Arc};

use document_core::{
    Affinity, BlockNode, BlockSequence, DocumentPosition, DocumentSnapshot, ListKind, NodeId,
    RichText, Selection, TableBorder,
};
use rustc_hash::FxHashMap;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProjectionContext {
    pub list_depth: usize,
    pub list_marker: Option<String>,
    pub task_checked: Option<bool>,
    pub quote_depth: usize,
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

#[derive(Clone, Debug, Default)]
pub struct TextProjection {
    text: String,
    segments: Vec<ProjectionSegment>,
    by_node: FxHashMap<NodeId, usize>,
    blocks: FxHashMap<NodeId, Arc<BlockNode>>,
    image_segments: Vec<usize>,
    utf16_ranges: Vec<Range<usize>>,
}

impl TextProjection {
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
            segments: Vec::with_capacity(segment_capacity),
            by_node,
            blocks,
            image_segments: Vec::new(),
            utf16_ranges: Vec::with_capacity(segment_capacity),
        };
        append_sequence(
            snapshot.blocks(),
            &mut projection,
            &ProjectionContext::default(),
            None,
        );
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

fn append_sequence(
    blocks: &BlockSequence,
    projection: &mut TextProjection,
    context: &ProjectionContext,
    top_level_node_id: Option<NodeId>,
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
                    );
                    for segment in projection.segments.iter_mut().skip(start + 1) {
                        if segment.context.list_depth == list_context.list_depth {
                            segment.context.list_marker = None;
                            segment.context.task_checked = None;
                        }
                    }
                }
            }
            BlockNode::BlockQuote { blocks, .. } => {
                let mut quote_context = context.clone();
                quote_context.quote_depth += 1;
                append_sequence(blocks, projection, &quote_context, Some(top_level_node_id));
            }
            BlockNode::Alert { blocks, .. } | BlockNode::FootnoteDefinition { blocks, .. } => {
                append_sequence(blocks, projection, context, Some(top_level_node_id));
            }
            BlockNode::Table(table) => {
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
                        );
                    }
                }
            }
            BlockNode::PreservedSource {
                id, description, ..
            } => {
                let mut preserved_context = context.clone();
                preserved_context.preserved_source = true;
                projection.push_text(*id, top_level_node_id, description, preserved_context);
            }
            BlockNode::ThematicBreak { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use document_core::{Document, EditCommand};

    use super::*;

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
