//! Authored dates nominate a timeline, never item count or guessed chronology.
use super::{LayoutSlot, ListBlock, ListKind, NodeId};
use document_core::{BlockNode, Paragraph};

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Placement {
    pub next: Option<NodeId>,
    /// A horizontal timeline uses one row of measured, source-ordered slots.
    pub slot: Option<LayoutSlot>,
    /// Narrow vertical timelines keep date above event instead of squeezing it.
    pub stacked: bool,
}

pub(crate) fn is_timeline(list: &ListBlock) -> bool {
    matches!(list.kind, ListKind::Unordered)
        && (2..=64).contains(&list.items.len())
        && list.items.iter().all(|item| {
            item.checked.is_none()
                && matches!(item.blocks.get(0).map(AsRef::as_ref), Some(BlockNode::Paragraph(p)) if date_end(p).is_some())
        })
}

/// Rich events retain their supporting source blocks in one vertical sequence.
/// Only complete single-paragraph events may form a horizontal strip.
pub(crate) fn has_supporting_blocks(list: &ListBlock) -> bool {
    list.items.iter().any(|item| item.blocks.len() > 1)
}

/// Accepted source syntax: `2026: event`, `2026-09-08 — event`,
/// `September 8, 2026: event`, or a bold/code date followed by the event.
/// Ambiguous slash dates, versions, durations and impossible dates stay prose.
pub(crate) fn date_end(paragraph: &Paragraph) -> Option<usize> {
    if paragraph.content.len() > 4096 {
        return None;
    }
    let text = paragraph.content.as_string();
    let styled = super::authored_label_end(paragraph);
    let delimited = [": ", " — ", " – ", " - "]
        .into_iter()
        .filter_map(|separator| text.find(separator).map(|i| i + separator.len()))
        .min();
    let end = styled.or(delimited)?;
    let prefix = text
        .get(..end)?
        .trim()
        .trim_end_matches([':', '—', '–', '-'])
        .trim();
    if !valid_date(prefix) {
        return None;
    }
    let body = text.get(end..)?.trim_start();
    (!body.is_empty()).then_some(text.len() - body.len())
}

fn valid_date(text: &str) -> bool {
    fn number(s: &str) -> Option<u32> {
        (!s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
            .then(|| s.parse().ok())
            .flatten()
    }
    fn year(s: &str) -> Option<u32> {
        (s.len() == 4)
            .then(|| number(s))
            .flatten()
            .filter(|y| *y > 0)
    }
    fn day_valid(y: u32, m: u32, d: u32) -> bool {
        let limit = match m {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if y.is_multiple_of(4) && (!y.is_multiple_of(100) || y.is_multiple_of(400)) {
                    29
                } else {
                    28
                }
            }
            _ => return false,
        };
        (1..=limit).contains(&d)
    }
    if year(text).is_some() {
        return true;
    }
    let parts = text.split('-').collect::<Vec<_>>();
    if (2..=3).contains(&parts.len()) && parts[1].len() == 2 {
        return year(parts[0]).zip(number(parts[1])).is_some_and(|(y, m)| {
            if parts.len() == 2 {
                (1..=12).contains(&m)
            } else {
                parts[2].len() == 2 && number(parts[2]).is_some_and(|d| day_valid(y, m, d))
            }
        });
    }
    let words = text.split_whitespace().collect::<Vec<_>>();
    if !(2..=3).contains(&words.len()) {
        return false;
    }
    let month = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ]
    .iter()
    .position(|name| {
        words[0].eq_ignore_ascii_case(name) || words[0].eq_ignore_ascii_case(&name[..3])
    });
    month
        .zip(year(words[words.len() - 1]))
        .is_some_and(|(m, y)| {
            words.len() == 2
                || number(words[1].trim_end_matches(','))
                    .is_some_and(|d| day_valid(y, m as u32 + 1, d))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn supporting_blocks_do_not_invent_dates_or_change_event_order() {
        for (source, expected) in [
            (
                "- 2026-09-10: Current.\n\n  ```sh\n  echo retained\n  ```\n\n- 2026-09-08: Earlier.\n\n  > Supporting evidence.\n",
                true,
            ),
            (
                "- **2026:** Current.\n\n  - A nested qualification.\n\n- **2024:** Earlier.\n",
                true,
            ),
            (
                "- 2026:\n\n  Supporting evidence.\n\n- 2025: Earlier.\n",
                false,
            ),
            (
                "- v2.0: Current.\n\n  Supporting evidence.\n\n- v1.0: Earlier.\n",
                false,
            ),
            (
                "- [ ] 2026: Planned.\n\n  Supporting evidence.\n\n- [x] 2025: Done.\n",
                false,
            ),
            (
                "1. 2026: First.\n\n   Supporting evidence.\n\n2. 2025: Second.\n",
                false,
            ),
        ] {
            let document = document_core::Document::from_markdown(source).unwrap();
            let projection = crate::TextProjection::from_snapshot(&document.snapshot());
            let BlockNode::List(list) = projection.roots().next().unwrap() else {
                panic!()
            };
            assert!(has_supporting_blocks(list));
            assert_eq!(is_timeline(list), expected, "{source}");
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        }
    }

    #[test]
    fn recognizes_dates_not_versions_or_unverified_tasks() {
        for date in [
            "2022",
            "2026-09",
            "2024-02-29",
            "September 8, 2026",
            "Sep 2026",
        ] {
            for separator in [": ", " — ", " – ", " - "] {
                let doc = document_core::Document::from_markdown(format!(
                    "- {date}{separator}Initial research.\n- {date}{separator}Public release.\n"
                ))
                .unwrap();
                let projection = crate::TextProjection::from_snapshot(&doc.snapshot());
                let BlockNode::List(list) = projection.roots().next().unwrap() else {
                    panic!()
                };
                assert!(is_timeline(list), "{date}{separator}");
            }
        }
        for date in [
            "2023-02-29",
            "2026-13",
            "2026-04-31",
            "v2026",
            "2.0",
            "3 days",
            "08/09/26",
            "0000",
        ] {
            assert!(!valid_date(date), "{date}");
        }
        for source in [
            "- [ ] 2026: Due.\n- [x] 2027: Done.\n",
            "1. 2026: First.\n2. 2027: Second.\n",
            "- 2026: First.\n- Unknown: Second.\n",
        ] {
            let doc = document_core::Document::from_markdown(source).unwrap();
            let projection = crate::TextProjection::from_snapshot(&doc.snapshot());
            let BlockNode::List(list) = projection.roots().next().unwrap() else {
                panic!()
            };
            assert!(!is_timeline(list));
        }
    }
}
