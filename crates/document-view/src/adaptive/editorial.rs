//! Authored, named editorial objects. No labels, outcomes or completion states
//! are inferred from ordinary prose, and these descriptors never own content.
use super::{AdaptivePlan, CardAccent, LayoutSlot};
use crate::TextProjection;
use document_core::{BlockNode, ListKind, NodeId};
use std::{collections::HashMap, ops::Range};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Kind {
    Decision,
    Selected,
    Example,
    Pros,
    Cons,
    Valid,
    Invalid,
    Request,
    Response,
    Metric,
    Color,
}

impl Kind {
    pub fn accepts_counterpart(self, next: Self) -> bool {
        // Exchanges have direction. A response must not be visually attached
        // to the next request just because that rectangle scores better.
        self != Self::Response && self.counterpart() == Some(next)
    }

    pub fn counterpart(self) -> Option<Self> {
        match self {
            Self::Pros => Some(Self::Cons),
            Self::Cons => Some(Self::Pros),
            Self::Valid => Some(Self::Invalid),
            Self::Invalid => Some(Self::Valid),
            Self::Request => Some(Self::Response),
            Self::Response => Some(Self::Request),
            _ => None,
        }
    }
    pub fn from_title(title: &str) -> Option<Self> {
        let label = title.split_once(':').map_or(title, |(label, _)| label);
        match label.trim().to_ascii_lowercase().as_str() {
            "decision" | "decision record" => Some(Self::Decision),
            "selected option" => Some(Self::Selected),
            "example" | "worked example" => Some(Self::Example),
            "pros" => Some(Self::Pros),
            "cons" => Some(Self::Cons),
            "valid" => Some(Self::Valid),
            "invalid" => Some(Self::Invalid),
            "request" | "http request" => Some(Self::Request),
            "response" | "http response" => Some(Self::Response),
            "metric" => Some(Self::Metric),
            "color" | "colour" | "color token" | "colour token" => Some(Self::Color),
            _ => None,
        }
    }

    pub fn peer_shape(self) -> u8 {
        match self {
            Self::Pros | Self::Cons => 16,
            Self::Valid | Self::Invalid => 17,
            Self::Decision | Self::Selected => 18,
            Self::Example => 19,
            Self::Request | Self::Response => 20,
            Self::Metric => 21,
            Self::Color => 22,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Member {
    pub owner: NodeId,
    pub kind: Kind,
    pub wide: bool,
    pub metric_value: Option<NodeId>,
    pub color_value: Option<NodeId>,
}

impl Member {
    pub fn color_role(self, node: NodeId) -> Option<crate::signals::ColorRole> {
        self.color_value.map(|value| {
            if node == self.owner {
                crate::signals::ColorRole::Label
            } else if node == value {
                crate::signals::ColorRole::Literal
            } else {
                crate::signals::ColorRole::Context
            }
        })
    }

    pub fn metric_role(self, node: NodeId) -> Option<crate::metrics::TextRole> {
        self.metric_value.map(|value| {
            if node == self.owner {
                crate::metrics::TextRole::Label
            } else if node == value {
                crate::metrics::TextRole::Value
            } else {
                crate::metrics::TextRole::Context
            }
        })
    }
}

pub(crate) fn section(roots: &[&BlockNode], start: usize) -> Option<(Kind, Range<usize>)> {
    let BlockNode::Heading(heading) = roots.get(start)? else {
        return None;
    };
    if heading.level < 2 || heading.content.len() > 160 {
        return None;
    }
    let kind = Kind::from_title(&heading.content.as_cow())?;
    let mut end = start + 1;
    let mut bytes = 0;
    let mut has_code = false;
    while let Some(block) = roots.get(end) {
        match block {
            BlockNode::Heading(next) => {
                // A whole chapter with subsections is not an isolated object.
                if next.level > heading.level {
                    return None;
                }
                break;
            }
            BlockNode::Paragraph(p) if super::resource::classify(p).is_none() => {
                bytes += p.content.len();
            }
            BlockNode::List(list)
                if list.kind == ListKind::Unordered
                    && list.items.len() <= 6
                    && !super::resource::is_resource_list(list) =>
            {
                for item in list.items.iter() {
                    if item.checked.is_some() || item.blocks.len() != 1 {
                        return None;
                    }
                    let Some(BlockNode::Paragraph(p)) = item.blocks.get(0).map(AsRef::as_ref)
                    else {
                        return None;
                    };
                    bytes += p.content.len();
                }
            }
            BlockNode::CodeBlock(code)
                if matches!(
                    kind,
                    Kind::Example | Kind::Valid | Kind::Invalid | Kind::Request | Kind::Response
                ) && !crate::math::is_math(block) =>
            {
                bytes += code.content.len();
                has_code = true;
            }
            _ => return None,
        }
        end += 1;
        // Do not turn a long argument into a page-sized callout. Actual row fit
        // is separately decided using native wraps and the viewport height.
        if end - start > 4 || bytes > 1600 {
            return None;
        }
    }
    // A prose-only "Request" can be an ordinary request to the reader, not a
    // technical exchange. Require an actual authored payload/example, without
    // guessing a protocol, status or relationship from the literal code.
    let exchange = matches!(kind, Kind::Request | Kind::Response);
    if kind == Kind::Metric && crate::metrics::value_node(&roots[start..end]).is_none() {
        return None;
    }
    if kind == Kind::Color && crate::signals::color_value(&roots[start..end]).is_none() {
        return None;
    }
    (end > start + 1 && bytes > 0 && (!exchange || has_code)).then_some((kind, start..end))
}

pub(crate) fn analyze(roots: &[&BlockNode]) -> HashMap<NodeId, Member> {
    let mut members = HashMap::new();
    for start in 0..roots.len() {
        if let Some((kind, range)) = section(roots, start) {
            let member = Member {
                owner: roots[start].id(),
                kind,
                wide: roots[range.clone()]
                    .iter()
                    .any(|root| matches!(root, BlockNode::CodeBlock(_))),
                metric_value: (kind == Kind::Metric)
                    .then(|| crate::metrics::value_node(&roots[range.clone()]))
                    .flatten(),
                color_value: (kind == Kind::Color)
                    .then(|| crate::signals::color_value(&roots[range.clone()]))
                    .flatten(),
            };
            members.extend(range.map(|i| (roots[i].id(), member)));
        }
    }
    members
}

impl AdaptivePlan {
    /// Text growth or a temporarily edited label must not dissolve its row
    /// under the caret. Structural boundary changes still release the grouping.
    pub fn retain_editorials(
        &mut self,
        projection: &TextProjection,
        old: Option<&Self>,
        keep: bool,
    ) {
        let Some(old) = old else { return };
        let editing_owner = self
            .editing_node
            .and_then(|id| projection.segment_for_node(id))
            .and_then(|s| old.editorials.get(&s.top_level_node_id))
            .map(|m| m.owner);
        let mut old_members = HashMap::<NodeId, Vec<NodeId>>::new();
        for &id in &old.root_ids {
            if let Some(member) = old.editorials.get(&id) {
                old_members.entry(member.owner).or_default().push(id);
            }
        }
        for (&owner, member) in &old.editorials {
            if owner != member.owner || (!keep && editing_owner != Some(owner)) {
                continue;
            }
            let old_roots = &old_members[&owner];
            let Some(start) = self.root_ordinal(owner) else {
                continue;
            };
            if self.root_ids.get(start..start + old_roots.len()) != Some(old_roots.as_slice()) {
                continue;
            }
            let compatible = old_roots.iter().all(|id| {
                matches!(
                    (
                        old.root_ordinal(*id).map(|i| old.root_content[i].as_ref()),
                        projection.block(*id)
                    ),
                    (Some(BlockNode::Heading(_)), Some(BlockNode::Heading(_)))
                        | (Some(BlockNode::Paragraph(_)), Some(BlockNode::Paragraph(_)))
                        | (Some(BlockNode::CodeBlock(_)), Some(BlockNode::CodeBlock(_)))
                        | (Some(BlockNode::List(_)), Some(BlockNode::List(_)))
                )
            });
            if compatible {
                for &root in old_roots {
                    self.editorials.insert(root, *member);
                }
            }
        }
    }

    pub fn place_editorials(&mut self, projection: &TextProjection) {
        if self.editorials.is_empty() {
            return;
        }
        // One slot belongs to the whole object, including its list or code.
        // The row search may nominate a peer slot; otherwise use a readable
        // single card, with the same outer/inner dimensions as measurement.
        let owners = self
            .editorials
            .iter()
            .filter(|(id, m)| **id == m.owner)
            .map(|(&id, &m)| (id, m))
            .collect::<Vec<_>>();
        let mut placements = HashMap::new();
        for (owner, member) in owners {
            let mut slot = self.slots.get(&owner).copied().unwrap_or(LayoutSlot {
                align_components: false,
                group: owner,
                item: 0,
                row: 0,
                columns: 1,
                cards: true,
                card_accent: CardAccent::Editorial(member.kind),
                track_start: 0,
                span: 12,
                fixed_canvas: None,
            });
            slot.cards = true;
            slot.card_accent = CardAccent::Editorial(member.kind);
            if slot.columns == 1 {
                slot.fixed_canvas = Some(
                    slot.fixed_canvas
                        .unwrap_or(self.canvas)
                        .min(self.canvas)
                        .min(if member.wide {
                            self.canvas
                        } else {
                            self.prose_measures.reference + 2. * super::CARD_PADDING
                        }),
                );
            }
            placements.insert(owner, slot);
        }
        for segment in projection.segments() {
            if let Some(member) = self.editorials.get(&segment.top_level_node_id) {
                self.slots
                    .insert(segment.node_id, placements[&member.owner]);
                self.label_rows.remove(&segment.node_id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn request_response_requires_an_explicit_label_and_a_code_example() {
        for label in [
            "Request",
            "Response: 201 Created",
            "HTTP request",
            "HTTP response",
        ] {
            let doc = document_core::Document::from_markdown(format!(
                "### {label}\n\n```http\nAuthored literal\n```\n"
            ))
            .unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            assert_eq!(
                analyze(&projection.roots().collect::<Vec<_>>()).len(),
                2,
                "{label}"
            );
        }
        for source in [
            "### Request\n\nPlease review the document.\n",
            "### Response handling\n\n```http\n200 OK\n```\n",
            "### Request\n\n```math\nx + y\n```\n",
            "# Request\n\n```http\nGET /\n```\n",
        ] {
            let doc = document_core::Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            assert!(
                analyze(&projection.roots().collect::<Vec<_>>()).is_empty(),
                "{source}"
            );
        }
    }

    #[test]
    fn only_explicit_bounded_objects_receive_an_enclosure() {
        for title in [
            "Decision",
            "Selected option: Local files",
            "Example: Reading",
            "Pros",
            "Invalid: Missing name",
        ] {
            let doc = document_core::Document::from_markdown(format!(
                "### {title}\n\nAuthored content.\n"
            ))
            .unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            assert_eq!(
                analyze(&projection.roots().collect::<Vec<_>>()).len(),
                2,
                "{title}"
            );
        }
        for source in [
            "# Decision\n\nDocument title.\n",
            "### Decision making\n\nOrdinary prose.\n",
            "### Example\n",
            "### Pros\n\n- [x] Done\n",
            "### Decision\n\nShort.\n\n#### Nested chapter\n\nLonger argument.\n",
        ] {
            let doc = document_core::Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            assert!(
                analyze(&projection.roots().collect::<Vec<_>>()).is_empty(),
                "{source}"
            );
        }
    }
}
