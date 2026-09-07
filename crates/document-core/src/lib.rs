//! Rich Markdown document ownership boundary.
//!
//! This crate deliberately has no GPUI dependency. It owns structured content,
//! stable positions, transactions, undo, import/export, and the source spine.

mod clipboard;
mod command;
mod document;
mod error;
mod html;
mod links;
mod markdown;
mod model;
mod source;
mod table;
mod text;

pub use clipboard::{ClipboardPayload, RICH_CLIPBOARD_MIME, RichClipboard};
pub use command::{
    BlockStyle, EditCommand, FormatState, HtmlTextEdit, InlineFormat, InsertBlockKind,
    InverseOperation, PreviewPosition, PreviewSelection, TransactionResult,
};
pub use document::{Document, DocumentSnapshot, RevisionTagged};
pub use error::{DocumentError, PositionError};
pub use html::{
    HtmlConversionLeaf, HtmlTextPosition, InertHtmlFragment, InertHtmlImage, editable_html_leaves,
    editable_html_markdown, editable_html_text_leaves, editable_html_text_nodes,
    html_text_disclosures, inert_html_fragment,
};
pub use links::{LinkDestination, LinkError, heading_node, resolve_link};
pub use model::{
    Affinity, AlertKind, BlockNode, BlockSequence, CodeBlock, CodeBlockSyntax, ColumnAlignment,
    ColumnSpec, DocumentPosition, Heading, ImageLink, ImageNode, LinkTarget, ListBlock, ListItem,
    ListKind, NodeId, Paragraph, RectangularSelection, Revision, Selection, Table, TableBorder,
    TableCell, TableRow, TextSelection,
};
pub use source::{LineEnding, SaveSnapshot, SourceIdentity, SourceSpine};
pub use text::{InlineRun, InlineStyle, RichText, TextRange};

/// The pause that terminates a continuous typing undo group.
pub const TYPING_GROUP_TIMEOUT_MS: u64 = 500;
