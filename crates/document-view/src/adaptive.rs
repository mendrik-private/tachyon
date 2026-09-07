//! Content analysis and bounded candidate selection. This module runs when
//! geometry changes, never while painting or scrolling. Presentation is not an
//! edit to the document model.

use std::collections::{HashMap, HashSet};
use std::{ops::Range, sync::Arc};

use document_core::{BlockNode, ListBlock, ListKind, NodeId};

use crate::TextProjection;

pub(crate) mod candidates;
mod groups;
pub(crate) mod rows;
use groups::GroupAnalysis;

pub(crate) const PROSE_WIDTH: f32 = 960.;
pub(crate) const LAYOUT_GAP: f32 = 16.;
pub(crate) const LAYOUT_HEADER: f32 = 24.;
pub(crate) const CARD_PADDING: f32 = 16.;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) enum ListLayout {
    List,
    Grid(usize),
    Steps,
    Checklist,
    Outline,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutSlot {
    pub group: NodeId,
    pub item: usize,
    pub columns: usize,
    pub cards: bool,
    /// Exact twelve-track placement; item/columns still own source-order rows.
    pub track_start: u8,
    pub span: u8,
    /// Preserve measured inline geometry while the containing row is edited.
    pub fixed_canvas: Option<f32>,
}

impl LayoutSlot {
    pub fn same_column(self, other: Self) -> bool {
        self.group == other.group && self.item == other.item && self.columns == other.columns
    }

    pub fn width(self, canvas: f32) -> f32 {
        candidates::span_width(self.fixed_canvas.unwrap_or(canvas), self.span).unwrap_or(1.)
    }

    pub fn left(self, canvas: f32) -> f32 {
        if self.track_start == 0 {
            0.
        } else {
            candidates::span_width(self.fixed_canvas.unwrap_or(canvas), self.track_start)
                .unwrap_or(0.)
                + LAYOUT_GAP
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ListArrangement {
    pub layout: ListLayout,
    pub first_node: NodeId,
    pub completed: usize,
    pub count: usize,
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AdaptivePlan {
    pub canvas: f32,
    pub editing_node: Option<NodeId>,
    root_ids: Vec<NodeId>,
    root_content: Vec<Arc<BlockNode>>,
    root_ordinals: HashMap<NodeId, usize>,
    pub windows: Vec<Range<usize>>,
    pub measurement_ranges: Option<Vec<Range<usize>>>,
    /// Exact geometry needed immediately during structural editing, without
    /// claiming these windows have a newly optimized/committable row plan.
    pub edit_geometry_ranges: Vec<Range<usize>>,
    pub measurement_identity: Option<Arc<()>>,
    pub resource_generation: u64,
    pub pending_images: HashSet<NodeId>,
    pub lists: HashMap<NodeId, ListArrangement>,
    pub slots: HashMap<NodeId, LayoutSlot>,
    pub lead: Option<NodeId>,
    pub measured_lists: HashMap<NodeId, candidates::ListDecision>,
    pub measured_rows: rows::RowSelection,
    groups: std::sync::Arc<GroupAnalysis>,
}

/// Only the planner outputs consumed by the renderer. Candidate scores and
/// inspector state may change without changing geometry; measurement windows
/// cannot, since they select exact wraps versus complete estimated fallback.
pub(crate) struct PlanGeometryKey {
    slots: HashMap<NodeId, LayoutSlot>,
    lists: HashMap<NodeId, ListArrangement>,
    lead: Option<NodeId>,
    groups: Arc<GroupAnalysis>,
    measured_windows: Option<Vec<Range<usize>>>,
    edit_geometry_ranges: Vec<Range<usize>>,
}

impl PlanGeometryKey {
    pub(crate) fn matches(&self, plan: &AdaptivePlan) -> bool {
        self.slots == plan.slots
            && self.lists == plan.lists
            && self.lead == plan.lead
            && self.measured_windows.as_deref() == plan.geometry_measurement_windows()
            && self.edit_geometry_ranges == plan.edit_geometry_ranges
            && (Arc::ptr_eq(&self.groups, &plan.groups) || self.groups == plan.groups)
    }
}

impl AdaptivePlan {
    pub(crate) fn geometry_key(&self) -> PlanGeometryKey {
        PlanGeometryKey {
            slots: self.slots.clone(),
            lists: self.lists.clone(),
            lead: self.lead,
            groups: self.groups.clone(),
            measured_windows: self.geometry_measurement_windows().map(<[_]>::to_vec),
            edit_geometry_ranges: self.edit_geometry_ranges.clone(),
        }
    }

    pub fn build(
        projection: &TextProjection,
        width: f32,
        previous: Option<&Self>,
        keep_arrangements: bool,
    ) -> Self {
        let mut plan = Self::default();
        let mut seen = HashSet::new();
        let first_nodes = projection
            .segments()
            .iter()
            .filter(|segment| seen.insert(segment.top_level_node_id))
            .map(|segment| (segment.top_level_node_id, segment.node_id))
            .collect::<HashMap<_, _>>();
        let roots = projection.roots().collect::<Vec<_>>();
        plan.canvas = width;
        plan.root_ids = roots.iter().map(|root| root.id()).collect();
        plan.root_content = plan
            .root_ids
            .iter()
            .filter_map(|id| projection.block_handle(*id).cloned())
            .collect();
        plan.root_ordinals = plan
            .root_ids
            .iter()
            .enumerate()
            .map(|(i, id)| (*id, i))
            .collect();
        plan.groups = std::sync::Arc::new(GroupAnalysis::build(&roots));
        // Refuse advanced geometry on an invalid ownership partition. The
        // existing renderer remains a complete source-order stack fallback.
        if !plan.groups.validate(&roots) {
            return plan;
        }
        if let [BlockNode::Heading(title), BlockNode::Paragraph(intro), ..] = roots.as_slice()
            && title.level == 1
            && intro.content.len() <= 420
        {
            plan.lead = Some(intro.id);
        }
        for root in &roots {
            let BlockNode::List(list) = root else {
                continue;
            };
            let Some(&first_node) = first_nodes.get(&list.id) else {
                continue;
            };
            let facts = ListFacts::analyze(list);
            let old = previous
                .and_then(|plan| plan.lists.get(&list.id))
                .map(|list| list.layout);
            let layout = if keep_arrangements && let Some(old) = old {
                // Text may grow within a card; it must not jump columns under
                // the caret. Structural edits introducing nesting stay safe.
                if matches!(old, ListLayout::Grid(_))
                    && (facts.tasks
                        || list.items.iter().any(|item| {
                            item.blocks.len() != 1
                                || !matches!(
                                    item.blocks.iter().next().map(AsRef::as_ref),
                                    Some(BlockNode::Paragraph(_))
                                )
                        }))
                {
                    facts.stack_layout()
                } else {
                    old
                }
            } else {
                facts.stack_layout()
            };
            if let ListLayout::Grid(columns) = layout {
                for (index, item) in list.items.iter().enumerate() {
                    for block in &item.blocks {
                        plan.slots.insert(
                            block.id(),
                            LayoutSlot {
                                group: list.id,
                                item: index,
                                columns,
                                cards: true,
                                track_start: ((index % columns) * (12 / columns)) as u8,
                                span: (12 / columns) as u8,
                                fixed_canvas: previous
                                    .filter(|_| keep_arrangements)
                                    .and_then(|plan| plan.slots.get(&block.id()))
                                    .filter(|slot| slot.group == list.id && slot.columns == columns)
                                    .and_then(|slot| slot.fixed_canvas),
                            },
                        );
                    }
                }
            }
            plan.lists.insert(
                list.id,
                ListArrangement {
                    layout,
                    first_node,
                    completed: list
                        .items
                        .iter()
                        .filter(|item| item.checked == Some(true))
                        .count(),
                    count: list.items.len(),
                },
            );
        }
        // Peer sections and galleries require measured candidates; initial
        // geometry stays in complete source-order stacks.
        plan.windows = rows::planning_windows(&plan, projection);
        plan
    }

    pub fn root_ordinal(&self, id: NodeId) -> Option<usize> {
        self.root_ordinals.get(&id).copied()
    }

    pub fn windows_for(&self, requested: &Range<usize>) -> Vec<Range<usize>> {
        let start = self
            .windows
            .partition_point(|range| range.end <= requested.start);
        let end = self
            .windows
            .partition_point(|range| range.start < requested.end);
        self.windows[start.min(end)..end].to_vec()
    }

    pub fn covers(&self, requested: &Range<usize>) -> bool {
        self.windows_for(requested).iter().all(|window| {
            let index = self
                .measured_rows
                .covered
                .partition_point(|range| range.end <= window.start);
            self.measured_rows.covered.get(index) == Some(window)
        })
    }

    pub fn measures_root(&self, id: NodeId) -> bool {
        self.measurement_ranges.as_ref().is_none_or(|ranges| {
            self.root_ordinal(id)
                .is_some_and(|ordinal| ranges.iter().any(|range| range.contains(&ordinal)))
        })
    }

    // Candidate scope is the current request; final geometry also retains all
    // compatible windows measured on earlier visits. Their coverage is a real
    // renderer input: newly visited plain prose can change wraps without slots.
    fn geometry_measurement_windows(&self) -> Option<&[Range<usize>]> {
        self.measurement_ranges
            .as_ref()
            .map(|_| self.measured_rows.covered.as_slice())
    }

    pub fn has_measured_geometry(&self, id: NodeId) -> bool {
        self.geometry_measurement_windows().is_none_or(|windows| {
            self.root_ordinal(id).is_some_and(|ordinal| {
                let index = windows.partition_point(|range| range.end <= ordinal);
                windows
                    .get(index)
                    .is_some_and(|range| range.contains(&ordinal))
                    || self
                        .edit_geometry_ranges
                        .iter()
                        .any(|range| range.contains(&ordinal))
            })
        })
    }

    fn compatible_environment(&self, old: &Self) -> bool {
        self.canvas.to_bits() == old.canvas.to_bits()
            && self.resource_generation == old.resource_generation
            && self.measured_rows.viewport.to_bits() == old.measured_rows.viewport.to_bits()
            && self
                .measurement_identity
                .as_ref()
                .zip(old.measurement_identity.as_ref())
                .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
    }

    fn unchanged_root(&self, old: &Self, id: NodeId) -> bool {
        self.root_ordinal(id)
            .zip(old.root_ordinal(id))
            .is_some_and(|(new, before)| {
                Arc::ptr_eq(&self.root_content[new], &old.root_content[before])
            })
    }

    /// Refine list nominations with the actual native renderer. The callback
    /// only visits at most nine simple paragraph nodes at three widths.
    pub fn measure_lists(
        &mut self,
        projection: &TextProjection,
        width: f32,
        previous: Option<&Self>,
        keep_arrangements: bool,
        mut measure: impl FnMut(NodeId, f32, bool) -> Option<candidates::ItemMeasurement>,
    ) {
        for root in projection.roots() {
            let BlockNode::List(list) = root else {
                continue;
            };
            let Some(arrangement) = self.lists.get(&list.id) else {
                continue;
            };
            if !self.measures_root(list.id) {
                // No speculative grid before native measurement. Previously
                // measured, unchanged lists keep their exact offscreen layout.
                self.slots.retain(|_, slot| slot.group != list.id);
                if let Some(old) = previous.filter(|old| {
                    self.compatible_environment(old) && self.unchanged_root(old, list.id)
                }) && let Some(decision) = old.measured_lists.get(&list.id)
                {
                    self.lists.get_mut(&list.id).unwrap().layout = decision.layout;
                    self.measured_lists.insert(list.id, decision.clone());
                    for (&node, &slot) in &old.slots {
                        if slot.group == list.id {
                            self.slots.insert(node, slot);
                        }
                    }
                } else if matches!(arrangement.layout, ListLayout::Grid(_)) {
                    self.lists.get_mut(&list.id).unwrap().layout = ListLayout::List;
                }
                continue;
            }
            // A whole-list grid cannot simultaneously be a child grid inside
            // a peer card. Its outer section owns that geometry.
            if self
                .slots
                .get(&arrangement.first_node)
                .is_some_and(|slot| slot.group != list.id)
            {
                continue;
            }
            let facts = ListFacts::analyze(list);
            let eligible = (3..=9).contains(&list.items.len())
                && facts.simple
                && !facts.tasks
                && !facts.nested
                && !facts.sequence
                && list.items.iter().all(|item| {
                    item.blocks.iter().all(|block| {
                        // Bound hostile single-node work before native shaping.
                        block.text().is_some_and(|text| text.len() <= 4096)
                    })
                });
            if self.editing_node.is_some_and(|node| {
                projection
                    .segment_for_node(node)
                    .is_some_and(|s| s.top_level_node_id == list.id)
            }) && let Some(old_plan) = previous
                && let Some(old) = old_plan.lists.get(&list.id)
            {
                let frozen_width = old_plan
                    .slots
                    .values()
                    .find(|slot| slot.group == list.id)
                    .and_then(|slot| slot.fixed_canvas)
                    .unwrap_or(old_plan.canvas);
                self.slots.retain(|_, slot| slot.group != list.id);
                let structurally_flat = !facts.tasks
                    && list.items.iter().all(|item| {
                        item.blocks.len() == 1
                            && matches!(
                                item.blocks.iter().next().map(AsRef::as_ref),
                                Some(BlockNode::Paragraph(_))
                            )
                    });
                let layout = if frozen_width <= width + 0.01 && structurally_flat {
                    old.layout
                } else if matches!(old.layout, ListLayout::Grid(_)) {
                    ListLayout::List
                } else {
                    old.layout
                };
                self.lists.get_mut(&list.id).unwrap().layout = layout;
                if let ListLayout::Grid(columns) = layout {
                    for (item, entry) in list.items.iter().enumerate() {
                        for block in &entry.blocks {
                            self.slots.insert(
                                block.id(),
                                LayoutSlot {
                                    group: list.id,
                                    item,
                                    columns,
                                    cards: true,
                                    track_start: ((item % columns) * (12 / columns)) as u8,
                                    span: (12 / columns) as u8,
                                    fixed_canvas: Some(frozen_width),
                                },
                            );
                        }
                    }
                }
                self.measured_rows.edit_locked = true;
                continue;
            }
            if keep_arrangements
                && let Some(old) = previous.and_then(|p| p.lists.get(&list.id))
                && old.layout == arrangement.layout
            {
                if let Some(decision) = previous.and_then(|p| p.measured_lists.get(&list.id)) {
                    self.measured_lists.insert(list.id, decision.clone());
                }
                continue;
            }
            self.slots.retain(|_, slot| slot.group != list.id);
            if !eligible {
                if matches!(arrangement.layout, ListLayout::Grid(_)) {
                    self.lists.get_mut(&list.id).unwrap().layout = ListLayout::List;
                }
                continue;
            }
            let nodes = list
                .items
                .iter()
                .map(|item| item.blocks.iter().next().unwrap().id())
                .collect::<Vec<_>>();
            let old = previous
                .and_then(|p| p.measured_lists.get(&list.id))
                .map(|d| d.layout);
            let decision =
                candidates::choose_list(nodes.len(), width, old, |index, width, cards| {
                    measure(nodes[index], width, cards)
                });
            if !decision.is_valid(nodes.len(), width, old) {
                self.lists.get_mut(&list.id).unwrap().layout = ListLayout::List;
                continue;
            }
            self.lists.get_mut(&list.id).unwrap().layout = decision.layout;
            if let ListLayout::Grid(columns) = decision.layout {
                for (item, node) in nodes.into_iter().enumerate() {
                    self.slots.insert(
                        node,
                        LayoutSlot {
                            group: list.id,
                            item,
                            columns,
                            cards: true,
                            track_start: ((item % columns) * (12 / columns)) as u8,
                            span: (12 / columns) as u8,
                            fixed_canvas: None,
                        },
                    );
                }
            }
            self.measured_lists.insert(list.id, decision);
        }
    }

    /// Pairwise whitespace policy in unscaled logical pixels. Content inset
    /// (quote rails, code headers, table cells) is accounted for separately.
    pub fn gap_between(&self, previous: &BlockNode, next: &BlockNode) -> f32 {
        if matches!(previous, BlockNode::Heading(_)) {
            return 10.;
        }
        if let BlockNode::Heading(heading) = next {
            return if heading.level <= 2 { 44. } else { 32. };
        }
        let before = self.groups.group_for_root(previous.id());
        let after = self.groups.group_for_root(next.id());
        if let (Some(before), Some(after)) = (before, after) {
            // Invisible/non-text canonical roots still separate compositions.
            if before.roots.end < after.roots.start {
                return 40.;
            }
            if before.id == after.id
                && before.relationships.iter().any(|relation| {
                    relation.basis == [previous.id(), next.id()]
                        && relation.kind == groups::RelationshipKind::AdjacentExplanation
                })
            {
                return 12.;
            }
        }
        24.
    }
}

fn compact_section(roots: &[&BlockNode], start: usize) -> Option<(u8, usize, u8)> {
    let BlockNode::Heading(heading) = roots.get(start)? else {
        return None;
    };
    if heading.level < 3 || heading.content.len() > 64 {
        return None;
    }
    let mut end = start + 1;
    let mut size = 0;
    let mut shape = 0;
    while let Some(block) = roots.get(end) {
        match block {
            BlockNode::Heading(_) => break,
            BlockNode::Paragraph(paragraph) => {
                size += paragraph.content.len();
                shape |= 1;
            }
            BlockNode::List(list)
                if list.items.len() <= 6
                    && list.items.iter().all(|item| {
                        item.blocks.len() == 1
                            && matches!(
                                item.blocks.iter().next().map(AsRef::as_ref),
                                Some(BlockNode::Paragraph(_))
                            )
                    }) =>
            {
                size += list
                    .items
                    .iter()
                    .flat_map(|item| item.blocks.iter())
                    .filter_map(|block| block.text())
                    .map(|text| text.len())
                    .sum::<usize>();
                shape |= 2;
            }
            _ => return None,
        }
        end += 1;
        if end - start > 4 || size > 360 {
            return None;
        }
    }
    (end > start + 1).then_some((heading.level, end, shape))
}

struct ListFacts {
    simple: bool,
    nested: bool,
    tasks: bool,
    sequence: bool,
}

impl ListFacts {
    fn analyze(list: &ListBlock) -> Self {
        let mut actions = 0;
        let mut references = false;
        let mut simple = (3..=12).contains(&list.items.len());
        let mut nested = false;
        for item in list.items.iter() {
            nested |= item
                .blocks
                .iter()
                .any(|block| matches!(block.as_ref(), BlockNode::List(_)));
            if item.blocks.len() != 1 {
                simple = false;
            }
            if let Some(BlockNode::Paragraph(paragraph)) =
                item.blocks.iter().next().map(AsRef::as_ref)
            {
                // Bounded analysis even for a giant paragraph/list.
                if paragraph.content.len() > 4096 {
                    simple = false;
                    continue;
                }
                let text = paragraph.content.as_string();
                actions += usize::from(is_instruction(&text));
                references |= has_step_reference(&text)
                    || paragraph.content.runs().iter().any(|run| run.styles.iter().any(|style| {
                        matches!(style, document_core::InlineStyle::Link(target) if target.0.starts_with('#'))
                    }));
                simple &= !text.contains('\n');
            } else {
                simple = false;
            }
        }
        let ordered = matches!(list.kind, ListKind::Ordered { .. });
        Self {
            sequence: ordered && (actions * 2 >= list.items.len() || !simple || references),
            simple,
            nested,
            tasks: list.items.iter().any(|item| item.checked.is_some()),
        }
    }

    // Structural analysis can select a semantic vertical treatment, not a
    // column arrangement. Only measure_lists may nominate a new grid using
    // native, width-dependent item geometry; initial views remain valid stacks.
    fn stack_layout(&self) -> ListLayout {
        if self.tasks {
            return ListLayout::Checklist;
        }
        if self.nested {
            return ListLayout::Outline;
        }
        if self.sequence {
            return ListLayout::Steps;
        }
        ListLayout::List
    }
}

fn is_instruction(text: &str) -> bool {
    let mut words = text.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    if words.next().is_none() {
        return false;
    }
    matches!(
        first
            .trim_matches(|c: char| !c.is_alphabetic())
            .to_ascii_lowercase()
            .as_str(),
        "first"
            | "then"
            | "next"
            | "finally"
            | "install"
            | "run"
            | "open"
            | "create"
            | "configure"
            | "execute"
            | "click"
            | "select"
            | "enter"
            | "verify"
            | "restart"
            | "download"
            | "save"
            | "copy"
            | "add"
            | "remove"
            | "set"
            | "record"
            | "review"
            | "confirm"
    )
}

fn has_step_reference(text: &str) -> bool {
    let mut previous_is_step = false;
    text.split_whitespace().any(|word| {
        let numeric = previous_is_step && word.starts_with(|c: char| c.is_ascii_digit());
        previous_is_step = word.eq_ignore_ascii_case("step") || word.eq_ignore_ascii_case("item");
        numeric
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_core::Document;

    fn plan(source: &str, width: f32) -> AdaptivePlan {
        let doc = Document::from_markdown(source).unwrap();
        AdaptivePlan::build(
            &TextProjection::from_snapshot(&doc.snapshot()),
            width,
            None,
            false,
        )
    }

    #[test]
    fn unmeasured_lists_keep_semantic_vertical_treatments_at_every_width() {
        let short = "1. North\n2. South\n3. East\n4. West\n5. Above\n6. Below\n";
        for width in [1000., 620., 696., 400.] {
            assert_eq!(
                plan(short, width).lists.values().next().unwrap().layout,
                ListLayout::List
            );
        }
        for (source, expected) in [
            (
                "1. Install the package\n2. Configure your workspace\n3. Run the command\n",
                ListLayout::Steps,
            ),
            ("- [x] One\n- [ ] Two\n- [ ] Three\n", ListLayout::Checklist),
            ("- One\n  - Nested\n- Two\n- Three\n", ListLayout::Outline),
        ] {
            assert_eq!(
                plan(source, 1200.).lists.values().next().unwrap().layout,
                expected
            );
        }
        let uneven = format!("- Short\n- Brief\n- {}\n", "substantial detail ".repeat(12));
        assert_eq!(
            plan(&uneven, 1000.).lists.values().next().unwrap().layout,
            ListLayout::List
        );
    }

    #[test]
    fn lead_position_does_not_imply_unmeasured_column_geometry() {
        let p = plan(
            "# Title\n\nAn introduction.\n\n1. One\n2. Two\n3. Three\n4. Four\n5. Five\n6. Six\n",
            1000.,
        );
        assert!(p.lead.is_some());
        assert!(p.slots.is_empty());
        assert_eq!(p.lists.values().next().unwrap().count, 6);
        assert!(
            plan("An ordinary paragraph.\n\n# Later title\n", 1000.)
                .lead
                .is_none()
        );
    }
}
