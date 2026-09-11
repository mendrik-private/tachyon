//! Literal document signals. Recognition runs at projection publication, not
//! during scrolling. Labels, classification and color values are never invented.
use crate::{MineralPalette, ProjectionSegment, TextProjection};
use document_core::{BlockNode, InlineStyle, NodeId};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Tone {
    Neutral,
    Positive,
    Information,
    Warning,
    Critical,
}

impl Tone {
    pub fn style(self, palette: MineralPalette) -> crate::theme::SignalStyle {
        use document_core::AlertKind;
        match self {
            Self::Neutral => crate::theme::SignalStyle {
                ink: palette.text,
                paper: palette.surface_quiet,
                rule: palette.border,
            },
            Self::Positive => palette.signal(&AlertKind::Tip),
            Self::Information => palette.signal(&AlertKind::Note),
            Self::Warning => palette.signal(&AlertKind::Warning),
            Self::Critical => palette.signal(&AlertKind::Caution),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Badge {
    pub start: usize,
    pub end: usize,
    pub tone: Tone,
}

fn status_label(label: &str) -> bool {
    matches!(
        label
            .trim()
            .trim_end_matches(':')
            .trim()
            .to_ascii_lowercase()
            .as_str(),
        "status" | "state" | "classification" | "maturity"
    )
}

/// Most table edits keep their cheap row-local path. Only a field-name edit
/// that can change other cells' signal meaning needs whole-tree publication.
pub(crate) fn affects_badge_peers(
    projection: &TextProjection,
    segment: &ProjectionSegment,
    next: &BlockNode,
) -> bool {
    let Some((id, row, column)) = segment.context.table_cell else {
        return false;
    };
    let Some(BlockNode::Table(table)) = projection.block(id) else {
        return false;
    };
    let Some(before) = projection.block(segment.node_id).and_then(BlockNode::text) else {
        return false;
    };
    let Some(after) = next.text() else {
        return false;
    };
    let before = before.as_cow();
    let after = after.as_cow();
    if before == after {
        return false;
    }
    let status = status_label(&before) || status_label(&after);
    if row < table.header_rows {
        return status
            || [before.as_ref(), after.as_ref()].iter().any(|s| {
                matches!(
                    s.trim().to_ascii_lowercase().as_str(),
                    "property" | "field" | "key" | "value"
                )
            });
    }
    column == 0
        && table.column_count() == 2
        && status
        && table
            .rows
            .first()
            .and_then(|r| r.cells.first())
            .and_then(|c| c.blocks.get(0))
            .and_then(|b| b.text())
            .is_some_and(|t| {
                matches!(
                    t.as_cow().trim().to_ascii_lowercase().as_str(),
                    "property" | "field" | "key"
                )
            })
}

/// Explicit status columns, property/value tables and document metadata. A
/// sentence mentioning "accepted" is not an acceptance; unknown short labels
/// have a neutral treatment instead of a guessed success/warning meaning.
pub(crate) fn badge(projection: &TextProjection, segment: &ProjectionSegment) -> Option<Badge> {
    let BlockNode::Paragraph(p) = projection.block(segment.node_id)? else {
        return None;
    };
    let text = p.content.as_cow();
    let start = if let Some((id, row, column)) = segment.context.table_cell {
        let BlockNode::Table(table) = projection.block(id)? else {
            return None;
        };
        if table.header_rows == 0 || row < table.header_rows {
            return None;
        }
        let cell = table.rows.get(row)?.cells.get(column)?;
        if cell.blocks.len() != 1 {
            return None;
        }
        let header_cell = table.rows.first()?.cells.get(column)?;
        let header = header_cell.blocks.get(0)?.text()?.as_cow();
        let property = table.rows[row]
            .cells
            .first()?
            .blocks
            .get(0)?
            .text()?
            .as_cow();
        let property_header = table
            .rows
            .first()?
            .cells
            .first()?
            .blocks
            .get(0)?
            .text()?
            .as_cow();
        let key_value = table.rows[row].cells.len() == 2
            && column == 1
            && matches!(
                property_header.trim().to_ascii_lowercase().as_str(),
                "property" | "field" | "key"
            )
            && header.trim().eq_ignore_ascii_case("value")
            && status_label(&property);
        if !status_label(&header) && !key_value {
            return None;
        }
        text.len() - text.trim_start().len()
    } else if segment.context.metadata {
        let end = crate::adaptive::authored_label_end(p)?;
        if !status_label(&text[..end]) {
            return None;
        }
        end + text[end..].len() - text[end..].trim_start().len()
    } else {
        return None;
    };
    let end = text.trim_end().len();
    let value = text.get(start..end)?;
    if value.is_empty()
        || value.chars().count() > 24
        || value.contains(['\n', '\r'])
        || value.split_whitespace().count() > 3
        || value.ends_with(['.', '!', '?', ':'])
        || p.content.runs().iter().any(|run| {
            run.range.end > start
                && run
                    .styles
                    .iter()
                    .any(|style| !matches!(style, InlineStyle::Bold | InlineStyle::Italic))
        })
    {
        return None;
    }
    let tone = match value.to_ascii_lowercase().as_str() {
        "accepted" | "approved" | "ready" | "stable" | "complete" | "completed" | "valid" => {
            Tone::Positive
        }
        "beta" | "informational" | "optional" | "planned" | "extension" => Tone::Information,
        "in review" | "proposed" | "pending" => Tone::Warning,
        "failed" | "blocked" | "deprecated" | "invalid" => Tone::Critical,
        _ => Tone::Neutral,
    };
    Some(Badge { start, end, tone })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum ColorRole {
    Label,
    Literal,
    Context,
}

/// CSS hexadecimal forms only, including alpha. Return RGBA without rounding
/// or changing the authored spelling, casing or shorthand in the text model.
pub(crate) fn hex_color(text: &str) -> Option<u32> {
    let digits = text.trim().strip_prefix('#')?;
    if !matches!(digits.len(), 3 | 4 | 6 | 8) || !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let value = u32::from_str_radix(digits, 16).ok()?;
    Some(match digits.len() {
        3 => {
            (((value >> 8) * 17) << 24)
                | ((((value >> 4) & 15) * 17) << 16)
                | (((value & 15) * 17) << 8)
                | 255
        }
        4 => {
            (((value >> 12) * 17) << 24)
                | ((((value >> 8) & 15) * 17) << 16)
                | ((((value >> 4) & 15) * 17) << 8)
                | ((value & 15) * 17)
        }
        6 => (value << 8) | 255,
        _ => value,
    })
}

pub(crate) fn color_literal(block: &BlockNode) -> Option<u32> {
    let BlockNode::Paragraph(p) = block else {
        return None;
    };
    if p.content.runs().iter().any(|run| {
        run.styles.iter().any(|style| {
            !matches!(
                style,
                InlineStyle::Code | InlineStyle::Bold | InlineStyle::Italic
            )
        })
    }) {
        return None;
    }
    hex_color(&p.content.as_cow())
}

pub(crate) fn color_value(roots: &[&BlockNode]) -> Option<NodeId> {
    let [BlockNode::Heading(h), literal, context @ ..] = roots else {
        return None;
    };
    let title = h.content.as_cow();
    let (label, name) = title.split_once(':')?;
    if !matches!(
        label.trim().to_ascii_lowercase().as_str(),
        "color" | "colour" | "color token" | "colour token"
    ) || name.trim().is_empty()
        || context.is_empty()
        || context.len() > 2
        || !context
            .iter()
            .all(|b| matches!(b, BlockNode::Paragraph(p) if !p.content.as_cow().trim().is_empty()))
    {
        return None;
    }
    color_literal(literal).map(|_| literal.id())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn literal_colors_preserve_css_rgba_without_guessing_names_or_invalid_values() {
        for (text, expected) in [
            ("#3F6247", 0x3f6247ff),
            ("#fff", 0xffffffff),
            ("#1234", 0x11223344),
            ("#0aB", 0x00aabbff),
            ("#11223300", 0x11223300),
        ] {
            assert_eq!(hex_color(text), Some(expected));
        }
        for text in [
            "green",
            "rgb(1, 2, 3)",
            "#12345",
            "#gggggg",
            "#123456789",
            "#3f6247 extra",
            "#12🍀",
        ] {
            assert_eq!(hex_color(text), None);
        }
    }

    #[test]
    fn status_signals_need_explicit_context_and_never_change_source() {
        let source = "# Signals\n\n- **Status:** Accepted\n- **Owner:** Accepted\n\n## Registry\n\n| Status | Description |\n| --- | --- |\n| Draft | Accepted |\n| Beta | Deprecated |\n| Internal | Draft |\n| [Ready](https://example.org) | Plain |\n| This is a complete sentence. | Plain |\n\n| Property | Value |\n| --- | --- |\n| Classification | Confidential |\n| Owner | Accepted |\n\nAccepted is ordinary prose.\n";
        let doc = document_core::Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&doc.snapshot());
        let values = projection
            .segments()
            .iter()
            .filter_map(|s| {
                badge(&projection, s).map(|b| {
                    (
                        projection
                            .block(s.node_id)
                            .unwrap()
                            .text()
                            .unwrap()
                            .as_string()[b.start..b.end]
                            .to_owned(),
                        b.tone,
                    )
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(
            values,
            [
                ("Accepted".into(), Tone::Positive),
                ("Draft".into(), Tone::Neutral),
                ("Beta".into(), Tone::Information),
                ("Internal".into(), Tone::Neutral),
                ("Confidential".into(), Tone::Neutral)
            ]
        );
        assert_eq!(doc.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn edited_labels_refresh_signals_and_header_changes_republish_siblings() {
        use document_core::{Document, EditCommand};
        let source = "| Status | Detail |\n| --- | --- |\n| Accepted | Keep source |\n";
        let mut doc = Document::from_markdown(source).unwrap();
        let mut projection = TextProjection::from_snapshot(&doc.snapshot());
        let value = projection
            .segments()
            .iter()
            .find(|s| s.context.badge.is_some())
            .unwrap()
            .node_id;
        doc.apply(EditCommand::ReplaceText {
            node_id: value,
            range: 0..8,
            text: "Deprecated".into(),
            typing: true,
            selection_after: None,
        })
        .unwrap();
        assert!(
            projection
                .refresh_text_node(&doc.snapshot(), value)
                .is_some()
        );
        assert_eq!(
            projection.segment_for_node(value).unwrap().context.badge,
            Some(Badge {
                start: 0,
                end: 10,
                tone: Tone::Critical
            })
        );
        let header = projection
            .segments()
            .iter()
            .find(|s| s.context.table_header)
            .unwrap()
            .node_id;
        doc.apply(EditCommand::ReplaceText {
            node_id: header,
            range: 0..6,
            text: "Owner".into(),
            typing: true,
            selection_after: None,
        })
        .unwrap();
        assert!(
            projection
                .refresh_text_node(&doc.snapshot(), header)
                .is_none()
        );
        let after = TextProjection::from_snapshot(&doc.snapshot());
        assert!(after.segments().iter().all(|s| s.context.badge.is_none()));
        doc.undo().unwrap();
        doc.undo().unwrap();
        assert_eq!(doc.snapshot().serialize().unwrap(), source);
    }

    #[test]
    fn color_cards_require_a_named_literal_and_authored_context() {
        for (source, accepted) in [
            ("### Color: Moss Green\n\n`#3F6247`\n\nEmphasis.\n", true),
            (
                "### Colour token: Transparent\n\n#1234\n\nAlpha over paper.\n",
                true,
            ),
            ("### Color: No context\n\n`#3F6247`\n", false),
            (
                "### Color discussion\n\n`#3F6247`\n\nOrdinary prose.\n",
                false,
            ),
            (
                "### Color: Invalid\n\n`#GGHHII`\n\nKeep this text.\n",
                false,
            ),
            (
                "### Color: Linked\n\n[#3F6247](https://example.org)\n\nNot a token.\n",
                false,
            ),
            ("### Color: Nested\n\n`#3F6247`\n\n- A list.\n", false),
        ] {
            let doc = document_core::Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            assert_eq!(
                color_value(&projection.roots().collect::<Vec<_>>()).is_some(),
                accepted,
                "{source}"
            );
            assert_eq!(doc.snapshot().serialize().unwrap(), source);
        }
    }
}
