//! Authored external objects, not link previews or generated summaries.
use document_core::{BlockNode, InlineStyle, ListBlock, ListKind, Paragraph};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Resource {
    /// Canonical byte boundary after the linked title and its delimiter.
    /// None denotes a compact, link-only row.
    pub body_start: Option<usize>,
}

impl Resource {
    pub fn inset(self) -> f32 {
        if self.body_start.is_some() { 24. } else { 12. }
    }
}

/// A title is one contiguous destination, possibly with mixed inline styles.
/// A following description needs an explicit separator; ordinary linked prose
/// ("[Rust] is useful") is not a self-contained resource.
pub(crate) fn classify(paragraph: &Paragraph) -> Option<Resource> {
    if paragraph.content.len() > 4096 {
        return None;
    }
    let text = paragraph.content.as_string();
    let mut end = 0;
    let mut destination = None;
    for run in paragraph.content.runs() {
        if run.styles.iter().any(|style| {
            matches!(
                style,
                InlineStyle::Image { .. }
                    | InlineStyle::Math { .. }
                    | InlineStyle::PreservedHtml(_)
                    | InlineStyle::FootnoteReference(_)
            )
        }) {
            return None;
        }
        if let Some(target) = run.styles.iter().find_map(|style| {
            if let InlineStyle::Link(target) = style {
                Some(target)
            } else {
                None
            }
        }) {
            if run.range.start != end || destination.is_some_and(|old| old != target) {
                return None;
            }
            destination = Some(target);
            end = run.range.end;
        }
    }
    let destination = &destination?.0;
    if destination.is_empty()
        || destination.starts_with('#')
        || destination.chars().any(char::is_control)
        || (destination.contains(':')
            && !destination.starts_with("https://")
            && !destination.starts_with("http://"))
    {
        return None;
    }
    let title = text.get(..end)?;
    if title.trim().is_empty() || title.contains('\n') {
        return None;
    }
    let suffix = text.get(end..)?;
    if suffix.trim().is_empty() {
        return Some(Resource { body_start: None });
    }
    // Keep authored punctuation in the title range, including for clipboard
    // and selection. Presentation never deletes a separator from the source.
    let body = suffix
        .strip_prefix(": ")
        .or_else(|| suffix.strip_prefix(" — "))
        .or_else(|| suffix.strip_prefix(" – "))
        .or_else(|| suffix.strip_prefix(" - "))
        .or_else(|| suffix.strip_prefix("  \n"))
        .or_else(|| suffix.strip_prefix('\n'))?;
    let body = body.trim_start();
    if body.is_empty() {
        return None;
    }
    Some(Resource {
        body_start: Some(text.len() - body.len()),
    })
}

pub(crate) fn is_resource_list(list: &ListBlock) -> bool {
    matches!(list.kind, ListKind::Unordered)
        && !list.items.is_empty()
        && list.items.iter().all(|item| {
            item.checked.is_none()
                && item.blocks.len() == 1
                && matches!(item.blocks.get(0).map(AsRef::as_ref),
                Some(BlockNode::Paragraph(p)) if classify(p).is_some())
        })
}

/// Resolve at activation as well as publication, so stale accessibility trees
/// cannot follow a destination removed or replaced by an edit.
pub(crate) fn target(paragraph: &Paragraph) -> Option<&str> {
    classify(paragraph)?;
    paragraph
        .content
        .runs()
        .first()?
        .styles
        .iter()
        .find_map(|style| {
            if let InlineStyle::Link(target) = style {
                Some(target.0.as_str())
            } else {
                None
            }
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    fn resource(source: &str) -> Option<Resource> {
        let doc = document_core::Document::from_markdown(source).unwrap();
        let snapshot = doc.snapshot();
        let BlockNode::Paragraph(p) = snapshot.blocks().iter().next()?.as_ref() else {
            return None;
        };
        classify(p)
    }
    #[test]
    fn resources_require_a_grounded_title_and_explicit_relationship() {
        for source in [
            "[Handbook](https://example.org): Practical guidance.",
            "[**Design** handbook](../handbook.md) — Readable documents.",
            "[Guide](https://example.org)  \nA description.",
        ] {
            assert!(
                resource(source).is_some_and(|r| r.body_start.is_some()),
                "{source}"
            );
        }
        assert_eq!(
            resource("[Local guide](guide.md)"),
            Some(Resource { body_start: None })
        );
        for source in [
            "[Rust](https://rust-lang.org) is useful.",
            "Read [this](guide.md): some guidance.",
            "[Go](#section)",
            "[One](one.md): compare [two](two.md).",
            "Ordinary: prose.",
            "[Bad](javascript:alert): no.",
            "[Empty](guide.md): ",
            "[Image](guide.md): ![alt](image.png)",
        ] {
            assert!(resource(source).is_none(), "{source}");
        }
    }
}
