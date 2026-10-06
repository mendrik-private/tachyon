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

/// Space before a paragraph that begins inside a reading column: exactly one
/// line of its own leading. Every column then advances in whole lines, so
/// neighbouring columns share one baseline grid. Balancing and rendering both
/// use this value; they must never disagree about a column's height.
pub(crate) fn paragraph_gap(leading: f32) -> f32 {
    leading
}

/// Balance a finite set of two- or three-column bands. Every transition consumes lines.
/// Columns of a band differ by at most one line wherever a legal break allows it,
/// and a band's closing column is never taller than the columns before it.
/// Paragraph boundaries are preferred only within that one-line tolerance.
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
    let split = |end: usize| end < lines.len() && lines[end - 1].paragraph == lines[end].paragraph;
    // Column ends reachable from `start` with at least `minimum_lines` lines,
    // in increasing order, within the band height.
    let ends = |start: usize, minimum_lines: usize| {
        (start + minimum_lines..=lines.len())
            .take_while(move |&end| height(start, end) <= height_limit + 0.5)
            .filter(move |&end| safe(end))
    };
    // One line of the passage's leading is the balancing unit.
    let unit = lines.iter().map(|line| line.height).fold(1., f32::max);
    let total = *prefix.last()?;
    let minimum =
        ((total / (band_columns as f32 * height_limit)).ceil() as usize).max(1) * band_columns;
    // Try the minimum band count first; do not create extra sparse bands for
    // a marginal improvement in balance. More bands are an overflow escape.
    for columns in (minimum..=(lines.len() + 1) / 4)
        .step_by(band_columns)
        .take(4)
    {
        let bands = columns / band_columns;
        let target = total / columns as f32;
        // Best cost of laying out lines[..end] in `band` complete bands, and
        // the column starts of the band that closes there.
        let mut costs = vec![vec![f32::INFINITY; lines.len() + 1]; bands + 1];
        let mut previous = vec![vec![[0; 3]; lines.len() + 1]; bands + 1];
        costs[0][0] = 0.;
        for band in 1..=bands {
            let closing = band == bands;
            // The last column may close with three lines. All other columns
            // retain the useful four-line minimum; safe() still protects both
            // sides of every paragraph split, regardless of balance.
            let last_lines = if closing { 3 } else { 4 };
            for start in 0..lines.len() {
                let base = costs[band - 1][start];
                if !base.is_finite() {
                    continue;
                }
                let mut consider = |starts: [usize; 3], end: usize| {
                    if closing && end != lines.len() {
                        return;
                    }
                    let mut heights = [0.; 3];
                    let mut splits = 0;
                    for column in 0..band_columns {
                        let to = if column + 1 == band_columns {
                            end
                        } else {
                            starts[column + 1]
                        };
                        heights[column] = height(starts[column], to);
                        splits += usize::from(split(to));
                    }
                    let heights = &heights[..band_columns];
                    let last = heights[band_columns - 1];
                    // Closing columns must fit below every preceding column
                    // in this band, including paragraph gaps. A balance score
                    // must never buy its way out of this typesetting rule.
                    if heights.iter().any(|&h| last > h + 0.5) {
                        return;
                    }
                    let tallest = heights.iter().copied().fold(0., f32::max);
                    let shortest = heights.iter().copied().fold(f32::INFINITY, f32::min);
                    let spread = (tallest - shortest) / unit;
                    // Any spread beyond one line outweighs every preference;
                    // within it, avoiding a paragraph split outweighs exact
                    // equality, and exact equality outweighs band evenness.
                    let excess = (spread - 1. - 0.5 / unit).max(0.);
                    let evenness = heights
                        .iter()
                        .map(|h| ((h - target) / unit).powi(2))
                        .sum::<f32>();
                    let cost = base
                        + 100. * excess
                        + 0.08 * splits as f32
                        + 0.01 * spread
                        + 0.001 * evenness;
                    if cost < costs[band][end] {
                        costs[band][end] = cost;
                        previous[band][end] = starts;
                    }
                };
                for second in ends(start, 4) {
                    if band_columns == 2 {
                        for end in ends(second, last_lines) {
                            consider([start, second, 0], end);
                        }
                        continue;
                    }
                    for third in ends(second, 4) {
                        for end in ends(third, last_lines) {
                            consider([start, second, third], end);
                        }
                    }
                }
            }
        }
        if costs[bands][lines.len()].is_finite() {
            let mut end = lines.len();
            let mut result = Vec::with_capacity(columns);
            for band in (1..=bands).rev() {
                let starts = previous[band][end];
                result.extend(starts[..band_columns].iter().rev());
                end = starts[0];
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
    use crate::theme::DocumentStyle;
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

    /// Synthetic paragraphs of `counts` lines with one-line paragraph gaps.
    fn passage(counts: &[usize], leading: f32) -> Vec<Line> {
        counts
            .iter()
            .enumerate()
            .flat_map(|(paragraph, &count)| {
                (0..count).map(move |index| Line {
                    height: leading,
                    gap: if paragraph > 0 && index == 0 {
                        paragraph_gap(leading)
                    } else {
                        0.
                    },
                    paragraph,
                })
            })
            .collect()
    }

    /// Whole lines of leading in each column, counting a paragraph gap that
    /// falls inside a column as one line.
    fn column_lines(lines: &[Line], starts: &[usize], leading: f32) -> Vec<usize> {
        starts
            .iter()
            .enumerate()
            .map(|(i, &start)| {
                let end = starts.get(i + 1).copied().unwrap_or(lines.len());
                let height = lines[start..end]
                    .iter()
                    .map(|l| l.height + l.gap)
                    .sum::<f32>()
                    - lines[start].gap;
                let count = (height / leading).round();
                assert!((height - count * leading).abs() < 0.01, "off-grid {height}");
                count as usize
            })
            .collect()
    }

    #[test]
    fn a_short_final_column_is_better_than_a_bottom_heavy_band() {
        // Splitting the middle paragraph 2/3 gives seven lines (including
        // one paragraph gap) in each column. The paragraph boundary after
        // four lines would leave a nine-line closing column, which the
        // reading convention forbids however balanced the alternative.
        let lines = passage(&[4, 5, 3], 28.);
        let starts = breaks(&lines, 560., 2).unwrap();
        assert_eq!(starts, [0, 6]);
        assert_eq!(column_lines(&lines, &starts, 28.), [7, 7]);
    }

    #[test]
    fn columns_balance_to_one_line_on_a_shared_grid() {
        let leading = DocumentStyle::BODY_LEADING;
        for (counts, columns) in [
            (vec![5, 6, 9], 3),
            (vec![8, 7], 2),
            (vec![12, 3, 7], 2),
            (vec![6, 6, 6, 6], 3),
            (vec![10, 10, 10], 2),
        ] {
            let lines = passage(&counts, leading);
            let starts = breaks(&lines, 560., columns).unwrap();
            assert_eq!(starts.len(), columns, "{counts:?}: one band");
            let heights = column_lines(&lines, &starts, leading);
            let tallest = *heights.iter().max().unwrap();
            let shortest = *heights.iter().min().unwrap();
            assert!(tallest - shortest <= 1, "{counts:?}: {heights:?}");
            assert!(
                heights[..columns - 1]
                    .iter()
                    .all(|h| *heights.last().unwrap() <= *h),
                "{counts:?}: final column taller than an earlier one: {heights:?}"
            );
        }
        // Seven then eight lines cannot balance under the reading rules:
        // breaking at the paragraph leaves a taller (8) closing column, one
        // line further is an orphan, so the only legal band is 10 beside 6.
        // The planner's narrower-measure retry exists for exactly this case.
        let lines = passage(&[7, 8], leading);
        let starts = breaks(&lines, 560., 2).unwrap();
        assert_eq!(column_lines(&lines, &starts, leading), [10, 6]);
    }

    #[test]
    fn balance_matches_the_best_legal_band() {
        // Exhaustive oracle for single bands: whenever any legal set of
        // breaks (widow control, minimum column lines, closing column not
        // taller) balances to one line, the planner must find one.
        let leading = 28.;
        let legal = |lines: &[Line], index: usize| {
            index == 0
                || index == lines.len()
                || lines[index - 1].paragraph != lines[index].paragraph
                || (index >= 2
                    && index + 2 <= lines.len()
                    && lines[index - 2].paragraph == lines[index].paragraph
                    && lines[index + 1].paragraph == lines[index].paragraph)
        };
        let mut checked = 0;
        for a in 2..9 {
            for b in 1..8 {
                for c in [0, 1, 3, 6] {
                    let counts = [a, b, c]
                        .into_iter()
                        .filter(|&count| count > 0)
                        .collect::<Vec<_>>();
                    let lines = passage(&counts, leading);
                    for columns in [2, 3] {
                        let n = lines.len();
                        if n < 4 * columns {
                            assert!(breaks(&lines, 560., columns).is_none());
                            continue;
                        }
                        let mut best: Option<usize> = None;
                        let mut visit = |starts: &[usize]| {
                            let mut bounds = starts.to_vec();
                            bounds.push(n);
                            let sizes_ok = bounds
                                .windows(2)
                                .enumerate()
                                .all(|(i, w)| w[1] - w[0] >= if i + 1 == columns { 3 } else { 4 });
                            if !sizes_ok || !starts.iter().all(|&s| legal(&lines, s)) {
                                return;
                            }
                            let heights = column_lines(&lines, starts, leading);
                            if heights.iter().any(|h| *h as f32 * leading > 560.5)
                                || heights[..columns - 1]
                                    .iter()
                                    .any(|h| heights[columns - 1] > *h)
                            {
                                return;
                            }
                            let spread =
                                heights.iter().max().unwrap() - heights.iter().min().unwrap();
                            best = Some(best.map_or(spread, |b| b.min(spread)));
                        };
                        for first in 1..n {
                            if columns == 2 {
                                visit(&[0, first]);
                            } else {
                                for second in first + 1..n {
                                    visit(&[0, first, second]);
                                }
                            }
                        }
                        let found = breaks(&lines, 560., columns);
                        let Some(best) = best else {
                            continue;
                        };
                        let starts = found.unwrap_or_else(|| panic!("{counts:?}/{columns}"));
                        assert_eq!(starts.len(), columns);
                        let heights = column_lines(&lines, &starts, leading);
                        let spread = heights.iter().max().unwrap() - heights.iter().min().unwrap();
                        assert!(
                            heights[..columns - 1]
                                .iter()
                                .all(|h| heights[columns - 1] <= *h),
                            "{counts:?}/{columns}: {heights:?}"
                        );
                        assert!(
                            spread <= best.max(1),
                            "{counts:?}/{columns}: {heights:?}, best spread {best}"
                        );
                        checked += 1;
                    }
                }
            }
        }
        assert!(checked > 100, "the oracle must exercise real bands");
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
