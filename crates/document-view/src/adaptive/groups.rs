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
    CriticalInstruction,
    FigureText(crate::FigureTextRole),
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
        let figure_roles = crate::figures::associations(roots);
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
            // A bounded source-adjacent explanation nominates a pair. Native
            // measurement owns fit and shaping budgets, not paragraph bytes.
            let explanation = end - 1;
            if let Some(action_end) = critical_instruction_end(roots, explanation)
                && owners[action_end - 1] == owners[start]
            {
                // One source-order unit prevents outer row search from
                // separating a critical condition from its following action.
                for index in end..action_end {
                    relationships.push(Relationship {
                        kind: RelationshipKind::CriticalInstruction,
                        basis: [roots[index - 1].id(), roots[index].id()],
                    });
                }
                end = action_end;
                kind = GroupKind::Quote;
            } else if let Some((_, role)) = figure_roles.get(&roots[start].id())
                && role.gallery_start().is_some()
            {
                // Shared text is a full-span band after the gallery, never
                // the last image's private caption or a separate photo tile.
                kind = GroupKind::Opaque;
                relationships.push(Relationship {
                    kind: RelationshipKind::FigureText(*role),
                    basis: [roots[start - 1].id(), roots[start].id()],
                });
                if let Some(next) = roots.get(end)
                    && let Some((_, next_role)) = figure_roles.get(&next.id())
                    && next_role.gallery_start() == role.gallery_start()
                {
                    relationships.push(Relationship {
                        kind: RelationshipKind::FigureText(*next_role),
                        basis: [roots[start].id(), next.id()],
                    });
                    end += 1;
                }
            } else if let Some(content) = explanation_content(roots, explanation)
                && owners[content] == owners[start]
            {
                for index in end..content {
                    relationships.push(Relationship {
                        kind: RelationshipKind::ProseContinuation,
                        basis: [roots[index - 1].id(), roots[index].id()],
                    });
                }
                relationships.push(Relationship {
                    kind: RelationshipKind::AdjacentExplanation,
                    basis: [roots[content - 1].id(), roots[content].id()],
                });
                end = content + 1;
                attach_figure_text(roots, &mut end, &mut relationships);
                kind = GroupKind::ExplanationContent;
            } else if kind == GroupKind::Prose {
                // Bound groups structurally; a huge paragraph stays intact.
                // Leave a complete bounded introduction for its adjacent example.
                while end < roots.len()
                    && end - start < 8
                    && owners[end] == owners[start]
                    && matches!(roots[end], BlockNode::Paragraph(_))
                    && !figure_roles
                        .get(&roots[end].id())
                        .is_some_and(|(_, role)| role.gallery_start().is_some())
                {
                    if let Some(BlockNode::BlockQuote { blocks, .. }) = roots.get(end + 1)
                        && crate::quotes::margin_note_anchor(Some(roots[end]), blocks).is_some()
                    {
                        // Keep an explicit note's own anchor separate. Earlier
                        // prose must not rise into its main/rail composition.
                        break;
                    }
                    if explanation_content(roots, end).is_some() {
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
                let mut previous_figure = first_figure;
                let mut figure_count = 1;
                attach_figure_text(roots, &mut end, &mut relationships);
                while end < roots.len()
                    && figure_count < 9
                    && owners[end] == owners[start]
                    && matches!(roots[end], BlockNode::Image(_))
                {
                    relationships.push(Relationship {
                        kind: RelationshipKind::ConsecutiveImages,
                        basis: [roots[previous_figure].id(), roots[end].id()],
                    });
                    previous_figure = end;
                    figure_count += 1;
                    end += 1;
                    attach_figure_text(roots, &mut end, &mut relationships);
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
                let adjacent_shared_caption = matches!(
                    relationship.kind,
                    RelationshipKind::FigureText(crate::FigureTextRole::GalleryCaption { .. })
                ) && relationship.basis[1] == group.id
                    && group
                        .roots
                        .start
                        .checked_sub(1)
                        .is_some_and(|previous| roots[previous].id() == relationship.basis[0]);
                !adjacent_shared_caption
                    && (!group.nodes.contains(&relationship.basis[0])
                        || !group.nodes.contains(&relationship.basis[1]))
            }) {
                return false;
            }
            next = group.roots.end;
        }
        next == roots.len()
    }
}

fn explanation_content(roots: &[&BlockNode], start: usize) -> Option<usize> {
    let mut content = start;
    while content - start < 4 && matches!(roots.get(content), Some(BlockNode::Paragraph(_))) {
        content += 1;
    }
    if content == start
        || !matches!(
            roots.get(content),
            Some(BlockNode::CodeBlock(_) | BlockNode::Table(_) | BlockNode::Image(_))
        )
    {
        return None;
    }
    // An introduction describes a complete adjacent gallery, not its first
    // image in isolation. Preserve the existing gallery ownership rule.
    if matches!(roots[content], BlockNode::Image(_))
        && matches!(
            roots.get(crate::figures::end(roots, content)),
            Some(BlockNode::Image(_))
        )
    {
        return None;
    }
    Some(content)
}

fn critical_instruction_end(roots: &[&BlockNode], warning: usize) -> Option<usize> {
    let BlockNode::Alert {
        kind:
            document_core::AlertKind::Warning
            | document_core::AlertKind::Caution
            | document_core::AlertKind::Important,
        blocks,
        ..
    } = roots[warning]
    else {
        return None;
    };
    if !(1..=2).contains(&blocks.len())
        || !blocks.iter().all(
            |block| matches!(block.as_ref(), BlockNode::Paragraph(p) if p.content.len() <= 480),
        )
    {
        return None;
    }
    let mut action = warning + 1;
    if matches!(roots.get(action), Some(BlockNode::Paragraph(p)) if p.content.len() <= 240 && p.content.as_cow().trim_end().ends_with(':'))
    {
        action += 1;
    }
    match roots.get(action)? {
        BlockNode::CodeBlock(_) => Some(action + 1),
        BlockNode::List(list) if matches!(list.kind, document_core::ListKind::Ordered { .. }) => {
            Some(action + 1)
        }
        _ => None,
    }
}

fn attach_figure_text(
    roots: &[&BlockNode],
    end: &mut usize,
    relationships: &mut Vec<Relationship>,
) {
    let image = *end - 1;
    for index in *end..crate::figures::end(roots, image) {
        relationships.push(Relationship {
            kind: RelationshipKind::FigureText(crate::figures::classify(roots[index]).unwrap()),
            basis: [roots[index - 1].id(), roots[index].id()],
        });
        *end += 1;
    }
}

fn block_kind(block: &BlockNode) -> GroupKind {
    match block {
        BlockNode::Heading(_) => GroupKind::Heading,
        BlockNode::Paragraph(_) => GroupKind::Prose,
        BlockNode::List(_) => GroupKind::List,
        BlockNode::Definition { .. } => GroupKind::Opaque,
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
        | BlockNode::Definition { blocks, .. }
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
    #[test]
    fn margin_note_anchor_groups_preserve_complete_source_ownership() {
        let source = include_str!("../../../../performance/layout-fixtures/101-margin-notes.md");
        let document = document_core::Document::from_markdown(source).unwrap();
        let snapshot = document.snapshot();
        let roots = snapshot
            .blocks()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>();
        let groups = super::GroupAnalysis::build(&roots);
        assert!(groups.validate(&roots));
        let mut anchored = 0;
        for (i, root) in roots.iter().enumerate() {
            if let document_core::BlockNode::BlockQuote { blocks, .. } = root
                && let Some(anchor) = crate::quotes::margin_note_anchor(
                    i.checked_sub(1).map(|previous| roots[previous]),
                    blocks,
                )
            {
                anchored += 1;
                let group = groups.group_for_root(anchor).unwrap();
                assert_eq!(group.roots.end, i);
                assert!(
                    group.roots.start == i - 1
                        || matches!(
                            roots[group.roots.start],
                            document_core::BlockNode::Heading(_)
                        ) && group.roots.len() == 2
                );
            }
        }
        assert_eq!(anchored, 2);
        assert_eq!(snapshot.serialize().unwrap(), source);
    }

    #[test]
    fn critical_warning_and_following_action_share_one_source_group() {
        for action in [
            "```sh\npwd\n```\n",
            "1. Save the current document\n2. Verify the copy\n",
        ] {
            for lead in ["", "Run the following check:\n\n"] {
                for kind in ["WARNING", "CAUTION", "IMPORTANT"] {
                    let source = format!(
                        "## Before the action\n\n> [!{kind}]\n> Keep the original file available.\n\n{lead}{action}\n## Next topic\n\nAfter.\n"
                    );
                    let document = document_core::Document::from_markdown(source.as_str()).unwrap();
                    let snapshot = document.snapshot();
                    let roots = snapshot
                        .blocks()
                        .iter()
                        .map(AsRef::as_ref)
                        .collect::<Vec<_>>();
                    let plan = super::GroupAnalysis::build(&roots);
                    assert!(plan.validate(&roots));
                    let action = roots
                        .iter()
                        .find(|root| {
                            matches!(
                                root,
                                document_core::BlockNode::CodeBlock(_)
                                    | document_core::BlockNode::List(_)
                            )
                        })
                        .unwrap();
                    assert_eq!(
                        plan.group_for_root(roots[1].id()).unwrap().id,
                        plan.group_for_root(action.id()).unwrap().id,
                        "warning and action must not become separate composition units: {source}"
                    );
                    assert_eq!(snapshot.serialize().unwrap(), source);
                }
            }
        }
    }

    #[test]
    fn shared_caption_cross_group_attachment_preserves_valid_ownership() {
        let document = document_core::Document::from_markdown(
            "![A](a.png)\n\n![B](b.png)\n\nGallery: The pair\n\nGallery credit: Notebook\n",
        )
        .unwrap();
        let snapshot = document.snapshot();
        let roots = snapshot
            .blocks()
            .iter()
            .map(AsRef::as_ref)
            .collect::<Vec<_>>();
        let mut groups = super::GroupAnalysis::build(&roots);
        assert!(
            groups.validate(&roots),
            "an adjacent shared-caption edge is not duplicate source ownership"
        );
        let caption = groups.groups.last_mut().unwrap();
        caption.relationships[0].basis[0] = roots[0].id();
        assert!(
            !groups.validate(&roots),
            "a nonadjacent attachment remains invalid"
        );
    }

    #[test]
    fn critical_instruction_attachment_respects_severity_and_source_boundaries() {
        for source in [
            "> [!NOTE]\n> Optional context.\n\n```sh\npwd\n```\n".to_owned(),
            "> [!TIP]\n> Helpful context.\n\n1. Open the file\n".to_owned(),
            "> [!WARNING]\n> Before another section.\n\n## Other\n\n```sh\npwd\n```\n".to_owned(),
            "> [!WARNING]\n> Before ordinary prose.\n\nAn unrelated paragraph.\n\n```sh\npwd\n```\n".to_owned(),
            "> [!WARNING]\n> Before unordered facts.\n\n- A fact\n- Another fact\n".to_owned(),
            format!("> [!WARNING]\n> {}\n\n```sh\npwd\n```\n", "A long explanation. ".repeat(40)),
        ] {
            let document = document_core::Document::from_markdown(source.as_str()).unwrap();
            let snapshot = document.snapshot();
            let roots = snapshot.blocks().iter().map(AsRef::as_ref).collect::<Vec<_>>();
            let groups = super::GroupAnalysis::build(&roots);
            assert!(groups.validate(&roots));
            assert!(groups.groups.iter().flat_map(|g| &g.relationships).all(|r| r.kind != super::RelationshipKind::CriticalInstruction), "{source}");
            assert_eq!(snapshot.serialize().unwrap(), source);
        }
    }

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
    fn complete_bounded_explanations_keep_their_heading_and_source_partition() {
        for heading in ["", "## Shape\n\n"] {
            for count in 1..=4 {
                let source = format!(
                    "{heading}{}```json\n{{}}\n```\n",
                    "A short explanation.\n\n".repeat(count)
                );
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let roots: Vec<_> = projection.roots().collect();
                let analysis = GroupAnalysis::build(&roots);
                assert!(analysis.validate(&roots));
                assert_eq!(analysis.groups.len(), 1);
                assert_eq!(analysis.groups[0].kind, GroupKind::ExplanationContent);
                assert_eq!(analysis.groups[0].roots, 0..roots.len());
                assert_eq!(
                    analysis.groups[0]
                        .relationships
                        .iter()
                        .filter(|r| r.kind == RelationshipKind::ProseContinuation)
                        .count(),
                    count - 1
                );
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        }
    }

    #[test]
    fn extended_explanations_are_nominated_by_structure_not_encoding_size() {
        for text in [
            "x".repeat(421),
            "界".repeat(421),
            "A complete explanation. ".repeat(240),
        ] {
            for heading in ["", "## Context\n\n"] {
                let source =
                    format!("{heading}{text}\n\nSecond paragraph.\n\n```json\n{{}}\n```\n");
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let roots = projection.roots().collect::<Vec<_>>();
                let analysis = GroupAnalysis::build(&roots);
                assert!(analysis.validate(&roots));
                assert_eq!(analysis.groups.len(), 1);
                assert_eq!(analysis.groups[0].kind, GroupKind::ExplanationContent);
                assert_eq!(analysis.groups[0].roots, 0..roots.len());
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        }
    }

    #[test]
    fn explanation_nomination_respects_boundaries_and_complete_galleries() {
        for source in [
            "First.\n\n## Boundary\n\n```json\n{}\n```\n".to_owned(),
            "First.\n\n---\n\n```json\n{}\n```\n".to_owned(),
            "First.\n\n![A](a.png)\n\n![B](b.png)\n".to_owned(),
            format!("{}```json\n{{}}\n```\n", "Paragraph.\n\n".repeat(5)),
        ] {
            let document = Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let roots: Vec<_> = projection.roots().collect();
            assert!(explanation_content(&roots, 0).is_none());
            assert!(GroupAnalysis::build(&roots).validate(&roots));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
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
