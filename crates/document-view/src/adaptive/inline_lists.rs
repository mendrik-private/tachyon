//! Explicit paragraph enumerations. Boundaries reference canonical text bytes.
use super::prose::Flow;
use document_core::{InlineStyle, RichText};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct InlineList {
    pub flow: Flow,
    pub rows: Vec<usize>,
}

impl InlineList {
    pub fn placement(&self, mut item: usize) -> Option<(usize, usize, usize)> {
        for (row, &columns) in self.rows.iter().enumerate() {
            if item < columns {
                return Some((row + 1, item, columns));
            }
            item -= columns;
        }
        None
    }
}

/// An authored introduction ending in a colon, followed by explicitly labeled
/// clauses. Comma-only prose, nested delimiters, code/link punctuation and
/// unfinished or overlong clauses do not provide sufficient evidence for this
/// family.
pub(crate) fn starts(content: &RichText) -> Option<Vec<usize>> {
    if content.len() > 4096 {
        return None;
    }
    let text = content.as_string();
    if text.contains(['\n', '\r']) {
        return None;
    }
    let runs = content.runs();
    let protected = |offset| {
        runs.get(runs.partition_point(|run| run.range.end <= offset))
            .is_some_and(|run| {
                run.range.contains(&offset)
                    && run.styles.iter().any(|style| {
                        matches!(
                            style,
                            InlineStyle::Code
                                | InlineStyle::Link(_)
                                | InlineStyle::Math { .. }
                                | InlineStyle::Image { .. }
                                | InlineStyle::PreservedHtml(_)
                        )
                    })
            })
    };
    let mut depth = 0_usize;
    let mut intro = None;
    let mut boundaries = vec![0];
    for (offset, c) in text.char_indices() {
        if protected(offset) {
            continue;
        }
        match c {
            '(' | '[' | '{' => depth += 1,
            ')' | ']' | '}' => depth = depth.checked_sub(1)?,
            '"' | '“' | '”' => return None,
            ';' if depth > 0 => return None,
            ':' if depth == 0 && intro.is_none() => {
                if offset == 0
                    || offset > 160
                    || !text[offset + 1..].starts_with(char::is_whitespace)
                {
                    return None;
                }
                let end =
                    offset + 1 + text[offset + 1..].len() - text[offset + 1..].trim_start().len();
                intro = Some(end);
                boundaries.push(end);
            }
            ';' if depth == 0 && intro.is_some() => {
                let end =
                    offset + 1 + text[offset + 1..].len() - text[offset + 1..].trim_start().len();
                boundaries.push(end);
                if boundaries.len() > 13 {
                    return None;
                }
            }
            _ => {}
        }
    }
    if depth != 0 || !(3..=13).contains(&boundaries.len()) {
        return None;
    }
    for (index, &start) in boundaries.iter().enumerate().skip(1) {
        let end = boundaries.get(index + 1).copied().unwrap_or(text.len());
        let clause = text.get(start..end)?.trim().trim_end_matches(';').trim();
        if clause.is_empty() || clause.len() > 512 || !explicit_clause_label(clause) {
            return None;
        }
    }
    Some(boundaries)
}

/// A semicolon alone is ordinary prose punctuation. The author must mark every
/// item with either a parenthesized enumerator or a short term followed by a
/// colon before the renderer can turn the paragraph into a grid.
fn explicit_clause_label(clause: &str) -> bool {
    let clause = clause.trim_start();
    if let Some(marker) = clause
        .strip_prefix('(')
        .and_then(|rest| rest.split_once(')').map(|(marker, _)| marker))
        && (1..=4).contains(&marker.chars().count())
        && marker.chars().all(char::is_alphanumeric)
    {
        return true;
    }
    let Some((label, _)) = clause.split_once(':') else {
        return false;
    };
    let label = label.trim();
    (1..=48).contains(&label.chars().count())
        && label.chars().any(char::is_alphabetic)
        && !label.contains(['/', '@'])
}

#[cfg(test)]
mod tests {
    use super::*;
    use document_core::{BlockNode, Document};

    #[test]
    fn explicit_clauses_keep_every_delimiter_and_rich_range() {
        for source in [
            "Consider: (a) field notes; (b) reading notes; (c) review notes.",
            "Evidence: **Direct observation:** notes; `a;b`: source code; [Published work](https://example.test): review.",
            "Methods: (a) analyse café; (b) compare 結果; (c) review outcomes.",
        ] {
            let document = Document::from_markdown(source).unwrap();
            let snapshot = document.snapshot();
            let BlockNode::Paragraph(p) = snapshot.blocks().iter().next().unwrap().as_ref() else {
                panic!()
            };
            let boundaries = starts(&p.content).unwrap();
            assert_eq!(boundaries.len(), 4);
            let text = p.content.as_string();
            let fragments = boundaries
                .iter()
                .enumerate()
                .map(|(i, &start)| {
                    &text[start..boundaries.get(i + 1).copied().unwrap_or(text.len())]
                })
                .collect::<String>();
            assert_eq!(fragments, text);
            assert_eq!(snapshot.serialize().unwrap(), source);
        }
    }

    #[test]
    fn recognition_has_explicit_count_and_byte_bounds() {
        for count in 1..=13 {
            let text = format!(
                "Consider: {}.",
                (0..count)
                    .map(|n| format!("({}) clause {n}", n + 1))
                    .collect::<Vec<_>>()
                    .join("; ")
            );
            let result = starts(&RichText::new(text));
            assert_eq!(result.is_some(), (2..=12).contains(&count));
            if let Some(boundaries) = result {
                assert_eq!(boundaries.len(), count + 1);
            }
        }
        assert!(
            starts(&RichText::new(format!(
                "Consider: {}; brief.",
                "x".repeat(513)
            )))
            .is_none()
        );
        assert!(starts(&RichText::new(format!("{}: one; two.", "x".repeat(161)))).is_none());
    }

    #[test]
    fn ambiguous_or_incomplete_prose_stays_a_paragraph() {
        for source in [
            "First, second, and third.",
            "Evidence: field notes, interviews, reading.",
            "One clause; another clause.",
            "Evidence: one; ; three.",
            "Evidence: one; two;",
            "Evidence: (one; two); three.",
            "Evidence: \"one; two\"; three.",
            "Read https://example.test; then decide.",
        ] {
            assert!(starts(&RichText::new(source)).is_none(), "{source}");
        }
    }

    #[test]
    fn ranking_prose_with_semicolon_phrases_stays_a_paragraph() {
        let source = "Sort inside a group: pin descending; hard consequence before soft; action-required before waiting; human before service at equal consequence; exact next applicable time ascending (unknown last); last factual change descending; stable object ID ascending. Locale text is never a sort predicate.";
        assert!(
            starts(&RichText::new(source)).is_none(),
            "a prose ranking sentence must not be converted into a visual grid"
        );
    }
}
