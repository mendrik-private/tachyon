//! Explicit quantitative editorial objects. Values and context remain authored
//! text; this module never computes a percentage, trend, unit, or completion.
use document_core::{BlockNode, InlineStyle, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TextRole {
    Label,
    Value,
    Context,
}

/// Require a named object, a compact quantitative value, and nearby context.
/// A bare number or a section discussing metrics is not a metric card.
pub(crate) fn value_node(roots: &[&BlockNode]) -> Option<NodeId> {
    let [
        BlockNode::Heading(heading),
        BlockNode::Paragraph(value),
        context @ ..,
    ] = roots
    else {
        return None;
    };
    let title = heading.content.as_cow();
    let (label, name) = title.split_once(':')?;
    if !label.trim().eq_ignore_ascii_case("metric")
        || name.trim().is_empty()
        || context.is_empty()
        || context.len() > 2
        || !context.iter().all(|block| matches!(block, BlockNode::Paragraph(p) if !p.content.as_cow().trim().is_empty()))
        || value.content.runs().iter().any(|run| {
            run.styles.iter().any(|style| {
                !matches!(style, InlineStyle::Bold | InlineStyle::Italic)
            })
        })
    {
        return None;
    }
    let text = value.content.as_cow();
    let text = text.trim();
    if text.len() > 64 || text.contains(['\n', '\r']) || text.split_whitespace().count() > 3 {
        return None;
    }
    // Recognize the shape, not the meaning, of an authored quantity. Do not
    // parse/reformat locale-specific separators or guess the unit vocabulary.
    let number = text.trim_start_matches([
        '+', '-', '−', '~', '≈', '<', '>', '≤', '≥', '$', '€', '£', '¥',
    ]);
    let end = number
        .find(|c: char| !(c.is_ascii_digit() || matches!(c, '.' | ',' | '/' | '\u{202f}')))
        .unwrap_or(number.len());
    let numeric = &number[..end];
    (!numeric.is_empty()
        && numeric.starts_with(|c: char| c.is_ascii_digit())
        && numeric.chars().any(|c| c.is_ascii_digit())
        && !text.ends_with(['.', '!', '?']))
    .then_some(value.id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::TextProjection;

    #[test]
    fn metric_values_need_explicit_identity_and_authored_context() {
        for (source, accepted) in [
            (
                "### Metric: Review completion\n\n**75%**\n\n9 of 12 documents reviewed.\n",
                true,
            ),
            (
                "### Metric: Response time\n\n128 ms\n\nMedian, September 2026; 240 requests.\n",
                true,
            ),
            (
                "### Metric: Cost\n\n€1.234,50\n\nPer month, September 2026.\n",
                true,
            ),
            (
                "### Metric: Changes\n\n−2.4%\n\nCompared with 125 ms in August.\n",
                true,
            ),
            (
                "### Metric: Count\n\n9\n\nDocuments reviewed of 12 submitted.\n",
                true,
            ),
            ("### Metric: Review\n\n75%\n", false),
            ("### Metric\n\n75%\n\n9 of 12 reviewed.\n", false),
            ("### Metric: \n\n75%\n\n9 of 12 reviewed.\n", false),
            (
                "### Metrics matter\n\n75%\n\nAn ordinary explanation.\n",
                false,
            ),
            (
                "### Metric: Review\n\n75 percent of documents are reviewed.\n\nAn explanation.\n",
                false,
            ),
            ("### Metric: Review\n\nGood\n\nAn explanation.\n", false),
            (
                "### Metric: Review\n\n[75%](https://example.org)\n\n9 of 12 reviewed.\n",
                false,
            ),
            (
                "### Metric: Review\n\n75%  \nremaining\n\nContext.\n",
                false,
            ),
            ("### Metric: Review\n\n75%\n\n- Context.\n", false),
        ] {
            let document = document_core::Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            assert_eq!(
                value_node(&projection.roots().collect::<Vec<_>>()).is_some(),
                accepted,
                "{source}"
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }
}
