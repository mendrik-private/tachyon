//! Read-only composition metadata. Canonical nodes remain in document-core;
//! these IDs and intervals never become a second editable document.

use std::{
    collections::{HashMap, HashSet},
    ops::Range,
};

use document_core::{BlockNode, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum GroupKind {
    Heading,
    Prose,
    List,
    Table,
    Code,
    Figure,
    Gallery,
    ExplanationContent,
    Quote,
    Opaque,
    Barrier,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RelationshipKind {
    HeadingContent,
    ProseContinuation,
    AdjacentExplanation,
    ConsecutiveImages,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Relationship {
    pub kind: RelationshipKind,
    pub basis: [NodeId; 2],
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct ContentGroup {
    pub id: NodeId,
    /// Half-open canonical root interval, not projected text offsets.
    pub roots: Range<usize>,
    /// Preorder ownership, including list items and table rows/cells.
    pub nodes: Vec<NodeId>,
    pub section: Option<NodeId>,
    pub kind: GroupKind,
    pub relationships: Vec<Relationship>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct Section {
    pub id: NodeId,
    pub parent: Option<NodeId>,
    pub level: u8,
    /// Includes descendant subsections. Skipped levels create no fake nodes.
    pub roots: Range<usize>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(super) struct GroupAnalysis {
    pub groups: Vec<ContentGroup>,
    pub sections: HashMap<NodeId, Section>,
    pub preamble: Range<usize>,
    by_root: HashMap<NodeId, usize>,
}

impl GroupAnalysis {
    pub fn build(roots: &[&BlockNode]) -> Self {
        let mut analysis = Self::default();
        let mut ancestors: Vec<NodeId> = Vec::new();
        let mut owners = Vec::with_capacity(roots.len());
        analysis.preamble = 0..roots.len();
        for (ordinal, block) in roots.iter().enumerate() {
            if let BlockNode::Heading(heading) = block {
                if analysis.sections.is_empty() {
                    analysis.preamble.end = ordinal;
                }
                while let Some(id) = ancestors.last().copied() {
                    if analysis.sections[&id].level < heading.level {
                        break;
                    }
                    analysis.sections.get_mut(&id).unwrap().roots.end = ordinal;
                    ancestors.pop();
                }
                analysis.sections.insert(
                    heading.id,
                    Section {
                        id: heading.id,
                        parent: ancestors.last().copied(),
                        level: heading.level,
                        roots: ordinal..roots.len(),
                    },
                );
                ancestors.push(heading.id);
            }
            owners.push(ancestors.last().copied());
        }

        let mut start = 0;
        while start < roots.len() {
            let mut end = start + 1;
            let mut kind = block_kind(roots[start]);
            let mut relationships = Vec::new();
            if kind == GroupKind::Heading
                && let Some(next) = roots.get(end)
                && !matches!(block_kind(next), GroupKind::Heading | GroupKind::Barrier)
            {
                relationships.push(Relationship {
                    kind: RelationshipKind::HeadingContent,
                    basis: [roots[start].id(), next.id()],
                });
                end += 1;
            }
            // A short explanation nominates a pair; actual width/height
            // measurement, not this byte bound, must decide its presentation.
            let explanation = end - 1;
            if matches!(roots[explanation], BlockNode::Paragraph(p) if p.content.len() <= 420)
                && let Some(content) = roots.get(end)
                && owners[end] == owners[start]
                // Do not extract the first figure from a consecutive gallery
                // merely because a short introduction precedes the whole run.
                && !(matches!(content, BlockNode::Image(_))
                    && matches!(roots.get(end + 1), Some(BlockNode::Image(_))))
                && matches!(
                    content,
                    BlockNode::CodeBlock(_) | BlockNode::Table(_) | BlockNode::Image(_)
                )
            {
                relationships.push(Relationship {
                    kind: RelationshipKind::AdjacentExplanation,
                    basis: [roots[explanation].id(), content.id()],
                });
                end += 1;
                kind = GroupKind::ExplanationContent;
            } else if kind == GroupKind::Prose {
                // Bound groups structurally; a huge paragraph stays intact.
                // Leave the final short paragraph for an adjacent example.
                while end < roots.len()
                    && end - start < 8
                    && owners[end] == owners[start]
                    && matches!(roots[end], BlockNode::Paragraph(_))
                {
                    if matches!(roots[end], BlockNode::Paragraph(p) if p.content.len() <= 420)
                        && roots.get(end + 1).is_some_and(|next| {
                            matches!(
                                next,
                                BlockNode::CodeBlock(_) | BlockNode::Table(_) | BlockNode::Image(_)
                            )
                        })
                    {
                        break;
                    }
                    relationships.push(Relationship {
                        kind: RelationshipKind::ProseContinuation,
                        basis: [roots[end - 1].id(), roots[end].id()],
                    });
                    end += 1;
                }
            } else if matches!(roots[end - 1], BlockNode::Image(_)) {
                let first_figure = end - 1;
                while end < roots.len()
                    && end - first_figure < 9
                    && owners[end] == owners[start]
                    && matches!(roots[end], BlockNode::Image(_))
                {
                    relationships.push(Relationship {
                        kind: RelationshipKind::ConsecutiveImages,
                        basis: [roots[end - 1].id(), roots[end].id()],
                    });
                    end += 1;
                    kind = GroupKind::Gallery;
                }
            }
            let mut nodes = Vec::new();
            for root in &roots[start..end] {
                collect_nodes(root, &mut nodes);
                analysis.by_root.insert(root.id(), analysis.groups.len());
            }
            analysis.groups.push(ContentGroup {
                id: roots[start].id(),
                roots: start..end,
                nodes,
                section: owners[start],
                kind,
                relationships,
            });
            start = end;
        }
        analysis
    }

    pub fn group_for_root(&self, id: NodeId) -> Option<&ContentGroup> {
        self.by_root.get(&id).map(|index| &self.groups[*index])
    }

    /// Exact canonical ownership and traversal check; independent of rendering
    /// omissions, text equality, hash iteration order, or source-offset shifts.
    pub fn validate(&self, roots: &[&BlockNode]) -> bool {
        let mut next = 0;
        let mut owned = HashSet::new();
        for group in &self.groups {
            if group.roots.start != next || group.roots.end > roots.len() || group.roots.is_empty()
            {
                return false;
            }
            let mut expected = Vec::new();
            for root in &roots[group.roots.clone()] {
                collect_nodes(root, &mut expected);
                if self.group_for_root(root.id()).map(|g| g.id) != Some(group.id) {
                    return false;
                }
            }
            if group.id != roots[next].id()
                || expected != group.nodes
                || group.nodes.iter().any(|id| !owned.insert(*id))
            {
                return false;
            }
            if let Some(id) = group.section {
                let Some(section) = self.sections.get(&id) else {
                    return false;
                };
                if group.roots.start < section.roots.start || group.roots.end > section.roots.end {
                    return false;
                }
            }
            if group.relationships.iter().any(|relationship| {
                !group.nodes.contains(&relationship.basis[0])
                    || !group.nodes.contains(&relationship.basis[1])
            }) {
                return false;
            }
            next = group.roots.end;
        }
        next == roots.len()
    }
}

fn block_kind(block: &BlockNode) -> GroupKind {
    match block {
        BlockNode::Heading(_) => GroupKind::Heading,
        BlockNode::Paragraph(_) => GroupKind::Prose,
        BlockNode::List(_) => GroupKind::List,
        BlockNode::Table(_) => GroupKind::Table,
        BlockNode::CodeBlock(_) => GroupKind::Code,
        BlockNode::Image(_) => GroupKind::Figure,
        BlockNode::BlockQuote { .. } | BlockNode::Alert { .. } => GroupKind::Quote,
        BlockNode::ThematicBreak { .. } => GroupKind::Barrier,
        BlockNode::PreservedSource { .. } | BlockNode::FootnoteDefinition { .. } => {
            GroupKind::Opaque
        }
    }
}

fn collect_nodes(block: &BlockNode, nodes: &mut Vec<NodeId>) {
    nodes.push(block.id());
    match block {
        BlockNode::List(list) => {
            for item in list.items.iter() {
                nodes.push(item.id);
                for child in &item.blocks {
                    collect_nodes(child, nodes);
                }
            }
        }
        BlockNode::Table(table) => {
            for row in table.rows.iter() {
                nodes.push(row.id);
                for cell in row.cells.iter() {
                    nodes.push(cell.id);
                    for child in &cell.blocks {
                        collect_nodes(child, nodes);
                    }
                }
            }
        }
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => {
            for child in blocks {
                collect_nodes(child, nodes);
            }
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextProjection;
    use document_core::Document;

    #[test]
    fn hierarchy_uses_actual_levels_preamble_and_duplicate_identity() {
        let document = Document::from_markdown(
            "Preamble.\n\n# Same\n\nIntro.\n\n#### Same\n\nText.\n\n## Same\n\nLast.\n",
        )
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let roots = projection.roots().collect::<Vec<_>>();
        let analysis = GroupAnalysis::build(&roots);
        assert!(analysis.validate(&roots));
        assert_eq!(analysis.preamble, 0..1);
        assert_eq!(analysis.sections.len(), 3);
        assert_eq!(
            analysis.sections[&roots[3].id()].parent,
            Some(roots[1].id())
        );
        assert_eq!(
            analysis.sections[&roots[5].id()].parent,
            Some(roots[1].id())
        );
        assert_eq!(analysis.sections[&roots[3].id()].roots, 3..5);
        assert_eq!(analysis.sections[&roots[1].id()].roots, 1..7);
    }

    #[test]
    fn galleries_keep_their_heading_and_do_not_lose_a_figure_to_an_explanation() {
        for source in [
            "## Gallery\n\n![A](a.png)\n\n![B](b.png)\n",
            "## Gallery\n\nA short introduction.\n\n![A](a.png)\n\n![B](b.png)\n",
        ] {
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let roots = projection.roots().collect::<Vec<_>>();
            let analysis = GroupAnalysis::build(&roots);
            let gallery = analysis
                .groups
                .iter()
                .find(|group| group.kind == GroupKind::Gallery)
                .expect("consecutive figures stay together");
            assert_eq!(
                roots[gallery.roots.clone()]
                    .iter()
                    .filter(|root| matches!(root, BlockNode::Image(_)))
                    .count(),
                2
            );
            assert!(analysis.validate(&roots));
            assert_eq!(
                gallery
                    .relationships
                    .iter()
                    .filter(|relation| relation.kind == RelationshipKind::ConsecutiveImages)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn groups_preserve_barriers_and_structural_relationships() {
        let source = "# Heading\n\nIntro.\n\n```rs\nlet n = 1;\n```\n\n---\n\nOne.\n\nTwo.\n\n![A](a.png)\n\n![B](b.png)\n\n<div>Opaque</div>\n\n- Parent\n  - Child\n\n| A | B |\n| - | - |\n| c | d |\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let roots = projection.roots().collect::<Vec<_>>();
        let mut analysis = GroupAnalysis::build(&roots);
        assert!(analysis.validate(&roots));
        assert_eq!(analysis.groups[0].roots, 0..3);
        assert_eq!(analysis.groups[0].kind, GroupKind::ExplanationContent);
        assert_eq!(analysis.groups[0].relationships.len(), 2);
        assert_eq!(analysis.groups[1].kind, GroupKind::Barrier);
        assert!(
            analysis
                .groups
                .iter()
                .any(|group| group.kind == GroupKind::Opaque)
        );
        let list = analysis
            .groups
            .iter()
            .find(|group| group.kind == GroupKind::List)
            .unwrap();
        assert!(list.nodes.len() > 3, "nested identities must be owned");
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        analysis.groups[0].nodes.pop();
        assert!(
            !analysis.validate(&roots),
            "coverage must detect a missing node"
        );
    }

    #[test]
    fn generated_sequences_are_exact_deterministic_partitions() {
        let snippets = [
            "A.\n\n",
            "# A\n\n",
            "### A\n\n",
            "---\n\n",
            "![A](a.png)\n\n",
            "```\nx\n```\n\n",
        ];
        for seed in 0..1296 {
            let mut index = seed;
            let mut source = String::new();
            for _ in 0..4 {
                source.push_str(snippets[index % snippets.len()]);
                index /= snippets.len();
            }
            let document = Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let roots = projection.roots().collect::<Vec<_>>();
            let analysis = GroupAnalysis::build(&roots);
            assert!(analysis.validate(&roots), "{source}");
            assert_eq!(analysis.groups, GroupAnalysis::build(&roots).groups);
        }
    }

    #[test]
    fn ordinary_typing_does_not_reassign_duplicate_group_anchors() {
        let source = "# Same\n\nSame.\n\n# Same\n\nSame.\n";
        let mut document = Document::from_markdown(source).unwrap();
        let mut projection = TextProjection::from_snapshot(&document.snapshot());
        let roots = projection.roots().collect::<Vec<_>>();
        let old = GroupAnalysis::build(&roots);
        let target = roots[3].id();
        let edit = document
            .apply(document_core::EditCommand::ReplaceText {
                node_id: target,
                range: 0..0,
                text: "Edited ".into(),
                typing: true,
                selection_after: None,
            })
            .unwrap();
        projection
            .refresh_text_node(&edit.snapshot, target)
            .unwrap();
        let roots = projection.roots().collect::<Vec<_>>();
        let new = GroupAnalysis::build(&roots);
        assert!(new.validate(&roots));
        assert_eq!(old.groups, new.groups);
        assert_ne!(new.groups[0].id, new.groups[1].id);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }
}
