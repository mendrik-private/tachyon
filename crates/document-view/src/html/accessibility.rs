//! Read-only semantic text for the bounded, already-resolved inert preview.
//! Traverse DOM children, not anonymous layout/paint boxes, to retain source order.

use blitz_dom::{BaseDocument, NodeData, NodeId};

pub(super) fn visible_text(doc: &BaseDocument) -> String {
    let Some(body) = doc.find_body_node() else {
        return String::new();
    };
    let mut text = String::new();
    append(doc, body.id, false, &mut text);
    text.trim_matches('\n').to_owned()
}

fn boundary(text: &mut String) {
    while text.ends_with(' ') {
        text.pop();
    }
    if !text.is_empty() && !text.ends_with('\n') {
        text.push('\n');
    }
}

fn append(doc: &BaseDocument, id: NodeId, preformatted: bool, text: &mut String) {
    let Some(node) = doc.get_node(id) else {
        return;
    };
    if let NodeData::Text(data) = &node.data {
        if preformatted {
            text.push_str(&data.content);
        } else {
            for ch in data.content.chars() {
                if matches!(ch, ' ' | '\t' | '\n' | '\r' | '\u{c}') {
                    if !text.is_empty() && !text.ends_with([' ', '\n']) {
                        text.push(' ');
                    }
                } else {
                    text.push(ch);
                }
            }
        }
        return;
    }
    let Some(element) = node.element_data() else {
        return;
    };
    let Some(styles) = node.primary_styles() else {
        return;
    };
    // The sanitizer strips authored html/body wrappers and their attributes;
    // this body's default visible style is a trusted reference, not authored CSS.
    let Some(body_styles) = doc.find_body_node().and_then(|body| body.primary_styles()) else {
        return;
    };
    if styles.clone_display().is_none()
        || styles.clone_visibility() != body_styles.clone_visibility()
    {
        return;
    }
    let tag = element.name.local.as_ref();
    if tag == "img" {
        // An accessibility description, not a generated visible caption.
        text.push_str(node.attr("alt".into()).unwrap_or_default());
        return;
    }
    if tag == "br" {
        text.push('\n');
        return;
    }
    let block = matches!(
        tag,
        "main"
            | "section"
            | "article"
            | "div"
            | "p"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "blockquote"
            | "pre"
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
    );
    if block {
        boundary(text);
    }
    let closed = tag == "details" && node.attr("open".into()).is_none();
    for child in &node.children {
        if closed
            && !doc.get_node(*child).is_some_and(|child| {
                child
                    .element_data()
                    .is_some_and(|e| e.name.local.as_ref() == "summary")
            })
        {
            continue;
        }
        append(doc, *child, preformatted || tag == "pre", text);
        if closed {
            break;
        }
    }
    if block {
        boundary(text);
    }
}

#[cfg(test)]
mod tests {
    use crate::html::{render, render_with_disclosures};

    #[test]
    fn resolved_text_preserves_inline_boundaries_breaks_and_disclosure_state() {
        let source = "<div><p>One <strong>bold</strong> word&nbsp;here.<br><br>After break.</p><pre>  exact\n    spacing</pre><p style='display:none'>Display hidden</p><p style='visibility:hidden'>Visibility hidden</p><details><summary>Outer</summary>Direct closed text<details open><summary>Inner</summary><p>Nested body</p></details></details><details open><summary>Visible</summary><p>Open body</p></details></div>";
        let closed = render(source, 600).unwrap();
        assert_eq!(
            closed.accessible_text,
            "One bold word\u{a0}here.\n\nAfter break.\n  exact\n    spacing\nOuter\nVisible\nOpen body"
        );
        let opened = render_with_disclosures(source, 600, &[(0, true)].into()).unwrap();
        assert!(
            opened
                .accessible_text
                .contains("Outer\nDirect closed text\nInner\nNested body")
        );
        assert!(!opened.accessible_text.contains("hidden"));
        assert_eq!(
            opened.text, closed.text,
            "copy text must remain source-based"
        );
        assert_eq!(&*opened.source, source);
        let restored = render(source, 600).unwrap();
        assert_eq!(restored.accessible_text, closed.accessible_text);
    }
}
