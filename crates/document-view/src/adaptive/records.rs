//! Repeated field records: flat labelled lists that share one authored schema.
//! A repeated label sequence is evidence of fields, not of independent features,
//! so every instance uses the same aligned label rail instead of feature cards.
use super::{BlockNode, HashMap, HashSet, ListBlock, ListKind, NodeId, resource, timeline};

/// Ordered, case-insensitive labels of a flat unordered list whose every item
/// is one labelled paragraph. Bounded by the same limits as label rows.
fn schema(list: &ListBlock) -> Option<Vec<String>> {
    if !matches!(list.kind, ListKind::Unordered)
        || !(2..=64).contains(&list.items.len())
        || timeline::is_timeline(list)
        || resource::is_resource_list(list)
    {
        return None;
    }
    list.items
        .iter()
        .map(|item| {
            if item.checked.is_some() || item.blocks.len() != 1 {
                return None;
            }
            let BlockNode::Paragraph(paragraph) = item.blocks.get(0)?.as_ref() else {
                return None;
            };
            let end = super::authored_label_end(paragraph)?;
            let text = paragraph.content.as_string();
            let label = text.get(..end)?.trim().trim_end_matches(':').trim();
            (!label.is_empty() && !label.contains('\n')).then(|| label.to_lowercase())
        })
        .collect()
}

/// Top-level list -> first list of the same schema, in source order. Only
/// schemas that occur in at least two lists qualify; one pass over the roots.
pub(super) fn analyze(roots: &[&BlockNode], excluded: &HashSet<NodeId>) -> HashMap<NodeId, NodeId> {
    let mut first = HashMap::<Vec<String>, (NodeId, usize)>::new();
    let mut members = Vec::new();
    for root in roots {
        let BlockNode::List(list) = root else {
            continue;
        };
        if excluded.contains(&list.id) {
            continue;
        }
        let Some(labels) = schema(list) else {
            continue;
        };
        let entry = first.entry(labels).or_insert((list.id, 0));
        entry.1 += 1;
        members.push((list.id, entry.0));
    }
    let repeated = first
        .into_values()
        .filter(|&(_, count)| count >= 2)
        .map(|(id, _)| id)
        .collect::<HashSet<_>>();
    members
        .into_iter()
        .filter(|(_, schema)| repeated.contains(schema))
        .collect()
}
