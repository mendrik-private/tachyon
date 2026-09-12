//! Content analysis and bounded candidate selection. This module runs when
//! geometry changes, never while painting or scrolling. Presentation is not an
//! edit to the document model.

use std::collections::{HashMap, HashSet};
use std::{ops::Range, sync::Arc};

use document_core::{BlockNode, ListBlock, ListKind, NodeId};

use crate::TextProjection;

pub(crate) mod candidates;
pub(crate) mod definitions;
pub(crate) mod editorial;
mod groups;
pub(crate) mod inline_lists;
pub(crate) mod metadata;
pub(crate) mod prose;
pub(crate) mod resource;
pub(crate) mod rows;
pub(crate) mod timeline;
use groups::GroupAnalysis;

pub(crate) const PROSE_WIDTH: f32 = crate::theme::DocumentStyle::PROSE_WIDTH;
pub(crate) const LAYOUT_GAP: f32 = crate::theme::DocumentStyle::GUTTER;
pub(crate) const LAYOUT_HEADER: f32 = crate::theme::DocumentStyle::TASK_HEADER_HEIGHT;
pub(crate) const CARD_PADDING: f32 = crate::theme::DocumentStyle::INSET;
// Cheap work bound only; native measurements still decide whether a grid fits.
const SHORT_LIST_ITEMS: std::ops::RangeInclusive<usize> = 2..=12;
pub(crate) const NEARBY_LIST_GRID_GAP_LINES: usize = 5;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) enum ListLayout {
    List,
    Grid(usize),
    Steps,
    Checklist,
    Outline,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum CardAccent {
    #[default]
    None,
    Leading,
    /// Authored labels provide markerless anchors without an enclosure.
    OpenLabeled,
    /// Independent features share whitespace and anchors, not an enclosure.
    Open,
    /// Authored ordered siblings use a large numeral, never a category icon.
    Numbered,
    /// A source-owned external object; never a generated link preview.
    Resource(resource::Resource),
    /// An explicitly named decision, option, example or trade-off object.
    Editorial(editorial::Kind),
}

impl CardAccent {
    /// Open modules align to the document edge; only enclosures own an inset.
    pub fn inset(self) -> f32 {
        match self {
            Self::Open | Self::OpenLabeled => 0.,
            Self::Resource(resource) => resource.inset(),
            _ => CARD_PADDING,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LayoutSlot {
    /// Align the table/code start across compact technical sibling columns.
    /// Re-resolve the anchor from current text geometry during local edits.
    pub align_components: bool,
    pub group: NodeId,
    pub item: usize,
    /// Source-ordered row identity; independent of this row's column count.
    pub row: usize,
    pub columns: usize,
    pub cards: bool,
    /// Structural emphasis inferred from the authored relationship. Companion
    /// panels use a leading rail; repeated labelled features remain open.
    pub card_accent: CardAccent,
    /// Exact twelve-track placement inside the explicit source-order row.
    pub track_start: u8,
    pub span: u8,
    /// Preserve measured inline geometry while the containing row is edited.
    pub fixed_canvas: Option<f32>,
}

impl LayoutSlot {
    pub fn inset(self) -> f32 {
        if self.cards {
            self.card_accent.inset()
        } else {
            0.
        }
    }

    pub fn same_column(self, other: Self) -> bool {
        self.same_row(other) && self.item == other.item && self.columns == other.columns
    }

    pub fn same_row(self, other: Self) -> bool {
        self.group == other.group && self.row == other.row
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

/// One source paragraph displayed as a term and its description. The split is
/// a byte boundary in that paragraph, never new content or a second node.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct LabelColumns {
    pub label_end: usize,
    pub label_width: f32,
    pub body_width: f32,
    pub presentation: LabelPresentation,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum LabelPresentation {
    Terms,
    Timeline(timeline::Placement),
    Metadata(metadata::Placement),
}

impl LabelColumns {
    pub fn timeline(self) -> Option<timeline::Placement> {
        match self.presentation {
            LabelPresentation::Timeline(value) => Some(value),
            _ => None,
        }
    }

    pub fn metadata(self) -> Option<metadata::Placement> {
        match self.presentation {
            LabelPresentation::Metadata(value) => Some(value),
            _ => None,
        }
    }

    pub fn slot(self) -> Option<LayoutSlot> {
        match self.presentation {
            LabelPresentation::Terms => None,
            LabelPresentation::Timeline(value) => value.slot,
            LabelPresentation::Metadata(value) => value.slot,
        }
    }

    pub fn stacked(self) -> bool {
        match self.presentation {
            LabelPresentation::Terms => false,
            LabelPresentation::Timeline(value) => value.stacked,
            LabelPresentation::Metadata(value) => value.stacked,
        }
    }
}

/// Loaded-font reading measures shared by candidates, final geometry and
/// retained editing slots. These are document pixels, before reader zoom.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ProseMeasures {
    pub reference: f32,
    pub narrative: f32,
    pub lead: f32,
}

impl Default for ProseMeasures {
    fn default() -> Self {
        use crate::theme::DocumentStyle;
        Self {
            reference: PROSE_WIDTH,
            narrative: PROSE_WIDTH * DocumentStyle::READING_SIZE / DocumentStyle::REFERENCE_SIZE,
            lead: PROSE_WIDTH * DocumentStyle::LEAD_SIZE / DocumentStyle::REFERENCE_SIZE,
        }
    }
}

impl ProseMeasures {
    pub fn fit_width(self, available: f32, narrative: bool, lead: bool) -> f32 {
        use crate::theme::DocumentStyle;
        available.min(
            self.for_role(narrative, lead) * DocumentStyle::MAX_PROSE_CHARACTERS
                / DocumentStyle::PROSE_CHARACTERS,
        )
    }

    pub fn for_role(self, narrative: bool, lead: bool) -> f32 {
        if lead {
            self.lead
        } else if narrative {
            self.narrative
        } else {
            self.reference
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(crate) struct AdaptivePlan {
    pub canvas: f32,
    pub prose_measures: ProseMeasures,
    pub editing_node: Option<NodeId>,
    pub command_strips: HashMap<NodeId, u32>,
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
    pub label_rows: HashMap<NodeId, LabelColumns>,
    /// Each projected leaf in a dated event points at its authored header.
    /// Prepared once so visible supporting blocks can paint an offscreen rail.
    pub timeline_owners: HashMap<NodeId, NodeId>,
    /// Nested event header -> enclosing event header. Canonical ancestry makes
    /// this acyclic; visible descendants can paint both rails without a tree walk.
    pub timeline_parents: HashMap<NodeId, NodeId>,
    pub metadata_lists: HashSet<NodeId>,
    pub resources: HashMap<NodeId, resource::Resource>,
    pub editorials: HashMap<NodeId, editorial::Member>,
    pub slots: HashMap<NodeId, LayoutSlot>,
    /// Canonical paragraphs may span several source-ordered reading columns.
    pub prose_flows: HashMap<NodeId, Arc<prose::Flow>>,
    pub figure_flows: HashMap<NodeId, Arc<prose::FigureFlow>>,
    pub inline_lists: HashMap<NodeId, Arc<inline_lists::InlineList>>,
    pub lead: Option<NodeId>,
    /// First section after a real title and only introductory prose/metadata.
    opening_section: Option<NodeId>,
    /// Next heading -> preceding technical sibling. Independent of whether
    /// the measured arrangement is paired or stacked at this viewport.
    reference_boundaries: HashMap<NodeId, NodeId>,
    /// Published section typography, retained while a source edit has focus.
    pub reading_modes: HashMap<NodeId, bool>,
    pub quote_roles: HashMap<NodeId, crate::quotes::TextRole>,
    pub bibliography: HashMap<NodeId, NodeId>,
    pub measured_lists: HashMap<NodeId, candidates::ListDecision>,
    pub nearby_list_gap_lines: HashMap<NodeId, usize>,
    pub measured_inline_lists: HashMap<NodeId, candidates::ListDecision>,
    pub measured_rows: rows::RowSelection,
    groups: std::sync::Arc<GroupAnalysis>,
}

/// Only the planner outputs consumed by the renderer. Candidate scores and
/// inspector state may change without changing geometry; measurement windows
/// cannot, since they select exact wraps versus complete estimated fallback.
pub(crate) struct PlanGeometryKey {
    prose_measures: ProseMeasures,
    slots: HashMap<NodeId, LayoutSlot>,
    prose_flows: HashMap<NodeId, Arc<prose::Flow>>,
    figure_flows: HashMap<NodeId, Arc<prose::FigureFlow>>,
    inline_lists: HashMap<NodeId, Arc<inline_lists::InlineList>>,
    lists: HashMap<NodeId, ListArrangement>,
    label_rows: HashMap<NodeId, LabelColumns>,
    timeline_owners: HashMap<NodeId, NodeId>,
    timeline_parents: HashMap<NodeId, NodeId>,
    resources: HashMap<NodeId, resource::Resource>,
    editorials: HashMap<NodeId, editorial::Member>,
    lead: Option<NodeId>,
    opening_section: Option<NodeId>,
    reference_boundaries: HashMap<NodeId, NodeId>,
    reading_modes: HashMap<NodeId, bool>,
    quote_roles: HashMap<NodeId, crate::quotes::TextRole>,
    bibliography: HashMap<NodeId, NodeId>,
    groups: Arc<GroupAnalysis>,
    measured_windows: Option<Vec<Range<usize>>>,
    edit_geometry_ranges: Vec<Range<usize>>,
}

impl PlanGeometryKey {
    pub(crate) fn matches(&self, plan: &AdaptivePlan) -> bool {
        self.prose_measures == plan.prose_measures
            && self.slots == plan.slots
            && self.prose_flows == plan.prose_flows
            && self.figure_flows == plan.figure_flows
            && self.inline_lists == plan.inline_lists
            && self.lists == plan.lists
            && self.label_rows == plan.label_rows
            && self.timeline_owners == plan.timeline_owners
            && self.timeline_parents == plan.timeline_parents
            && self.resources == plan.resources
            && self.editorials == plan.editorials
            && self.lead == plan.lead
            && self.opening_section == plan.opening_section
            && self.reference_boundaries == plan.reference_boundaries
            && self.reading_modes == plan.reading_modes
            && self.quote_roles == plan.quote_roles
            && self.bibliography == plan.bibliography
            && self.measured_windows.as_deref() == plan.geometry_measurement_windows()
            && self.edit_geometry_ranges == plan.edit_geometry_ranges
            && (Arc::ptr_eq(&self.groups, &plan.groups) || self.groups == plan.groups)
    }
}

impl AdaptivePlan {
    pub(crate) fn geometry_key(&self) -> PlanGeometryKey {
        PlanGeometryKey {
            prose_measures: self.prose_measures,
            slots: self.slots.clone(),
            prose_flows: self.prose_flows.clone(),
            figure_flows: self.figure_flows.clone(),
            inline_lists: self.inline_lists.clone(),
            lists: self.lists.clone(),
            label_rows: self.label_rows.clone(),
            timeline_owners: self.timeline_owners.clone(),
            timeline_parents: self.timeline_parents.clone(),
            resources: self.resources.clone(),
            editorials: self.editorials.clone(),
            lead: self.lead,
            opening_section: self.opening_section,
            reference_boundaries: self.reference_boundaries.clone(),
            reading_modes: self.reading_modes.clone(),
            quote_roles: self.quote_roles.clone(),
            bibliography: self.bibliography.clone(),
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
        plan.metadata_lists = projection
            .segments()
            .iter()
            .filter(|segment| segment.context.metadata)
            .map(|segment| segment.top_level_node_id)
            .collect();
        plan.reading_modes = first_nodes
            .iter()
            .filter_map(|(root, node)| {
                projection
                    .segment_for_node(*node)
                    .map(|s| (*root, s.context.narrative))
            })
            .collect();
        plan.canvas = width;
        plan.bibliography = projection
            .segments()
            .iter()
            .filter_map(|s| {
                s.context
                    .bibliography
                    .map(|owner| (s.top_level_node_id, owner))
            })
            .collect();
        plan.quote_roles = projection
            .segments()
            .iter()
            .filter_map(|s| crate::quotes::TextRole::of(&s.context).map(|role| (s.node_id, role)))
            .collect();
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
        plan.editorials = editorial::analyze(&roots);
        plan.retain_editorials(projection, previous, keep_arrangements);
        // Refuse advanced geometry on an invalid ownership partition. The
        // existing renderer remains a complete source-order stack fallback.
        if !plan.groups.validate(&roots) {
            return plan;
        }
        for (start, root) in roots.iter().enumerate() {
            if let Some((level, end, TECHNICAL_SECTION_SHAPE)) = compact_section(&roots, start)
                && let Some(next) = roots.get(end)
                && compact_section(&roots, end).is_some_and(|(next_level, _, shape)| {
                    next_level == level && shape == TECHNICAL_SECTION_SHAPE
                })
                && plan.groups.sections[&root.id()].parent
                    == plan.groups.sections[&next.id()].parent
            {
                plan.reference_boundaries.insert(next.id(), root.id());
            }
        }
        plan.retain_reference_rhythm(projection, previous, keep_arrangements, None);
        for root in &roots {
            if plan.bibliography.contains_key(&root.id()) {
                continue;
            }
            match root {
                BlockNode::Paragraph(paragraph) => {
                    if projection
                        .segment_for_node(paragraph.id)
                        .is_some_and(|s| s.context.figure_text.is_some())
                    {
                        continue;
                    }
                    if let Some(resource) = resource::classify(paragraph) {
                        plan.resources.insert(paragraph.id, resource);
                    }
                }
                BlockNode::List(list) if resource::is_resource_list(list) => {
                    for item in list.items.iter() {
                        if let Some(BlockNode::Paragraph(p)) = item.blocks.get(0).map(AsRef::as_ref)
                        {
                            plan.resources.insert(p.id, resource::classify(p).unwrap());
                        }
                    }
                }
                _ => {}
            }
        }
        if let [BlockNode::Heading(title), BlockNode::Paragraph(intro), ..] = roots.as_slice()
            && title.level == 1
            && intro.content.len() <= 420
            && !plan.resources.contains_key(&intro.id)
            && !plan.bibliography.contains_key(&intro.id)
        {
            plan.lead = Some(intro.id);
        }
        if matches!(roots.first(), Some(BlockNode::Heading(title)) if title.level == 1) {
            // Resolve this once from the canonical prefix, independently of
            // visible planning windows. A list, timeline, figure or code block
            // starts real content; a subsequent paragraph cannot turn it back
            // into a title/lead opening.
            plan.opening_section = roots
                .iter()
                .skip(1)
                .find(|root| match root {
                    BlockNode::Paragraph(p) => {
                        plan.resources.contains_key(&p.id) || plan.bibliography.contains_key(&p.id)
                    }
                    BlockNode::List(list) => !plan.metadata_lists.contains(&list.id),
                    _ => true,
                })
                .and_then(|root| match root {
                    BlockNode::Heading(heading) if heading.level == 2 => Some(heading.id),
                    _ => None,
                });
        }
        for root in &roots {
            let BlockNode::List(list) = root else {
                continue;
            };
            let Some(&first_node) = first_nodes.get(&list.id) else {
                continue;
            };
            // Citation labels are references, not procedural numbers or cards.
            if plan.bibliography.contains_key(&list.id) {
                continue;
            }
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
                        let (row, column, columns) = previous
                            .and_then(|p| p.measured_lists.get(&list.id))
                            .and_then(|d| d.placement(index))
                            .unwrap_or((index / columns, index % columns, columns));
                        plan.slots.insert(
                            block.id(),
                            LayoutSlot {
                                align_components: false,
                                group: list.id,
                                item: index,
                                row,
                                columns,
                                cards: true,
                                card_accent: facts.card_accent(list),
                                track_start: (column * (12 / columns)) as u8,
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
            let (completed, count) = if layout == ListLayout::Checklist {
                checklist_progress(root)
            } else {
                (0, list.items.len())
            };
            plan.lists.insert(
                list.id,
                ListArrangement {
                    layout,
                    first_node,
                    completed,
                    count,
                },
            );
        }
        // Peer sections and galleries require measured candidates; initial
        // geometry stays in complete source-order stacks.
        plan.windows = rows::planning_windows(&plan, projection);
        plan.place_resources(projection, previous, keep_arrangements);
        plan.place_editorials(projection);
        plan
    }

    /// Reuse source-order slots for both natural-height cards and compact rows.
    /// Measured list grids may supply columns; no second layout tree is created.
    pub fn place_resources(
        &mut self,
        projection: &TextProjection,
        previous: Option<&Self>,
        keep: bool,
    ) {
        if let Some(old) = previous {
            for (&node, &resource) in &old.resources {
                if (keep || self.editing_node == Some(node))
                    && projection.segment_for_node(node).is_some_and(|s| {
                        s.context.table_cell.is_none()
                            && s.context.bibliography.is_none()
                            && s.context.quote_depth == 0
                            && s.context.alert.is_none()
                            && s.context.list_depth <= 1
                            && s.context.ordered_list_depth == 0
                    })
                    && matches!(projection.block(node), Some(BlockNode::Paragraph(_)))
                {
                    self.resources.insert(node, resource);
                }
            }
        }
        for (&node, &resource) in &self.resources {
            let Some(segment) = projection.segment_for_node(node) else {
                continue;
            };
            if self.lead == Some(node) {
                self.lead = None;
            }
            let old = previous
                .and_then(|plan| {
                    plan.slots.get(&node).copied().map(|mut slot| {
                        slot.fixed_canvas.get_or_insert(plan.canvas);
                        slot
                    })
                })
                .filter(|slot| {
                    (keep || self.editing_node == Some(node))
                        && slot.fixed_canvas.unwrap_or(self.canvas) <= self.canvas
                });
            let slot = old
                .or_else(|| self.slots.get(&node).copied())
                .filter(|slot| slot.group == segment.top_level_node_id || slot.group == node);
            let slot = if let Some(mut slot) = slot {
                slot.cards = true;
                slot.card_accent = CardAccent::Resource(resource);
                if slot.columns == 1 && old.is_none() {
                    slot.fixed_canvas = Some(
                        self.canvas
                            .min(self.prose_measures.reference + 2. * resource.inset() + 24.),
                    );
                }
                slot
            } else {
                let (group, item) = match projection.block(segment.top_level_node_id) {
                    Some(BlockNode::List(list)) => (
                        list.id,
                        list.items
                            .iter()
                            .position(|item| item.blocks.iter().any(|block| block.id() == node))
                            .unwrap_or(0),
                    ),
                    _ => (node, 0),
                };
                LayoutSlot {
                    align_components: false,
                    group,
                    item,
                    row: item,
                    columns: 1,
                    cards: true,
                    card_accent: CardAccent::Resource(resource),
                    track_start: 0,
                    span: 12,
                    fixed_canvas: Some(
                        self.canvas
                            .min(self.prose_measures.reference + 2. * resource.inset() + 24.),
                    ),
                }
            };
            self.slots.insert(node, slot);
        }
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

    /// Coalesce neighboring viewport requests without changing the semantic
    /// planning windows themselves. At most 15 windows are added at either
    /// edge; even a very long document never becomes a whole-document request.
    pub(crate) fn planning_batch(&self, requested: Range<usize>) -> Range<usize> {
        const WINDOWS_PER_BATCH: usize = 16;
        let start = self
            .windows
            .partition_point(|range| range.end <= requested.start);
        let end = self
            .windows
            .partition_point(|range| range.start < requested.end);
        if start >= end {
            return requested;
        }
        let first = start / WINDOWS_PER_BATCH * WINDOWS_PER_BATCH;
        let last = end
            .div_ceil(WINDOWS_PER_BATCH)
            .saturating_mul(WINDOWS_PER_BATCH)
            .min(self.windows.len());
        self.windows[first].start..self.windows[last - 1].end
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

    pub(crate) fn compatible_environment(&self, old: &Self) -> bool {
        self.canvas.to_bits() == old.canvas.to_bits()
            && self.resource_generation == old.resource_generation
            && self.measured_rows.viewport.to_bits() == old.measured_rows.viewport.to_bits()
            && self
                .measurement_identity
                .as_ref()
                .zip(old.measurement_identity.as_ref())
                .is_some_and(|(a, b)| Arc::ptr_eq(a, b))
    }

    pub(crate) fn unchanged_root(&self, old: &Self, id: NodeId) -> bool {
        self.root_ordinal(id)
            .zip(old.root_ordinal(id))
            .is_some_and(|(new, before)| {
                Arc::ptr_eq(&self.root_content[new], &old.root_content[before])
            })
    }

    /// Refine list nominations with the actual native renderer. The callback
    /// only visits at most twelve simple paragraph nodes at four widths.
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
            // A pair of authored terms already has a compact aligned-row
            // vocabulary. Expanding the work bound must not turn those rows
            // into entity cards. Explicit resource identities remain eligible.
            let term_pair = list.items.len() == 2
                && facts.labeled
                && list.items.iter().all(|item| {
                    item.blocks.get(0).is_some_and(|block| {
                        !self.resources.contains_key(&block.id())
                            && matches!(block.as_ref(), BlockNode::Paragraph(p) if has_authored_label(p))
                    })
                });
            let eligible = SHORT_LIST_ITEMS.contains(&list.items.len())
                && !term_pair
                && !timeline::is_timeline(list)
                && !self.metadata_lists.contains(&list.id)
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
                let layout = if frozen_width <= width + 0.01
                    && structurally_flat
                    && old.count == list.items.len()
                {
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
                            let (row, column, columns) = old_plan
                                .measured_lists
                                .get(&list.id)
                                .and_then(|d| d.placement(item))
                                .unwrap_or((item / columns, item % columns, columns));
                            self.slots.insert(
                                block.id(),
                                LayoutSlot {
                                    align_components: false,
                                    group: list.id,
                                    item,
                                    row,
                                    columns,
                                    cards: true,
                                    card_accent: facts.card_accent(list),
                                    track_start: (column * (12 / columns)) as u8,
                                    span: (12 / columns) as u8,
                                    fixed_canvas: Some(frozen_width),
                                },
                            );
                        }
                    }
                }
                if let Some(decision) = old_plan
                    .measured_lists
                    .get(&list.id)
                    .filter(|d| d.layout == layout)
                {
                    self.measured_lists.insert(list.id, decision.clone());
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
            let old = previous.and_then(|p| p.measured_lists.get(&list.id));
            let decision = candidates::choose_list(
                nodes.len(),
                width,
                self.prose_measures.reference
                    + nodes
                        .iter()
                        .filter_map(|node| self.resources.get(node))
                        .map(|resource| resource.inset() * 2. + 24.)
                        .fold(0_f32, f32::max),
                old,
                facts.labeled,
                facts.card_accent(list) == CardAccent::Open
                    && nodes.iter().all(|node| !self.resources.contains_key(node)),
                |index, width, cards| measure(nodes[index], width, cards),
            );
            if !decision.is_valid(nodes.len(), width, old) {
                self.lists.get_mut(&list.id).unwrap().layout = ListLayout::List;
                continue;
            }
            self.lists.get_mut(&list.id).unwrap().layout = decision.layout;
            if let ListLayout::Grid(_) = decision.layout {
                for (item, node) in nodes.into_iter().enumerate() {
                    let (row, column, columns) = decision.placement(item).unwrap();
                    self.slots.insert(
                        node,
                        LayoutSlot {
                            align_components: false,
                            group: list.id,
                            item,
                            row,
                            columns,
                            cards: true,
                            card_accent: facts.card_accent(list),
                            track_start: (column * (12 / columns)) as u8,
                            span: (12 / columns) as u8,
                            fixed_canvas: None,
                        },
                    );
                }
            }
            self.measured_lists.insert(list.id, decision);
        }
    }

    /// Nearby horizontal lists share the densest grid that every member has
    /// already measured as legal. Brief prose and headings may bridge the
    /// lists; richer blocks and longer passages preserve section independence.
    pub fn align_nearby_list_grids(
        &mut self,
        projection: &TextProjection,
        text_lines: &HashMap<NodeId, usize>,
    ) {
        let mut groups = Vec::<Vec<NodeId>>::new();
        let mut group = Vec::new();
        let mut gap_lines = 0_usize;
        let mut gap_is_nearby = true;

        for root in projection.roots() {
            if let BlockNode::List(list) = root {
                let horizontal = self
                    .measured_lists
                    .get(&list.id)
                    .is_some_and(|decision| matches!(decision.layout, ListLayout::Grid(_)));
                if horizontal {
                    if group.is_empty() || gap_is_nearby && gap_lines <= NEARBY_LIST_GRID_GAP_LINES
                    {
                        group.push(list.id);
                    } else {
                        if group.len() > 1 {
                            groups.push(std::mem::take(&mut group));
                        } else {
                            group.clear();
                        }
                        group.push(list.id);
                    }
                    gap_lines = 0;
                    gap_is_nearby = true;
                } else {
                    if group.len() > 1 {
                        groups.push(std::mem::take(&mut group));
                    } else {
                        group.clear();
                    }
                    gap_lines = 0;
                    gap_is_nearby = true;
                }
                continue;
            }

            if group.is_empty() {
                continue;
            }
            match root {
                BlockNode::Paragraph(_) | BlockNode::Heading(_) => {
                    let Some(lines) = text_lines.get(&root.id()).copied() else {
                        gap_is_nearby = false;
                        continue;
                    };
                    gap_lines = gap_lines.saturating_add(lines);
                    if gap_lines > NEARBY_LIST_GRID_GAP_LINES {
                        gap_is_nearby = false;
                    }
                }
                _ => gap_is_nearby = false,
            }
        }
        if group.len() > 1 {
            groups.push(group);
        }

        for group in groups {
            let editing_ordinal = self
                .editing_node
                .and_then(|node| projection.segment_for_node(node))
                .and_then(|segment| self.root_ordinal(segment.top_level_node_id));
            let group_range = group
                .first()
                .and_then(|id| self.root_ordinal(*id))
                .zip(group.last().and_then(|id| self.root_ordinal(*id)));
            if editing_ordinal.is_some_and(|editing| {
                group_range.is_some_and(|(start, end)| (start..=end).contains(&editing))
            }) {
                continue;
            }
            let Some(columns) = (2..=4).rev().find(|columns| {
                group.iter().all(|id| {
                    self.measured_lists.get(id).is_some_and(|decision| {
                        decision.candidates.iter().any(|candidate| {
                            candidate.columns == *columns && candidate.supports_shared_grid()
                        })
                    })
                })
            }) else {
                continue;
            };

            for id in group {
                let changed = self
                    .measured_lists
                    .get_mut(&id)
                    .is_some_and(|decision| decision.select_shared_grid(columns));
                if !changed {
                    continue;
                }
                let Some(BlockNode::List(list)) = projection.block(id) else {
                    continue;
                };
                let decision = self.measured_lists[&id].clone();
                let facts = ListFacts::analyze(list);
                self.lists.get_mut(&id).unwrap().layout = decision.layout;
                self.slots.retain(|_, slot| slot.group != id);
                for (item, entry) in list.items.iter().enumerate() {
                    let Some((row, column, columns)) = decision.placement(item) else {
                        continue;
                    };
                    for block in &entry.blocks {
                        self.slots.insert(
                            block.id(),
                            LayoutSlot {
                                align_components: false,
                                group: id,
                                item,
                                row,
                                columns,
                                cards: true,
                                card_accent: facts.card_accent(list),
                                track_start: (column * (12 / columns)) as u8,
                                span: (12 / columns) as u8,
                                fixed_canvas: None,
                            },
                        );
                    }
                }
            }
        }
    }

    /// Refine vertical/peer lists after their outer arrangement is known.
    /// Nested dated lists retain their enclosing source container and never
    /// acquire a horizontal strip. Existing feature grids keep their vocabulary.
    pub fn measure_label_rows(
        &mut self,
        projection: &TextProjection,
        previous: Option<&Self>,
        mut measure: impl FnMut(&ListBlock, f32, bool, bool) -> Option<Vec<(NodeId, LabelColumns)>>,
    ) {
        let mut seen = HashSet::new();
        for (segment, list_id) in projection.segments().iter().flat_map(|segment| {
            segment
                .context
                .list_ancestors
                .iter()
                .map(move |id| (segment, *id))
        }) {
            if !seen.insert(list_id) {
                continue;
            }
            let Some(BlockNode::List(list)) = projection.block(list_id) else {
                continue;
            };
            let root = segment.top_level_node_id;
            let enclosed = root != list.id;
            let dated = timeline::is_timeline(list);
            let first_node = if enclosed {
                if segment.context.bibliography.is_some() || !dated {
                    continue;
                }
                segment.node_id
            } else {
                let Some(arrangement) = self.lists.get(&list.id) else {
                    continue;
                };
                if self.editorials.contains_key(&list.id)
                    || self.resources.contains_key(&arrangement.first_node)
                    || matches!(
                        arrangement.layout,
                        ListLayout::Grid(_) | ListLayout::Steps | ListLayout::Checklist
                    )
                    || (arrangement.layout == ListLayout::Outline && !dated)
                {
                    continue;
                }
                arrangement.first_node
            };
            // Incremental measurement and edit locks belong to the enclosing
            // canonical root, not to the nested list's unindexed root ordinal.
            let old = previous.filter(|old| self.compatible_environment(old));
            let focused = self.editing_node.is_some_and(|node| {
                projection
                    .segment_for_node(node)
                    .is_some_and(|s| s.top_level_node_id == root)
            });
            if let Some(old) = old
                && (focused || (!self.measures_root(root) && self.unchanged_root(old, root)))
            {
                for item in list.items.iter() {
                    for block in &item.blocks {
                        if let Some(columns) = old.label_rows.get(&block.id()) {
                            self.label_rows.insert(block.id(), *columns);
                            if let Some(slot) = columns.slot() {
                                self.slots.insert(block.id(), slot);
                            }
                        }
                    }
                }
                continue;
            }
            if !self.measures_root(root) {
                continue;
            }
            let external_slot = self
                .slots
                .get(&first_node)
                .filter(|slot| slot.group != list.id)
                .copied();
            let metadata = self.metadata_lists.contains(&list.id);
            let width = self.slots.get(&first_node).map_or(
                if dated || metadata {
                    self.canvas
                } else {
                    self.canvas.min(self.prose_measures.reference)
                },
                |slot| slot.width(self.canvas) - 2. * slot.inset() + 8.,
            );
            if let Some(rows) = measure(list, width, !enclosed && external_slot.is_none(), metadata)
            {
                for (node, columns) in &rows {
                    if let Some(slot) = columns.slot() {
                        self.slots.insert(*node, slot);
                    }
                }
                self.label_rows.extend(rows);
            }
        }
        self.timeline_owners.clear();
        self.timeline_parents.clear();
        let mut active = HashMap::new();
        for segment in projection.segments() {
            let Some(list) = segment.context.list_ancestors.last() else {
                continue;
            };
            if self
                .label_rows
                .get(&segment.node_id)
                .is_some_and(|row| row.timeline().is_some())
            {
                if let Some(parent) = segment
                    .context
                    .list_ancestors
                    .iter()
                    .rev()
                    .skip(1)
                    .find_map(|ancestor| active.get(ancestor))
                {
                    self.timeline_parents.insert(segment.node_id, *parent);
                }
                active.insert(*list, segment.node_id);
            }
            if let Some(owner) = segment
                .context
                .list_ancestors
                .iter()
                .rev()
                .find_map(|ancestor| active.get(ancestor))
            {
                self.timeline_owners.insert(segment.node_id, *owner);
            }
        }
    }

    pub(crate) fn timeline_chain(&self, node: NodeId) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::successors(self.timeline_owners.get(&node).copied(), |event| {
            self.timeline_parents.get(event).copied()
        })
    }

    /// Retain existing boundary decisions while their source section is edited.
    /// Neither growth nor shrinkage should introduce a spacing jump mid-edit.
    pub(crate) fn retain_reference_rhythm(
        &mut self,
        projection: &TextProjection,
        previous: Option<&Self>,
        keep: bool,
        editing: Option<NodeId>,
    ) {
        let Some(previous) = previous else {
            return;
        };
        let editing = editing
            .and_then(|node| projection.segment_for_node(node))
            .and_then(|segment| self.root_ordinal(segment.top_level_node_id));
        self.reference_boundaries.retain(|next, before| {
            let affected = keep
                || editing.is_some_and(|root| {
                    [before, next].iter().any(|id| {
                        self.groups
                            .sections
                            .get(id)
                            .is_some_and(|section| section.roots.contains(&root))
                    })
                });
            !affected || previous.reference_boundaries.get(next) == Some(before)
        });
        for (&next, &before) in &previous.reference_boundaries {
            let Some((a, b)) = self
                .groups
                .sections
                .get(&before)
                .zip(self.groups.sections.get(&next))
            else {
                continue;
            };
            if a.roots.end == b.roots.start
                && a.parent == b.parent
                && a.level == b.level
                && (keep
                    || editing
                        .is_some_and(|root| a.roots.contains(&root) || b.roots.contains(&root)))
            {
                self.reference_boundaries.insert(next, before);
            }
        }
    }

    /// Pairwise whitespace policy in unscaled logical pixels. Content inset
    /// (quote rails, code headers, table cells) is accounted for separately.
    pub fn gap_between(&self, previous: &BlockNode, next: &BlockNode) -> f32 {
        use crate::theme::DocumentStyle;
        // Sibling value objects are modules of one overview, not new prose sections.
        // Keep the modular gutter when they stack; real chapter boundaries
        // and different hierarchy levels still use the heading rhythm below.
        if let Some(before) = self
            .editorials
            .get(&previous.id())
            .filter(|m| m.metric_value.is_some() || m.color_value.is_some())
            && let Some(after) = self.editorials.get(&next.id()).filter(|m| {
                (m.metric_value.is_some() || m.color_value.is_some()) && m.owner == next.id()
            })
            && before.owner != after.owner
            && before.kind == after.kind
            && self
                .groups
                .sections
                .get(&before.owner)
                .zip(self.groups.sections.get(&after.owner))
                .is_some_and(|(a, b)| a.parent == b.parent && a.level == b.level)
        {
            return LAYOUT_GAP;
        }
        if let Some(owner) = self.bibliography.get(&previous.id())
            && self.bibliography.get(&next.id()) == Some(owner)
        {
            return crate::theme::DocumentStyle::BIBLIOGRAPHY_GAP;
        }
        if matches!(previous, BlockNode::FootnoteDefinition { .. })
            && matches!(next, BlockNode::FootnoteDefinition { .. })
        {
            return 16.;
        }
        // Tables are dense, ruled components. Give their top edge a clear
        // boundary from any preceding content; document-start tables never
        // call this pairwise spacing policy and retain the page inset alone.
        if matches!(next, BlockNode::Table(_)) {
            return DocumentStyle::TABLE_TOP_GAP;
        }
        if let BlockNode::Heading(previous) = previous {
            return if matches!(next, BlockNode::Heading(next) if previous.level == 1 && next.level == 2)
            {
                DocumentStyle::TITLE_SECTION_HEADING_GAP
            } else if previous.level <= 2 {
                24.
            } else {
                16.
            };
        }
        if let BlockNode::Heading(heading) = next {
            return if self.reference_boundaries.contains_key(&heading.id) {
                DocumentStyle::REFERENCE_SECTION_GAP
            } else if self.opening_section == Some(heading.id) {
                DocumentStyle::OPENING_SECTION_GAP
            } else if heading.level <= 2 {
                DocumentStyle::MAJOR_SECTION_GAP
            } else {
                DocumentStyle::SUBSECTION_GAP
            };
        }
        if crate::math::is_math(previous) || crate::math::is_math(next) {
            return crate::theme::DocumentStyle::EQUATION_GAP;
        }
        let before = self.groups.group_for_root(previous.id());
        let after = self.groups.group_for_root(next.id());
        if let (Some(before), Some(after)) = (before, after) {
            for relation in &after.relationships {
                if relation.basis == [previous.id(), next.id()]
                    && relation.kind == groups::RelationshipKind::CriticalInstruction
                {
                    return DocumentStyle::INSTRUCTION_GAP;
                }
                if relation.basis == [previous.id(), next.id()]
                    && let groups::RelationshipKind::FigureText(role) = relation.kind
                    && role.gallery_start().is_some()
                {
                    return role.gap();
                }
            }
            if before.id == after.id {
                for relation in &before.relationships {
                    if relation.basis == [previous.id(), next.id()]
                        && let groups::RelationshipKind::FigureText(role) = relation.kind
                    {
                        return role.gap();
                    }
                }
            }
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

const TECHNICAL_SECTION_SHAPE: u8 = 4;

fn compact_section(roots: &[&BlockNode], start: usize) -> Option<(u8, usize, u8)> {
    let BlockNode::Heading(heading) = roots.get(start)? else {
        return None;
    };
    if heading.level < 2 || heading.content.len() > 64 {
        return None;
    }
    let mut end = start + 1;
    let mut size = 0;
    let mut shape = 0;
    let mut technical = false;
    while let Some(block) = roots.get(end) {
        match block {
            // A section with subsections is a chapter, not a compact peer.
            BlockNode::Heading(next) if next.level > heading.level => return None,
            BlockNode::Heading(_) => break,
            BlockNode::Paragraph(paragraph) => {
                if resource::classify(paragraph).is_some() {
                    return None;
                }
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
                if resource::is_resource_list(list) {
                    return None;
                }
                size += list
                    .items
                    .iter()
                    .flat_map(|item| item.blocks.iter())
                    .filter_map(|block| block.text())
                    .map(|text| text.len())
                    .sum::<usize>();
                shape |= 2;
            }
            BlockNode::CodeBlock(code) if code.content.len() <= 1600 => {
                size += code.content.len();
                technical = true;
            }
            BlockNode::Table(table) if table.rows.len() <= 12 && table.columns.len() <= 12 => {
                for cell in table.rows.iter().flat_map(|row| row.cells.iter()) {
                    let Some(BlockNode::Paragraph(paragraph)) =
                        cell.blocks.get(0).map(AsRef::as_ref)
                    else {
                        return None;
                    };
                    if cell.blocks.len() != 1 || paragraph.content.len() > 1600 {
                        return None;
                    }
                    size += paragraph.content.len();
                }
                technical = true;
            }
            _ => return None,
        }
        end += 1;
        if end - start > 4 || size > 1600 {
            return None;
        }
    }
    // Major sections with an explicit, simple checklist may accompany compact
    // reference entries. Ordinary bullets and prose still define chapters.
    // H3+ checklists retain the existing mixed list/prose peer vocabulary.
    let reference_checklist = heading.level == 2
        && end > start + 1
        && roots[start + 1..end.saturating_sub(1)]
            .iter()
            .all(|root| matches!(root, BlockNode::Paragraph(_)))
        && matches!(roots.get(end.saturating_sub(1)), Some(BlockNode::List(list))
            if !list.items.is_empty() && list.items.iter().all(|item| item.checked.is_some()));
    let technical = technical || reference_checklist;
    // Code, tables and explicit checklists negotiate from actual native size,
    // not identical Markdown block shapes. A trailing paragraph belongs to
    // the same complete entry, not a detached explanation after the peer row.
    // Existing whole-section work bounds and native measurements determine
    // whether the additional content still permits a readable compact pair.
    let ends_with_content = roots.get(end.saturating_sub(1)).is_some_and(|root| {
        matches!(
            root,
            BlockNode::Table(_) | BlockNode::CodeBlock(_) | BlockNode::Paragraph(_)
        )
    }) || reference_checklist;
    (end > start + 1 && (heading.level >= 3 || technical) && (!technical || ends_with_content))
        .then_some((
            heading.level,
            end,
            if technical {
                TECHNICAL_SECTION_SHAPE
            } else {
                shape
            },
        ))
}

struct ListFacts {
    simple: bool,
    nested: bool,
    tasks: bool,
    sequence: bool,
    labeled: bool,
}

impl ListFacts {
    fn analyze(list: &ListBlock) -> Self {
        let mut actions = 0;
        let mut references = false;
        let mut simple = SHORT_LIST_ITEMS.contains(&list.items.len());
        let mut nested = false;
        let mut labels = 0;
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
                labels += usize::from(
                    has_authored_label(paragraph) || resource::classify(paragraph).is_some(),
                );
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
            // One unlabelled summary item should not discard a clear repeated
            // label/description vocabulary in the rest of the group.
            labeled: labels >= 2 && labels * 4 >= list.items.len() * 3,
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

    fn card_accent(&self, list: &ListBlock) -> CardAccent {
        if matches!(list.kind, ListKind::Ordered { .. }) {
            CardAccent::Numbered
        } else if self.labeled {
            CardAccent::OpenLabeled
        } else {
            CardAccent::Open
        }
    }
}

pub(crate) fn has_authored_label(paragraph: &document_core::Paragraph) -> bool {
    authored_label_end(paragraph).is_some()
}

/// Exact source boundary, shared by shaping and painting. A label is authored
/// structure, not a generated summary. URLs, giant prefixes and bare titles do
/// not qualify. Both `**Label:** body` and plain `Label: body` are supported.
pub(crate) fn authored_label_end(paragraph: &document_core::Paragraph) -> Option<usize> {
    if paragraph.content.len() > 4096 {
        return None;
    }
    let text = paragraph.content.as_string();
    let styled = paragraph
        .content
        .runs()
        .first()
        .filter(|prefix| {
            prefix.range.start == 0
                && prefix.styles.iter().any(|style| {
                    matches!(
                        style,
                        document_core::InlineStyle::Bold | document_core::InlineStyle::Code
                    )
                })
        })
        .map(|prefix| prefix.range.end);
    let end = styled.or_else(|| {
        let (label, body) = text.split_once(':')?;
        (!label.contains(['\n', '/', ':']) && body.starts_with(char::is_whitespace))
            .then_some(label.len() + 1)
    })?;
    let label = text.get(..end)?;
    if !(1..=48).contains(&label.chars().count()) {
        return None;
    }
    let suffix = text.get(end..)?;
    let after_colon = suffix.strip_prefix(':').unwrap_or(suffix);
    let body = after_colon.trim_start();
    if body.is_empty() || body.len() == suffix.len() {
        return None;
    }
    Some(text.len() - body.len())
}

/// The summary belongs to the complete rendered subtree, not just its first
/// nesting level. Only explicit task states count; ordinary bullets are not
/// invented prerequisites. Called during plan preparation, never painting.
fn checklist_progress(block: &BlockNode) -> (usize, usize) {
    fn sequence(blocks: &document_core::BlockSequence) -> (usize, usize) {
        blocks
            .iter()
            .map(|block| checklist_progress(block))
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1))
    }
    match block {
        BlockNode::List(list) => list.items.iter().fold((0, 0), |(done, count), item| {
            let children = sequence(&item.blocks);
            (
                done + children.0 + usize::from(item.checked == Some(true)),
                count + children.1 + usize::from(item.checked.is_some()),
            )
        }),
        BlockNode::BlockQuote { blocks, .. }
        | BlockNode::Alert { blocks, .. }
        | BlockNode::Definition { blocks, .. }
        | BlockNode::FootnoteDefinition { blocks, .. } => sequence(blocks),
        BlockNode::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| row.cells.iter())
            .map(|cell| sequence(&cell.blocks))
            .fold((0, 0), |a, b| (a.0 + b.0, a.1 + b.1)),
        _ => (0, 0),
    }
}

pub(crate) fn list_has_authored_labels(list: &ListBlock) -> bool {
    ListFacts::analyze(list).labeled
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
            | "inspect"
            | "measure"
            | "reject"
            | "publish"
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

    #[test]
    fn planning_batches_cover_requests_and_bound_extra_work() {
        let document = document_core::Document::from_markdown("# Scope\n").unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let mut plan = AdaptivePlan::build(&projection, 800., None, false);
        plan.windows = (0..101).map(|n| n * 3..n * 3 + 3).collect();
        assert_eq!(plan.planning_batch(4..5), 0..48);
        assert_eq!(plan.planning_batch(46..50), 0..96);
        assert_eq!(plan.planning_batch(299..303), 288..303);
        for start in 0..303 {
            for end in start + 1..=303 {
                let batch = plan.planning_batch(start..end);
                assert!(batch.start <= start && batch.end >= end);
                assert!(start - batch.start < 48 && batch.end - end < 48);
                assert_eq!(plan.planning_batch(batch.clone()), batch);
            }
        }
        let mut publications = 0;
        for root in (0..303).chain((0..303).rev()) {
            let batch = plan.planning_batch(root..root + 1);
            if !plan.covers(&batch) {
                publications += 1;
                plan.measured_rows.covered.extend(plan.windows_for(&batch));
                plan.measured_rows.covered.sort_by_key(|range| range.start);
                plan.measured_rows.covered.dedup();
            }
            assert!(plan.covers(&(root..root + 1)));
        }
        assert_eq!(publications, 7, "one publication per batch, none on return");
        assert_eq!(plan.planning_batch(400..401), 400..401);
        plan.windows.clear();
        assert_eq!(plan.planning_batch(4..5), 4..5);
    }

    #[test]
    fn reference_rhythm_is_structural_source_bound_and_edit_stable() {
        let source = "## First\n\n```rust\nlet value = 42;\n```\n\n## Second\n\n```json\n{}\n```\n";
        let mut document = document_core::Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let previous = AdaptivePlan::build(&projection, 480., None, false);
        assert_eq!(previous.reference_boundaries.len(), 1);
        let key = previous.geometry_key();
        let mut changed = previous.clone();
        changed.reference_boundaries.clear();
        assert!(
            !key.matches(&changed),
            "spacing is part of the geometry cache key"
        );
        for source in [
            "# First\n\n```rust\nrun();\n```\n\n# Second\n\n```json\n{}\n```\n",
            "## First\n\n```rust\nrun();\n```\n\n## Second\n\nOrdinary prose.\n",
            "## First\n\n```rust\nrun();\n```\n\n### Nested\n\n```json\n{}\n```\n\n## Second\n\n```rust\nrun();\n```\n",
            "## First\n\n```rust\nrun();\n```\n\n---\n\n## Second\n\n```json\n{}\n```\n",
        ] {
            let document = document_core::Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            assert!(
                AdaptivePlan::build(&projection, 480., None, false)
                    .reference_boundaries
                    .is_empty(),
                "chapter boundary: {source}"
            );
        }
        let code = projection
            .segments()
            .iter()
            .find(|segment| {
                matches!(
                    projection.block(segment.node_id),
                    Some(BlockNode::CodeBlock(_))
                )
            })
            .unwrap()
            .node_id;
        document
            .apply(document_core::EditCommand::ReplaceText {
                node_id: code,
                range: 0..0,
                text: "long ".repeat(400),
                typing: false,
                selection_after: None,
            })
            .unwrap();
        let grown = TextProjection::from_snapshot(&document.snapshot());
        let mut reading = AdaptivePlan::build(&grown, 480., Some(&previous), false);
        assert!(
            reading.reference_boundaries.is_empty(),
            "oversized examples leave compact rhythm after blur"
        );
        reading.retain_reference_rhythm(&grown, Some(&previous), false, Some(code));
        assert_eq!(
            reading.reference_boundaries, previous.reference_boundaries,
            "typing preserves existing spacing"
        );
        let retained = AdaptivePlan::build(&grown, 480., Some(&previous), true);
        assert_eq!(retained.reference_boundaries, previous.reference_boundaries);
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), source);
        let restored = TextProjection::from_snapshot(&document.snapshot());
        let grown_reading = AdaptivePlan::build(&grown, 480., Some(&previous), false);
        let mut shrinking = AdaptivePlan::build(&restored, 480., Some(&grown_reading), false);
        assert_eq!(shrinking.reference_boundaries.len(), 1);
        shrinking.retain_reference_rhythm(&restored, Some(&grown_reading), false, Some(code));
        assert!(
            shrinking.reference_boundaries.is_empty(),
            "shrinking during an edit must not introduce a new spacing boundary"
        );
    }
    use document_core::Document;

    #[test]
    fn compact_major_checklists_require_explicit_tasks_and_complete_sections() {
        let qualifies = |source: &str| {
            let document = Document::from_markdown(source).unwrap();
            let snapshot = document.snapshot();
            let roots = snapshot
                .blocks()
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>();
            compact_section(&roots, 0)
        };
        for source in [
            "## Review\n\n- [x] First\n- [ ] Second\n",
            "## Review\n\nA short explanation.\n\n- [ ] First\n- [ ] Second\n",
        ] {
            assert!(
                qualifies(source).is_some_and(|(_, _, shape)| shape == TECHNICAL_SECTION_SHAPE)
            );
        }
        for source in [
            "## Empty\n\n## Next\n",
            "## Ordinary\n\n- First\n- Second\n",
            "## Mixed\n\n- [x] First\n- Second\n",
            "## Review\n\n- [x] First\n- [ ] Second\n\nA continuation stays attached.\n",
            "## Parent\n\n- [x] First\n\n### Child\n\nDetails.\n",
            "## Rich\n\n- [x] First\n\n  A second paragraph.\n",
            "## Nested\n\n- [x] First\n  - [ ] Child\n",
        ] {
            assert!(
                qualifies(source).is_none(),
                "not a compact major checklist: {source}"
            );
        }
        assert!(
            qualifies("### Local peer\n\nA label.\n\n- [x] First\n- [ ] Second\n")
                .is_some_and(|(_, _, shape)| shape == 3)
        );
    }

    #[test]
    fn compact_technical_sections_keep_chapters_and_continuations_intact() {
        let qualifies = |source: &str| {
            let document = Document::from_markdown(source).unwrap();
            let snapshot = document.snapshot();
            let roots = snapshot
                .blocks()
                .iter()
                .map(AsRef::as_ref)
                .collect::<Vec<_>>();
            compact_section(&roots, 0)
        };
        assert_eq!(
            qualifies("## Values\n\n```toml\na = 1\n```\n"),
            Some((2, 2, TECHNICAL_SECTION_SHAPE))
        );
        assert_eq!(
            qualifies(
                "## Explanation\n\nBefore.\n\n```toml\na = 1\n```\n\nA continuation after the example.\n"
            ),
            Some((2, 4, TECHNICAL_SECTION_SHAPE))
        );
        for source in [
            "# Whole document\n\n```toml\na = 1\n```\n",
            "## Ordinary chapter\n\nThis remains prose.\n",
            "## Parent chapter\n\n```toml\na = 1\n```\n\n### Child section\n\nDetails.\n",
            "## Long section\n\nBefore.\n\n```toml\na = 1\n```\n\nFirst continuation.\n\nSecond continuation.\n",
            "## Warning\n\n> [!CAUTION]\n> Do not detach this warning.\n\n```toml\na = 1\n```\n",
        ] {
            assert!(
                qualifies(source).is_none(),
                "not a compact technical sibling: {source}"
            );
        }
        assert!(
            qualifies(&format!(
                "## Long example\n\n```text\n{}\n```\n",
                "x".repeat(1601)
            ))
            .is_none()
        );
        let mut table = String::from("## Large table\n\n| Key | Value |\n| --- | --- |\n");
        for _ in 0..13 {
            table.push_str("| a | b |\n");
        }
        assert!(qualifies(&table).is_none());
    }

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
    fn checklist_progress_includes_unfinished_descendant_tasks() {
        let p = plan("- [x] Parent\n  - [ ] Child\n", 360.);
        let list = p.lists.values().next().unwrap();
        assert_eq!((list.completed, list.count), (1, 2));
        for (source, expected) in [
            (
                "- [x] Parent\n  - Ordinary context\n    - [ ] Child\n",
                (1, 2),
            ),
            (
                "- [x] Parent\n  - [x] Child\n    - [ ] Grandchild\n- [ ] Sibling\n",
                (2, 4),
            ),
            ("- [x] Parent\n\n  > - [ ] Quoted task\n", (1, 2)),
            (
                "- [x] Parent\n\n  ```text\n  - [ ] Literal, not a task\n  ```\n",
                (1, 1),
            ),
        ] {
            let p = plan(source, 360.);
            let list = p.lists.values().next().unwrap();
            assert_eq!((list.completed, list.count), expected, "{source}");
        }
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

    #[test]
    fn opening_rhythm_is_part_of_the_published_geometry_key() {
        let mut plan = plan(
            "# Title\n\nAn introduction.\n\n## Section\n\nBody.\n",
            1000.,
        );
        assert!(plan.opening_section.is_some());
        let key = plan.geometry_key();
        assert!(key.matches(&plan));
        plan.opening_section = None;
        assert!(
            !key.matches(&plan),
            "different spacing cannot reuse old geometry"
        );
    }

    #[test]
    fn repeated_bold_and_code_prefixes_nominate_labelled_groups() {
        for source in [
            "- **Core:** Stable nodes.\n- **View:** Measured layout.\n- **App:** Native shell.\n",
            "- `Snapshot`: Immutable content.\n- `Plan`: Measured geometry.\n- `Viewport`: Window state.\n",
        ] {
            let document = Document::from_markdown(source).unwrap();
            let snapshot = document.snapshot();
            let projection = TextProjection::from_snapshot(&snapshot);
            let list = projection
                .roots()
                .find_map(|root| match root {
                    BlockNode::List(list) => Some(list),
                    _ => None,
                })
                .unwrap();
            assert!(ListFacts::analyze(list).labeled, "source={source}");
        }
        let document = Document::from_markdown(
            "- **Core:** Stable nodes.\n- An ordinary point.\n- Another ordinary point.\n",
        )
        .unwrap();
        let snapshot = document.snapshot();
        let projection = TextProjection::from_snapshot(&snapshot);
        let Some(BlockNode::List(list)) = projection.roots().next() else {
            panic!("fixture must parse as a list");
        };
        assert!(!ListFacts::analyze(list).labeled);
    }
}
