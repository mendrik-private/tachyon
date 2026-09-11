use std::rc::Rc;

use html5ever::{
    LocalName, ParseOpts, parse_document, serialize,
    serialize::{SerializeOpts, TraversalScope},
    tendril::TendrilSink as _,
};
use markup5ever_rcdom::{Handle, NodeData, RcDom, SerializableHandle};

use crate::DocumentError;

/// A position in the source-order text leaves of the semantic HTML conversion.
/// This is a temporary conversion address, never a persistent editor identity.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct HtmlTextPosition {
    pub text_node: usize,
    pub byte_offset: usize,
}

/// Containing disclosure ordinals for a verified conversion-text position.
/// Uses inert authored DOM order, not a substring search (which misaddresses
/// repeated text). Opening these is a view operation, never source conversion.
#[must_use]
pub fn html_text_disclosures(source: &str, position: HtmlTextPosition) -> Option<Vec<usize>> {
    let leaves = editable_html_leaves(source)?;
    let text = leaves.get(position.text_node)?.text.as_string();
    if position.byte_offset > text.len() || !text.is_char_boundary(position.byte_offset) {
        return None;
    }
    let mut canonical = String::new();
    let mut target = None;
    for (ordinal, leaf) in leaves.iter().enumerate() {
        if leaf.image_description {
            continue;
        }
        for (byte, ch) in leaf.text.as_string().char_indices() {
            if !ch.is_whitespace() {
                if ordinal == position.text_node && byte >= position.byte_offset && target.is_none()
                {
                    target = Some(canonical.len());
                }
                canonical.push(ch);
            }
        }
    }
    let target = target?;
    let fragment = inert_html_fragment(source)?;
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(fragment.html());
    fn visit(
        handle: &Handle,
        ancestors: &[usize],
        ordinal: &mut usize,
        text: &mut String,
        target: usize,
        found: &mut Option<Vec<usize>>,
    ) {
        if let NodeData::Text { contents } = &handle.data {
            for ch in contents.borrow().chars().filter(|ch| !ch.is_whitespace()) {
                if text.len() == target {
                    *found = Some(ancestors.to_vec());
                }
                text.push(ch);
            }
        }
        let details = matches!(&handle.data, NodeData::Element { name, .. } if name.local.as_ref() == "details");
        let mut nested = ancestors.to_vec();
        if details {
            nested.push(*ordinal);
            *ordinal += 1;
        }
        let mut summary_seen = false;
        for child in handle.children.borrow().iter() {
            let summary = details
                && !summary_seen
                && matches!(&child.data, NodeData::Element { name, .. } if name.local.as_ref() == "summary");
            summary_seen |= summary;
            visit(
                child,
                if summary { ancestors } else { &nested },
                ordinal,
                text,
                target,
                found,
            );
        }
    }
    let mut rendered = String::new();
    let mut found = None;
    visit(
        &dom.document,
        &[],
        &mut 0,
        &mut rendered,
        target,
        &mut found,
    );
    (rendered == canonical).then_some(found).flatten()
}

/// Text used to verify a renderer's correspondence before offering an edit hit.
/// Uses exactly the same converter and parser as the eventual edit transaction.
#[must_use]
pub fn editable_html_text_nodes(source: &str) -> Option<Vec<String>> {
    Some(
        editable_html_text_leaves(source)?
            .iter()
            .map(crate::RichText::as_string)
            .collect(),
    )
}

/// Immutable semantic text and inline styles from the same conversion used by
/// the first edit. These are inspection values, not a second editable document.
#[must_use]
pub fn editable_html_text_leaves(source: &str) -> Option<Vec<crate::RichText>> {
    Some(
        editable_html_leaves(source)?
            .into_iter()
            .map(|leaf| leaf.text)
            .collect(),
    )
}

/// Source-order conversion leaf metadata. Image descriptions retain their
/// ordinal for selection/copy, but are not painted text hit-test targets.
#[derive(Clone, Debug)]
pub struct HtmlConversionLeaf {
    pub text: crate::RichText,
    pub image_description: bool,
}

#[must_use]
pub fn editable_html_leaves(source: &str) -> Option<Vec<HtmlConversionLeaf>> {
    let markdown = editable_html_markdown(source)?;
    let imported = crate::markdown::import(std::sync::Arc::<str>::from(markdown)).ok()?;
    Some(
        conversion_text_blocks(imported.blocks())
            .into_iter()
            .filter_map(|block| {
                Some(HtmlConversionLeaf {
                    text: block.text()?.clone(),
                    image_description: matches!(block, crate::BlockNode::Image(_)),
                })
            })
            .collect(),
    )
}

pub(crate) fn conversion_text_blocks(blocks: &crate::BlockSequence) -> Vec<&crate::BlockNode> {
    fn visit<'a>(block: &'a crate::BlockNode, output: &mut Vec<&'a crate::BlockNode>) {
        use crate::BlockNode;
        if block.text().is_some() {
            output.push(block);
            return;
        }
        match block {
            BlockNode::List(list) => {
                for item in list.items.iter() {
                    for block in item.blocks.iter() {
                        visit(block, output);
                    }
                }
            }
            BlockNode::BlockQuote { blocks, .. }
            | BlockNode::Alert { blocks, .. }
            | BlockNode::Definition { blocks, .. }
            | BlockNode::FootnoteDefinition { blocks, .. } => {
                for block in blocks.iter() {
                    visit(block, output);
                }
            }
            BlockNode::Table(table) => {
                for row in table.rows.iter() {
                    for cell in row.cells.iter() {
                        for block in cell.blocks.iter() {
                            visit(block, output);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    let mut output = Vec::new();
    for block in blocks.iter() {
        visit(block, &mut output);
    }
    output
}

/// Read-only rendering input. This never replaces the authored source spine.
/// Constructed only by the bounded, inert HTML adapter below.
#[derive(Clone, Debug)]
pub struct InertHtmlFragment {
    html: String,
    text: String,
    images: Vec<InertHtmlImage>,
    link_targets: Vec<String>,
}

/// A source-order local image reference, never a renderer fetch instruction.
/// The host must resolve and validate it through its existing image policy.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InertHtmlImage {
    pub source: String,
    pub alt: String,
}

impl InertHtmlFragment {
    #[must_use]
    pub fn html(&self) -> &str {
        &self.html
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn images(&self) -> &[InertHtmlImage] {
        &self.images
    }

    /// Authored link destinations retained outside the temporary renderer DOM.
    /// The HTML contains only generated numeric descriptors, so the inert
    /// renderer can measure links without gaining navigation or fetch access.
    #[must_use]
    pub fn link_targets(&self) -> &[String] {
        &self.link_targets
    }
}

/// Prepare a static fragment for an inert renderer. Local image references are
/// descriptors only: the renderer must require host-supplied pixels, or retain
/// the complete source fallback. Unsupported media,
/// namespaces and interactive widgets retain the existing source fallback.
/// External stylesheets, scripts and embedded documents are never instantiated.
/// Inline CSS is retained, but the renderer MUST use a denying resource provider.
#[must_use]
pub fn inert_html_fragment(source: &str) -> Option<InertHtmlFragment> {
    bounded_html_fragment(source, false)
}

/// Export the complete owning root, without renderer-private descriptors,
/// active attributes, or CSS that relies on a denying renderer resource host.
pub(crate) fn clipboard_html_fragment(source: &str) -> Option<String> {
    Some(bounded_html_fragment(source, true)?.html)
}

/// The input is serialized from an already selected canonical tree. Unlike a
/// single preview, a clipboard selection may span many bounded HTML owners.
pub(crate) fn clipboard_html_document(source: &str) -> Option<String> {
    Some(html_fragment_with_budget(source, true, source.len().saturating_add(1))?.html)
}

fn bounded_html_fragment(source: &str, clipboard: bool) -> Option<InertHtmlFragment> {
    if source.len() > 32 * 1024 {
        return None;
    }
    html_fragment_with_budget(source, clipboard, 512)
}

fn html_fragment_with_budget(
    source: &str,
    clipboard: bool,
    mut budget: usize,
) -> Option<InertHtmlFragment> {
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(source);
    let mut fragment = InertHtmlFragment {
        html: String::new(),
        text: String::new(),
        images: Vec::new(),
        link_targets: Vec::new(),
    };
    inert_node(&dom.document, &mut fragment, 0, &mut budget, clipboard)?;
    Some(fragment)
}

/// Explicit conversion is offered only when the existing semantic converter
/// understands the complete fragment. It intentionally replaces HTML styling
/// with Markdown formatting, but must not silently discard unknown content.
#[must_use]
pub fn editable_html_markdown(source: &str) -> Option<String> {
    inert_html_fragment(source)?;
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(source);
    if descendants_named(&dom.document, "table")
        .iter()
        .any(|table| {
            !descendants_named(table, "table").is_empty() || table_has_spanning_cells(table)
        })
    {
        // The typed cell editor does not model nested or spanning geometry. Retain
        // the complete inert fragment instead of offering a lossy conversion.
        return None;
    }
    if descendants_named(&dom.document, "ol")
        .iter()
        .any(|list| markdown_list_start(list).is_none())
    {
        return None;
    }
    let markdown = html_fragment_to_markdown(source).ok()?;
    (!markdown.trim().is_empty() && !markdown.contains("<!-- Unsupported")).then_some(markdown)
}

fn html_escape(value: &str, output: &mut String) {
    for ch in value.chars() {
        match ch {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            _ => output.push(ch),
        }
    }
}

fn inert_node(
    handle: &Handle,
    fragment: &mut InertHtmlFragment,
    depth: usize,
    budget: &mut usize,
    clipboard: bool,
) -> Option<()> {
    if depth > 32 || *budget == 0 {
        return None;
    }
    *budget -= 1;
    match &handle.data {
        NodeData::Text { contents } => {
            html_escape(&contents.borrow(), &mut fragment.html);
            fragment.text.push_str(&contents.borrow());
        }
        NodeData::Element { name, attrs, .. } => {
            let tag = name.local.as_ref();
            if matches!(
                tag,
                "script" | "style" | "iframe" | "object" | "embed" | "link" | "meta" | "base"
            ) {
                return Some(());
            }
            if name.ns.as_ref() != "http://www.w3.org/1999/xhtml" {
                return None;
            }
            if clipboard && tag == "input" {
                if !attribute(attrs, "type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("checkbox"))
                    || attribute(attrs, "disabled").is_none()
                {
                    return None;
                }
                fragment.html.push_str("<input type=\"checkbox\" disabled");
                if attribute(attrs, "checked").is_some() {
                    fragment.html.push_str(" checked");
                }
                fragment.html.push('>');
                return Some(());
            }
            if matches!(tag, "html" | "head" | "body") {
                for child in handle.children.borrow().iter() {
                    inert_node(child, fragment, depth + 1, budget, clipboard)?;
                }
                return Some(());
            }
            if !matches!(
                tag,
                "main"
                    | "section"
                    | "article"
                    | "div"
                    | "span"
                    | "p"
                    | "h1"
                    | "h2"
                    | "h3"
                    | "h4"
                    | "h5"
                    | "h6"
                    | "blockquote"
                    | "strong"
                    | "b"
                    | "em"
                    | "i"
                    | "s"
                    | "del"
                    | "small"
                    | "mark"
                    | "code"
                    | "pre"
                    | "kbd"
                    | "sub"
                    | "sup"
                    | "a"
                    | "br"
                    | "hr"
                    | "ul"
                    | "ol"
                    | "li"
                    | "dl"
                    | "dt"
                    | "dd"
                    | "details"
                    | "summary"
                    | "table"
                    | "thead"
                    | "tbody"
                    | "tfoot"
                    | "tr"
                    | "th"
                    | "td"
                    | "caption"
                    | "img"
            ) && !(clipboard && tag == "aside")
            {
                return None;
            }
            fragment.html.push('<');
            fragment.html.push_str(tag);
            if tag == "a"
                && let Some(target) = attribute(attrs, "href")
                && !target.is_empty()
            {
                if clipboard {
                    if matches!(
                        crate::resolve_link(&target, None),
                        Ok(_) | Err(crate::LinkError::NoDirectory)
                    ) {
                        fragment.html.push_str(" href=\"");
                        html_escape(&target, &mut fragment.html);
                        fragment.html.push('"');
                    }
                } else {
                    fragment.html.push_str(" data-mineral-link=\"");
                    fragment
                        .html
                        .push_str(&fragment.link_targets.len().to_string());
                    fragment.html.push('"');
                    fragment.link_targets.push(target);
                }
            }
            if tag == "img" {
                // URLs remain outside the inert DOM. In particular, no srcset,
                // remote/file/data URL or base-element resolution is introduced.
                // Relative parent components retain ordinary Markdown semantics;
                // this descriptor grants no filesystem access on its own.
                if attribute(attrs, "srcset").is_some() || fragment.images.len() >= 32 {
                    return None;
                }
                let source = attribute(attrs, "src")?;
                let local = !source.is_empty()
                    && !source.starts_with('/')
                    && !source
                        .chars()
                        .any(|ch| ch.is_control() || matches!(ch, ':' | '\\' | '?' | '#'));
                let web = clipboard
                    && source.trim() == source
                    && !source.chars().any(char::is_control)
                    && !source.contains('\\')
                    && url::Url::parse(&source).is_ok_and(|url| {
                        matches!(url.scheme(), "http" | "https") && url.host_str().is_some()
                    });
                if !clipboard && !local {
                    return None;
                }
                let alt = attribute(attrs, "alt").unwrap_or_default();
                if clipboard {
                    if local || web {
                        fragment.html.push_str(" src=\"");
                        html_escape(&source, &mut fragment.html);
                        fragment.html.push('"');
                    }
                } else {
                    fragment.html.push_str(" data-mineral-image=\"");
                    fragment.html.push_str(&fragment.images.len().to_string());
                    fragment.html.push('"');
                }
                fragment.html.push_str(" alt=\"");
                html_escape(&alt, &mut fragment.html);
                fragment.html.push('"');
                fragment.text.push_str(&alt);
                fragment.images.push(InertHtmlImage { source, alt });
            }
            for attr in attrs.borrow().iter() {
                let key = attr.name.local.as_ref();
                if clipboard && key == "style" {
                    continue;
                }
                if tag == "img"
                    && attr.name.ns.as_ref().is_empty()
                    && matches!(key, "width" | "height")
                {
                    let size = attr.value.parse::<u32>().ok()?;
                    if !(1..=16384).contains(&size) {
                        return None;
                    }
                    fragment.html.push(' ');
                    fragment.html.push_str(key);
                    fragment.html.push_str("=\"");
                    fragment.html.push_str(&size.to_string());
                    fragment.html.push('"');
                    continue;
                }
                // Preserve authored IDs solely as inert navigation metadata.
                // Never install them as DOM/native IDs, and never accept a
                // caller-supplied data-mineral-anchor attribute as authority.
                if attr.name.ns.as_ref().is_empty() && key == "id" {
                    fragment.html.push_str(if clipboard {
                        " id=\""
                    } else {
                        " data-mineral-anchor=\""
                    });
                    html_escape(&attr.value, &mut fragment.html);
                    fragment.html.push('"');
                    continue;
                }
                // No URLs, event handlers, focus, forms, active editing or
                // author IDs that could collide with native document anchors.
                if attr.name.ns.as_ref().is_empty()
                    && (matches!(
                        key,
                        "style"
                            | "title"
                            | "lang"
                            | "dir"
                            | "open"
                            | "start"
                            | "value"
                            | "colspan"
                            | "rowspan"
                            | "scope"
                    ) || (tag == "ol" && matches!(key, "reversed" | "type"))
                        || (matches!(
                            tag,
                            "table" | "thead" | "tbody" | "tfoot" | "tr" | "th" | "td"
                        ) && key == "align"))
                {
                    fragment.html.push(' ');
                    fragment.html.push_str(key);
                    fragment.html.push_str("=\"");
                    html_escape(&attr.value, &mut fragment.html);
                    fragment.html.push('"');
                }
            }
            fragment.html.push('>');
            for child in handle.children.borrow().iter() {
                inert_node(child, fragment, depth + 1, budget, clipboard)?;
            }
            if !matches!(tag, "br" | "hr" | "img") {
                fragment.html.push_str("</");
                fragment.html.push_str(tag);
                fragment.html.push('>');
            }
            if !matches!(
                tag,
                "span"
                    | "strong"
                    | "b"
                    | "em"
                    | "i"
                    | "s"
                    | "del"
                    | "small"
                    | "mark"
                    | "code"
                    | "kbd"
                    | "sub"
                    | "sup"
                    | "a"
                    | "img"
            ) {
                fragment.text.push('\n');
            }
        }
        NodeData::Document => {
            for child in handle.children.borrow().iter() {
                inert_node(child, fragment, depth + 1, budget, clipboard)?;
            }
        }
        _ => {}
    }
    Some(())
}

#[cfg(test)]
mod inert_tests {
    use super::*;

    #[test]
    fn clipboard_containers_retain_read_only_tasks_and_safe_images() {
        let source = "<aside><input type='checkbox' checked disabled onclick='run()'><img src='https://example.test/p.png' alt='Web'><img src='javascript:run()' alt='Unsafe'></aside>";
        assert_eq!(
            clipboard_html_document(source).unwrap(),
            "<aside><input type=\"checkbox\" disabled checked><img src=\"https://example.test/p.png\" alt=\"Web\"><img alt=\"Unsafe\"></aside>"
        );
        assert!(inert_html_fragment(source).is_none());
        assert!(clipboard_html_document("<input type='text' disabled>").is_none());
        assert!(clipboard_html_document("<input type='checkbox'>").is_none());
    }

    #[test]
    fn clipboard_export_has_real_safe_links_without_renderer_or_active_attributes() {
        let source = "<div id='root' style='background:url(https://remote.test/image)' onclick='run()'><a href='https://example.test/?a=1&amp;b=2'>Web</a><a href='javascript:run()'>Unsafe</a><a href='notes.md#next'>Local</a><img src='photo.png' alt='Photo'><script>run()</script></div>";
        let html = clipboard_html_fragment(source).unwrap();
        assert_eq!(
            html,
            "<div id=\"root\"><a href=\"https://example.test/?a=1&amp;b=2\">Web</a><a>Unsafe</a><a href=\"notes.md#next\">Local</a><img src=\"photo.png\" alt=\"Photo\"></div>"
        );
        assert!(!html.contains("data-mineral"));
        let preview = inert_html_fragment(source).unwrap();
        assert!(preview.html().contains("data-mineral-link"));
        assert!(preview.html().contains("style="));
        assert!(!preview.html().contains("href="));
        assert!(clipboard_html_fragment(&"x".repeat(32 * 1024 + 1)).is_none());
    }

    #[test]
    fn explicit_html_conversion_is_editable_and_undo_restores_original_bytes() {
        use crate::{BlockNode, Document, EditCommand, Selection};
        let source = "# Before\n\n<div style='padding:12px'><p>A <strong>bold</strong> idea and <a href='https://example.test'>a link</a>.</p><p>Second paragraph.</p></div>\n\nAfter\n";
        let mut document = Document::from_markdown(source).unwrap();
        let original = document.snapshot();
        let id = original.blocks().get(1).unwrap().id();
        assert!(matches!(
            original.node(id),
            Some(BlockNode::PreservedSource { .. })
        ));
        document
            .apply(EditCommand::ConvertHtmlToMarkdown { node_id: id })
            .unwrap();
        let snapshot = document.snapshot();
        let Selection::Text(selection) = snapshot.selection() else {
            panic!("editable caret");
        };
        assert!(
            snapshot
                .node(selection.head.node_id)
                .unwrap()
                .text()
                .is_some()
        );
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.contains("**bold**"));
        assert!(saved.contains("[a link](https://example.test)"));
        assert!(saved.starts_with("# Before") && saved.ends_with("After\n"));
        assert!(!saved.contains("<div"));
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), saved);
    }

    #[test]
    fn authored_ids_are_inert_metadata_not_spoofable_dom_ids() {
        let fragment = inert_html_fragment("<p id='café&amp;&quot;' data-mineral-anchor='forged' onclick='bad()'>Body</p><script id='secret'>bad()</script>").unwrap();
        assert!(
            fragment
                .html()
                .contains("data-mineral-anchor=\"café&amp;&quot;\"")
        );
        assert!(!fragment.html().contains(" id="));
        assert!(!fragment.html().contains("forged"));
        assert!(!fragment.html().contains("onclick"));
        assert!(!fragment.html().contains("secret"));
        assert_eq!(fragment.text().trim(), "Body");
    }

    #[test]
    fn disclosure_conversion_keeps_all_authored_text_and_exact_undo() {
        use crate::{Document, EditCommand, HtmlTextEdit};
        let html = "<details open>Before<summary>Repeat</summary><p>Repeat <strong>café</strong></p><details><summary>Inner</summary><p>Hidden <em>body</em></p></details>After</details>";
        let source = format!("# Before\n\n{html}\n\nFollowing content.\n");
        let mut document = Document::from_markdown(source.as_str()).unwrap();
        let original = document.snapshot();
        let block = original.blocks().get(1).unwrap();
        let crate::BlockNode::PreservedSource {
            source: preserved, ..
        } = block.as_ref()
        else {
            panic!("authored HTML must remain preserved");
        };
        // Merely importing and inspecting disclosures must retain authored state.
        assert_eq!(original.serialize().unwrap(), source);
        let texts = editable_html_text_nodes(html).expect("convert disclosure text");
        assert_eq!(
            texts,
            [
                "Before",
                "Repeat",
                "Repeat café",
                "Inner",
                "Hidden body",
                "After"
            ]
        );
        document
            .apply(EditCommand::EditHtmlSelection {
                node_id: block.id(),
                expected_source: preserved.to_string(),
                anchor: HtmlTextPosition {
                    text_node: 2,
                    byte_offset: 7,
                },
                head: HtmlTextPosition {
                    text_node: 2,
                    byte_offset: 7,
                },
                edit: HtmlTextEdit::Replace("edited ".into()),
            })
            .unwrap();
        let converted = document.snapshot().serialize().unwrap();
        assert!(converted.contains("Repeat edited **café**"), "{converted}");
        assert!(converted.contains("Hidden *body*"), "{converted}");
        assert!(!converted.contains("<details"));
        assert!(!converted.contains("Details"));
        assert!(converted.ends_with("Following content.\n"));
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document.redo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), converted);
    }

    #[test]
    fn disclosure_conversion_never_invents_legends_or_drops_unknown_bodies() {
        assert_eq!(
            editable_html_text_nodes("<details><p>Only body</p></details>").unwrap(),
            ["Only body"]
        );
        assert_eq!(
            editable_html_text_nodes("<details><summary></summary>Body</details>").unwrap(),
            ["Body"]
        );
        assert!(editable_html_markdown("<details><summary>Known</summary><custom-widget>Unknown body</custom-widget></details>").is_none());
    }

    #[test]
    fn static_html_keeps_text_styles_and_authored_disclosure_state() {
        let source = "<details open><summary>A &amp; B</summary><p style=\"color:red\">Text <strong>bold</strong></p></details>";
        let fragment = inert_html_fragment(source).unwrap();
        assert!(fragment.html().contains("<details open=\"\">"));
        assert!(fragment.html().contains("style=\"color:red\""));
        assert!(fragment.text().contains("A & B\nText bold"));
        assert!(
            !inert_html_fragment("<details><summary>Closed</summary>Body</details>")
                .unwrap()
                .html()
                .contains("open=")
        );
    }

    #[test]
    fn local_image_references_are_inert_and_conversion_preserves_media() {
        let source = "<div><p>Before <strong>bold</strong>.</p><a href='guide.md'><img src='../assets/chart.png' alt='A &amp; B' title='Chart' width='120' height='60' onerror='bad()' data-mineral-image='99'></a><p>After.</p></div>";
        let fragment = inert_html_fragment(source).expect("local images have an inert descriptor");
        assert!(fragment.html().contains("data-mineral-image=\"0\""));
        assert!(!fragment.html().contains("src="));
        assert!(!fragment.html().contains("onerror"));
        assert!(!fragment.html().contains("99"));
        assert_eq!(
            fragment.images(),
            &[InertHtmlImage {
                source: "../assets/chart.png".into(),
                alt: "A & B".into()
            }]
        );
        let markdown = editable_html_markdown(source).unwrap();
        assert!(
            markdown.contains("[![A \\& B](../assets/chart.png \"Chart\")](guide.md)"),
            "{markdown}"
        );
        assert!(markdown.contains("**bold**"));
        let imported = crate::markdown::import(markdown.into()).unwrap();
        let blocks = imported.blocks().iter().collect::<Vec<_>>();
        assert_eq!(
            blocks.len(),
            3,
            "image and following paragraph stay separate"
        );
        assert_eq!(blocks[0].plain_text(), "Before bold.");
        assert_eq!(blocks[1].plain_text(), "A & B");
        assert_eq!(blocks[2].plain_text(), "After.");
        let crate::BlockNode::Image(image) = blocks[1].as_ref() else {
            panic!("image lost in conversion")
        };
        assert_eq!(image.source, "../assets/chart.png");
        assert_eq!(image.title.as_deref(), Some("Chart"));
        assert_eq!(image.link.as_ref().unwrap().target.0, "guide.md");

        let mut document = crate::Document::from_markdown(source).unwrap();
        let node_id = document.snapshot().blocks().iter().next().unwrap().id();
        document
            .apply(crate::EditCommand::ConvertHtmlToMarkdown { node_id })
            .unwrap();
        assert_eq!(document.snapshot().blocks().len(), 3);
        let restored = document.undo().unwrap();
        assert_eq!(restored.serialize().unwrap(), source);
    }

    #[test]
    fn html_media_rows_convert_to_figures_without_rewriting_inline_icons() {
        let source = "<div><a href='guide.md'><img src='a.png' alt='First'></a> <img src='b.png' alt='Second'></div>";
        let markdown = editable_html_markdown(source).unwrap();
        let imported = crate::Document::from_markdown(markdown).unwrap();
        let snapshot = imported.snapshot();
        assert_eq!(snapshot.blocks().len(), 2);
        let crate::BlockNode::Image(first) = snapshot.blocks().get(0).unwrap().as_ref() else {
            panic!("expected first figure")
        };
        let crate::BlockNode::Image(second) = snapshot.blocks().get(1).unwrap().as_ref() else {
            panic!("expected second figure")
        };
        assert_eq!(first.source, "a.png");
        assert_eq!(first.link.as_ref().unwrap().target.0, "guide.md");
        assert_eq!(second.source, "b.png");
        let inline =
            editable_html_markdown("<p>Before <img src='a.png' alt='icon'> after.</p>").unwrap();
        let inline = crate::Document::from_markdown(inline).unwrap();
        assert_eq!(inline.snapshot().blocks().len(), 1);
    }

    #[test]
    fn image_descriptors_reject_active_sources_and_ambiguous_variants() {
        for source in [
            "",
            "/etc/passwd",
            "file:///etc/passwd",
            "https://example.test/a.png",
            "//example.test/a.png",
            "data:image/png;base64,abc",
            "a.png?token=x",
            "a.png#part",
            "a\\b.png",
            "a\nb.png",
        ] {
            assert!(
                inert_html_fragment(&format!("<img src='{source}'>")).is_none(),
                "{source}"
            );
        }
        for attributes in [
            "srcset='other.png 2x'",
            "width='0'",
            "height='-1'",
            "width='50000'",
            "height='auto'",
        ] {
            assert!(
                inert_html_fragment(&format!("<img src='local.png' {attributes}>")).is_none(),
                "{attributes}"
            );
        }
        assert!(inert_html_fragment(&"<img src='local.png'>".repeat(33)).is_none());
        let repeated = inert_html_fragment("<img src='a.png'><img src='a.png'>").unwrap();
        assert_eq!(repeated.images().len(), 2);
        assert!(repeated.html().contains("data-mineral-image=\"1\""));
    }

    #[test]
    fn active_resources_and_unsupported_structures_do_not_enter_the_renderer() {
        let source = "<script>secret()</script><iframe src='https://example.test'>hidden</iframe><p onclick='bad()' tabindex='0'><a href='javascript:bad()' data-mineral-link='99'>Visible</a></p>";
        let fragment = inert_html_fragment(source).unwrap();
        assert_eq!(
            fragment.html(),
            "<p><a data-mineral-link=\"0\">Visible</a></p>"
        );
        assert_eq!(fragment.link_targets(), &["javascript:bad()"]);
        assert!(!fragment.html().contains("javascript:"));
        assert_eq!(fragment.text(), "Visible\n");
        for unsupported in [
            "<img src='file:///etc/passwd'>",
            "<svg><text>X</text></svg>",
            "<input value='x'>",
            "<custom-widget>Keep me</custom-widget>",
        ] {
            assert!(inert_html_fragment(unsupported).is_none());
        }
        assert!(inert_html_fragment(&"<div>".repeat(40)).is_none());
        assert!(inert_html_fragment(&"x".repeat(32769)).is_none());
    }
}

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

/// A complete semantic HTML glossary, including separate empty authored
/// paragraphs. Native import is conservative: unfamiliar wrappers, attributes
/// or children remain in the lossless inert-HTML path.
pub(crate) fn html_definition_data(
    source: &str,
) -> Option<Vec<(crate::DefinitionKind, Vec<String>)>> {
    inert_html_fragment(source)?;
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(source);
    let body = descendants_named(&dom.document, "body")
        .into_iter()
        .next()?;
    let meaningful = |child: &&Handle| {
        !matches!(&child.data,
        NodeData::Text { contents } if contents.borrow().trim().is_empty())
    };
    let body_children = body.children.borrow();
    let mut content = body_children.iter().filter(meaningful);
    let list = content.next()?;
    if element_name(list) != Some("dl") || content.next().is_some() {
        return None;
    }
    let no_attributes = |node: &Handle| matches!(&node.data, NodeData::Element { attrs, .. } if attrs.borrow().is_empty());
    if !no_attributes(list) {
        return None;
    }
    let mut groups = Vec::new();
    for group in list.children.borrow().iter().filter(meaningful) {
        let kind = match element_name(group) {
            Some("dt") => crate::DefinitionKind::Term,
            Some("dd") => crate::DefinitionKind::Description,
            _ => return None,
        };
        if !no_attributes(group) {
            return None;
        }
        let mut blocks = Vec::new();
        let mut inline = String::new();
        for child in group.children.borrow().iter() {
            let block = matches!(
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
                        | "dl"
                        | "table"
                        | "hr"
                        | "div"
                        | "details"
                )
            );
            if block {
                if !inline.trim().is_empty() {
                    blocks.push(inline.trim_matches('\n').to_owned());
                }
                inline.clear();
                let mut markdown = String::new();
                if matches!(element_name(child), Some("div" | "details")) {
                    render_preserved_element(child, &mut markdown);
                } else {
                    render_node(child, &mut markdown, 0, true);
                }
                blocks.push(markdown.trim_matches('\n').to_owned());
            } else {
                render_node(child, &mut inline, 0, true);
            }
        }
        if !inline.trim().is_empty() {
            blocks.push(inline.trim_matches('\n').to_owned());
        }
        if blocks.iter().any(|s| s.contains("<!-- Unsupported")) {
            return None;
        }
        groups.push((kind, blocks));
    }
    Some(groups)
}

pub(crate) fn html_table_data(source: &str) -> Option<HtmlTableData> {
    let dom = parse_document(RcDom::default(), ParseOpts::default()).one(source);
    if descendants_named(&dom.document, "head").iter().any(|head| {
        head.children
            .borrow()
            .iter()
            .any(|child| matches!(child.data, NodeData::Element { .. }))
    }) {
        return None;
    }
    // Adapt only a complete standalone table. Searching for any descendant
    // silently drops wrapper text, following blocks and additional tables.
    let body = descendants_named(&dom.document, "body")
        .into_iter()
        .next()?;
    let children = body.children.borrow();
    let mut content = children.iter().filter(|child| match &child.data {
        NodeData::Text { contents } => !contents.borrow().trim().is_empty(),
        NodeData::Comment { .. } => false,
        _ => true,
    });
    let table = Rc::clone(content.next()?);
    if element_name(&table) != Some("table") || content.next().is_some() {
        return None;
    }
    if !descendants_named(&table, "table").is_empty() || table_has_spanning_cells(&table) {
        return None;
    }
    // GFM cannot represent descending, per-item, negative, or oversized
    // ordinals. Keep the complete authored table in the inert HTML path.
    if descendants_named(&table, "ol")
        .iter()
        .any(|list| markdown_list_start(list).is_none())
    {
        return None;
    }
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

// The canonical Markdown table has no span model. Preserve nontrivial or
// uncertain HTML spans verbatim for Blitz, rather than guessing relationships.
fn table_has_spanning_cells(table: &Handle) -> bool {
    ["th", "td"].into_iter().any(|tag| {
        descendants_named(table, tag).iter().any(|cell| {
            ["colspan", "rowspan"].into_iter().any(|attribute| {
                attribute_for_handle(cell, attribute)
                    .is_some_and(|value| value.trim().parse::<u32>() != Ok(1))
            })
        })
    })
}

fn render_table_cell(cell: &Handle, output: &mut String) {
    let preserved_container = |child: &Handle| {
        matches!(
            element_name(child),
            Some("details" | "div" | "section" | "article")
        ) || !descendants_named(child, "details").is_empty()
    };
    let has_block_children = cell.children.borrow().iter().any(|child| {
        preserved_container(child)
            || matches!(
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
        // Table import is presentation, not an explicit rich-text conversion.
        // Retain authored disclosure state and styled block containers for the
        // inert fragment renderer; first editing remains an explicit conversion.
        if preserved_container(child) {
            render_preserved_element(child, output);
        } else {
            render_node(child, output, 0, false);
        }
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
    // A media-only block container maps to consecutive standalone figures,
    // not one text run of image alt labels. Paragraph-inline icons remain inline.
    if matches!(
        element_name(handle),
        Some("div" | "section" | "article" | "main" | "body")
    ) {
        let children = handle.children.borrow();
        let figures = children.iter().filter(|child| !matches!(&child.data, NodeData::Text { contents } if contents.borrow().trim().is_empty())).collect::<Vec<_>>();
        if figures.len() >= 2 && figures.iter().all(|child| image_only(child)) {
            for child in figures {
                paragraph_boundary(output);
                render_node(child, output, depth, tables_allowed);
                paragraph_boundary(output);
            }
            return;
        }
    }
    for child in handle.children.borrow().iter() {
        render_node(child, output, depth, tables_allowed);
    }
}

fn image_only(handle: &Handle) -> bool {
    match element_name(handle) {
        Some("img") => true,
        Some("a") => {
            let children = handle.children.borrow();
            let meaningful = children.iter().filter(|child| !matches!(&child.data, NodeData::Text { contents } if contents.borrow().trim().is_empty())).collect::<Vec<_>>();
            meaningful.len() == 1 && element_name(meaningful[0]) == Some("img")
        }
        _ => false,
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
                "p" => {
                    paragraph_boundary(output);
                    block_wrap(handle, output, depth, tables_allowed, "", "\n\n");
                }
                // Conversion is an explicit content edit, not disclosure
                // navigation. Keep even closed bodies in source order and do
                // not invent a default legend, heading, or quote semantics.
                // Boundaries on both sides also separate direct text siblings.
                "details" | "summary" => {
                    paragraph_boundary(output);
                    render_children(handle, output, depth, tables_allowed);
                    paragraph_boundary(output);
                }
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
                    let code =
                        element_children(handle).find(|child| element_name(child) == Some("code"));
                    let language = code
                        .as_ref()
                        .and_then(|code| attribute_for_handle(code, "class"))
                        .and_then(|class| {
                            class
                                .split_whitespace()
                                .find_map(|c| c.strip_prefix("language-").map(str::to_owned))
                        })
                        .filter(|language| {
                            language.chars().all(|c| {
                                c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '+' | '.')
                            })
                        });
                    let source = descendant_text(handle);
                    let fence = "`".repeat(
                        source
                            .split(|c| c != '`')
                            .map(str::len)
                            .max()
                            .unwrap_or(0)
                            .max(2)
                            + 1,
                    );
                    output.push_str(&fence);
                    output.push_str(language.as_deref().unwrap_or_default());
                    output.push('\n');
                    output.push_str(&source);
                    if !output.ends_with('\n') {
                        output.push('\n');
                    }
                    output.push_str(&fence);
                    output.push_str("\n\n");
                }
                "a" => {
                    output.push('[');
                    render_children(handle, output, depth, tables_allowed);
                    output.push_str("](");
                    output.push_str(&crate::markdown::serialize_destination(
                        &attribute(attrs, "href").unwrap_or_default(),
                    ));
                    if let Some(title) = attribute(attrs, "title") {
                        output.push(' ');
                        output.push_str(&crate::markdown::serialize_title(&title));
                    }
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
                "span"
                    if attribute(attrs, "style")
                        .is_some_and(|style| style.trim() == "white-space: pre-wrap")
                        && handle
                            .children
                            .borrow()
                            .iter()
                            .all(|child| matches!(&child.data, NodeData::Text { .. })) =>
                {
                    for child in handle.children.borrow().iter() {
                        if let NodeData::Text { contents } = &child.data {
                            let escaped = crate::markdown::escape_inline(&contents.borrow());
                            for character in escaped.chars() {
                                match character {
                                    ' ' => output.push_str("&#32;"),
                                    '\t' => output.push_str("&#9;"),
                                    '\n' => output.push_str("&#10;"),
                                    '\r' => output.push_str("&#13;"),
                                    _ => output.push(character),
                                }
                            }
                        }
                    }
                }
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
                "ul" => render_list(handle, output, false, tables_allowed),
                "ol" => render_list(handle, output, true, tables_allowed),
                "li" => render_children(handle, output, depth, tables_allowed),
                "dl" => render_preserved_element(handle, output),
                "hr" => output.push_str("---\n\n"),
                "table" if tables_allowed => render_preserved_element(handle, output),
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

fn paragraph_boundary(output: &mut String) {
    if !output.is_empty() {
        if !output.ends_with('\n') {
            output.push('\n');
        }
        if !output.ends_with("\n\n") {
            output.push('\n');
        }
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

fn render_list(handle: &Handle, output: &mut String, ordered: bool, tables_allowed: bool) {
    // A nested list can follow an item's inline text without an intervening
    // paragraph element. Its marker must start a new line before the parent
    // adds continuation indentation.
    if !output.is_empty() && !output.ends_with('\n') {
        output.push('\n');
    }
    let mut ordinal = if ordered {
        let Some(start) = markdown_list_start(handle) else {
            render_preserved_element(handle, output);
            return;
        };
        start
    } else {
        1
    };
    for child in element_children(handle).filter(|child| element_name(child) == Some("li")) {
        // Render item content independently: paragraph boundaries must not
        // separate the marker from its first paragraph. This item owns all
        // continuation indentation, including nested lists and code fences.
        let mut content = String::new();
        let mut checked = None;
        for node in child.children.borrow().iter() {
            if matches!(&node.data, NodeData::Text { contents } if contents.borrow().trim().is_empty() && contents.borrow().contains('\n'))
            {
                continue;
            }
            // Canonical task controls precede the item's first block. Keep
            // their state separate from paragraph-boundary generation.
            if content.trim().is_empty()
                && checked.is_none()
                && element_name(node) == Some("input")
                && attribute_for_handle(node, "type")
                    .is_some_and(|kind| kind.eq_ignore_ascii_case("checkbox"))
            {
                checked = Some(attribute_for_handle(node, "checked").is_some());
                continue;
            }
            if content.trim().is_empty() && checked.is_none() && element_name(node) == Some("p") {
                let children = node.children.borrow();
                let mut meaningful = children.iter().skip_while(|child| {
                    matches!(&child.data, NodeData::Text { contents } if contents.borrow().trim().is_empty())
                });
                if let Some(first) = meaningful.next()
                    && element_name(first) == Some("input")
                    && attribute_for_handle(first, "type")
                        .is_some_and(|kind| kind.eq_ignore_ascii_case("checkbox"))
                {
                    checked = Some(attribute_for_handle(first, "checked").is_some());
                    for child in meaningful {
                        render_node(child, &mut content, 0, tables_allowed);
                    }
                    content.push_str("\n\n");
                    continue;
                }
            }
            render_node(node, &mut content, 0, tables_allowed);
        }
        let marker = if ordered {
            let marker = format!("{ordinal}. ");
            ordinal += 1;
            marker
        } else {
            "- ".to_owned()
        };
        output.push_str(&marker);
        if let Some(checked) = checked {
            output.push_str(if checked { "[x] " } else { "[ ] " });
        }
        let continuation = " ".repeat(marker.len());
        let content = content.trim_matches('\n');
        for (index, line) in content.split('\n').enumerate() {
            if index > 0 {
                output.push_str(&continuation);
            }
            output.push_str(line);
            output.push('\n');
        }
    }
    output.push('\n');
}

/// Only ascending decimal ordinals within Markdown's nine-digit marker
/// range have a lossless representation in the canonical Markdown list.
fn markdown_list_start(handle: &Handle) -> Option<u64> {
    if attribute_for_handle(handle, "reversed").is_some()
        || attribute_for_handle(handle, "type").is_some_and(|kind| kind != "1")
    {
        return None;
    }
    let start = match attribute_for_handle(handle, "start") {
        Some(value) => value.trim().parse::<u64>().ok()?,
        None => 1,
    };
    let mut count = 0_u64;
    for item in element_children(handle).filter(|child| element_name(child) == Some("li")) {
        if attribute_for_handle(&item, "value").is_some() {
            return None;
        }
        count += 1;
    }
    (start.checked_add(count.saturating_sub(1))? <= 999_999_999).then_some(start)
}

fn render_preserved_element(handle: &Handle, output: &mut String) {
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
    fn first_edit_of_a_cell_fragment_is_local_atomic_and_composition_safe() {
        let source = include_str!("../../../performance/layout-fixtures/36-rich-cell-panels.md");
        let mut document = Document::from_markdown(source).unwrap();
        let before = document.snapshot();
        let table = before
            .blocks()
            .iter()
            .filter_map(|block| match block.as_ref() {
                BlockNode::Table(table) => Some(table),
                _ => None,
            })
            .nth(1)
            .unwrap();
        let table_id = table.id;
        let fragment = table.rows[1].cells[0].blocks.get(0).unwrap();
        let node_id = fragment.id();
        let BlockNode::PreservedSource { source: html, .. } = fragment.as_ref() else {
            panic!("cell HTML")
        };
        let neighbor = table.rows[1].cells[1].blocks.get(0).unwrap().clone();
        let position = HtmlTextPosition {
            text_node: 1,
            byte_offset: 7,
        };
        document
            .apply(EditCommand::EditHtmlSelection {
                node_id,
                expected_source: html.to_string(),
                anchor: position,
                head: position,
                edit: crate::HtmlTextEdit::Replace("edited ".into()),
            })
            .unwrap();
        let edited = document.snapshot();
        let BlockNode::Table(table) = edited.node(table_id).unwrap() else {
            panic!("table retained")
        };
        assert!(Arc::ptr_eq(
            &neighbor,
            table.rows[1].cells[1].blocks.get(0).unwrap()
        ));
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[1].cells.len(), 2);
        assert!(
            edited
                .serialize()
                .unwrap()
                .contains("Visibleedited  cell body")
        );
        assert!(
            edited
                .serialize()
                .unwrap()
                .contains("<strong>strong text</strong>")
        );
        assert!(edited.node(node_id).is_none());
        let closed_html = "<details><summary>Closed cell details</summary><p>Closed cell body marker.</p></details>";
        let saved = edited.serialize().unwrap();
        assert!(saved.contains(closed_html), "{saved}");
        let reopened = Document::from_markdown(saved).unwrap();
        let reopened = reopened.snapshot();
        let reopened_table = reopened
            .blocks()
            .iter()
            .filter_map(|block| match block.as_ref() {
                BlockNode::Table(table) => Some(table),
                _ => None,
            })
            .nth(1)
            .unwrap();
        assert!(
            matches!(reopened_table.rows[2].cells[0].blocks.get(0).unwrap().as_ref(),
            BlockNode::PreservedSource { source, .. } if source.as_ref() == closed_html),
            "save/reopen must preserve the untouched disclosure, not its plain-text description: {:?}",
            reopened_table.rows[2].cells[0].blocks
        );
        let crate::Selection::Text(selection) = edited.selection() else {
            panic!("text caret")
        };
        assert_eq!(
            edited.table_cell_containing(selection.head.node_id),
            Some((table_id, 1, 0))
        );
        assert_eq!(document.undo().unwrap().serialize().unwrap(), source);
        document
            .begin_html_composition(node_id, html, position, position)
            .unwrap();
        document.update_composition("仮".into()).unwrap();
        assert!(document.snapshot().serialize().unwrap().contains('仮'));
        document.cancel_composition().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn spanning_html_cells_keep_geometry_and_never_offer_lossy_conversion() {
        for attributes in [
            "colspan='2'",
            "rowspan='2'",
            "rowspan='0'",
            "colspan='unknown'",
        ] {
            let source = format!(
                "<table><tr><th {attributes}>Group</th><th>Other</th></tr><tr><td>First</td><td>Second</td></tr></table>\n"
            );
            assert!(html_table_data(&source).is_none(), "{attributes}");
            assert!(editable_html_markdown(&source).is_none(), "{attributes}");
            assert!(editable_html_text_nodes(&source).is_none(), "{attributes}");
            let mut document =
                Document::from_markdown(format!("{source}\nEditable neighbor.\n")).unwrap();
            let original = document.snapshot();
            assert!(
                matches!(original.blocks().get(0).unwrap().as_ref(), BlockNode::PreservedSource { source: kept, .. } if kept.as_ref() == source)
            );
            document
                .apply(EditCommand::ReplaceText {
                    node_id: original.blocks().get(1).unwrap().id(),
                    range: 0..0,
                    text: "Changed ".into(),
                    selection_after: None,
                    typing: false,
                })
                .unwrap();
            let saved = document.snapshot().serialize().unwrap();
            assert!(saved.contains(&source));
            assert!(
                matches!(Document::from_markdown(saved).unwrap().snapshot().blocks().get(0).unwrap().as_ref(), BlockNode::PreservedSource { source: kept, .. } if kept.as_ref() == source)
            );
            assert_eq!(
                document.undo().unwrap().serialize().unwrap(),
                original.serialize().unwrap()
            );
        }
        assert!(
            html_table_data("<table><tr><td colspan='1' rowspan='1'>Ordinary</td></tr></table>")
                .is_some()
        );
    }

    #[test]
    fn inert_table_alignment_is_retained_without_active_attributes() {
        let fragment = inert_html_fragment(
            "<table><tr align='center'><td align='right' onclick='bad()'>17</td></tr></table>",
        )
        .unwrap();
        assert!(fragment.html().contains("align=\"center\""));
        assert!(fragment.html().contains("align=\"right\""));
        assert!(!fragment.html().contains("onclick"));
    }

    #[test]
    fn only_standalone_html_tables_enter_the_typed_table_adapter() {
        let table = "<table><tr><th>Key</th><th>Value</th></tr><tr><td>Mode</td><td>Local</td></tr></table>";
        assert!(html_table_data(table).is_some());
        for source in [
            format!("<div><p>Before marker</p>{table}<p>After marker</p></div>"),
            format!("<html><head><title>Authored title</title></head><body>{table}</body></html>"),
            format!("<section>{table}<p>After marker</p></section>"),
            format!("{table}\n{table}"),
            format!("{table}\n<p>After marker</p>"),
        ] {
            assert!(
                html_table_data(&source).is_none(),
                "adapter would discard surrounding content: {source}"
            );
            let document = Document::from_markdown(source.clone()).unwrap();
            let snapshot = document.snapshot();
            assert!(
                matches!(snapshot.blocks().get(0).unwrap().as_ref(), BlockNode::PreservedSource { source: preserved, .. } if preserved.as_ref() == source)
            );
            assert_eq!(snapshot.serialize().unwrap(), source);
        }
        let source = format!("<div><p>Before marker</p>{table}<p>After marker</p></div>");
        let converted = editable_html_markdown(&source).unwrap();
        for marker in ["Before marker", "Mode", "Local", "After marker"] {
            assert_eq!(converted.matches(marker).count(), 1, "{converted}");
        }
    }

    #[test]
    fn nested_html_tables_preserve_complete_source_instead_of_placeholder_cells() {
        let source = include_str!("../../../performance/layout-fixtures/37-nested-html-tables.md");
        let mut document = Document::from_markdown(source).unwrap();
        let original = document.snapshot();
        let fragment = original
            .blocks()
            .iter()
            .find_map(|block| match block.as_ref() {
                BlockNode::PreservedSource { source, .. } => Some(source.clone()),
                _ => None,
            })
            .expect("nested HTML must use the complete inert fragment renderer");
        assert!(fragment.contains("<strong>Keep all evidence</strong>"));
        assert!(
            editable_html_markdown(&fragment).is_none(),
            "do not offer conversion that loses inner cells"
        );
        let paragraph = original.blocks().iter().next_back().unwrap();
        document
            .apply(EditCommand::ReplaceText {
                node_id: paragraph.id(),
                range: 0..0,
                text: "Edited: ".into(),
                selection_after: None,
                typing: false,
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.contains(fragment.as_ref()));
        assert!(!saved.contains("Unsupported nested table"));
        let reopened = Document::from_markdown(saved).unwrap();
        assert!(
            reopened
                .snapshot()
                .blocks()
                .iter()
                .any(|block| matches!(block.as_ref(),
            BlockNode::PreservedSource { source, .. } if source == &fragment))
        );
        assert_eq!(document.undo().unwrap().serialize().unwrap(), source);
    }

    #[test]
    fn table_disclosures_keep_their_html_and_authored_state_until_edited() {
        let source = include_str!("../../../performance/layout-fixtures/36-rich-cell-panels.md");
        let document = Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let table = snapshot
            .blocks()
            .iter()
            .filter_map(|block| match block.as_ref() {
                BlockNode::Table(table) => Some(table),
                _ => None,
            })
            .nth(1)
            .unwrap();
        for (row, open) in [(1, true), (2, false)] {
            let block = table.rows[row].cells[0].blocks.get(0).unwrap();
            let BlockNode::PreservedSource { source, .. } = block.as_ref() else {
                panic!("authored disclosure flattened into Markdown")
            };
            assert_eq!(source.contains("<details open"), open);
            assert!(source.contains("cell body"));
            assert!(editable_html_markdown(source).is_some());
        }
        assert_eq!(snapshot.serialize().unwrap(), source);
    }

    #[test]
    fn html_ordered_list_start_survives_rich_cell_import_and_conversion() {
        for start in [0, 9, 42, 999_999_998] {
            let list = format!("<ol start='{start}'><li>First</li><li>Second</li></ol>");
            let markdown = editable_html_markdown(&list).unwrap();
            let document = Document::from_markdown(markdown).unwrap();
            let snapshot = document.snapshot();
            let BlockNode::List(imported) = snapshot.blocks().get(0).unwrap().as_ref() else {
                panic!("ordered list")
            };
            assert_eq!(imported.kind, crate::ListKind::Ordered { start });
            let source = format!("<table><tr><td>{list}</td><td>Neighbor</td></tr></table>");
            let mut document = Document::from_markdown(source.as_str()).unwrap();
            let snapshot = document.snapshot();
            let BlockNode::Table(table) = snapshot.blocks().get(0).unwrap().as_ref() else {
                panic!("rich table")
            };
            let BlockNode::List(imported) = table.rows[0].cells[0].blocks.get(0).unwrap().as_ref()
            else {
                panic!("cell list")
            };
            assert_eq!(imported.kind, crate::ListKind::Ordered { start });
            assert_eq!(imported.items.len(), 2);
            assert_eq!(snapshot.serialize().unwrap(), source);
            let node = imported.items[0].blocks.get(0).unwrap().id();
            document
                .apply(EditCommand::SetSelection(crate::Selection::Text(
                    crate::TextSelection::caret(crate::DocumentPosition::new(
                        node,
                        0,
                        crate::Affinity::Downstream,
                    )),
                )))
                .unwrap();
            document
                .apply(EditCommand::ReplaceSelection {
                    text: "Edited ".into(),
                    typing: false,
                })
                .unwrap();
            assert_eq!(document.undo().unwrap().serialize().unwrap(), source);
        }
    }

    #[test]
    fn html_non_markdown_numbering_stays_preserved_instead_of_being_renumbered() {
        for attributes in [
            "start='-2'",
            "start='1000000000'",
            "start='999999999'",
            "reversed",
            "type='A'",
            "start='not-a-number'",
        ] {
            let list = format!("<ol {attributes}><li>First</li><li>Second</li></ol>");
            assert!(editable_html_markdown(&list).is_none(), "{attributes}");
            let source = format!("<table><tr><td>{list}</td></tr></table>");
            assert!(html_table_data(&source).is_none());
            let document = Document::from_markdown(source.as_str()).unwrap();
            assert!(matches!(
                document.snapshot().blocks().get(0).unwrap().as_ref(),
                BlockNode::PreservedSource { .. }
            ));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            let converted = html_fragment_to_markdown(&list).unwrap();
            assert!(converted.contains("First") && converted.contains("Second"));
        }
        assert!(
            editable_html_markdown("<ol><li value='7'>First</li><li>Second</li></ol>").is_none()
        );
        let inert = inert_html_fragment("<ol reversed type='A'><li>First</li></ol>").unwrap();
        assert!(inert.html().contains("reversed") && inert.html().contains("type=\"A\""));
    }

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
    #[test]
    fn html_conversion_targets_duplicate_unicode_text_and_rejects_stale_addresses() {
        use crate::{Document, EditCommand, Selection};
        let html =
            "<div><p>Repeat <strong>café</strong></p><p>Repeat <strong>café</strong></p></div>";
        let source = format!("# Before\n\n{html}\n\nAfter\n");
        let mut document = Document::from_markdown(source.as_str()).unwrap();
        let original = document.snapshot();
        let node_id = original.blocks().get(1).unwrap().id();
        let crate::BlockNode::PreservedSource {
            source: preserved, ..
        } = original.node(node_id).unwrap()
        else {
            panic!("preserved HTML")
        };
        assert_eq!(
            editable_html_text_nodes(html).unwrap(),
            ["Repeat café", "Repeat café"]
        );
        for (expected_source, byte_offset) in [
            ("stale", 7),
            (preserved.as_ref(), 11),
            (preserved.as_ref(), 99),
        ] {
            assert!(
                document
                    .apply(EditCommand::ConvertHtmlToMarkdownAt {
                        node_id,
                        expected_source: expected_source.into(),
                        position: HtmlTextPosition {
                            text_node: 1,
                            byte_offset
                        },
                    })
                    .is_err()
            );
            assert_eq!(document.snapshot().revision(), original.revision());
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
        document
            .apply(EditCommand::ConvertHtmlToMarkdownAt {
                node_id,
                expected_source: preserved.to_string(),
                position: HtmlTextPosition {
                    text_node: 1,
                    byte_offset: 8,
                },
            })
            .unwrap();
        let snapshot = document.snapshot();
        let Selection::Text(selection) = snapshot.selection() else {
            panic!("text selection")
        };
        assert_eq!(
            selection.head.node_id,
            snapshot.blocks().get(2).unwrap().id()
        );
        assert_eq!(selection.head.text_offset, 8);
        document
            .apply(EditCommand::ReplaceSelection {
                text: "X".into(),
                typing: true,
            })
            .unwrap();
        let saved = document.snapshot().serialize().unwrap();
        assert!(saved.contains("Repeat **café**"));
        assert!(saved.contains("Repeat **cXafé**"));
        document.undo().unwrap();
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn first_html_text_edit_is_atomic_and_preserves_rich_text() {
        use crate::{Document, EditCommand, HtmlTextEdit};
        let html = "<div><p>Repeat <strong>café</strong></p><p>Repeat <strong>café</strong> <a href='https://example.test'>link</a></p></div>";
        let source = format!("# Before\n\n{html}\n\nAfter\n");
        let mut document = Document::from_markdown(source.as_str()).unwrap();
        let snapshot = document.snapshot();
        let node = snapshot.blocks().get(1).unwrap();
        let crate::BlockNode::PreservedSource {
            source: preserved, ..
        } = node.as_ref()
        else {
            panic!("HTML")
        };
        for expected in ["stale", preserved.as_ref()] {
            let command = EditCommand::EditHtmlSelection {
                node_id: node.id(),
                expected_source: expected.into(),
                anchor: HtmlTextPosition {
                    text_node: 1,
                    byte_offset: 7,
                },
                head: HtmlTextPosition {
                    text_node: 1,
                    byte_offset: 12,
                },
                edit: HtmlTextEdit::Replace("tea".into()),
            };
            if expected == "stale" {
                assert!(document.apply(command).is_err());
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            } else {
                document.apply(command).unwrap();
                let edited = document.snapshot().serialize().unwrap();
                assert!(edited.contains("Repeat **café**"));
                // Replacing a whole run inherits the preceding insertion
                // style, exactly like ordinary Markdown editing. Conversion
                // must not invent a different formatting policy.
                assert!(
                    edited.contains("Repeat tea [link](https://example.test)"),
                    "{edited}"
                );
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                document.redo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), edited);
            }
        }
    }

    #[test]
    fn atomic_html_edit_matches_explicit_conversion_and_normal_edit() {
        use crate::{Document, EditCommand, HtmlTextEdit, Selection, TextSelection};
        let source =
            "<div><p>Repeat <strong>café</strong></p><p>Repeat <strong>café</strong></p></div>\n";
        for edit in [
            HtmlTextEdit::Replace("X".into()),
            HtmlTextEdit::Replace(String::new()),
            HtmlTextEdit::PasteMarkdown("**new**".into()),
            HtmlTextEdit::Format(crate::InlineFormat::Italic),
            HtmlTextEdit::Link(Some("https://example.test".into())),
            HtmlTextEdit::Split,
        ] {
            let mut atomic = Document::from_markdown(source).unwrap();
            let mut reference = Document::from_markdown(source).unwrap();
            let original = atomic.snapshot();
            let node = original.blocks().get(0).unwrap();
            let crate::BlockNode::PreservedSource { source: html, .. } = node.as_ref() else {
                panic!("HTML")
            };
            let anchor = HtmlTextPosition {
                text_node: 1,
                byte_offset: 8,
            };
            let head = HtmlTextPosition {
                text_node: 1,
                byte_offset: 10,
            };
            reference
                .apply(EditCommand::ConvertHtmlToMarkdownAt {
                    node_id: node.id(),
                    expected_source: html.to_string(),
                    position: anchor,
                })
                .unwrap();
            let Selection::Text(mut selection) = reference.snapshot().selection().clone() else {
                panic!("caret")
            };
            selection.head.text_offset = head.byte_offset;
            reference
                .apply(EditCommand::SetSelection(Selection::Text(TextSelection {
                    ..selection
                })))
                .unwrap();
            reference
                .apply(match edit.clone() {
                    HtmlTextEdit::Replace(text) => EditCommand::ReplaceSelection {
                        text,
                        typing: false,
                    },
                    HtmlTextEdit::PasteMarkdown(markdown) => {
                        EditCommand::PasteMarkdown { markdown }
                    }
                    HtmlTextEdit::Format(format) => EditCommand::ToggleInlineSelection { format },
                    HtmlTextEdit::Link(target) => EditCommand::SetLinkSelection { target },
                    HtmlTextEdit::Split => EditCommand::SplitSelection,
                })
                .unwrap();
            atomic
                .apply(EditCommand::EditHtmlSelection {
                    node_id: node.id(),
                    expected_source: html.to_string(),
                    anchor,
                    head,
                    edit,
                })
                .unwrap();
            assert_eq!(
                atomic.snapshot().serialize().unwrap(),
                reference.snapshot().serialize().unwrap()
            );
            atomic.undo().unwrap();
            assert_eq!(atomic.snapshot().serialize().unwrap(), source);
            assert!(matches!(
                atomic.undo(),
                Err(crate::DocumentError::NothingToUndo)
            ));
        }
    }

    #[test]
    fn html_first_edit_rejects_bad_targets_without_history_or_source_changes() {
        use crate::{Document, EditCommand, HtmlTextEdit};
        let source = "<div>café</div>\n";
        let mut document = Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let node = snapshot.blocks().get(0).unwrap();
        let crate::BlockNode::PreservedSource { source: html, .. } = node.as_ref() else {
            panic!("HTML")
        };
        let start = HtmlTextPosition {
            text_node: 0,
            byte_offset: 0,
        };
        for head in [
            HtmlTextPosition {
                text_node: 99,
                byte_offset: 0,
            },
            HtmlTextPosition {
                text_node: 0,
                byte_offset: 4,
            },
        ] {
            assert!(
                document
                    .apply(EditCommand::EditHtmlSelection {
                        node_id: node.id(),
                        expected_source: html.to_string(),
                        anchor: start,
                        head,
                        edit: HtmlTextEdit::Replace("X".into()),
                    })
                    .is_err()
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
        document
            .apply(EditCommand::EditHtmlSelection {
                node_id: node.id(),
                expected_source: html.to_string(),
                anchor: start,
                head: start,
                edit: HtmlTextEdit::Replace(String::new()),
            })
            .unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        assert!(matches!(
            document.undo(),
            Err(crate::DocumentError::NothingToUndo)
        ));
    }

    #[test]
    fn html_composition_reuses_converted_ids_and_restores_the_original_transaction() {
        use crate::{Document, DocumentError, Selection};
        let source =
            "<div><p>Repeat <strong>café</strong></p><p>Repeat <strong>café</strong></p></div>\n";
        for commit in [false, true] {
            let mut document = Document::from_markdown(source).unwrap();
            let before = document.snapshot();
            let node = before.blocks().get(0).unwrap();
            let crate::BlockNode::PreservedSource { source: html, .. } = node.as_ref() else {
                panic!("HTML")
            };
            let target = HtmlTextPosition {
                text_node: 1,
                byte_offset: 8,
            };
            assert!(
                document
                    .begin_html_composition(node.id(), "stale", target, target)
                    .is_err()
            );
            assert!(!document.composition_active());
            document
                .begin_html_composition(node.id(), html, target, target)
                .unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
            assert_eq!(document.snapshot().revision(), before.revision());
            let mut caret_node = None;
            for text in ["仮", "😀", "確定"] {
                let snapshot = document.update_composition(text.into()).unwrap();
                assert!(
                    snapshot
                        .serialize()
                        .unwrap()
                        .contains(&format!("Repeat **c{text}afé**"))
                );
                let Selection::Text(selection) = snapshot.selection() else {
                    panic!("caret")
                };
                if let Some(id) = caret_node {
                    assert_eq!(selection.head.node_id, id);
                }
                caret_node = Some(selection.head.node_id);
                assert!(matches!(
                    document.undo(),
                    Err(DocumentError::CompositionAlreadyActive)
                ));
            }
            if commit {
                let after = document.commit_composition().unwrap().serialize().unwrap();
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
                document.redo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), after);
            } else {
                document.cancel_composition().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
                assert_eq!(document.snapshot().selection(), before.selection());
                assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
            }
        }
    }

    #[test]
    fn html_composition_accepts_cross_leaf_but_rejects_invalid_targets_before_starting() {
        use crate::{Document, DocumentError};
        let source = "<div><p>café</p><p>other</p></div>\n\nAfter\n";
        let mut document = Document::from_markdown(source).unwrap();
        let before = document.snapshot();
        let node = before.blocks().get(0).unwrap();
        let crate::BlockNode::PreservedSource { source: html, .. } = node.as_ref() else {
            panic!("HTML")
        };
        let anchor = HtmlTextPosition {
            text_node: 0,
            byte_offset: 0,
        };
        for head in [
            HtmlTextPosition {
                text_node: 0,
                byte_offset: 4,
            },
            HtmlTextPosition {
                text_node: 99,
                byte_offset: 0,
            },
        ] {
            assert!(
                document
                    .begin_html_composition(node.id(), html, anchor, head)
                    .is_err()
            );
            assert!(!document.composition_active());
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
        document
            .begin_html_composition(
                node.id(),
                html,
                anchor,
                HtmlTextPosition {
                    text_node: 1,
                    byte_offset: 0,
                },
            )
            .unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document.update_composition("仮".into()).unwrap();
        assert_eq!(
            document.snapshot().serialize().unwrap(),
            "仮other\n\nAfter\n"
        );
        document.cancel_composition().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        document
            .begin_html_composition(node.id(), html, anchor, anchor)
            .unwrap();
        document.commit_composition().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        assert!(matches!(document.undo(), Err(DocumentError::NothingToUndo)));
    }
}
