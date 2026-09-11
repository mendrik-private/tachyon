//! Authored quotation roles; no invented citation, duplicated passage or
//! presentation annotation is written into the canonical document.
use document_core::{BlockNode, BlockSequence, NodeId};

/// Transient source-owned typography. The focused quotation retains these
/// roles until blur; deleting an attribution marker cannot shrink the line
/// underneath the caret. Structural ownership changes release the lock.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TextRole {
    pub owner: NodeId,
    pub pull: bool,
    pub attribution: bool,
    pub before_attribution: bool,
    pub margin_anchor: Option<NodeId>,
}

impl TextRole {
    pub fn of(context: &crate::ProjectionContext) -> Option<Self> {
        Some(Self {
            owner: context.quote?,
            pull: context.quote_pull,
            attribution: context.quote_attribution,
            before_attribution: context.quote_before_attribution,
            margin_anchor: context.margin_note_anchor,
        })
    }
}

pub(crate) fn margin_note_anchor(
    previous: Option<&BlockNode>,
    blocks: &BlockSequence,
) -> Option<NodeId> {
    let Some(BlockNode::Paragraph(anchor)) = previous else {
        return None;
    };
    if !(1..=2).contains(&blocks.len())
        || !blocks.iter().all(
            |block| matches!(block.as_ref(), BlockNode::Paragraph(p) if p.content.len() <= 480),
        )
    {
        return None;
    }
    let BlockNode::Paragraph(first) = blocks.get(0)?.as_ref() else {
        return None;
    };
    let text = first.content.as_cow();
    let (label, body) = text.trim().split_once(':')?;
    (label.eq_ignore_ascii_case("Margin note") && !body.trim().is_empty()).then_some(anchor.id)
}

pub(crate) fn attribution(blocks: &BlockSequence) -> Option<NodeId> {
    if blocks.len() < 2 {
        return None;
    }
    let BlockNode::Paragraph(last) = blocks.get(blocks.len() - 1)?.as_ref() else {
        return None;
    };
    let text = last.content.as_cow();
    let text = text.trim();
    ["— ", "– ", "Attribution: ", "Source: "]
        .iter()
        .any(|prefix| {
            text.strip_prefix(prefix)
                .is_some_and(|s| !s.trim().is_empty())
        })
        .then_some(last.id)
}

pub(crate) fn is_pull_quote(previous: Option<&BlockNode>, blocks: &BlockSequence) -> bool {
    // A short passage or italics alone is not editorial intent. The author
    // must label the quotation; the heading itself stays visible and editable.
    let Some(BlockNode::Heading(heading)) = previous else {
        return false;
    };
    let label = heading.content.as_cow();
    let label = label.trim();
    (label.eq_ignore_ascii_case("Pull quote")
        || label.split_once(':').is_some_and(|(prefix, title)| {
            prefix.eq_ignore_ascii_case("Pull quote") && !title.trim().is_empty()
        }))
        && !blocks.is_empty()
        && blocks
            .iter()
            .all(|b| matches!(b.as_ref(), BlockNode::Paragraph(_)))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn adjacent_margin_notes_share_only_their_original_paragraph() {
        let source =
            include_str!("../../../performance/layout-fixtures/110-margin-note-cluster.md");
        let document = document_core::Document::from_markdown(source).unwrap();
        let projection = crate::TextProjection::from_snapshot(&document.snapshot());
        let notes = projection
            .segments()
            .iter()
            .filter(|s| s.context.margin_note_anchor.is_some())
            .collect::<Vec<_>>();
        assert_eq!(
            notes.len(),
            3,
            "all three explicit notes must retain their source anchor"
        );
        assert!(
            notes
                .iter()
                .all(|note| note.context.margin_note_anchor == notes[0].context.margin_note_anchor)
        );
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn margin_notes_require_an_explicit_adjacent_top_level_paragraph_anchor() {
        for (source, count) in [
            ("Anchor.\n\n> Margin note: Context.\n", 1),
            (
                "Anchor.\n\n> **Margin note:** Context.\n>\n> More context.\n",
                2,
            ),
            ("> Margin note: No preceding paragraph.\n", 0),
            ("# Heading\n\n> Margin note: No paragraph.\n", 0),
            ("Anchor.\n\n> An ordinary quotation.\n", 0),
            ("Anchor.\n\n> Margin note:\n", 0),
            (
                "Anchor.\n\n> Margin note: Context.\n>\n> - Nested structure.\n",
                0,
            ),
            ("- Anchor.\n\n  > Margin note: Nested note.\n", 0),
        ] {
            let document = document_core::Document::from_markdown(source).unwrap();
            let projection = crate::TextProjection::from_snapshot(&document.snapshot());
            let notes = projection
                .segments()
                .iter()
                .filter(|s| s.context.margin_note_anchor.is_some())
                .collect::<Vec<_>>();
            assert_eq!(notes.len(), count, "{source}");
            for note in notes {
                let anchor = projection
                    .segment_for_node(note.context.margin_note_anchor.unwrap())
                    .unwrap();
                assert!(anchor.projection_end() < note.projection_start());
                assert!(!note.context.quote_attribution);
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn editing_a_margin_note_keeps_the_following_cluster_stable_until_blur() {
        let source =
            include_str!("../../../performance/layout-fixtures/110-margin-note-cluster.md");
        for edited in 0..3 {
            let mut document = document_core::Document::from_markdown(source).unwrap();
            let projection = crate::TextProjection::from_snapshot(&document.snapshot());
            let notes = projection
                .segments()
                .iter()
                .filter(|s| s.context.margin_note_anchor.is_some())
                .map(|s| s.node_id)
                .collect::<Vec<_>>();
            let anchor = projection
                .segment_for_node(notes[0])
                .unwrap()
                .context
                .margin_note_anchor;
            let roles = projection
                .segments()
                .iter()
                .filter_map(|s| TextRole::of(&s.context).map(|role| (s.node_id, role)))
                .collect();
            document
                .apply(document_core::EditCommand::ReplaceText {
                    node_id: notes[edited],
                    range: 0..0,
                    text: "x".into(),
                    selection_after: None,
                    typing: false,
                })
                .unwrap();
            let mut changed = crate::TextProjection::from_snapshot(&document.snapshot());
            changed.retain_quote_roles(&roles, Some(notes[edited]));
            assert_eq!(
                changed
                    .segments()
                    .iter()
                    .filter(|s| s.context.margin_note_anchor.is_some())
                    .count(),
                3,
                "the ordinary quotation must still break the anchor chain"
            );
            for id in &notes {
                assert_eq!(
                    changed
                        .segment_for_node(*id)
                        .unwrap()
                        .context
                        .margin_note_anchor,
                    anchor,
                    "editing note {edited} must not release its neighbors"
                );
            }
            let blurred = crate::TextProjection::from_snapshot(&document.snapshot());
            for (index, id) in notes.iter().enumerate() {
                assert_eq!(
                    blurred
                        .segment_for_node(*id)
                        .unwrap()
                        .context
                        .margin_note_anchor,
                    if index < edited { anchor } else { None }
                );
            }
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn margin_note_typography_survives_label_edits_until_blur() {
        let source = "Anchor.\n\n> Margin note: Context.\n";
        let mut document = document_core::Document::from_markdown(source).unwrap();
        let projection = crate::TextProjection::from_snapshot(&document.snapshot());
        let note = projection
            .segments()
            .iter()
            .find(|s| s.context.margin_note_anchor.is_some())
            .unwrap();
        let id = note.node_id;
        let anchor = note.context.margin_note_anchor;
        let roles = projection
            .segments()
            .iter()
            .filter_map(|s| TextRole::of(&s.context).map(|role| (s.node_id, role)))
            .collect();
        document
            .apply(document_core::EditCommand::ReplaceText {
                node_id: id,
                range: 0..6,
                text: "Edited".into(),
                selection_after: None,
                typing: false,
            })
            .unwrap();
        let mut changed = crate::TextProjection::from_snapshot(&document.snapshot());
        assert_eq!(
            changed
                .segment_for_node(id)
                .unwrap()
                .context
                .margin_note_anchor,
            None
        );
        changed.retain_quote_roles(&roles, Some(id));
        assert_eq!(
            changed
                .segment_for_node(id)
                .unwrap()
                .context
                .margin_note_anchor,
            anchor
        );
        let blurred = crate::TextProjection::from_snapshot(&document.snapshot());
        assert_eq!(
            blurred
                .segment_for_node(id)
                .unwrap()
                .context
                .margin_note_anchor,
            None
        );
        document
            .apply(document_core::EditCommand::DeleteBlock {
                node_id: anchor.unwrap(),
            })
            .unwrap();
        let mut detached = crate::TextProjection::from_snapshot(&document.snapshot());
        detached.retain_quote_roles(&roles, Some(id));
        assert_eq!(
            detached
                .segment_for_node(id)
                .unwrap()
                .context
                .margin_note_anchor,
            None,
            "a deleted source anchor must not survive in a retained description"
        );
        document.undo().unwrap();
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn only_an_authored_final_attribution_and_explicit_pull_label_change_roles() {
        let source = "## Pull quote: Room to read\n\n> Give each idea room to be understood.\n>\n> — Editorial specimen\n\n> — This is the quotation, not its author.\n>\n> Its final paragraph is still quoted prose.\n\n## A short quotation\n\n> *Brevity is not a pull-quote instruction.*\n";
        let document = document_core::Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let blocks = snapshot.blocks();
        let BlockNode::BlockQuote { blocks: quote, .. } = blocks.get(1).unwrap().as_ref() else {
            panic!()
        };
        assert_eq!(attribution(quote), Some(quote.get(1).unwrap().id()));
        assert!(is_pull_quote(blocks.get(0).map(AsRef::as_ref), quote));
        let BlockNode::BlockQuote {
            blocks: ordinary, ..
        } = blocks.get(2).unwrap().as_ref()
        else {
            panic!()
        };
        assert_eq!(attribution(ordinary), None);
        assert!(!is_pull_quote(None, ordinary));
        let BlockNode::BlockQuote { blocks: brief, .. } = blocks.get(4).unwrap().as_ref() else {
            panic!()
        };
        assert!(!is_pull_quote(blocks.get(3).map(AsRef::as_ref), brief));
        assert_eq!(snapshot.serialize().unwrap(), source);
    }
}
