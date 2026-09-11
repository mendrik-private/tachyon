use std::{fmt, num::NonZeroU64, sync::Arc};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::RichText;

#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct NodeId(NonZeroU64);

impl NodeId {
    #[must_use]
    pub const fn new_unchecked(value: u64) -> Self {
        match NonZeroU64::new(value) {
            Some(value) => Self(value),
            None => panic!("node IDs cannot be zero"),
        }
    }

    #[must_use]
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

impl fmt::Debug for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "NodeId({})", self.get())
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.get().fmt(formatter)
    }
}

#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct Revision(pub u64);

impl Revision {
    #[must_use]
    pub const fn next(self) -> Self {
        Self(self.0.saturating_add(1))
    }
}

impl fmt::Display for Revision {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Affinity {
    Upstream,
    #[default]
    Downstream,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentPosition {
    pub node_id: NodeId,
    /// UTF-8 byte offset into the node's text.
    pub text_offset: usize,
    pub affinity: Affinity,
}

impl DocumentPosition {
    #[must_use]
    pub const fn new(node_id: NodeId, text_offset: usize, affinity: Affinity) -> Self {
        Self {
            node_id,
            text_offset,
            affinity,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct TextSelection {
    pub anchor: DocumentPosition,
    pub head: DocumentPosition,
}

impl TextSelection {
    #[must_use]
    pub fn caret(position: DocumentPosition) -> Self {
        Self {
            anchor: position,
            head: position,
        }
    }

    #[must_use]
    pub fn is_caret(&self) -> bool {
        self.anchor == self.head
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RectangularSelection {
    pub table_id: NodeId,
    pub anchor_row: usize,
    pub anchor_column: usize,
    pub head_row: usize,
    pub head_column: usize,
}

impl RectangularSelection {
    #[must_use]
    pub fn normalized(
        self,
    ) -> (
        std::ops::RangeInclusive<usize>,
        std::ops::RangeInclusive<usize>,
    ) {
        let rows = self.anchor_row.min(self.head_row)..=self.anchor_row.max(self.head_row);
        let columns =
            self.anchor_column.min(self.head_column)..=self.anchor_column.max(self.head_column);
        (rows, columns)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Selection {
    Text(TextSelection),
    Table(RectangularSelection),
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct LinkTarget(pub String);

#[derive(Clone, Debug)]
pub struct Paragraph {
    pub id: NodeId,
    pub content: RichText,
}

#[derive(Clone, Debug)]
pub struct Heading {
    pub id: NodeId,
    pub level: u8,
    pub content: RichText,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ListKind {
    Unordered,
    Ordered { start: u64 },
    Task,
}

#[derive(Clone, Debug)]
pub struct ListItem {
    pub id: NodeId,
    pub checked: Option<bool>,
    pub blocks: BlockSequence,
}

#[derive(Clone, Debug)]
pub struct ListBlock {
    pub id: NodeId,
    pub kind: ListKind,
    pub tight: bool,
    pub items: Arc<[ListItem]>,
}

#[derive(Clone, Debug)]
pub struct CodeBlock {
    pub id: NodeId,
    pub language: Option<String>,
    pub content: RichText,
    /// Authored literal-block syntax; independent of presentation or language.
    pub syntax: CodeBlockSyntax,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum CodeBlockSyntax {
    #[default]
    Fenced,
    DisplayMath,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum AlertKind {
    Note,
    Tip,
    Important,
    Warning,
    Caution,
    Other(String),
}

#[derive(Clone, Debug)]
pub struct ImageLink {
    pub target: LinkTarget,
    pub title: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ImageNode {
    pub id: NodeId,
    pub source: String,
    pub alt: RichText,
    pub title: Option<String>,
    pub intrinsic_size: Option<(u32, u32)>,
    /// Authored enclosing link, separate from the image's source and alt text.
    pub link: Option<ImageLink>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnAlignment {
    #[default]
    None,
    Left,
    Center,
    Right,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ColumnSpec {
    pub alignment: ColumnAlignment,
    /// Logical pixels. `None` means content/default sizing.
    pub width: Option<f32>,
}

impl Default for ColumnSpec {
    fn default() -> Self {
        Self {
            alignment: ColumnAlignment::None,
            width: None,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableBorder {
    None,
    Dotted,
    /// One device pixel, independent of document zoom and display density.
    PhysicalPixel,
    /// One document pixel, scaled with text like the other grammar tokens.
    #[default]
    LogicalPixel,
}

#[derive(Clone, Debug)]
pub struct TableCell {
    pub id: NodeId,
    pub blocks: BlockSequence,
}

#[derive(Clone, Debug)]
pub struct TableRow {
    pub id: NodeId,
    pub cells: Arc<[TableCell]>,
}

#[derive(Clone, Debug)]
pub struct Table {
    pub id: NodeId,
    pub columns: Arc<[ColumnSpec]>,
    pub rows: Arc<[TableRow]>,
    pub header_rows: usize,
    pub border: TableBorder,
    /// Recognized metadata is removed from this list; unknown fields/comments
    /// stay byte-preserved here so future versions are not destructive.
    pub preserved_metadata: Arc<[String]>,
}

#[derive(Clone, Debug)]
pub enum BlockNode {
    Paragraph(Paragraph),
    Heading(Heading),
    List(ListBlock),
    /// Authored definition relationships, separate from list markers/tables.
    /// List children retain the source order of term and description groups;
    /// groups retain ordinary editable blocks and their canonical identities.
    Definition {
        id: NodeId,
        kind: DefinitionKind,
        blocks: BlockSequence,
    },
    BlockQuote {
        id: NodeId,
        blocks: BlockSequence,
    },
    CodeBlock(CodeBlock),
    Image(ImageNode),
    Table(Table),
    Alert {
        id: NodeId,
        kind: AlertKind,
        title: Option<RichText>,
        blocks: BlockSequence,
    },
    FootnoteDefinition {
        id: NodeId,
        label: String,
        blocks: BlockSequence,
    },
    ThematicBreak {
        id: NodeId,
    },
    /// Unsupported HTML/source remains round-trippable and visibly compact.
    PreservedSource {
        id: NodeId,
        source: Arc<str>,
        description: String,
    },
}

impl BlockNode {
    #[must_use]
    pub const fn id(&self) -> NodeId {
        match self {
            Self::Paragraph(node) => node.id,
            Self::Heading(node) => node.id,
            Self::List(node) => node.id,
            Self::BlockQuote { id, .. }
            | Self::Definition { id, .. }
            | Self::Alert { id, .. }
            | Self::FootnoteDefinition { id, .. }
            | Self::ThematicBreak { id }
            | Self::PreservedSource { id, .. } => *id,
            Self::CodeBlock(node) => node.id,
            Self::Image(node) => node.id,
            Self::Table(node) => node.id,
        }
    }

    #[must_use]
    pub fn text(&self) -> Option<&RichText> {
        match self {
            Self::Paragraph(node) => Some(&node.content),
            Self::Heading(node) => Some(&node.content),
            Self::CodeBlock(node) => Some(&node.content),
            Self::Image(node) => Some(&node.alt),
            _ => None,
        }
    }

    pub(crate) fn text_mut(&mut self) -> Option<&mut RichText> {
        match self {
            Self::Paragraph(node) => Some(&mut node.content),
            Self::Heading(node) => Some(&mut node.content),
            Self::CodeBlock(node) => Some(&mut node.content),
            Self::Image(node) => Some(&mut node.alt),
            _ => None,
        }
    }

    #[must_use]
    pub fn plain_text(&self) -> String {
        match self {
            Self::Paragraph(node) => node.content.as_string(),
            Self::Heading(node) => node.content.as_string(),
            Self::List(list) => list
                .items
                .iter()
                .flat_map(|item| item.blocks.iter())
                .map(|block| block.plain_text())
                .collect::<Vec<_>>()
                .join("\n"),
            Self::BlockQuote { blocks, .. }
            | Self::Alert { blocks, .. }
            | Self::Definition { blocks, .. }
            | Self::FootnoteDefinition { blocks, .. } => blocks
                .iter()
                .map(|block| block.plain_text())
                .collect::<Vec<_>>()
                .join("\n"),
            Self::CodeBlock(node) => node.content.as_string(),
            Self::Image(node) => node.alt.as_string(),
            Self::Table(table) => table
                .rows
                .iter()
                .map(|row| {
                    row.cells
                        .iter()
                        .map(|cell| {
                            cell.blocks
                                .iter()
                                .map(|block| block.plain_text())
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .collect::<Vec<_>>()
                        .join("\t")
                })
                .collect::<Vec<_>>()
                .join("\n"),
            Self::ThematicBreak { .. } => String::new(),
            Self::PreservedSource { description, .. } => description.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DefinitionKind {
    List,
    Term,
    Description,
}

const BLOCK_SEQUENCE_CHUNK_SIZE: usize = 256;

/// A persistent sequence whose snapshots share all untouched chunks.
///
/// Structural operations can still materialize a `Vec`, while the ordinary
/// text-edit path replaces one 256-entry chunk instead of cloning every block
/// in a large document.
#[derive(Clone, Debug, Default)]
pub struct BlockSequence {
    chunks: Arc<[Arc<[Arc<BlockNode>]>]>,
    node_index: BlockNodeIndex,
    len: usize,
}

#[derive(Clone, Debug, Default)]
enum BlockNodeIndex {
    #[default]
    Empty,
    Single(NodeId),
    Many(Arc<FxHashMap<NodeId, usize>>),
}

impl BlockSequence {
    #[must_use]
    pub fn new(blocks: Vec<Arc<BlockNode>>) -> Self {
        let len = blocks.len();
        let node_index = block_node_index(&blocks);
        let mut chunks = Vec::with_capacity(len.div_ceil(BLOCK_SEQUENCE_CHUNK_SIZE));
        let mut current = Vec::with_capacity(BLOCK_SEQUENCE_CHUNK_SIZE.min(len));
        for block in blocks {
            current.push(block);
            if current.len() == BLOCK_SEQUENCE_CHUNK_SIZE {
                chunks.push(Arc::from(std::mem::take(&mut current)));
                current = Vec::with_capacity(BLOCK_SEQUENCE_CHUNK_SIZE);
            }
        }
        if !current.is_empty() {
            chunks.push(Arc::from(current));
        }
        Self {
            chunks: chunks.into(),
            node_index,
            len,
        }
    }

    #[must_use]
    pub fn iter(&self) -> BlockSequenceIter<'_> {
        BlockSequenceIter {
            sequence: self,
            front: 0,
            back: self.len,
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    #[must_use]
    pub fn to_vec(&self) -> Vec<Arc<BlockNode>> {
        self.iter().cloned().collect()
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Arc<BlockNode>> {
        if index >= self.len {
            return None;
        }
        self.chunks
            .get(index / BLOCK_SEQUENCE_CHUNK_SIZE)?
            .get(index % BLOCK_SEQUENCE_CHUNK_SIZE)
    }

    #[must_use]
    pub(crate) fn replacing(&self, index: usize, block: Arc<BlockNode>) -> Option<Self> {
        if index >= self.len {
            return None;
        }
        let chunk_index = index / BLOCK_SEQUENCE_CHUNK_SIZE;
        let within_chunk = index % BLOCK_SEQUENCE_CHUNK_SIZE;
        let previous = self.get(index)?;
        debug_assert_eq!(previous.id(), block.id());
        let mut previous_nodes = FxHashMap::default();
        let mut replacement_nodes = FxHashMap::default();
        index_block_and_descendants(previous, index, &mut previous_nodes);
        index_block_and_descendants(&block, index, &mut replacement_nodes);
        let node_index = if previous_nodes.len() == replacement_nodes.len()
            && previous_nodes
                .keys()
                .all(|node_id| replacement_nodes.contains_key(node_id))
        {
            self.node_index.clone()
        } else if self.len == 1 {
            block_node_index(std::slice::from_ref(&block))
        } else {
            let BlockNodeIndex::Many(current) = &self.node_index else {
                unreachable!("multi-block sequences always use a node map")
            };
            let mut updated = (**current).clone();
            for node_id in previous_nodes.keys() {
                updated.remove(node_id);
            }
            updated.extend(replacement_nodes);
            BlockNodeIndex::Many(Arc::new(updated))
        };
        let mut chunks = self.chunks.to_vec();
        let mut changed_chunk = chunks.get(chunk_index)?.to_vec();
        changed_chunk[within_chunk] = block;
        chunks[chunk_index] = changed_chunk.into();
        Some(Self {
            chunks: chunks.into(),
            node_index,
            len: self.len,
        })
    }

    #[must_use]
    pub(crate) fn top_index_containing(&self, node_id: NodeId) -> Option<usize> {
        match &self.node_index {
            BlockNodeIndex::Empty => None,
            BlockNodeIndex::Single(id) => (*id == node_id).then_some(0),
            BlockNodeIndex::Many(index) => index.get(&node_id).copied(),
        }
    }

    #[must_use]
    pub(crate) fn top_index_of(&self, node_id: NodeId) -> Option<usize> {
        let index = self.top_index_containing(node_id)?;
        (self.get(index)?.id() == node_id).then_some(index)
    }

    #[must_use]
    /// Whether a canonical node belongs to this sequence or any descendant.
    /// Uses the sequence's persistent descendant index, not a tree scan.
    pub fn contains_node(&self, node_id: NodeId) -> bool {
        self.top_index_containing(node_id).is_some()
    }
}

fn block_node_index(blocks: &[Arc<BlockNode>]) -> BlockNodeIndex {
    let Some(first) = blocks.first() else {
        return BlockNodeIndex::Empty;
    };
    if blocks.len() == 1 && block_has_no_indexed_descendants(first) {
        return BlockNodeIndex::Single(first.id());
    }
    let mut index = FxHashMap::default();
    index.reserve(blocks.len().saturating_mul(3));
    for (top_index, block) in blocks.iter().enumerate() {
        index_block_and_descendants(block, top_index, &mut index);
    }
    BlockNodeIndex::Many(Arc::new(index))
}

fn block_has_no_indexed_descendants(block: &BlockNode) -> bool {
    matches!(
        block,
        BlockNode::Paragraph(_)
            | BlockNode::Heading(_)
            | BlockNode::CodeBlock(_)
            | BlockNode::Image(_)
            | BlockNode::ThematicBreak { .. }
            | BlockNode::PreservedSource { .. }
    )
}

fn index_block_and_descendants(
    block: &BlockNode,
    top_index: usize,
    output: &mut FxHashMap<NodeId, usize>,
) {
    output.insert(block.id(), top_index);
    match block {
        BlockNode::List(list) => {
            for item in list.items.iter() {
                output.insert(item.id, top_index);
                for child in &item.blocks {
                    index_block_and_descendants(child, top_index, output);
                }
            }
        }
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::Definition { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => {
            for child in blocks {
                index_block_and_descendants(child, top_index, output);
            }
        }
        BlockNode::Table(table) => {
            for row in table.rows.iter() {
                output.insert(row.id, top_index);
                for cell in row.cells.iter() {
                    output.insert(cell.id, top_index);
                    for child in &cell.blocks {
                        index_block_and_descendants(child, top_index, output);
                    }
                }
            }
        }
        BlockNode::Paragraph(_)
        | BlockNode::Heading(_)
        | BlockNode::CodeBlock(_)
        | BlockNode::Image(_)
        | BlockNode::ThematicBreak { .. }
        | BlockNode::PreservedSource { .. } => {}
    }
}

#[derive(Clone, Debug)]
pub struct BlockSequenceIter<'a> {
    sequence: &'a BlockSequence,
    front: usize,
    back: usize,
}

impl<'a> Iterator for BlockSequenceIter<'a> {
    type Item = &'a Arc<BlockNode>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        let index = self.front;
        self.front += 1;
        self.sequence.get(index)
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let len = self.back - self.front;
        (len, Some(len))
    }
}

impl DoubleEndedIterator for BlockSequenceIter<'_> {
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.front == self.back {
            return None;
        }
        self.back -= 1;
        self.sequence.get(self.back)
    }
}

impl ExactSizeIterator for BlockSequenceIter<'_> {}
impl std::iter::FusedIterator for BlockSequenceIter<'_> {}

impl From<Vec<Arc<BlockNode>>> for BlockSequence {
    fn from(value: Vec<Arc<BlockNode>>) -> Self {
        Self::new(value)
    }
}

impl<'a> IntoIterator for &'a BlockSequence {
    type Item = &'a Arc<BlockNode>;
    type IntoIter = BlockSequenceIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sequence(len: usize) -> BlockSequence {
        BlockSequence::new(
            (1..=len)
                .map(|id| {
                    Arc::new(BlockNode::ThematicBreak {
                        id: NodeId::new_unchecked(id as u64),
                    })
                })
                .collect(),
        )
    }

    #[test]
    fn chunked_sequence_iterates_both_directions_across_boundaries() {
        let blocks = sequence(BLOCK_SEQUENCE_CHUNK_SIZE * 2 + 7);
        assert_eq!(blocks.iter().len(), BLOCK_SEQUENCE_CHUNK_SIZE * 2 + 7);
        assert_eq!(blocks.iter().next().map(|block| block.id().get()), Some(1));
        assert_eq!(
            blocks.iter().next_back().map(|block| block.id().get()),
            Some((BLOCK_SEQUENCE_CHUNK_SIZE * 2 + 7) as u64)
        );
        assert_eq!(
            blocks
                .get(BLOCK_SEQUENCE_CHUNK_SIZE)
                .map(|block| block.id().get()),
            Some((BLOCK_SEQUENCE_CHUNK_SIZE + 1) as u64)
        );
        assert!(blocks.get(blocks.len()).is_none());
    }

    #[test]
    fn singleton_leaf_sequence_uses_the_inline_node_index() {
        let blocks = sequence(1);
        assert!(matches!(blocks.node_index, BlockNodeIndex::Single(_)));
        assert_eq!(
            blocks.top_index_containing(NodeId::new_unchecked(1)),
            Some(0)
        );
        assert!(!blocks.contains_node(NodeId::new_unchecked(2)));
    }

    #[test]
    fn replacing_one_block_shares_every_untouched_chunk() {
        let blocks = sequence(BLOCK_SEQUENCE_CHUNK_SIZE * 3);
        let replacement_id = NodeId::new_unchecked((BLOCK_SEQUENCE_CHUNK_SIZE + 11) as u64);
        let replacement = Arc::new(BlockNode::Paragraph(Paragraph {
            id: replacement_id,
            content: RichText::new("changed"),
        }));
        let changed = blocks
            .replacing(BLOCK_SEQUENCE_CHUNK_SIZE + 10, replacement)
            .expect("in bounds");

        assert!(Arc::ptr_eq(&blocks.chunks[0], &changed.chunks[0]));
        assert!(!Arc::ptr_eq(&blocks.chunks[1], &changed.chunks[1]));
        assert!(Arc::ptr_eq(&blocks.chunks[2], &changed.chunks[2]));
        assert!(matches!(
            changed.get(BLOCK_SEQUENCE_CHUNK_SIZE + 10).map(Arc::as_ref),
            Some(BlockNode::Paragraph(_))
        ));
        assert!(matches!(
            blocks.get(BLOCK_SEQUENCE_CHUNK_SIZE + 10).map(Arc::as_ref),
            Some(BlockNode::ThematicBreak { .. })
        ));
    }
}
