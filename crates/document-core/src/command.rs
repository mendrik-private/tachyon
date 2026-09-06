use std::{collections::BTreeSet, sync::Arc};

use serde::{Deserialize, Serialize};

use crate::{
    BlockNode, ColumnAlignment, DocumentPosition, DocumentSnapshot, InlineStyle, NodeId, Revision,
    Selection, TableBorder, TextRange,
};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineFormat {
    Bold,
    Italic,
    Strikethrough,
    Code,
    Link(String),
}

impl InlineFormat {
    #[must_use]
    pub(crate) fn as_style(&self) -> InlineStyle {
        match self {
            Self::Bold => InlineStyle::Bold,
            Self::Italic => InlineStyle::Italic,
            Self::Strikethrough => InlineStyle::Strikethrough,
            Self::Code => InlineStyle::Code,
            Self::Link(target) => InlineStyle::Link(crate::LinkTarget(target.clone())),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlockStyle {
    Paragraph,
    Heading(u8),
    BlockQuote,
    CodeBlock,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum InsertBlockKind {
    Paragraph,
    Heading(u8),
    UnorderedList,
    TaskList,
    BlockQuote,
    CodeBlock,
    Image,
    Table,
    ThematicBreak,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FormatState {
    Off,
    On,
    Mixed,
}

#[derive(Clone, Debug)]
pub enum EditCommand {
    SetSelection(Selection),
    ReplaceText {
        node_id: NodeId,
        range: TextRange,
        text: String,
        selection_after: Option<Selection>,
        typing: bool,
    },
    ReplaceSelection {
        text: String,
        typing: bool,
    },
    PasteMarkdown {
        markdown: String,
    },
    SplitSelection,
    ToggleInline {
        node_id: NodeId,
        range: TextRange,
        format: InlineFormat,
    },
    ToggleInlineSelection {
        format: InlineFormat,
    },
    SetLinkSelection {
        target: Option<String>,
    },
    SetBlockStyle {
        node_id: NodeId,
        style: BlockStyle,
    },
    SetImageAttributes {
        image_id: NodeId,
        source: String,
        alt: String,
    },
    InsertBlock {
        index: usize,
        block: Arc<BlockNode>,
    },
    InsertBlockAfterSelection {
        kind: InsertBlockKind,
    },
    DeleteBlock {
        node_id: NodeId,
    },
    InsertTable {
        index: usize,
    },
    InsertTableRow {
        table_id: NodeId,
        index: usize,
    },
    DeleteTableRow {
        table_id: NodeId,
        index: usize,
    },
    MoveTableRow {
        table_id: NodeId,
        from: usize,
        to: usize,
    },
    InsertTableColumn {
        table_id: NodeId,
        index: usize,
    },
    DeleteTableColumn {
        table_id: NodeId,
        index: usize,
    },
    MoveTableColumn {
        table_id: NodeId,
        from: usize,
        to: usize,
    },
    SetTableColumnAlignment {
        table_id: NodeId,
        column: usize,
        alignment: ColumnAlignment,
    },
    SetTableColumnWidth {
        table_id: NodeId,
        column: usize,
        width: f32,
    },
    SetTableBorder {
        table_id: NodeId,
        border: TableBorder,
    },
    PasteTsv {
        table_id: NodeId,
        row: usize,
        column: usize,
        text: String,
    },
    ToggleTask {
        item_id: NodeId,
    },
    IndentListItem {
        item_id: NodeId,
    },
    OutdentListItem {
        item_id: NodeId,
    },
}

impl EditCommand {
    #[must_use]
    pub(crate) fn is_selection_only(&self) -> bool {
        matches!(self, Self::SetSelection(_))
    }

    #[must_use]
    pub(crate) fn typing_node(&self) -> Option<NodeId> {
        match self {
            Self::ReplaceText {
                node_id,
                typing: true,
                ..
            } => Some(*node_id),
            _ => None,
        }
    }

    #[must_use]
    pub(crate) fn localized_text_node(&self, selection: &Selection) -> Option<NodeId> {
        match self {
            Self::ReplaceText { node_id, .. } | Self::ToggleInline { node_id, .. } => {
                Some(*node_id)
            }
            Self::ReplaceSelection { .. }
            | Self::ToggleInlineSelection { .. }
            | Self::SetLinkSelection { .. } => match selection {
                Selection::Text(selection)
                    if selection.anchor.node_id == selection.head.node_id =>
                {
                    Some(selection.head.node_id)
                }
                _ => None,
            },
            Self::SetImageAttributes { image_id, .. } => Some(*image_id),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum InverseOperation {
    Restore {
        snapshot: DocumentSnapshot,
        selection: Selection,
    },
}

#[derive(Clone, Debug)]
pub struct TransactionResult {
    pub snapshot: DocumentSnapshot,
    pub selection: Selection,
    pub inverse_operations: Vec<InverseOperation>,
    pub dirty_node_ids: BTreeSet<NodeId>,
    pub revision: Revision,
}

impl TransactionResult {
    #[must_use]
    pub(crate) fn unchanged(snapshot: DocumentSnapshot) -> Self {
        Self {
            selection: snapshot.selection().clone(),
            revision: snapshot.revision(),
            snapshot,
            inverse_operations: Vec::new(),
            dirty_node_ids: BTreeSet::new(),
        }
    }
}

#[must_use]
pub(crate) fn caret_after(node_id: NodeId, offset: usize) -> Selection {
    Selection::Text(crate::TextSelection::caret(DocumentPosition::new(
        node_id,
        offset,
        crate::Affinity::Downstream,
    )))
}
