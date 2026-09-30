//! Exact model comparison for attaching parsed source ranges to live IDs.
use rustc_hash::FxHashMap;

use crate::{BlockNode, BlockSequence, NodeId, RichText};

fn text(a: &RichText, b: &RichText) -> bool {
    a.as_cow() == b.as_cow() && a.runs() == b.runs()
}

/// Whether both trees have the same content and shape, ignoring IDs. While
/// comparing, records each node ID of `a` (including list items, rows and
/// cells) against the ID at the same position in `b`; on a mismatch the
/// recorded pairs are incomplete.
pub(super) fn same_blocks(
    a: &BlockSequence,
    b: &BlockSequence,
    ids: &mut FxHashMap<NodeId, NodeId>,
) -> bool {
    a.len() == b.len() && a.iter().zip(b).all(|(a, b)| same_block(a, b, ids))
}

fn same_block(a: &BlockNode, b: &BlockNode, ids: &mut FxHashMap<NodeId, NodeId>) -> bool {
    ids.insert(a.id(), b.id());
    match (a, b) {
        (BlockNode::Paragraph(a), BlockNode::Paragraph(b)) => text(&a.content, &b.content),
        (BlockNode::Heading(a), BlockNode::Heading(b)) => {
            a.level == b.level && text(&a.content, &b.content)
        }
        (BlockNode::CodeBlock(a), BlockNode::CodeBlock(b)) => {
            a.language == b.language && a.syntax == b.syntax && text(&a.content, &b.content)
        }
        (BlockNode::Image(a), BlockNode::Image(b)) => {
            a.source == b.source
                && text(&a.alt, &b.alt)
                && a.title == b.title
                && a.intrinsic_size == b.intrinsic_size
                && match (&a.link, &b.link) {
                    (None, None) => true,
                    (Some(a), Some(b)) => a.target == b.target && a.title == b.title,
                    _ => false,
                }
        }
        (BlockNode::List(a), BlockNode::List(b)) => {
            a.kind == b.kind
                && a.tight == b.tight
                && a.items.len() == b.items.len()
                && a.items.iter().zip(b.items.iter()).all(|(a, b)| {
                    ids.insert(a.id, b.id);
                    a.checked == b.checked && same_blocks(&a.blocks, &b.blocks, ids)
                })
        }
        (BlockNode::BlockQuote { blocks: a, .. }, BlockNode::BlockQuote { blocks: b, .. }) => {
            same_blocks(a, b, ids)
        }
        (
            BlockNode::Definition {
                kind: ak,
                blocks: a,
                ..
            },
            BlockNode::Definition {
                kind: bk,
                blocks: b,
                ..
            },
        ) => ak == bk && same_blocks(a, b, ids),
        (
            BlockNode::FootnoteDefinition {
                label: ak,
                blocks: a,
                ..
            },
            BlockNode::FootnoteDefinition {
                label: bk,
                blocks: b,
                ..
            },
        ) => ak == bk && same_blocks(a, b, ids),
        (
            BlockNode::Alert {
                kind: ak,
                title: at,
                blocks: a,
                ..
            },
            BlockNode::Alert {
                kind: bk,
                title: bt,
                blocks: b,
                ..
            },
        ) => {
            ak == bk
                && match (at, bt) {
                    (None, None) => true,
                    (Some(a), Some(b)) => text(a, b),
                    _ => false,
                }
                && same_blocks(a, b, ids)
        }
        (BlockNode::Table(a), BlockNode::Table(b)) => {
            a.columns == b.columns
                && a.header_rows == b.header_rows
                && a.border == b.border
                && a.preserved_metadata == b.preserved_metadata
                && a.rows.len() == b.rows.len()
                && a.rows.iter().zip(b.rows.iter()).all(|(a, b)| {
                    ids.insert(a.id, b.id);
                    a.cells.len() == b.cells.len()
                        && a.cells.iter().zip(b.cells.iter()).all(|(a, b)| {
                            ids.insert(a.id, b.id);
                            same_blocks(&a.blocks, &b.blocks, ids)
                        })
                })
        }
        (BlockNode::ThematicBreak { .. }, BlockNode::ThematicBreak { .. }) => true,
        (
            BlockNode::PreservedSource {
                source: a,
                description: ad,
                ..
            },
            BlockNode::PreservedSource {
                source: b,
                description: bd,
                ..
            },
        ) => a == b && ad == bd,
        _ => false,
    }
}
