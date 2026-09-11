//! Bibliographic presentation is inferred from an authored section, never from
//! a URL alone. Entries remain canonical paragraphs/list items in source order.
use document_core::{BlockNode, ListKind, NodeId};
use rustc_hash::FxHashMap;

/// Maps citation leaves to their authored section heading. Nested headings
/// inherit the section; a peer/ancestor heading ends it. No sorting, renumbering,
/// punctuation normalization or fabricated publication data is performed.
pub(crate) fn entries(roots: &[&BlockNode]) -> FxHashMap<NodeId, NodeId> {
    let mut result = FxHashMap::default();
    let mut section = None;
    for root in roots {
        if let BlockNode::Heading(heading) = root {
            if section.is_some_and(|(_, level)| heading.level <= level) {
                section = None;
            }
            let text = heading.content.as_cow();
            let name = text
                .trim()
                .trim_start_matches(|c: char| c.is_ascii_digit() || c == '.' || c.is_whitespace());
            let name = name.split([':', '·', '—']).next().unwrap_or(name).trim();
            if ["bibliography", "works cited", "references"]
                .iter()
                .any(|label| name.eq_ignore_ascii_case(label))
            {
                section = Some((heading.id, heading.level));
            }
            continue;
        }
        let Some((owner, _)) = section else { continue };
        match root {
            // Bibliography prose uses a hanging indent. An introductory label
            // ending in ':' remains open reference prose, not an entry.
            BlockNode::Paragraph(p) if !p.content.as_cow().trim_end().ends_with(':') => {
                result.insert(p.id, owner);
            }
            BlockNode::List(list)
                if list.kind != ListKind::Task
                    && list.items.iter().all(|item| {
                        item.checked.is_none()
                            && !item.blocks.is_empty()
                            && item
                                .blocks
                                .iter()
                                .all(|b| matches!(b.as_ref(), BlockNode::Paragraph(_)))
                    }) =>
            {
                for item in list.items.iter() {
                    for block in &item.blocks {
                        result.insert(block.id(), owner);
                    }
                }
            }
            _ => {}
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use crate::TextProjection;

    #[test]
    fn bibliography_scope_preserves_authored_order_and_rejects_tasks_and_nested_lists() {
        let source = "# Bibliography\n\nEntries:\n\nAuthor. *Title*.\n\n## Books\n\n7. Writer. Book.\n8. Editor. Volume.\n\n- [ ] Verify this source\n\n- Parent\n  - Child\n\n# Appendix\n\nOrdinary text.\n";
        let document = document_core::Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let citations = projection
            .segments()
            .iter()
            .filter(|s| s.context.bibliography.is_some())
            .collect::<Vec<_>>();
        assert_eq!(citations.len(), 3);
        assert_eq!(citations[1].context.list_marker.as_deref(), Some("7."));
        assert_eq!(citations[2].context.list_marker.as_deref(), Some("8."));
        assert!(
            citations
                .iter()
                .all(|s| s.context.bibliography == citations[0].context.bibliography)
        );
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn links_alone_are_not_bibliographies_and_section_aliases_work() {
        for label in [
            "Bibliography",
            "3. Works cited",
            "References · illustrative",
        ] {
            let source = format!(
                "## {label}\n\nAn author. *A title*. [Publication](https://example.org).\n\n## Another section\n\n[Resource](https://example.org)\n"
            );
            let document = document_core::Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            assert_eq!(
                projection
                    .segments()
                    .iter()
                    .filter(|s| s.context.bibliography.is_some())
                    .count(),
                1
            );
        }
    }
}
