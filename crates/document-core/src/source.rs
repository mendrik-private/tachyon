use std::{ops::Range, path::PathBuf, sync::Arc, time::SystemTime};

use rustc_hash::FxHashMap;
use serde::{Deserialize, Serialize};

use crate::{BlockNode, BlockSequence, NodeId, Revision};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum LineEnding {
    #[default]
    Lf,
    CrLf,
}

impl LineEnding {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Lf => "\n",
            Self::CrLf => "\r\n",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceUnit {
    pub(crate) prefix: Range<usize>,
    pub(crate) source: Range<usize>,
}

#[derive(Clone, Debug)]
pub(crate) struct ReferenceRecord {
    pub source: Range<usize>,
    pub standalone: String,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct NestedSourceSpans {
    pub list_items: FxHashMap<NodeId, Range<usize>>,
    pub note_paragraphs: FxHashMap<NodeId, Range<usize>>,
    pub table_cells: FxHashMap<NodeId, Range<usize>>,
    pub leaf_blocks: FxHashMap<NodeId, Range<usize>>,
    pub html_paragraphs: FxHashMap<NodeId, Range<usize>>,
    pub owned_roots: FxHashMap<NodeId, Vec<NodeId>>,
    pub references: FxHashMap<NodeId, Vec<ReferenceRecord>>,
}

#[derive(Clone, Debug)]
pub struct SourceSpine {
    original: Arc<str>,
    units: Arc<FxHashMap<NodeId, SourceUnit>>,
    order: Arc<[NodeId]>,
    tail_start: usize,
    line_ending: LineEnding,
    original_blocks: Arc<FxHashMap<NodeId, Arc<BlockNode>>>,
    nested: Arc<NestedSourceSpans>,
}

impl SourceSpine {
    #[must_use]
    pub fn empty() -> Self {
        Self {
            original: Arc::from(""),
            units: Arc::new(FxHashMap::default()),
            order: Arc::from([]),
            tail_start: 0,
            line_ending: LineEnding::Lf,
            original_blocks: Arc::default(),
            nested: Arc::default(),
        }
    }

    #[must_use]
    pub(crate) fn new(
        original: Arc<str>,
        units: FxHashMap<NodeId, SourceUnit>,
        order: Vec<NodeId>,
        tail_start: usize,
        blocks: &BlockSequence,
        nested: NestedSourceSpans,
    ) -> Self {
        let line_ending = if original.contains("\r\n") {
            LineEnding::CrLf
        } else {
            LineEnding::Lf
        };
        Self {
            original,
            units: Arc::new(units),
            order: order.into(),
            tail_start,
            line_ending,
            original_blocks: Arc::new(blocks.iter().map(|b| (b.id(), b.clone())).collect()),
            nested: Arc::new(nested),
        }
    }

    pub(crate) fn remap(&self, ids: &FxHashMap<NodeId, NodeId>, blocks: &BlockSequence) -> Self {
        let spans = |source: &FxHashMap<NodeId, Range<usize>>| {
            source
                .iter()
                .map(|(id, span)| (ids[id], span.clone()))
                .collect()
        };
        Self::new(
            self.original.clone(),
            self.units
                .iter()
                .map(|(id, unit)| (ids[id], unit.clone()))
                .collect(),
            self.order.iter().map(|id| ids[id]).collect(),
            self.tail_start,
            blocks,
            NestedSourceSpans {
                list_items: spans(&self.nested.list_items),
                note_paragraphs: spans(&self.nested.note_paragraphs),
                table_cells: spans(&self.nested.table_cells),
                leaf_blocks: spans(&self.nested.leaf_blocks),
                html_paragraphs: spans(&self.nested.html_paragraphs),
                references: self
                    .nested
                    .references
                    .iter()
                    .map(|(owner, records)| (ids[owner], records.clone()))
                    .collect(),
                owned_roots: self
                    .nested
                    .owned_roots
                    .iter()
                    .map(|(owner, children)| {
                        (ids[owner], children.iter().map(|id| ids[id]).collect())
                    })
                    .collect(),
            },
        )
    }

    #[must_use]
    pub fn original(&self) -> &str {
        &self.original
    }

    #[must_use]
    pub fn line_ending(&self) -> LineEnding {
        self.line_ending
    }

    #[must_use]
    pub(crate) fn unit(&self, node: NodeId) -> Option<&SourceUnit> {
        self.units.get(&node)
    }

    pub(crate) fn with_references(mut self, records: Vec<(Range<usize>, String)>) -> Self {
        let mut references = FxHashMap::<NodeId, Vec<ReferenceRecord>>::default();
        for (range, text) in records {
            let index = self
                .order
                .partition_point(|id| self.units[id].source.end < range.start);
            if let Some(owner) = self.order.get(index)
                && self.units[owner].source.start <= range.start
            {
                references.entry(*owner).or_default().push(ReferenceRecord {
                    source: range,
                    standalone: text,
                });
            }
        }
        Arc::make_mut(&mut self.nested).references = references;
        self
    }

    pub(crate) fn references(&self, owner: NodeId) -> &[ReferenceRecord] {
        self.nested
            .references
            .get(&owner)
            .map_or(&[], Vec::as_slice)
    }

    /// A source-local text patch cannot consume a non-rendered definition.
    pub(crate) fn reference_intersects(&self, range: &Range<usize>) -> bool {
        let first = self
            .order
            .partition_point(|id| self.units[id].source.end <= range.start);
        self.order[first..]
            .iter()
            .take_while(|id| self.units[id].source.start < range.end)
            .any(|id| {
                self.references(*id).iter().any(|record| {
                    record.source.start < range.end && range.start < record.source.end
                })
            })
    }

    pub(crate) fn owned_roots(&self) -> &FxHashMap<NodeId, Vec<NodeId>> {
        &self.nested.owned_roots
    }

    pub(crate) fn original_block(&self, node: NodeId) -> Option<&Arc<BlockNode>> {
        self.original_blocks.get(&node)
    }

    pub(crate) fn list_item(&self, node: NodeId) -> Option<Range<usize>> {
        self.nested.list_items.get(&node).cloned()
    }

    pub(crate) fn note_paragraph(&self, node: NodeId) -> Option<Range<usize>> {
        self.nested.note_paragraphs.get(&node).cloned()
    }

    pub(crate) fn table_cell(&self, node: NodeId) -> Option<Range<usize>> {
        self.nested.table_cells.get(&node).cloned()
    }

    pub(crate) fn leaf_block(&self, node: NodeId) -> Option<Range<usize>> {
        self.nested.leaf_blocks.get(&node).cloned()
    }

    pub(crate) fn html_paragraph(&self, node: NodeId) -> Option<Range<usize>> {
        self.nested.html_paragraphs.get(&node).cloned()
    }

    #[must_use]
    pub(crate) fn order(&self) -> &[NodeId] {
        &self.order
    }

    #[must_use]
    pub(crate) fn tail(&self) -> &str {
        &self.original[self.tail_start..]
    }

    #[must_use]
    pub(crate) fn slice(&self, range: Range<usize>) -> &str {
        &self.original[range]
    }
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SourceIdentity {
    pub path: PathBuf,
    pub length: u64,
    pub modified: Option<SystemTime>,
    pub content_hash: [u8; 32],
    #[cfg(unix)]
    pub device: u64,
    #[cfg(unix)]
    pub inode: u64,
}

#[derive(Clone, Debug)]
pub struct SaveSnapshot {
    pub revision: Revision,
    pub bytes: Arc<[u8]>,
    pub expected_identity: Option<SourceIdentity>,
}
