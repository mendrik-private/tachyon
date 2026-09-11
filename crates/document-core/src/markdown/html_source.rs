//! Verified original-HTML paragraph addresses for the existing source spine.
//! This is lexical address extraction, not an alternative HTML/Markdown model.
use std::{ops::Range, sync::Arc};

use rustc_hash::FxHashMap;

use crate::{BlockNode, NodeId, RichText, Table};

struct Element {
    name: String,
    outer: Range<usize>,
    inner: Range<usize>,
    parent: Option<usize>,
}

/// Require explicit balanced non-void tags. The canonical importer below
/// validates semantic correspondence; repaired/implicit HTML stays unmapped.
fn elements(source: &str) -> Option<Vec<Element>> {
    let mut elements: Vec<Element> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut cursor = 0;
    while let Some(offset) = source[cursor..].find('<') {
        let start = cursor + offset;
        if source[start..].starts_with("<!--") {
            cursor = start + 4 + source[start + 4..].find("-->")? + 3;
            elements.push(Element {
                name: "!--".into(),
                outer: start..cursor,
                inner: cursor..cursor,
                parent: stack.last().copied(),
            });
            continue;
        }
        let mut end = start + 1;
        let mut quote = None;
        for &byte in &source.as_bytes()[start + 1..] {
            end += 1;
            match (quote, byte) {
                (Some(q), b) if q == b => quote = None,
                (None, b'\'' | b'"') => quote = Some(byte),
                (None, b'>') => break,
                _ => {}
            }
        }
        if source.as_bytes().get(end - 1) != Some(&b'>') || quote.is_some() {
            return None;
        }
        let token = &source[start + 1..end - 1];
        let closing = token.starts_with('/');
        let token = token.strip_prefix('/').unwrap_or(token);
        let name_end = token
            .find(|c: char| !c.is_ascii_alphanumeric())
            .unwrap_or(token.len());
        if name_end == 0 {
            return None;
        }
        let name = token[..name_end].to_ascii_lowercase();
        // Raw text, foreign content and templates cannot license lexical
        // descendant addresses without their parser-specific state machines.
        if matches!(
            name.as_str(),
            "script"
                | "style"
                | "textarea"
                | "title"
                | "plaintext"
                | "xmp"
                | "iframe"
                | "noembed"
                | "noframes"
                | "noscript"
                | "svg"
                | "math"
                | "template"
        ) {
            return None;
        }
        if closing {
            if !token[name_end..].trim().is_empty() {
                return None;
            }
            let index = stack.pop()?;
            if elements[index].name != name {
                return None;
            }
            elements[index].outer.end = end;
            elements[index].inner.end = start;
        } else {
            let void = matches!(
                name.as_str(),
                "area"
                    | "base"
                    | "br"
                    | "col"
                    | "embed"
                    | "hr"
                    | "img"
                    | "input"
                    | "link"
                    | "meta"
                    | "param"
                    | "source"
                    | "track"
                    | "wbr"
            );
            if token.trim_end().ends_with('/') && !void {
                return None;
            }
            let index = elements.len();
            elements.push(Element {
                name,
                outer: start..end,
                inner: end..end,
                parent: stack.last().copied(),
            });
            if !void {
                stack.push(index);
            }
        }
        cursor = end;
    }
    stack.is_empty().then_some(elements)
}

fn canonical_leaf(source: &str) -> Option<Arc<BlockNode>> {
    let markdown = crate::html::html_fragment_to_markdown(source).ok()?;
    let snapshot = super::import(markdown.into()).ok()?;
    (snapshot.blocks().len() == 1).then(|| snapshot.blocks().get(0).unwrap().clone())
}

pub(super) fn paragraph_spans(
    source: &str,
    table: &Table,
) -> Option<FxHashMap<NodeId, Range<usize>>> {
    let elements = elements(source)?;
    let source_cells = elements
        .iter()
        .enumerate()
        .filter(|(_, e)| matches!(e.name.as_str(), "td" | "th"))
        .collect::<Vec<_>>();
    let cells = table
        .rows
        .iter()
        .flat_map(|row| row.cells.iter())
        .collect::<Vec<_>>();
    if source_cells.len() != cells.len() {
        return None;
    }
    let mut leaves_by_cell: FxHashMap<usize, Vec<&Element>> = FxHashMap::default();
    for element in &elements {
        if matches!(
            element.name.as_str(),
            "p" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "pre"
        ) && let Some(cell) = std::iter::successors(element.parent, |i| elements[*i].parent)
            .find(|i| matches!(elements[*i].name.as_str(), "td" | "th"))
        {
            leaves_by_cell.entry(cell).or_default().push(element);
        }
    }
    let mut result = FxHashMap::default();
    for ((cell_index, cell_source), cell) in source_cells.into_iter().zip(cells) {
        let row = &elements[cell_source.parent?];
        let section = &elements[row.parent?];
        if row.name != "tr"
            || !matches!(section.name.as_str(), "table" | "thead" | "tbody" | "tfoot")
        {
            return None;
        }
        let leaves = crate::html::conversion_text_blocks(&cell.blocks);
        let candidates = leaves_by_cell.remove(&cell_index).unwrap_or_default();
        if candidates.is_empty()
            && leaves.len() == 1
            && let BlockNode::Paragraph(paragraph) = leaves[0]
            && same_text(&source[cell_source.inner.clone()], &paragraph.content)
        {
            result.insert(paragraph.id, cell_source.inner.clone());
            continue;
        }
        if leaves.len() != candidates.len() {
            return None;
        }
        for (leaf, candidate) in leaves.into_iter().zip(candidates) {
            let parsed = canonical_leaf(&source[candidate.outer.clone()])?;
            if super::serialize_block(leaf, "\n", 0).ok()?
                != super::serialize_block(&parsed, "\n", 0).ok()?
            {
                return None;
            }
            if matches!(leaf, BlockNode::Paragraph(_)) && candidate.name == "p" {
                result.insert(leaf.id(), candidate.inner.clone());
            }
        }
    }
    Some(result)
}

fn same_text(source: &str, expected: &RichText) -> bool {
    let Some(block) = canonical_leaf(&format!("<p>{source}</p>")) else {
        return false;
    };
    matches!(block.as_ref(), BlockNode::Paragraph(p) if super::serialize_inline(&p.content) == super::serialize_inline(expected))
}

pub(super) fn replace_paragraph(
    source: &str,
    before: &RichText,
    after: &RichText,
) -> Option<String> {
    // Preserve the original inline syntax when an ordinary text insertion or
    // deletion can be proven against the canonical rich-text interpretation.
    if let Some(replacement) = text_patch(source, before, after) {
        return Some(replacement);
    }
    let replacement = super::serialize_inline_html(after);
    same_text(&replacement, after).then_some(replacement)
}

fn text_patch(source: &str, before: &RichText, after: &RichText) -> Option<String> {
    let old = before.as_string();
    let new = after.as_string();
    let prefix = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let suffix = old[prefix..]
        .chars()
        .rev()
        .zip(new[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    // Map literal text characters to source ranges, excluding actual tag and
    // comment bytes. Entities are retained when outside the changed range.
    let ranges = text_ranges(source)?;
    let mut decoded = String::new();
    let mut addresses = Vec::new();
    for range in ranges {
        let mut cursor = range.start;
        while cursor < range.end {
            let start = cursor;
            let ch = if source.as_bytes()[cursor] == b'&' {
                let end = cursor + source[cursor..range.end].find(';')? + 1;
                let entity = &source[cursor + 1..end - 1];
                cursor = end;
                match html5ever::data::NAMED_ENTITIES.get(&source[start + 1..end]) {
                    Some(&(first, second)) if first != 0 => {
                        let first = char::from_u32(first)?;
                        if second != 0 {
                            addresses.push((decoded.len(), start..cursor));
                            decoded.push(first);
                            char::from_u32(second)?
                        } else {
                            first
                        }
                    }
                    _ => {
                        let numeric = entity.strip_prefix('#')?;
                        let number = if let Some(hex) = numeric
                            .strip_prefix('x')
                            .or_else(|| numeric.strip_prefix('X'))
                        {
                            u32::from_str_radix(hex, 16).ok()?
                        } else {
                            numeric.parse().ok()?
                        };
                        char::from_u32(number)?
                    }
                }
            } else {
                let ch = source[cursor..range.end].chars().next()?;
                cursor += ch.len_utf8();
                ch
            };
            addresses.push((decoded.len(), start..cursor));
            decoded.push(ch);
        }
    }
    if decoded != old {
        return None;
    }
    let start_next = addresses
        .iter()
        .find(|(offset, _)| *offset == prefix)
        .map_or(source.len(), |(_, r)| r.start);
    let start_previous = addresses
        .iter()
        .take_while(|(offset, _)| *offset < prefix)
        .last()
        .map_or(0, |(_, r)| r.end);
    let end_offset = old.len() - suffix;
    let end = addresses
        .iter()
        .take_while(|(offset, _)| *offset < end_offset)
        .last()
        .map_or(0, |(_, r)| r.end);
    for start in [start_next, start_previous] {
        let end = if prefix == end_offset { start } else { end };
        if start > end {
            continue;
        }
        let mut candidate = source.to_owned();
        candidate.replace_range(
            start..end,
            &super::escape_html(&new[prefix..new.len() - suffix]),
        );
        if same_text(&candidate, after) {
            return Some(candidate);
        }
    }
    None
}

fn text_ranges(source: &str) -> Option<Vec<Range<usize>>> {
    // The same strict tag scanner protects quoted '<' and comments. Its outer
    // ranges span descendants, so subtract only the opening/closing tokens.
    let elements = elements(source)?;
    let mut tags = Vec::new();
    for e in elements {
        tags.push(e.outer.start..e.inner.start);
        if e.inner.end != e.outer.end {
            tags.push(e.inner.end..e.outer.end);
        }
    }
    tags.sort_by_key(|r| r.start);
    let mut ranges = Vec::new();
    let mut cursor = 0;
    for tag in tags {
        if tag.start < cursor {
            return None;
        }
        if cursor < tag.start {
            ranges.push(cursor..tag.start);
        }
        cursor = tag.end;
    }
    if cursor < source.len() {
        ranges.push(cursor..source.len());
    }
    Some(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn uncertain_html_cannot_license_source_addresses() {
        for source in [
            "<p>implicit",
            "<p>wrong</td>",
            "<p title='unterminated>",
            "<script>const p = '<p>fake</p>';</script>",
            "<svg><text>foreign</text></svg>",
            "<!-- unclosed",
            "<p/>text",
        ] {
            assert!(elements(source).is_none(), "{source}");
        }
        let source = "<b title='<!-- fake <p> -->'>A</b><!-- real -->B";
        let ranges = text_ranges(source).unwrap();
        assert_eq!(
            ranges
                .iter()
                .map(|r| &source[r.clone()])
                .collect::<Vec<_>>(),
            ["A", "B"]
        );
    }
}
