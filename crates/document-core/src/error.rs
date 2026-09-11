use std::ops::Range;

use crate::{NodeId, Revision};

#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq)]
pub enum PositionError {
    #[error("node {0} does not exist in this revision")]
    UnknownNode(NodeId),
    #[error("offset {offset} is outside node {node} (length {len})")]
    OffsetOutOfBounds {
        node: NodeId,
        offset: usize,
        len: usize,
    },
    #[error("range {range:?} does not lie on UTF-8 boundaries in node {node}")]
    InvalidTextRange { node: NodeId, range: Range<usize> },
    #[error("position points to node {0}, which is not editable text")]
    NotText(NodeId),
}

#[derive(Debug, thiserror::Error)]
pub enum DocumentError {
    #[error(transparent)]
    Position(#[from] PositionError),
    #[error("cannot apply a text command to a table selection")]
    TableSelectionForTextCommand,
    #[error("node {0} is not a table")]
    NotTable(NodeId),
    #[error("node {0} is not an image")]
    NotImage(NodeId),
    #[error("table coordinate row={row}, column={column} is out of bounds")]
    TableCoordinate { row: usize, column: usize },
    #[error("block index {0} is out of bounds")]
    BlockIndex(usize),
    #[error("a table must retain at least one row and one column")]
    EmptyTable,
    #[error("column width must be finite and positive")]
    InvalidColumnWidth,
    #[error("worker result is for revision {result}, current revision is {current}")]
    StaleWorkerResult { result: Revision, current: Revision },
    #[error("Markdown import failed: {0}")]
    Markdown(String),
    #[error("HTML import failed: {0}")]
    Html(String),
    #[error("clipboard representation is malformed: {0}")]
    Clipboard(String),
    #[error("document has no undo entry")]
    NothingToUndo,
    #[error("document has no redo entry")]
    NothingToRedo,
    #[error("IME composition is not active")]
    NoComposition,
    #[error("IME composition is already active")]
    CompositionAlreadyActive,
}
