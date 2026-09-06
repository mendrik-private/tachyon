use std::rc::Rc;

use html5ever::{
    LocalName, ParseOpts, parse_document, serialize,
    serialize::{SerializeOpts, TraversalScope},
    tendril::TendrilSink as _,
};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};

use crate::DocumentError;

/// Convert the supported, inert HTML subset to semantic Markdown. Script,
/// style, iframe, object, and embed contents are discarded; nothing executes.
pub(crate) fn html_fragment_to_markdown(source: &str) -> Result<String, DocumentError> {
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(source);
    let mut output = String::new();
    render_children(&dom.document, &mut output, 0, true);
    Ok(output.trim_matches('\n').to_owned())
}

pub(crate) struct HtmlTableData {
    pub rows: Vec<Vec<String>>,
    pub header_rows: usize,
    pub alignments: Vec<crate::ColumnAlignment>,
}

pub(crate) fn html_table_data(source: &str) -> Option<HtmlTableData> {
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(source);
    let table = descendants_named(&dom.document, "table")
        .into_iter()
        .next()?;
    let row_handles = table_rows(&table);
    let mut rows = Vec::new();
    let mut header_rows = 0;
    let mut alignments = Vec::new();
    for row in row_handles {
        let cells = element_children(&row)
            .filter(|cell| matches!(element_name(cell), Some("th" | "td")))
            .collect::<Vec<_>>();
        if cells.is_empty() {
            continue;
        }
        if rows.len() == header_rows && cells.iter().all(|cell| element_name(cell) == Some("th")) {
            header_rows += 1;
        }
        if alignments.len() < cells.len() {
            alignments.resize(cells.len(), crate::ColumnAlignment::None);
        }
        for (index, cell) in cells.iter().enumerate() {
            if alignments[index] == crate::ColumnAlignment::None {
                alignments[index] = table_cell_alignment(cell);
            }
        }
        rows.push(
            cells
                .iter()
                .map(|cell| {
                    let mut markdown = String::new();
                    render_table_cell(cell, &mut markdown);
                    markdown.trim_matches('\n').to_owned()
                })
                .collect(),
        );
    }
    (!rows.is_empty()).then_some(HtmlTableData {
        rows,
        header_rows,
        alignments,
    })
}

fn render_table_cell(cell: &Handle, output: &mut String) {
    let has_block_children = cell.children.borrow().iter().any(|child| {
        matches!(
            element_name(child),
            Some(
                "p" | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "pre"
                    | "blockquote"
                    | "ul"
                    | "ol"
                    | "hr"
            )
        )
    });
    for child in cell.children.borrow().iter() {
        if has_block_children
            && matches!(&child.data, NodeData::Text { contents } if contents.borrow().trim().is_empty())
        {
            continue;
        }
        render_node(child, output, 0, false);
    }
}

fn table_cell_alignment(cell: &Handle) -> crate::ColumnAlignment {
    let explicit = attribute_for_handle(cell, "align").map(|value| value.to_ascii_lowercase());
    let styled = attribute_for_handle(cell, "style").and_then(|style| {
        style.split(';').find_map(|declaration| {
            let (name, value) = declaration.split_once(':')?;
            (name.trim().eq_ignore_ascii_case("text-align"))
                .then(|| value.trim().to_ascii_lowercase())
        })
    });
    match explicit.or(styled).as_deref() {
        Some("left") => crate::ColumnAlignment::Left,
        Some("center") => crate::ColumnAlignment::Center,
        Some("right") => crate::ColumnAlignment::Right,
        _ => crate::ColumnAlignment::None,
    }
}

fn attribute_for_handle(handle: &Handle, name: &str) -> Option<String> {
    let NodeData::Element { attrs, .. } = &handle.data else {
        return None;
    };
    attribute(attrs, name)
}

fn render_children(handle: &Handle, output: &mut String, depth: usize, tables_allowed: bool) {
    for child in handle.children.borrow().iter() {
        render_node(child, output, depth, tables_allowed);
    }
}

fn render_node(handle: &Handle, output: &mut String, depth: usize, tables_allowed: bool) {
    match &handle.data {
        NodeData::Text { contents } => {
            output.push_str(&crate::markdown::escape_inline(&contents.borrow()));
        }
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.as_ref();
            match tag {
                "script" | "style" | "iframe" | "object" | "embed" => {}
                "p" => block_wrap(handle, output, depth, tables_allowed, "", "\n\n"),
                "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                    let level = tag.as_bytes()[1] - b'0';
                    output.push_str(&"#".repeat(usize::from(level)));
                    output.push(' ');
                    render_children(handle, output, depth, tables_allowed);
                    output.push_str("\n\n");
                }
                "strong" | "b" => inline_wrap(handle, output, depth, tables_allowed, "**", "**"),
                "em" | "i" => inline_wrap(handle, output, depth, tables_allowed, "*", "*"),
                "del" | "s" | "strike" => {
                    inline_wrap(handle, output, depth, tables_allowed, "~~", "~~")
                }
                "code" => {
                    output.push_str(&crate::markdown::serialize_code_span(&descendant_text(
                        handle,
                    )));
                }
                "pre" => {
                    output.push_str("```\n");
                    output.push_str(&descendant_text(handle));
                    if !output.ends_with('\n') {
                        output.push('\n');
                    }
                    output.push_str("```\n\n");
                }
                "a" => {
                    output.push('[');
                    render_children(handle, output, depth, tables_allowed);
                    output.push_str("](");
                    output.push_str(&crate::markdown::serialize_destination(
                        &attribute(attrs, "href").unwrap_or_default(),
                    ));
                    output.push(')');
                }
                "img" => {
                    output.push_str("![");
                    output.push_str(&crate::markdown::escape_inline(
                        &attribute(attrs, "alt").unwrap_or_default(),
                    ));
                    output.push_str("](");
                    output.push_str(&crate::markdown::serialize_destination(
                        &attribute(attrs, "src").unwrap_or_default(),
                    ));
                    if let Some(title) = attribute(attrs, "title") {
                        output.push(' ');
                        output.push_str(&crate::markdown::serialize_title(&title));
                    }
                    output.push(')');
                }
                "br" => output.push_str("  \n"),
                "blockquote" => {
                    let mut inner = String::new();
                    render_children(handle, &mut inner, depth + 1, tables_allowed);
                    for line in inner.trim_end().lines() {
                        output.push_str("> ");
                        output.push_str(line);
                        output.push('\n');
                    }
                    output.push('\n');
                }
                "ul" => render_list(handle, output, depth, false, tables_allowed),
                "ol" => render_list(handle, output, depth, true, tables_allowed),
                "li" => render_children(handle, output, depth, tables_allowed),
                "hr" => output.push_str("---\n\n"),
                "table" if tables_allowed => render_table(handle, output),
                "table" => output.push_str("<!-- Unsupported nested table -->"),
                "html" | "head" | "body" | "main" | "section" | "article" | "div" | "span"
                | "thead" | "tbody" | "tfoot" | "tr" | "th" | "td" => {
                    render_children(handle, output, depth, tables_allowed);
                }
                _ => {
                    output.push_str("<!-- Unsupported HTML: ");
                    output.push_str(tag);
                    output.push_str(" -->");
                }
            }
        }
        _ => render_children(handle, output, depth, tables_allowed),
    }
}

fn inline_wrap(
    handle: &Handle,
    output: &mut String,
    depth: usize,
    tables_allowed: bool,
    open: &str,
    close: &str,
) {
    output.push_str(open);
    render_children(handle, output, depth, tables_allowed);
    output.push_str(close);
}

fn block_wrap(
    handle: &Handle,
    output: &mut String,
    depth: usize,
    tables_allowed: bool,
    open: &str,
    close: &str,
) {
    output.push_str(open);
    render_children(handle, output, depth, tables_allowed);
    output.push_str(close);
}

fn render_list(
    handle: &Handle,
    output: &mut String,
    depth: usize,
    ordered: bool,
    tables_allowed: bool,
) {
    let mut ordinal = 1;
    for child in element_children(handle).filter(|child| element_name(child) == Some("li")) {
        output.push_str(&"    ".repeat(depth));
        if ordered {
            output.push_str(&format!("{ordinal}. "));
            ordinal += 1;
        } else {
            output.push_str("- ");
        }
        render_children(&child, output, depth + 1, tables_allowed);
        if !output.ends_with('\n') {
            output.push('\n');
        }
    }
    output.push('\n');
}

fn render_table(handle: &Handle, output: &mut String) {
    let mut bytes = Vec::new();
    if serialize(
        &mut bytes,
        &SerializableHandle::from(Rc::clone(handle)),
        SerializeOpts {
            traversal_scope: TraversalScope::IncludeNode,
            scripting_enabled: false,
            ..SerializeOpts::default()
        },
    )
    .is_ok()
        && let Ok(table) = String::from_utf8(bytes)
    {
        output.push_str(&table);
        output.push_str("\n\n");
    }
}

fn table_rows(table: &Handle) -> Vec<Handle> {
    let mut rows = Vec::new();
    for child in element_children(table) {
        match element_name(&child) {
            Some("tr") => rows.push(child),
            Some("thead" | "tbody" | "tfoot") => {
                rows.extend(element_children(&child).filter(|row| element_name(row) == Some("tr")));
            }
            _ => {}
        }
    }
    rows
}

fn element_children(handle: &Handle) -> impl Iterator<Item = Handle> {
    handle
        .children
        .borrow()
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .into_iter()
}

fn descendants_named(handle: &Handle, name: &str) -> Vec<Handle> {
    let mut found = Vec::new();
    for child in handle.children.borrow().iter() {
        if element_name(child) == Some(name) {
            found.push(Rc::clone(child));
        } else {
            found.extend(descendants_named(child, name));
        }
    }
    found
}

fn element_name(handle: &Handle) -> Option<&str> {
    match &handle.data {
        NodeData::Element { name, .. } => Some(name.local.as_ref()),
        _ => None,
    }
}

fn descendant_text(handle: &Handle) -> String {
    let mut output = String::new();
    match &handle.data {
        NodeData::Text { contents } => output.push_str(&contents.borrow()),
        _ => {
            for child in handle.children.borrow().iter() {
                output.push_str(&descendant_text(child));
            }
        }
    }
    output
}

fn attribute(attrs: &std::cell::RefCell<Vec<html5ever::Attribute>>, name: &str) -> Option<String> {
    let name = LocalName::from(name);
    attrs
        .borrow()
        .iter()
        .find(|attribute| attribute.name.local == name)
        .map(|attribute| attribute.value.to_string())
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::{BlockNode, Document, EditCommand, InlineStyle};

    #[test]
    fn html_tables_import_rich_block_content_into_cells() {
        let source = concat!(
            "<table><thead><tr><th><h2>Head</h2></th><th>B</th></tr></thead>",
            "<tbody><tr><td><p>One <strong>bold</strong></p>",
            "<ul><li>two</li></ul></td><td><pre>code</pre></td></tr></tbody></table>"
        );
        let mut document = Document::from_html(source).expect("HTML document");
        let converted = html_fragment_to_markdown(source).expect("convert HTML");
        let snapshot = document.snapshot();
        let BlockNode::Table(table) = snapshot.blocks().get(0).expect("table").as_ref() else {
            panic!("HTML table must become a typed table; converted source: {converted:?}");
        };
        assert_eq!(table.header_rows, 1);
        assert!(matches!(
            table.rows[0].cells[0].blocks.get(0).map(Arc::as_ref),
            Some(BlockNode::Heading(_))
        ));
        assert!(table.rows[1].cells[0].blocks.len() >= 2);
        let paragraph = table.rows[1].cells[0]
            .blocks
            .get(0)
            .and_then(|block| block.text())
            .expect("paragraph");
        assert!(
            paragraph
                .runs()
                .iter()
                .any(|run| run.styles.contains(&InlineStyle::Bold))
        );
        assert!(table.requires_html_serialization());
        let paragraph_id = table.rows[1].cells[0]
            .blocks
            .get(0)
            .expect("paragraph")
            .id();
        document
            .apply(EditCommand::ReplaceText {
                node_id: paragraph_id,
                range: 0..3,
                text: "Changed".into(),
                selection_after: None,
                typing: false,
            })
            .expect("edit rich cell");
        let serialized = document.snapshot().serialize().expect("serialize table");
        assert!(serialized.contains("<strong>bold</strong>"));
        assert!(serialized.contains("<ul>"));
        let reparsed = Document::from_markdown(serialized).expect("reparse HTML table");
        assert!(matches!(
            reparsed.snapshot().blocks().get(0).map(Arc::as_ref),
            Some(BlockNode::Table(_))
        ));
    }

    #[test]
    fn dangerous_html_descendants_are_dropped() {
        let markdown = html_fragment_to_markdown(
            "<p>safe<script>alert(1)</script><iframe>bad</iframe>end</p>",
        )
        .expect("convert");
        assert_eq!(markdown, "safeend");
    }

    #[test]
    fn html_literal_text_code_and_destinations_retain_semantics() {
        let target = "https://example.test/a_(b)?q=one &copy;<two>";
        let source = concat!(
            "<p># literal<br>- next &amp;copy; ",
            "<code>`edge`</code> ",
            "<a href=\"https://example.test/a_(b)?q=one &amp;copy;&lt;two&gt;\">link</a>",
            "</p>"
        );
        let document = Document::from_html(source).expect("HTML document");
        let snapshot = document.snapshot();
        assert_eq!(snapshot.blocks().len(), 1);
        let paragraph = snapshot
            .blocks()
            .get(0)
            .and_then(|block| block.text())
            .expect("paragraph");
        assert!(
            paragraph
                .as_string()
                .starts_with("# literal  \n- next &copy;")
        );
        assert!(
            paragraph
                .runs()
                .iter()
                .any(|run| run.styles.contains(&InlineStyle::Code))
        );
        assert!(paragraph.runs().iter().any(|run| {
            run.styles.iter().any(|style| {
                matches!(style, InlineStyle::Link(crate::LinkTarget(value)) if value == target)
            })
        }));
    }
}
