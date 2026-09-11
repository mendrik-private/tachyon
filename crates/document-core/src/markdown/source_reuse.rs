//! Source-local list, note and table edits. Original trees and spans share the source spine's
//! lifetime; no parsing, source diffing or layout work is done while scrolling.

use std::{ops::Range, sync::Arc};

use crate::{BlockNode, BlockSequence, DocumentError, DocumentSnapshot, ListKind, SourceSpine};

use super::{serialize_inline, serialize_list_content};

type Patch = (Range<usize>, String);

/// Comrak lifts definitions out of quote/list containers, so a note's original
/// span may overlap an otherwise untouched root. Patch the original file once
/// when every edit has a proven local span; emitting semantic roots separately
/// would duplicate such a definition. Structural/unsupported edits still take
/// the normal block serializer path.
pub(super) fn serialize_snapshot(
    snapshot: &DocumentSnapshot,
) -> Result<Option<String>, DocumentError> {
    let spine = snapshot.source_spine();
    let mut patches = Vec::new();
    for block in snapshot.blocks() {
        if snapshot.is_transient_caret(block) {
            continue;
        }
        let Some(original) = spine.original_block(block.id()) else {
            return Ok(None);
        };
        if !collect(original, block, spine, &mut patches)? {
            return Ok(None);
        }
    }
    Ok(apply(spine, 0..spine.original().len(), patches))
}

/// Patch the owner and lifted semantic roots once inside their shared source.
pub(super) fn serialize_owned(
    blocks: &[&Arc<BlockNode>],
    snapshot: &DocumentSnapshot,
) -> Result<Option<String>, DocumentError> {
    let spine = snapshot.source_spine();
    let Some(unit) = spine.unit(blocks[0].id()) else {
        return Ok(None);
    };
    let mut patches = Vec::new();
    for block in blocks {
        let Some(original) = spine.original_block(block.id()) else {
            return Ok(None);
        };
        if !collect(original, block, spine, &mut patches)? {
            return Ok(None);
        }
    }
    Ok(apply(spine, unit.source.clone(), patches))
}

pub(super) fn serialize(
    block: &Arc<BlockNode>,
    snapshot: &DocumentSnapshot,
) -> Result<Option<String>, DocumentError> {
    let spine = snapshot.source_spine();
    let Some(original) = spine.original_block(block.id()) else {
        return Ok(None);
    };
    let Some(unit) = spine.unit(block.id()) else {
        return Ok(None);
    };
    let mut patches = Vec::new();
    if !collect(original, block, spine, &mut patches)? {
        return Ok(None);
    }
    Ok(apply(spine, unit.source.clone(), patches))
}

fn apply(spine: &SourceSpine, source: Range<usize>, mut patches: Vec<Patch>) -> Option<String> {
    patches.sort_by_key(|(range, _)| range.start);
    let mut cursor = source.start;
    let mut output = String::new();
    for (range, replacement) in patches {
        // A malformed, synthetic, overlapping or out-of-root span can never
        // license reuse. Canonical serialization remains the semantic oracle.
        if range.start < cursor || range.end > source.end || spine.reference_intersects(&range) {
            return None;
        }
        output.push_str(spine.slice(cursor..range.start));
        output.push_str(&replacement);
        cursor = range.end;
    }
    output.push_str(spine.slice(cursor..source.end));
    Some(output)
}

fn sequence(
    before: &BlockSequence,
    after: &BlockSequence,
    spine: &SourceSpine,
    patches: &mut Vec<Patch>,
) -> Result<bool, DocumentError> {
    if before.len() != after.len() {
        return Ok(false);
    }
    for (old, new) in before.iter().zip(after) {
        if !collect(old, new, spine, patches)? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn leaf_block(
    block: &BlockNode,
    spine: &SourceSpine,
    patches: &mut Vec<Patch>,
) -> Result<bool, DocumentError> {
    let Some(mut range) = spine.leaf_block(block.id()) else {
        return Ok(false);
    };
    let raw = spine.slice(range.clone());
    range.end -= raw.len() - raw.trim_end_matches(['\r', '\n']).len();
    let line_start = spine.original()[..range.start]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    let context = &spine.original()[line_start..range.start];
    // The checkbox is authored first-line syntax, not four extra columns of
    // continuation indentation. Its original bytes remain outside the patch.
    let context = ["[ ]", "[x]", "[X]"]
        .into_iter()
        .find_map(|marker| context.trim_end().strip_suffix(marker))
        .unwrap_or(context);
    if !context
        .bytes()
        .all(|b| b.is_ascii_whitespace() || b.is_ascii_digit() || b">-*+.)".contains(&b))
    {
        return Ok(false);
    }
    let continuation = context
        .chars()
        .map(|c| {
            if c == '>' || c.is_ascii_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>();
    // Only this source-owned leaf is replaced. Children, neighboring syntax
    // and separators remain original; the canonical serializer owns escaping.
    let code = matches!(block, BlockNode::CodeBlock(_));
    let newline = spine.line_ending().as_str();
    let replacement = super::serialize_block(block, if code { newline } else { "\n" }, 0)?.replace(
        '\n',
        &format!("{}{continuation}", if code { "\n" } else { newline }),
    );
    patches.push((range, replacement));
    Ok(true)
}

fn collect(
    before: &Arc<BlockNode>,
    after: &Arc<BlockNode>,
    spine: &SourceSpine,
    patches: &mut Vec<Patch>,
) -> Result<bool, DocumentError> {
    if before.id() != after.id() {
        return Ok(false);
    }
    if Arc::ptr_eq(before, after) {
        return Ok(true);
    }
    match (before.as_ref(), after.as_ref()) {
        (BlockNode::CodeBlock(old), BlockNode::CodeBlock(new))
            if old.syntax == crate::CodeBlockSyntax::Fenced && new.syntax == old.syntax =>
        {
            leaf_block(after, spine, patches)
        }
        (BlockNode::Heading(_), BlockNode::Heading(_))
        | (BlockNode::Image(_), BlockNode::Image(_)) => leaf_block(after, spine, patches),
        (BlockNode::Paragraph(old), BlockNode::Paragraph(new)) => {
            if let Some(range) = spine.html_paragraph(new.id) {
                let Some(replacement) = super::html_source::replace_paragraph(
                    spine.slice(range.clone()),
                    &old.content,
                    &new.content,
                ) else {
                    return Ok(false);
                };
                patches.push((range, replacement));
                return Ok(true);
            }
            if let Some(mut range) = spine.table_cell(new.id) {
                let raw = spine.slice(range.clone());
                let leading = raw.len() - raw.trim_start_matches([' ', '\t']).len();
                let trailing = raw.len() - raw.trim_end_matches([' ', '\t']).len();
                // Cell padding belongs to source, not to its editable value.
                range.start += leading;
                range.end = range.end.saturating_sub(trailing).max(range.start);
                let value = new.content.as_string();
                let plain = old.content.runs().iter().all(|r| r.styles.is_empty())
                    && new.content.runs().iter().all(|r| r.styles.is_empty())
                    && spine.slice(range.clone()) == old.content.as_string()
                    && value.trim() == value
                    && !value.contains([
                        '\\', '`', '*', '_', '~', '[', ']', '<', '>', '&', '|', '$', '\n', '\r',
                        '@',
                    ])
                    && !value.contains("://")
                    && !value.to_ascii_lowercase().contains("www.");
                let replacement = if plain {
                    value
                } else {
                    serialize_inline(&new.content)
                        .replace('|', "\\|")
                        .replace('\n', "<br>")
                };
                patches.push((range, replacement));
                return Ok(true);
            }
            let Some(mut range) = spine.note_paragraph(new.id) else {
                return leaf_block(after, spine, patches);
            };
            let raw = spine.slice(range.clone());
            range.end -= raw.len() - raw.trim_end_matches(['\r', '\n']).len();
            let line_start = spine.original()[..range.start]
                .rfind('\n')
                .map_or(0, |i| i + 1);
            let context = &spine.original()[line_start..range.start];
            let Some(continuation) = note_continuation(context) else {
                return Ok(false);
            };
            let replacement = serialize_inline(&new.content).replace(
                '\n',
                &format!("{}{continuation}", spine.line_ending().as_str()),
            );
            patches.push((range, replacement));
            Ok(true)
        }
        (BlockNode::List(old), BlockNode::List(new)) => {
            if old.kind != new.kind
                || old.tight != new.tight
                || old.items.len() != new.items.len()
                || old
                    .items
                    .iter()
                    .zip(new.items.iter())
                    .any(|(a, b)| a.id != b.id)
            {
                return Ok(false);
            }
            for (index, (old_item, item)) in old.items.iter().zip(new.items.iter()).enumerate() {
                let checkpoint = patches.len();
                // Checkbox and body edits compose against the same original
                // source. Keep both local, or discard both before fallback.
                let state_reusable = if old_item.checked == item.checked {
                    true
                } else if let (Some(before), Some(after)) = (old_item.checked, item.checked)
                    && before != after
                    && let Some(range) = spine.list_item(item.id)
                    && let Some((prefix, _)) =
                        marker(spine.slice(range.clone()), &new.kind, index, item.checked)
                    && let Some(offset) = prefix.find('[')
                    && let Some(raw_marker) = spine.slice(range.clone()).get(offset..offset + 3)
                    && matches!(raw_marker, "[ ]" | "[x]" | "[X]")
                    && before == (raw_marker.as_bytes()[1] != b' ')
                {
                    let start = range.start + offset + 1;
                    patches.push((start..start + 1, if after { "x" } else { " " }.into()));
                    true
                } else {
                    false
                };
                if state_reusable && sequence(&old_item.blocks, &item.blocks, spine, patches)? {
                    continue;
                }
                patches.truncate(checkpoint);
                if state_reusable
                    && old_item.checked.is_none()
                    && blank_item_intro(old_item, item, &new.kind, index, spine, patches)?
                {
                    continue;
                }
                // Prefer the smallest changed nested item. If its structure
                // changed, replace this whole item rather than drop the edit.
                patches.truncate(checkpoint);
                let Some(mut range) = spine.list_item(item.id) else {
                    return Ok(false);
                };
                let raw = spine.slice(range.clone());
                // Keep source-owned blank separators outside the replacement.
                range.end -= raw.len() - raw.trim_end_matches(['\r', '\n']).len();
                let line_start = spine.original()[..range.start]
                    .rfind('\n')
                    .map_or(0, |i| i + 1);
                let context = &spine.original()[line_start..range.start];
                // Ancestor quote rails survive; ancestor list markers become
                // continuation indentation. These are syntax bytes, not text.
                if !context.bytes().all(|b| {
                    b.is_ascii_whitespace() || b.is_ascii_digit() || b">-*+.):".contains(&b)
                }) {
                    return Ok(false);
                }
                let continuation = context
                    .chars()
                    .map(|c| {
                        if c == '>' || c.is_ascii_whitespace() {
                            c
                        } else {
                            ' '
                        }
                    })
                    .collect::<String>();
                let Some((marker, padding)) = marker(raw, &new.kind, index, item.checked) else {
                    return Ok(false);
                };
                let newline = spine.line_ending().as_str();
                let content = serialize_list_content(&item.blocks, "\n", new.tight)?;
                let content = content.replace(
                    '\n',
                    &format!("{newline}{continuation}{}", " ".repeat(padding)),
                );
                patches.push((range, format!("{marker}{content}")));
            }
            Ok(true)
        }
        (BlockNode::BlockQuote { blocks: old, .. }, BlockNode::BlockQuote { blocks: new, .. }) => {
            sequence(old, new, spine, patches)
        }
        (BlockNode::Table(old), BlockNode::Table(new)) => {
            if old.columns != new.columns
                || old.header_rows != new.header_rows
                || old.border != new.border
                || old.preserved_metadata != new.preserved_metadata
                || old.rows.len() != new.rows.len()
            {
                return Ok(false);
            }
            for (a, b) in old.rows.iter().zip(new.rows.iter()) {
                if a.id != b.id || a.cells.len() != b.cells.len() {
                    return Ok(false);
                }
                for (a, b) in a.cells.iter().zip(b.cells.iter()) {
                    if a.id != b.id || !sequence(&a.blocks, &b.blocks, spine, patches)? {
                        return Ok(false);
                    }
                }
            }
            Ok(true)
        }
        (
            BlockNode::Definition {
                kind: a,
                blocks: old,
                ..
            },
            BlockNode::Definition {
                kind: b,
                blocks: new,
                ..
            },
        ) if a == b => sequence(old, new, spine, patches),
        (
            BlockNode::FootnoteDefinition {
                label: a,
                blocks: old,
                ..
            },
            BlockNode::FootnoteDefinition {
                label: b,
                blocks: new,
                ..
            },
        ) if a == b => sequence(old, new, spine, patches),
        (
            BlockNode::Alert {
                kind: a,
                title: at,
                blocks: old,
                ..
            },
            BlockNode::Alert {
                kind: b,
                title: bt,
                blocks: new,
                ..
            },
        ) if a == b && at.as_ref().map(serialize_inline) == bt.as_ref().map(serialize_inline) => {
            sequence(old, new, spine, patches)
        }
        _ => Ok(false),
    }
}

/// A marker-only item has no original text span for its empty caret host.
/// Insert into that verified marker line, then patch descendants independently;
/// regenerating the whole item would rewrite untouched child source.
fn blank_item_intro(
    before: &crate::ListItem,
    after: &crate::ListItem,
    kind: &ListKind,
    index: usize,
    spine: &SourceSpine,
    patches: &mut Vec<Patch>,
) -> Result<bool, DocumentError> {
    if before.blocks.len() != after.blocks.len() {
        return Ok(false);
    }
    let (Some(BlockNode::Paragraph(old)), Some(BlockNode::Paragraph(new))) = (
        before.blocks.get(0).map(AsRef::as_ref),
        after.blocks.get(0).map(AsRef::as_ref),
    ) else {
        return Ok(false);
    };
    if old.id != new.id || !old.content.is_empty() {
        return Ok(false);
    }
    let Some(range) = spine.list_item(after.id) else {
        return Ok(false);
    };
    let raw = spine.slice(range.clone());
    let Some((prefix, padding)) = marker(raw, kind, index, None) else {
        return Ok(false);
    };
    let first = raw.lines().next().unwrap_or_default();
    if first.trim_end() != prefix.trim_end() {
        return Ok(false);
    }
    let line_start = spine.original()[..range.start]
        .rfind('\n')
        .map_or(0, |i| i + 1);
    let context = &spine.original()[line_start..range.start];
    if !context
        .bytes()
        .all(|b| b.is_ascii_whitespace() || b.is_ascii_digit() || b">-*+.):".contains(&b))
    {
        return Ok(false);
    }
    let continuation = context
        .chars()
        .map(|c| {
            if c == '>' || c.is_ascii_whitespace() {
                c
            } else {
                ' '
            }
        })
        .collect::<String>();
    let body = serialize_inline(&new.content).replace(
        '\n',
        &format!(
            "{}{continuation}{}",
            spine.line_ending().as_str(),
            " ".repeat(padding)
        ),
    );
    patches.push((
        range.start..range.start + first.len(),
        format!("{prefix}{body}"),
    ));
    for (old, new) in before.blocks.iter().zip(&after.blocks).skip(1) {
        if !collect(old, new, spine, patches)? {
            return Ok(false);
        }
    }
    Ok(true)
}

/// A changed note paragraph may reflow, but its siblings and blank separators
/// stay byte-identical. Preserve outer quote rails; a first-line definition
/// marker becomes four spaces on continuation lines, never label-width spaces.
fn note_continuation(context: &str) -> Option<String> {
    if let Some(marker) = context.rfind("[^") {
        let (ancestors, definition) = context.split_at(marker);
        let (_, padding) = definition.rsplit_once("]:")?;
        if !padding.bytes().all(|b| matches!(b, b' ' | b'\t'))
            || !ancestors
                .bytes()
                .all(|b| b.is_ascii_whitespace() || b.is_ascii_digit() || b">-*+.)".contains(&b))
        {
            return None;
        }
        let mut continuation = ancestors
            .chars()
            .map(|c| {
                if c == '>' || c.is_ascii_whitespace() {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>();
        continuation.push_str("    ");
        Some(continuation)
    } else {
        context
            .bytes()
            .all(|b| matches!(b, b' ' | b'\t' | b'>'))
            .then(|| context.to_owned())
    }
}

/// Keep marker dialect and spacing: changing '+' to '-' (or ')' to '.')
/// between two siblings would split the authored list on reopening.
fn marker(
    raw: &str,
    kind: &ListKind,
    index: usize,
    checked: Option<bool>,
) -> Option<(String, usize)> {
    let first = raw.lines().next()?;
    let marker_end = match kind {
        ListKind::Unordered | ListKind::Task if first.starts_with(['-', '+', '*']) => 1,
        ListKind::Ordered { .. } | ListKind::Task => {
            let digits = first.bytes().take_while(u8::is_ascii_digit).count();
            if digits == 0 || !matches!(first.as_bytes().get(digits), Some(b'.' | b')')) {
                return None;
            }
            digits + 1
        }
        _ => return None,
    };
    let space_end = marker_end
        + first[marker_end..]
            .bytes()
            .take_while(|b| matches!(b, b' ' | b'\t'))
            .count();
    let mut prefix = first[..space_end].to_owned();
    if let ListKind::Ordered { start } = kind {
        // The first marker owns the start value. Subsequent authored numbers
        // do not affect list semantics and can retain their original spelling.
        if index == 0 && first[..marker_end - 1].parse::<u64>().ok() != Some(*start) {
            return None;
        }
    }
    if space_end == marker_end {
        prefix.push(' ');
    }
    let padding = prefix.bytes().fold(0, |column, b| {
        if b == b'\t' {
            (column / 4 + 1) * 4
        } else {
            column + 1
        }
    });
    if let Some(checked) = checked {
        let body = &first[space_end..];
        if body.starts_with("[ ]") || body.starts_with("[x]") || body.starts_with("[X]") {
            let end = 3 + body[3..]
                .bytes()
                .take_while(|b| matches!(b, b' ' | b'\t'))
                .count();
            let mut task = body[..end].to_owned();
            if checked != (body.as_bytes()[1] != b' ') {
                task.replace_range(1..2, if checked { "x" } else { " " });
            }
            prefix.push_str(&task);
        } else {
            prefix.push_str(if checked { "[x] " } else { "[ ] " });
        }
    }
    Some((prefix, padding))
}
