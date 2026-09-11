//! Semantic term rails. The existing row placer owns geometry and hit testing;
//! definition lists contribute source-order, unboxed slots, never fake tables.

use super::*;
use document_core::DefinitionKind;

impl AdaptivePlan {
    pub fn place_definitions(
        &mut self,
        projection: &TextProjection,
        previous: Option<&Self>,
        mut term_width: impl FnMut(&crate::ProjectionSegment) -> Option<f32>,
    ) {
        // Index once: many small glossaries must not rescan the whole document.
        let mut segments = HashMap::<NodeId, Vec<&crate::ProjectionSegment>>::new();
        for segment in projection.segments() {
            if segment.context.definition.is_some() {
                segments
                    .entry(segment.top_level_node_id)
                    .or_default()
                    .push(segment);
            }
        }
        for root in projection.roots() {
            let BlockNode::Definition {
                id,
                kind: DefinitionKind::List,
                blocks,
            } = root
            else {
                continue;
            };
            let Some(leaves) = segments.get(id) else {
                continue;
            };
            let old = previous.filter(|old| self.compatible_environment(old));
            let focused = self
                .editing_node
                .is_some_and(|node| leaves.iter().any(|s| s.node_id == node));
            if let Some(old) = old
                && (focused || (!self.measures_root(*id) && self.unchanged_root(old, *id)))
            {
                for segment in leaves {
                    let retained = old
                        .slots
                        .get(&segment.node_id)
                        .filter(|s| s.group == *id)
                        .or_else(|| {
                            // Enter can create a new leaf inside the focused
                            // term/description. It inherits that group's rail,
                            // not an unrelated full-width provisional stack.
                            let group =
                                blocks.iter().find(|b| contains_leaf(b, segment.node_id))?;
                            let old_root = old
                                .root_ordinals
                                .get(id)
                                .and_then(|i| old.root_content.get(*i))?;
                            let BlockNode::Definition {
                                blocks: old_groups, ..
                            } = old_root.as_ref()
                            else {
                                return None;
                            };
                            let old_group = old_groups.iter().find(|b| b.id() == group.id())?;
                            old.slots.iter().find_map(|(node, slot)| {
                                (slot.group == *id && contains_leaf(old_group, *node))
                                    .then_some(slot)
                            })
                        });
                    if let Some(slot) = retained {
                        self.slots.insert(segment.node_id, *slot);
                    }
                }
                continue;
            }
            if !self.measures_root(*id) {
                continue;
            }
            let mut groups = HashMap::new();
            let mut row = 0usize;
            let mut has_description = false;
            let mut valid = true;
            for block in blocks {
                match block.as_ref() {
                    BlockNode::Definition {
                        id,
                        kind: DefinitionKind::Term,
                        ..
                    } => {
                        if !groups.is_empty() {
                            if !has_description {
                                valid = false;
                                break;
                            }
                            row += 1;
                        }
                        has_description = false;
                        groups.insert(*id, (row, false));
                    }
                    BlockNode::Definition {
                        id,
                        kind: DefinitionKind::Description,
                        ..
                    } if !groups.is_empty() => {
                        has_description = true;
                        groups.insert(*id, (row, true));
                    }
                    _ => {
                        valid = false;
                        break;
                    }
                }
            }
            if !valid || !has_description {
                continue;
            }
            let mut preferred = 0_f32;
            for segment in leaves {
                if segment
                    .context
                    .definition
                    .is_some_and(|(group, _)| groups.get(&group).is_some_and(|(_, body)| !body))
                {
                    let Some(width) = term_width(segment) else {
                        valid = false;
                        break;
                    };
                    preferred = preferred.max(width + 8.);
                }
            }
            // The prose measure applies to the explanation, not to the
            // combined glossary. Reserve the term rail in addition to it.
            let canvas = self
                .canvas
                .min(self.prose_measures.reference + preferred + LAYOUT_GAP);
            let span = valid.then(|| term_span(canvas, preferred)).flatten();
            let Some(span) = span else { continue };
            // A descendant can be a table, list or nested definition. The
            // canonical container index keeps it in the same description.
            let mut group_index = 0;
            for segment in leaves {
                while group_index < blocks.len()
                    && !contains_leaf(blocks.get(group_index).unwrap(), segment.node_id)
                {
                    group_index += 1;
                }
                let Some(group) = blocks.get(group_index) else {
                    break;
                };
                let (row, body) = groups[&group.id()];
                self.slots.insert(
                    segment.node_id,
                    LayoutSlot {
                        align_components: false,
                        group: *id,
                        item: row * 2 + usize::from(body),
                        row,
                        columns: 2,
                        cards: false,
                        card_accent: CardAccent::Open,
                        track_start: if body { span } else { 0 },
                        span: if body { 12 - span } else { span },
                        fixed_canvas: Some(canvas),
                    },
                );
            }
        }
    }
}

fn contains_leaf(block: &BlockNode, id: NodeId) -> bool {
    match block {
        BlockNode::Definition { blocks, .. } => blocks.contains_node(id),
        _ => false,
    }
}

/// Choose the smallest shared rail that fits the loaded-font term widths.
/// Long labels or cramped descriptions stack at full reading measure.
fn term_span(canvas: f32, preferred: f32) -> Option<u8> {
    if canvas < crate::theme::DocumentStyle::SINGLE_COLUMN_WIDTH {
        return None;
    }
    (2..=5).find(|span| {
        candidates::span_width(canvas, *span).is_some_and(|w| w >= preferred)
            && candidates::span_width(canvas, 12 - *span).is_some_and(|w| w >= 320.)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn definition_rails_fit_real_labels_without_cramping_descriptions() {
        assert_eq!(term_span(480., 80.), None);
        assert_eq!(term_span(700., 100.), Some(3));
        assert_eq!(term_span(700., 500.), None);
        for width in [560., 640., 720., 900.] {
            if let Some(span) = term_span(width, 120.) {
                assert!(candidates::span_width(width, span).unwrap() >= 120.);
                assert!(candidates::span_width(width, 12 - span).unwrap() >= 320.);
            }
        }
    }

    #[test]
    fn definition_terms_and_multiple_descriptions_receive_open_source_order_slots() {
        let document = document_core::Document::from_markdown("Cache\n: Reusable geometry.\n: A second description.\n\nViewport\n: Visible document region.").unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let mut plan = AdaptivePlan::build(&projection, 1200., None, false);
        plan.prose_measures.reference = 512.;
        plan.place_definitions(&projection, None, |_| Some(120.));
        assert_eq!(
            plan.slots.len(),
            5,
            "roots={:?}; segments={:?}",
            projection.roots().collect::<Vec<_>>(),
            projection.segments()
        );
        let slots = projection
            .segments()
            .iter()
            .map(|s| plan.slots[&s.node_id])
            .collect::<Vec<_>>();
        assert_eq!(
            slots.iter().map(|s| s.item).collect::<Vec<_>>(),
            [0, 1, 1, 2, 3]
        );
        assert!(slots.iter().all(|s| !s.cards));
        assert_eq!(slots[0].left(1200.), 0.);
        assert_eq!(slots[1].left(1200.) - slots[0].width(1200.), LAYOUT_GAP);
    }

    #[test]
    fn definition_split_inherits_focused_group_rail() {
        use document_core::{
            Affinity, Document, DocumentPosition, EditCommand, Selection, TextSelection,
        };
        let mut document = Document::from_markdown("Term\n: Description.").unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let mut old = AdaptivePlan::build(&projection, 1000., None, false);
        old.measurement_identity = Some(Arc::new(()));
        old.prose_measures.reference = 512.;
        old.place_definitions(&projection, None, |_| Some(64.));
        let term = projection.segments()[0].node_id;
        document
            .apply(EditCommand::SetSelection(Selection::Text(
                TextSelection::caret(DocumentPosition::new(term, 2, Affinity::Downstream)),
            )))
            .unwrap();
        document.apply(EditCommand::SplitSelection).unwrap();
        let snapshot = document.snapshot();
        let Selection::Text(selection) = snapshot.selection() else {
            panic!("caret")
        };
        let projection = TextProjection::from_snapshot(&snapshot);
        let mut current = AdaptivePlan::build(&projection, 1000., Some(&old), true);
        current.measurement_identity = old.measurement_identity.clone();
        current.editing_node = Some(selection.head.node_id);
        current.place_definitions(&projection, Some(&old), |_| {
            panic!("do not remeasure a focused row")
        });
        assert_eq!(current.slots[&term], old.slots[&term]);
        assert_eq!(current.slots[&selection.head.node_id], old.slots[&term]);
    }
}
