//! Source-linked reading bands, independent of the native font/paint system.
use document_core::{ImageNode, NodeId};
use std::{ops::Range, sync::Arc};

/// Authored photographic/illustrative roles can accompany narrative text.
/// A technical/evidence role always wins; adjacency or file type is not proof.
pub(crate) fn supporting_image(image: &ImageNode) -> bool {
    if image.alt.len() > 4096
        || image.title.as_ref().is_some_and(|title| title.len() > 4096)
        || image.source.len() > 4096
        || image.link.is_some()
    {
        return false;
    }
    let description = format!(
        "{} {}",
        image.alt.as_string(),
        image.title.as_deref().unwrap_or("")
    )
    .to_lowercase();
    let mut supporting = false;
    for word in description.split(|c: char| !c.is_alphabetic()) {
        if matches!(
            word,
            "diagram" | "chart" | "map" | "screenshot" | "schema" | "evidence" | "essential"
        ) {
            return false;
        }
        supporting |= matches!(
            word,
            "photo" | "photograph" | "photography" | "portrait" | "illustration"
        );
    }
    supporting
}

/// An optional supporting figure followed by its source-contiguous prose.
/// The two text regions are beside the figure and below it, not peer columns.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct FigureFlow {
    pub text: Flow,
    pub image_source: String,
    pub dimensions: (u32, u32),
    pub height: f32,
    pub image_height: f32,
    pub labels: Vec<NodeId>,
    pub label_revisions: Vec<document_core::Revision>,
}

impl FigureFlow {
    pub fn nodes(&self) -> impl Iterator<Item = NodeId> + '_ {
        std::iter::once(self.text.group)
            .chain(self.labels.iter().copied())
            .chain(self.text.sources.iter().map(|(id, _)| *id))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Flow {
    pub group: NodeId,
    pub canvas: f32,
    pub columns: usize,
    /// Held geometry may grow, but must be reconsidered after editing ends.
    pub needs_balance: bool,
    pub sources: Vec<(NodeId, Arc<str>)>,
    pub revisions: Vec<document_core::Revision>,
    /// First source byte in each column, in canonical reading order.
    pub starts: Vec<(NodeId, usize)>,
}

impl Flow {
    /// Advance held anchors after each edit, so two distant edits do not look
    /// like one replacement spanning all the unchanged text between them.
    pub fn rebased(
        &self,
        node: NodeId,
        current: &str,
        revision: document_core::Revision,
    ) -> Option<Self> {
        let ordinal = self.sources.iter().position(|(id, _)| *id == node)?;
        let before = &self.sources[ordinal].1;
        if before.as_ref() == current && self.revisions[ordinal] == revision {
            return None;
        }
        let mut next = self.clone();
        next.needs_balance = true;
        for (id, offset) in &mut next.starts {
            if *id == node {
                *offset = rebase_boundary(before, current, *offset);
            }
        }
        next.sources[ordinal].1 = Arc::from(current);
        next.revisions[ordinal] = revision;
        Some(next)
    }

    pub fn fragments(&self, node: NodeId, current: &str) -> Vec<(Range<usize>, usize)> {
        let Some(ordinal) = self.sources.iter().position(|(id, _)| *id == node) else {
            return Vec::new();
        };
        let before = &self.sources[ordinal].1;
        let mut column = 0;
        let mut start = 0;
        let mut result = Vec::new();
        for (index, &(id, offset)) in self.starts.iter().enumerate().skip(1) {
            let Some(owner) = self
                .sources
                .iter()
                .position(|(candidate, _)| *candidate == id)
            else {
                continue;
            };
            if owner < ordinal {
                column = index;
                continue;
            }
            if owner > ordinal {
                break;
            }
            let offset = rebase_boundary(before, current, offset);
            if offset > start {
                result.push((start..offset, column));
            }
            start = offset;
            column = index;
        }
        // An empty paragraph remains a real caret host.
        if start < current.len() || result.is_empty() {
            result.push((start..current.len(), column));
        }
        result
    }
}

/// A held boundary follows unchanged text, not a new balancing decision.
/// Changes spanning a boundary attach the replacement to its preceding flow.
fn rebase_boundary(before: &str, after: &str, offset: usize) -> usize {
    if offset == 0 {
        return 0;
    }
    if before == after {
        return offset.min(after.len());
    }
    let prefix = before
        .chars()
        .zip(after.chars())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    let suffix = before[prefix..]
        .chars()
        .rev()
        .zip(after[prefix..].chars().rev())
        .take_while(|(a, b)| a == b)
        .map(|(c, _)| c.len_utf8())
        .sum::<usize>();
    if offset <= prefix {
        offset
    } else if offset >= before.len() - suffix {
        after.len() - (before.len() - offset)
    } else {
        after.len() - suffix
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Line {
    pub height: f32,
    pub gap: f32,
    pub paragraph: usize,
}

/// Balance a finite set of two- or three-column bands. Every transition consumes lines;
/// the bounded search prefers paragraph boundaries and avoids stranded lines.
pub(crate) fn breaks(lines: &[Line], height_limit: f32, band_columns: usize) -> Option<Vec<usize>> {
    if !(2..=3).contains(&band_columns)
        || lines.len() < 4 * band_columns
        || lines.len() > 2048
        || !height_limit.is_finite()
        || height_limit <= 0.
    {
        return None;
    }
    let mut prefix = vec![0.];
    for line in lines {
        prefix.push(prefix.last()? + line.height + line.gap);
    }
    let height = |a: usize, b: usize| prefix[b] - prefix[a] - lines[a].gap;
    let safe = |index: usize| {
        if index == 0
            || index == lines.len()
            || lines[index - 1].paragraph != lines[index].paragraph
        {
            return true;
        }
        // Two lines on each side prevent widows and orphans without forcing
        // a four- or five-line paragraph to stay entirely in one column.
        index >= 2
            && index + 2 <= lines.len()
            && lines[index - 2].paragraph == lines[index].paragraph
            && lines[index + 1].paragraph == lines[index].paragraph
    };
    let minimum = ((*prefix.last()? / (band_columns as f32 * height_limit)).ceil() as usize).max(1)
        * band_columns;
    // Try the minimum band count first; do not create extra sparse bands for
    // a marginal improvement in balance. More bands are an overflow escape.
    for columns in (minimum..=(lines.len() + 1) / 4)
        .step_by(band_columns)
        .take(4)
    {
        let target = *prefix.last()? / columns as f32;
        let mut costs = vec![vec![f32::INFINITY; lines.len() + 1]; columns + 1];
        let mut previous = vec![vec![0; lines.len() + 1]; columns + 1];
        costs[0][0] = 0.;
        for count in 1..=columns {
            // The last column may close with three lines. All other columns
            // retain the useful four-line minimum; safe() still protects both
            // sides of every paragraph split, regardless of balance.
            let minimum_lines = if count == columns { 3 } else { 4 };
            for end in (count - 1) * 4 + minimum_lines..=lines.len() {
                if !safe(end) {
                    continue;
                }
                for start in ((count - 1) * 4..=end - minimum_lines).rev() {
                    let h = height(start, end);
                    if h > height_limit + 0.5 {
                        break;
                    }
                    if !safe(start) || !costs[count - 1][start].is_finite() {
                        continue;
                    }
                    // Closing columns must fit below every preceding column
                    // in this band, including paragraph gaps. A balance score
                    // must never buy its way out of this typesetting rule.
                    if count % band_columns == 0 {
                        let mut boundary = start;
                        let mut fits = true;
                        for earlier in (count - band_columns + 1..count).rev() {
                            let before = previous[earlier][boundary];
                            if h > height(before, boundary) + 0.5 {
                                fits = false;
                                break;
                            }
                            boundary = before;
                        }
                        if !fits {
                            continue;
                        }
                    }
                    let split =
                        if end < lines.len() && lines[end - 1].paragraph == lines[end].paragraph {
                            0.08
                        } else {
                            0.
                        };
                    let cost =
                        costs[count - 1][start] + ((h - target) / target.max(1.)).powi(2) + split;
                    if cost < costs[count][end] {
                        costs[count][end] = cost;
                        previous[count][end] = start;
                    }
                }
            }
        }
        if costs[columns][lines.len()].is_finite() {
            let mut start = lines.len();
            let mut result = Vec::with_capacity(columns);
            for count in (1..=columns).rev() {
                start = previous[count][start];
                result.push(start);
            }
            result.reverse();
            return Some(result);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_column_never_exceeds_earlier_columns() {
        for (counts, columns, expected) in [
            // Split the middle paragraph 2/2, giving seven lines on the
            // left and five on the right, with one paragraph gap in each.
            (vec![5, 4, 3], 2, vec![0, 7]),
            (vec![5, 4, 4], 3, vec![0, 5, 9]),
        ] {
            let lines = counts
                .into_iter()
                .enumerate()
                .flat_map(|(paragraph, count)| {
                    (0..count).map(move |index| Line {
                        height: 28.,
                        gap: if paragraph > 0 && index == 0 { 24. } else { 0. },
                        paragraph,
                    })
                })
                .collect::<Vec<_>>();
            assert_eq!(breaks(&lines, 560., columns).unwrap(), expected);
        }
        for columns in [2, 3] {
            for count in 12..100 {
                let lines = (0..count)
                    .map(|i| Line {
                        height: 24. + (i / 7 % 3) as f32 * 4.,
                        gap: if i > 0 && i % 7 == 0 { 24. } else { 0. },
                        paragraph: i / 7,
                    })
                    .collect::<Vec<_>>();
                if let Some(starts) = breaks(&lines, 400., columns) {
                    let heights = starts
                        .iter()
                        .enumerate()
                        .map(|(i, &start)| {
                            let end = starts.get(i + 1).copied().unwrap_or(count);
                            lines[start..end]
                                .iter()
                                .map(|l| l.height + l.gap)
                                .sum::<f32>()
                                - lines[start].gap
                        })
                        .collect::<Vec<_>>();
                    for band in heights.chunks(columns) {
                        assert!(
                            band[..columns - 1]
                                .iter()
                                .all(|h| *band.last().unwrap() <= *h + 0.5),
                            "{heights:?}"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn a_short_final_column_is_better_than_a_bottom_heavy_band() {
        let lines = [4, 5, 3]
            .into_iter()
            .enumerate()
            .flat_map(|(paragraph, count)| {
                (0..count).map(move |index| Line {
                    height: 28.,
                    gap: if paragraph > 0 && index == 0 { 24. } else { 0. },
                    paragraph,
                })
            })
            .collect::<Vec<_>>();
        assert_eq!(breaks(&lines, 560., 2).unwrap(), [0, 6]);
    }

    #[test]
    fn three_column_bands_balance_without_sparse_columns_or_widows() {
        for count in [12, 24, 36, 72, 144, 288] {
            let lines = vec![
                Line {
                    height: 28.,
                    gap: 0.,
                    paragraph: 0
                };
                count
            ];
            let starts = breaks(&lines, 400., 3).expect("long prose supports three columns");
            assert_eq!(starts[0], 0);
            assert_eq!(starts.len() % 3, 0);
            for pair in starts
                .into_iter()
                .chain([count])
                .collect::<Vec<_>>()
                .windows(2)
            {
                assert!(pair[1] - pair[0] >= 4);
                assert!((pair[1] - pair[0]) as f32 * 28. <= 400.5);
            }
        }
        let short = vec![
            Line {
                height: 28.,
                gap: 0.,
                paragraph: 0
            };
            8
        ];
        assert!(breaks(&short, 400., 3).is_none());
        assert!(breaks(&short, 400., 2).is_some());
        assert!(breaks(&short, 400., 0).is_none());
        assert!(breaks(&short, 400., usize::MAX).is_none());
    }
    #[test]
    fn balanced_bands_keep_all_lines_and_avoid_widows() {
        for count in 8..130 {
            let lines = (0..count)
                .map(|i| Line {
                    height: 28.,
                    gap: if i % 11 == 0 && i > 0 { 24. } else { 0. },
                    paragraph: i / 11,
                })
                .collect::<Vec<_>>();
            if let Some(starts) = breaks(&lines, 400., 2) {
                assert_eq!(starts[0], 0);
                assert_eq!(starts.len() % 2, 0);
                for pair in starts
                    .iter()
                    .copied()
                    .chain([count])
                    .collect::<Vec<_>>()
                    .windows(2)
                {
                    assert!(pair[1] - pair[0] >= if pair[1] == count { 3 } else { 4 });
                    for &boundary in pair {
                        if boundary > 0
                            && boundary < count
                            && lines[boundary - 1].paragraph == lines[boundary].paragraph
                        {
                            let paragraph = lines[boundary].paragraph;
                            assert!(
                                lines[..boundary]
                                    .iter()
                                    .rev()
                                    .take_while(|line| line.paragraph == paragraph)
                                    .count()
                                    >= 2
                            );
                            assert!(
                                lines[boundary..]
                                    .iter()
                                    .take_while(|line| line.paragraph == paragraph)
                                    .count()
                                    >= 2
                            );
                        }
                    }
                    assert!(
                        lines[pair[0]..pair[1]]
                            .iter()
                            .map(|l| l.height + l.gap)
                            .sum::<f32>()
                            - lines[pair[0]].gap
                            <= 400.5
                    );
                }
            }
        }
        // A vacuous "all returned layouts fit" check must not allow a
        // planner that rejects every valid reading passage.
        for count in [8, 12, 24, 48, 96, 192] {
            let lines = vec![
                Line {
                    height: 28.,
                    gap: 0.,
                    paragraph: 0
                };
                count
            ];
            let starts = breaks(&lines, 400., 2).expect("a homogeneous long paragraph can flow");
            assert_eq!(
                starts.len(),
                ((count as f32 * 28. / 800.).ceil() as usize).max(1) * 2
            );
        }
    }
    #[test]
    fn held_boundaries_follow_unicode_edits_without_slicing_characters() {
        assert_eq!(
            rebase_boundary("café followed", "東京 café followed", 6),
            13
        );
        assert_eq!(rebase_boundary("abcdef", "abXYZef", 3), 5);
        assert_eq!(rebase_boundary("abc", "xabc", 0), 0);
    }

    #[test]
    fn successive_distant_edits_leave_the_middle_anchor_attached() {
        let node = NodeId::new_unchecked(1);
        let flow = Flow {
            group: node,
            canvas: 1000.,
            columns: 2,
            needs_balance: false,
            sources: vec![(node, Arc::from("alpha bravo charlie delta"))],
            revisions: vec![document_core::Revision(0)],
            starts: vec![(node, 0), (node, 6), (node, 12), (node, 20)],
        };
        let first = flow
            .rebased(
                node,
                "new alpha bravo charlie delta",
                document_core::Revision(1),
            )
            .unwrap();
        let second = first
            .rebased(
                node,
                "new alpha bravo charlie long delta",
                document_core::Revision(2),
            )
            .unwrap();
        assert_eq!(
            second.starts,
            vec![(node, 0), (node, 10), (node, 16), (node, 24)]
        );
        assert_eq!(
            second
                .fragments(node, &second.sources[0].1)
                .iter()
                .map(|(range, _)| &second.sources[0].1[range.clone()])
                .collect::<String>(),
            second.sources[0].1.as_ref()
        );
    }
}
