//! Authored document properties, not inferred success states or feature cards.
use super::{BlockNode, HashSet, LayoutSlot, NodeId, authored_label_end};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    pub slot: Option<LayoutSlot>,
    pub stacked: bool,
    pub badge_inset: f32,
}

pub(crate) fn analyze(roots: &[&BlockNode]) -> HashSet<NodeId> {
    let mut result = HashSet::new();
    let mut explicit = false;
    let mut opening = matches!(roots.first(), Some(BlockNode::Heading(h)) if h.level == 1);
    let mut introductory_paragraphs = 0;
    for (index, root) in roots.iter().enumerate() {
        match root {
            BlockNode::Heading(heading) => {
                explicit = matches!(
                    heading
                        .content
                        .as_string()
                        .trim()
                        .to_ascii_lowercase()
                        .as_str(),
                    "metadata" | "document metadata" | "properties" | "document properties"
                );
                if index != 0 {
                    opening = false;
                }
            }
            BlockNode::Paragraph(_) if opening && introductory_paragraphs < 2 => {
                introductory_paragraphs += 1;
            }
            BlockNode::List(list) => {
                let valid = matches!(list.kind, document_core::ListKind::Unordered)
                    && (2..=64).contains(&list.items.len())
                    && list.items.iter().all(|item| {
                        if item.checked.is_some() || item.blocks.len() != 1 {
                            return false;
                        }
                        let Some(BlockNode::Paragraph(p)) = item.blocks.get(0).map(AsRef::as_ref)
                        else {
                            return false;
                        };
                        let Some(end) = authored_label_end(p) else {
                            return false;
                        };
                        let text = p.content.as_string();
                        explicit || (opening && property_label(&text[..end]))
                    });
                if valid {
                    result.insert(list.id);
                }
                // Only one opening property group; later lists need an
                // explicit section label, not just coincidental vocabulary.
                opening = false;
            }
            _ => opening = false,
        }
    }
    result
}

fn property_label(label: &str) -> bool {
    matches!(
        label
            .trim()
            .trim_end_matches(':')
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "status"
            | "state"
            | "classification"
            | "maturity"
            | "version"
            | "revision"
            | "owner"
            | "author"
            | "authors"
            | "date"
            | "updated"
            | "created"
            | "license"
            | "platform"
            | "language"
            | "category"
            | "project"
            | "audience"
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextProjection;
    use document_core::Document;

    #[test]
    fn metadata_needs_an_authored_property_context() {
        for (source, expected) in [
            (
                "# Guide\n\n- **Status:** Draft\n- **Owner:** Editorial\n",
                1,
            ),
            (
                "# Guide\n\nA short introduction.\n\n- Version: 2.1\n- License: MIT\n",
                1,
            ),
            (
                "# Guide\n\n## Properties\n\n- Storage: Local files\n- Encoding: UTF-8\n",
                1,
            ),
            (
                "# Guide\n\n- Fast: Respond immediately\n- Clear: Keep intent visible\n",
                0,
            ),
            (
                "## Stages\n\n- Status: Check the service\n- Owner: Contact the team\n",
                0,
            ),
            (
                "# Guide\n\n- [x] Status: Ready\n- [ ] Owner: Editorial\n",
                0,
            ),
            ("# Guide\n\n1. Status: Check\n2. Owner: Contact\n", 0),
            (
                "## Metadata\n\n- Owner: Editorial\n  - Nested context\n- Version: 2\n",
                0,
            ),
            (
                "## Metadata\n\n- Owner: Editorial\n- Version: 2\n\n## Workflow\n\n- Owner: Contact the team\n- Status: Check again\n",
                1,
            ),
        ] {
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            assert_eq!(
                analyze(&projection.roots().collect::<Vec<_>>()).len(),
                expected,
                "{source}"
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }
}
