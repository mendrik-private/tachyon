use serde::{Deserialize, Serialize};

use std::sync::Arc;

use crate::{
    BlockNode, BlockSequence, DocumentError, DocumentSnapshot, Paragraph, RectangularSelection,
    Selection, Table, TextSelection,
};

pub const RICH_CLIPBOARD_MIME: &str = "application/x-mineral-markdown-fragment+json;version=1";

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ClipboardPayload {
    pub plain_text: Option<String>,
    pub html: Option<String>,
    pub rich_json: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RichClipboard {
    pub version: u32,
    pub markdown: String,
    pub plain_text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub html: Option<String>,
}

impl RichClipboard {
    #[must_use]
    pub fn new(markdown: impl Into<String>, plain_text: impl Into<String>) -> Self {
        Self {
            version: 1,
            markdown: markdown.into(),
            plain_text: plain_text.into(),
            html: None,
        }
    }

    #[must_use]
    pub fn with_html(mut self, html: impl Into<String>) -> Self {
        self.html = Some(html.into());
        self
    }

    pub fn to_json(&self) -> Result<String, DocumentError> {
        serde_json::to_string(self).map_err(|error| DocumentError::Clipboard(error.to_string()))
    }

    pub fn from_json(source: &str) -> Result<Self, DocumentError> {
        let payload: Self = serde_json::from_str(source)
            .map_err(|error| DocumentError::Clipboard(error.to_string()))?;
        if payload.version != 1 {
            return Err(DocumentError::Clipboard(format!(
                "unsupported rich clipboard version {}",
                payload.version
            )));
        }
        Ok(payload)
    }
}

impl ClipboardPayload {
    /// Returns Markdown using rich -> supported HTML -> plain-text precedence.
    pub fn preferred_markdown(&self) -> Result<Option<String>, DocumentError> {
        if let Some(rich) = &self.rich_json
            && let Ok(payload) = RichClipboard::from_json(rich)
        {
            return Ok(Some(payload.markdown));
        }
        if let Some(html) = &self.html
            && let Ok(markdown) = crate::html::html_fragment_to_markdown(html)
        {
            return Ok(Some(markdown));
        }
        Ok(self.plain_text.clone())
    }

    #[must_use]
    pub fn paste_as_markdown(&self) -> Option<&str> {
        self.plain_text.as_deref()
    }
}

impl DocumentSnapshot {
    /// Builds all semantic clipboard representations from the current selection.
    /// The UI backend may publish a subset when the native clipboard API cannot
    /// advertise arbitrary MIME types.
    pub fn clipboard_payload(&self) -> Result<Option<ClipboardPayload>, DocumentError> {
        match self.selection() {
            Selection::Text(selection) if selection.is_caret() => Ok(None),
            Selection::Text(selection) => text_payload(self, selection),
            Selection::Table(selection) => table_payload(self, *selection),
        }
    }
}

fn text_payload(
    snapshot: &DocumentSnapshot,
    selection: &TextSelection,
) -> Result<Option<ClipboardPayload>, DocumentError> {
    if selection.anchor.node_id == selection.head.node_id {
        let node = snapshot.node(selection.anchor.node_id).ok_or_else(|| {
            DocumentError::Clipboard("clipboard selection points to a missing node".into())
        })?;
        let text = node.text().ok_or_else(|| {
            DocumentError::Clipboard("clipboard selection is not editable text".into())
        })?;
        let range = selection.anchor.text_offset.min(selection.head.text_offset)
            ..selection.anchor.text_offset.max(selection.head.text_offset);
        text.validate_range(node.id(), &range)?;
        if matches!(node, BlockNode::Image(_)) && !range.is_empty() && range == (0..text.len()) {
            let blocks = BlockSequence::new(vec![Arc::new(node.clone())]);
            return Ok(Some(payload_from_markdown(
                text.as_string(),
                crate::markdown::serialize_clipboard_blocks(&blocks)?,
            )));
        }
        let fragment = text.slice(range);
        let markdown = crate::markdown::serialize_inline(&fragment);
        return Ok(Some(payload_from_markdown(fragment.as_string(), markdown)));
    }

    let anchor = snapshot
        .blocks()
        .iter()
        .position(|block| block.id() == selection.anchor.node_id);
    let head = snapshot
        .blocks()
        .iter()
        .position(|block| block.id() == selection.head.node_id);
    let (Some(anchor), Some(head)) = (anchor, head) else {
        return Err(DocumentError::Clipboard(
            "cross-block copy requires top-level editable blocks".into(),
        ));
    };
    let (start_index, start, end_index, end) = if anchor < head {
        (anchor, selection.anchor, head, selection.head)
    } else {
        (head, selection.head, anchor, selection.anchor)
    };
    let mut blocks = snapshot.blocks().to_vec()[start_index..=end_index].to_vec();
    let start_source = blocks[0].as_ref();
    let start_text = start_source.text().ok_or_else(|| {
        DocumentError::Clipboard("clipboard boundary is not editable text".into())
    })?;
    start_text.validate_range(start.node_id, &(start.text_offset..start.text_offset))?;
    blocks[0] = Arc::new(block_with_text(
        start_source,
        start_text.slice(start.text_offset..start_text.len()),
    )?);
    let last_index = blocks.len() - 1;
    let end_source = blocks[last_index].as_ref();
    let end_text = end_source.text().ok_or_else(|| {
        DocumentError::Clipboard("clipboard boundary is not editable text".into())
    })?;
    end_text.validate_range(end.node_id, &(end.text_offset..end.text_offset))?;
    blocks[last_index] = Arc::new(block_with_text(
        end_source,
        end_text.slice(0..end.text_offset),
    )?);
    let blocks = BlockSequence::new(blocks);
    let plain_text = blocks
        .iter()
        .map(|block| block.plain_text())
        .collect::<Vec<_>>()
        .join("\n");
    let markdown = crate::markdown::serialize_clipboard_blocks(&blocks)?;
    Ok(Some(payload_from_markdown(plain_text, markdown)))
}

fn table_payload(
    snapshot: &DocumentSnapshot,
    selection: RectangularSelection,
) -> Result<Option<ClipboardPayload>, DocumentError> {
    let Some(BlockNode::Table(table)) = snapshot.node(selection.table_id) else {
        return Err(DocumentError::Clipboard(
            "table clipboard selection points to a missing table".into(),
        ));
    };
    let (rows, columns) = selection.normalized();
    if rows.end() >= &table.rows.len() || columns.end() >= &table.columns.len() {
        return Err(DocumentError::Clipboard(
            "table clipboard selection is out of bounds".into(),
        ));
    }
    let selected_rows = table.rows[*rows.start()..=*rows.end()]
        .iter()
        .map(|row| crate::TableRow {
            id: row.id,
            cells: row.cells[*columns.start()..=*columns.end()].into(),
        })
        .collect::<Vec<_>>();
    let selected = Table {
        id: table.id,
        columns: table.columns[*columns.start()..=*columns.end()].into(),
        rows: selected_rows.into(),
        header_rows: table
            .header_rows
            .saturating_sub(*rows.start())
            .min(rows.clone().count()),
        border: table.border,
        preserved_metadata: Arc::from([]),
    };
    let plain_text = selected
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
        .join("\n");
    let blocks = BlockSequence::new(vec![Arc::new(BlockNode::Table(selected))]);
    let markdown = crate::markdown::serialize_clipboard_blocks(&blocks)?;
    Ok(Some(payload_from_markdown(plain_text, markdown)))
}

fn payload_from_markdown(plain_text: String, markdown: String) -> ClipboardPayload {
    let html = crate::markdown::clipboard_html(&markdown);
    let rich_json = RichClipboard::new(&markdown, &plain_text)
        .with_html(&html)
        .to_json()
        .ok();
    ClipboardPayload {
        plain_text: Some(plain_text),
        html: Some(html),
        rich_json,
    }
}

fn block_with_text(
    source: &BlockNode,
    content: crate::RichText,
) -> Result<BlockNode, DocumentError> {
    match source {
        BlockNode::Paragraph(paragraph) => Ok(BlockNode::Paragraph(Paragraph {
            id: paragraph.id,
            content,
        })),
        BlockNode::Heading(heading) => Ok(BlockNode::Heading(crate::Heading {
            id: heading.id,
            level: heading.level,
            content,
        })),
        BlockNode::CodeBlock(code) => Ok(BlockNode::CodeBlock(crate::CodeBlock {
            id: code.id,
            language: code.language.clone(),
            syntax: code.syntax,
            content,
        })),
        BlockNode::Image(image) => {
            if content.as_string() == image.alt.as_string() {
                Ok(source.clone())
            } else {
                Ok(BlockNode::Paragraph(Paragraph {
                    id: image.id,
                    content,
                }))
            }
        }
        _ => Err(DocumentError::Clipboard(
            "clipboard boundary must be a paragraph, heading, or code block".into(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Affinity, Document, DocumentPosition, EditCommand, TextSelection};

    #[test]
    fn gallery_copy_preserves_boundary_figures_and_enclosing_links() {
        let source = "[![First](first.png)](full-first.png)\n\n![Middle](middle.png)\n\n[![Last](last.png)](full-last.png)\n";
        let mut document = Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let first = snapshot.blocks().get(0).unwrap().id();
        let last = snapshot.blocks().get(2).unwrap().id();
        for (end, offset, expected_images) in [(first, 5, 1), (last, 4, 3)] {
            let selection = Selection::Text(TextSelection {
                anchor: DocumentPosition::new(first, 0, Affinity::Downstream),
                head: DocumentPosition::new(end, offset, Affinity::Upstream),
            });
            document
                .apply(EditCommand::SetSelection(selection))
                .unwrap();
            let payload = document.snapshot().clipboard_payload().unwrap().unwrap();
            let rich = RichClipboard::from_json(payload.rich_json.as_ref().unwrap()).unwrap();
            let copied = Document::from_markdown(rich.markdown).unwrap();
            let copied = copied.snapshot();
            assert_eq!(copied.blocks().len(), expected_images);
            let BlockNode::Image(image) = copied.blocks().get(0).unwrap().as_ref() else {
                panic!("first copied figure")
            };
            assert_eq!(image.link.as_ref().unwrap().target.0, "full-first.png");
            assert!(
                payload
                    .html
                    .as_ref()
                    .unwrap()
                    .contains("href=\"full-first.png\"")
            );
            if expected_images == 3 {
                let BlockNode::Image(image) = copied.blocks().get(2).unwrap().as_ref() else {
                    panic!("last copied figure")
                };
                assert_eq!(image.link.as_ref().unwrap().target.0, "full-last.png");
            }
        }
        assert_eq!(snapshot.serialize().unwrap(), source);
    }

    #[test]
    fn paste_precedence_is_rich_then_html_then_plain() {
        let rich = RichClipboard::new("**rich**", "rich")
            .to_json()
            .expect("rich JSON");
        let payload = ClipboardPayload {
            plain_text: Some("plain".into()),
            html: Some("<em>html</em>".into()),
            rich_json: Some(rich),
        };
        assert_eq!(
            payload.preferred_markdown().expect("preferred").as_deref(),
            Some("**rich**")
        );
        let payload = ClipboardPayload {
            rich_json: None,
            ..payload
        };
        assert_eq!(
            payload.preferred_markdown().expect("preferred").as_deref(),
            Some("*html*")
        );
        assert_eq!(payload.paste_as_markdown(), Some("plain"));

        let malformed = ClipboardPayload {
            rich_json: Some("not-json".into()),
            html: Some("<script>ignored()</script><em>safe</em>".into()),
            plain_text: Some("fallback".into()),
        };
        assert_eq!(
            malformed.preferred_markdown().expect("fallback").as_deref(),
            Some("*safe*")
        );
    }

    #[test]
    fn formatted_selection_produces_plain_markdown_html_and_rich_json() {
        let mut document = Document::from_markdown("before **bold** after").expect("document");
        let node_id = document.snapshot().blocks().get(0).expect("paragraph").id();
        document
            .apply(EditCommand::SetSelection(Selection::Text(TextSelection {
                anchor: DocumentPosition::new(node_id, 7, Affinity::Downstream),
                head: DocumentPosition::new(node_id, 11, Affinity::Upstream),
            })))
            .expect("selection");

        let payload = document
            .snapshot()
            .clipboard_payload()
            .expect("clipboard payload")
            .expect("non-empty selection");
        assert_eq!(payload.plain_text.as_deref(), Some("bold"));
        assert_eq!(
            payload.preferred_markdown().expect("Markdown").as_deref(),
            Some("**bold**")
        );
        assert!(payload.html.as_deref().is_some_and(|html| {
            html.contains("<strong>bold</strong>") && !html.contains("before")
        }));
        let rich = RichClipboard::from_json(payload.rich_json.as_deref().expect("rich JSON"))
            .expect("valid rich JSON");
        assert_eq!(rich.markdown, "**bold**");
        assert_eq!(rich.plain_text, "bold");
        assert_eq!(rich.html, payload.html);
    }

    #[test]
    fn rectangular_table_selection_produces_tsv_and_semantic_formats() {
        let mut document =
            Document::from_markdown("| A | B |\n| - | - |\n| 1 | 2 |").expect("table document");
        let table_id = document.snapshot().blocks().get(0).expect("table").id();
        document
            .apply(EditCommand::SetSelection(Selection::Table(
                RectangularSelection {
                    table_id,
                    anchor_row: 0,
                    anchor_column: 0,
                    head_row: 1,
                    head_column: 1,
                },
            )))
            .expect("selection");

        let payload = document
            .snapshot()
            .clipboard_payload()
            .expect("clipboard payload")
            .expect("table selection");
        assert_eq!(payload.plain_text.as_deref(), Some("A\tB\n1\t2"));
        assert!(
            payload
                .preferred_markdown()
                .expect("Markdown")
                .is_some_and(|markdown| markdown.contains("| A | B |"))
        );
        assert!(
            payload
                .html
                .as_deref()
                .is_some_and(|html| html.contains("<table>") && html.contains("<th>A</th>"))
        );
    }
}
