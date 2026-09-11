//! Preserve non-rendered reference records when their source container is deleted.
use comrak::{
    Arena,
    nodes::{AstNode, NodeValue},
    parse_document,
};
use std::ops::Range;

/// Only source that the parser consumes without rendering is a definition.
/// Visible inline/code/HTML spans prevent syntax-looking examples from becoming
/// active reference records when their enclosing container is removed.
pub(super) fn records<'a>(source: &str, root: &'a AstNode<'a>) -> Vec<(Range<usize>, String)> {
    let starts = super::line_starts(source);
    let paragraphs = root
        .descendants()
        .filter_map(|node| {
            let data = node.data.borrow();
            matches!(data.value, NodeValue::Paragraph).then_some(data.sourcepos)
        })
        .collect::<Vec<_>>();
    let mut visible = root
        .descendants()
        .filter_map(|node| {
            let data = node.data.borrow();
            if matches!(
                data.value,
                NodeValue::Text(_)
                    | NodeValue::Code(_)
                    | NodeValue::CodeBlock(_)
                    | NodeValue::HtmlBlock(_)
                    | NodeValue::HtmlInline(_)
                    | NodeValue::Math(_)
                    | NodeValue::FrontMatter(_)
            ) {
                super::source_range(data.sourcepos, &starts, source.len()).and_then(|(a, b)| {
                    // Comrak leaves inline positions at the old paragraph origin
                    // after removing leading definitions. Do not trust mismatched
                    // text spans; paragraph-prefix parsing below checks context.
                    if let NodeValue::Text(text) = &data.value
                        && source.get(a..b) != Some(text.as_ref())
                    {
                        return None;
                    }
                    Some(a..b)
                })
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    visible.sort_by_key(|range| range.start);
    let mut merged: Vec<Range<usize>> = Vec::new();
    for range in visible {
        if let Some(last) = merged.last_mut()
            && range.start <= last.end
        {
            last.end = last.end.max(range.end);
        } else {
            merged.push(range);
        }
    }
    let visible = merged;
    let lines = source.split_inclusive('\n').collect::<Vec<_>>();
    let mut paragraph_at_line = vec![None; lines.len()];
    for position in paragraphs {
        let start = position.start.line.saturating_sub(1).min(lines.len());
        let end = position.end.line.min(lines.len());
        if start <= end {
            paragraph_at_line[start..end].fill(Some(position));
        }
    }
    let mut result = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        let line = strip_container(lines[index]);
        let start = starts[index];
        if !line.starts_with('[') || line.starts_with("[^") {
            index += 1;
            continue;
        }
        let rendered_prefix = paragraph_at_line[index].is_some_and(|position| {
            let first = position.start.line.saturating_sub(1);
            if first >= index || index >= position.end.line {
                return false;
            }
            let prefix = lines[first..index]
                .iter()
                .map(|line| strip_container(line))
                .collect::<String>();
            let arena = Arena::new();
            parse_document(&arena, &prefix, &super::markdown_options())
                .first_child()
                .is_some()
        });
        if rendered_prefix {
            index += 1;
            continue;
        }
        let prefix = &lines[index][..lines[index].len() - line.len()];
        let continuation = prefix
            .chars()
            .map(|c| {
                if c == '>' || c.is_whitespace() {
                    c
                } else {
                    ' '
                }
            })
            .collect::<String>();
        let mut candidate = String::new();
        let mut accepted = None;
        for (offset, raw) in lines[index..].iter().enumerate() {
            let end = starts[index + offset] + raw.len();
            let covered = visible.partition_point(|range| range.end <= starts[index + offset]);
            if visible.get(covered).is_some_and(|range| range.start < end) {
                break;
            }
            let normalized = if offset == 0 {
                line
            } else {
                raw.strip_prefix(&continuation)
                    .unwrap_or_else(|| strip_container(raw))
            };
            if offset > 0
                && (normalized.trim().is_empty()
                    || (accepted.is_some() && normalized.starts_with('[')))
            {
                break;
            }
            candidate.push_str(normalized);
            let arena = Arena::new();
            let parsed = parse_document(&arena, &candidate, &super::markdown_options());
            if parsed.first_child().is_none() {
                accepted = Some((offset + 1, end, candidate.clone()));
            }
        }
        if let Some((count, end, text)) = accepted {
            result.push((start..end, text));
            index += count;
        } else {
            index += 1;
        }
    }
    result
}

fn strip_container(mut line: &str) -> &str {
    loop {
        line = line.trim_start_matches([' ', '\t']);
        if let Some(rest) = line.strip_prefix('>') {
            line = rest;
            continue;
        }
        if let Some(rest) = ["- ", "+ ", "* "]
            .iter()
            .find_map(|marker| line.strip_prefix(marker))
        {
            line = rest;
            continue;
        }
        let digits = line.bytes().take_while(u8::is_ascii_digit).count();
        if digits > 0
            && let Some(rest) = line[digits..]
                .strip_prefix(". ")
                .or_else(|| line[digits..].strip_prefix(") "))
        {
            line = rest;
            continue;
        }
        return line;
    }
}
