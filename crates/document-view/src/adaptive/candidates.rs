//! Measured internal list candidates. Selection never touches document content.

use super::{CARD_PADDING, LAYOUT_GAP, ListLayout, PROSE_WIDTH};

#[derive(Clone, Copy, Debug, serde::Serialize)]
pub(crate) struct ItemMeasurement {
    pub lines: usize,
    pub height: f32,
    pub preferred_width: f32,
    pub overflow: bool,
}

/// The seven policy terms in specification §12, each normalized before weighting.
#[derive(Clone, Copy, Debug, Default, PartialEq, serde::Serialize)]
pub(crate) struct Penalties {
    pub discomfort: f32,
    pub separation: f32,
    pub reading_jump: f32,
    pub imbalance: f32,
    pub complexity: f32,
    pub unused_width: f32,
    pub change: f32,
}

impl Penalties {
    pub fn weighted(self) -> f32 {
        [
            (self.discomfort, 8.),
            (self.separation, 5.),
            (self.reading_jump, 5.),
            (self.imbalance, 2.),
            (self.complexity, 2.),
            (self.unused_width, 1.),
            (self.change, 6.),
        ]
        .into_iter()
        .map(|(term, weight)| term.clamp(0., 1.) * weight)
        .sum()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
pub(crate) enum Rejection {
    ColumnTooNarrow,
    MeasurementUnavailable,
    TooManyLines,
    UnevenHeights,
    Overflow,
}

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct ListCandidate {
    pub columns: usize,
    pub row_columns: Vec<usize>,
    pub width: f32,
    pub items: Vec<ItemMeasurement>,
    /// One normalized breakdown per internal row, including the stack's rows.
    pub rows: Vec<Penalties>,
    pub rejected: Option<Rejection>,
}

impl ListCandidate {
    pub fn cost(&self) -> f32 {
        self.rows.iter().map(|row| row.weighted()).sum::<f32>() / self.rows.len().max(1) as f32
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct ListDecision {
    pub layout: ListLayout,
    pub row_columns: Vec<usize>,
    pub candidates: Vec<ListCandidate>,
    pub retained_previous: bool,
}

impl ListDecision {
    /// Source index to explicit row, column, and row capacity.
    pub fn placement(&self, mut item: usize) -> Option<(usize, usize, usize)> {
        for (row, &columns) in self.row_columns.iter().enumerate() {
            if item < columns {
                return Some((row, item, columns));
            }
            item -= columns;
        }
        None
    }

    pub fn is_valid(&self, count: usize, canvas: f32, previous: Option<&Self>) -> bool {
        if self.retained_previous
            && !previous
                .is_some_and(|old| old.layout == self.layout && old.row_columns == self.row_columns)
        {
            return false;
        }
        let ListLayout::Grid(columns) = self.layout else {
            return self.layout == ListLayout::List;
        };
        self.candidates.iter().any(|candidate| {
            candidate.columns == columns
                && candidate.row_columns == self.row_columns
                && candidate.rejected.is_none()
                && candidate.items.len() == count
                && candidate.rows.len() == self.row_columns.len()
                && self.placement(count.saturating_sub(1)).is_some()
                && self.row_columns.iter().all(|&n| (2..=4).contains(&n))
                && (candidate.width * columns as f32 + LAYOUT_GAP * (columns - 1) as f32 - canvas)
                    .abs()
                    < 0.01
        })
    }
}

/// Exact 12-track geometry; no integer rounding or breakpoint substitution.
pub(crate) fn span_width(canvas: f32, span: u8) -> Option<f32> {
    if !canvas.is_finite() || canvas <= 0. || !(1..=12).contains(&span) {
        return None;
    }
    let track = (canvas - 11. * LAYOUT_GAP) / 12.;
    let width = f32::from(span) * track + f32::from(span - 1) * LAYOUT_GAP;
    (width > 0.).then_some(width)
}

pub(crate) fn choose_list(
    count: usize,
    canvas: f32,
    prose_width: f32,
    previous: Option<&ListDecision>,
    labeled: bool,
    allow_four: bool,
    mut measure: impl FnMut(usize, f32, bool) -> Option<ItemMeasurement>,
) -> ListDecision {
    let max_columns = if allow_four { 4 } else { 3 };
    let shapes = (1..=count.min(max_columns))
        .map(|columns| vec![columns; count.div_ceil(columns)])
        .collect::<Vec<_>>();
    // A collection owns one set of column anchors. Keep the final row's
    // unused tracks instead of widening its items into a different grid.
    let mut measured = vec![[None; 4]; count];
    let mut candidates = Vec::with_capacity(shapes.len());
    for row_columns in shapes {
        let columns = *row_columns.iter().max().unwrap();
        // Measure at the same loaded-font width used by final geometry.
        // The content's intrinsic width must never include its outside margin.
        let width = if columns == 1 {
            canvas.clamp(
                1.,
                if prose_width.is_finite() && prose_width > 0. {
                    prose_width.max(1.)
                } else {
                    PROSE_WIDTH
                },
            )
        } else {
            span_width(canvas, (12 / columns) as u8).unwrap_or(1.)
        };
        let mut candidate = ListCandidate {
            columns,
            row_columns,
            width,
            items: Vec::with_capacity(count),
            rows: Vec::new(),
            rejected: None,
        };
        // Reserve a comfortable text column in addition to marker/card insets.
        // The subsequent native measurements, not this guard, decide fit.
        if columns > 1
            && (canvas < crate::theme::DocumentStyle::SINGLE_COLUMN_WIDTH
                || width - 2. * CARD_PADDING - 32. < 180.)
        {
            candidate.rejected = Some(Rejection::ColumnTooNarrow);
        } else {
            for (index, widths) in measured.iter_mut().enumerate() {
                let mut offset = index;
                let row_columns = *candidate
                    .row_columns
                    .iter()
                    .find(|&&n| {
                        if offset < n {
                            true
                        } else {
                            offset -= n;
                            false
                        }
                    })
                    .unwrap();
                let item_width = if row_columns == 1 {
                    width
                } else {
                    span_width(canvas, (12 / row_columns) as u8).unwrap_or(1.)
                };
                let Some(item) = *widths[row_columns - 1]
                    .get_or_insert_with(|| measure(index, item_width, row_columns > 1))
                else {
                    candidate.rejected = Some(Rejection::MeasurementUnavailable);
                    break;
                };
                if !item.height.is_finite()
                    || !item.preferred_width.is_finite()
                    || item.height <= 0.
                    || item.preferred_width < 0.
                {
                    candidate.rejected = Some(Rejection::MeasurementUnavailable);
                    break;
                }
                candidate.items.push(item);
                // Four tracks are a compact open-feature vocabulary, not a
                // way to squeeze explanations into more columns. The actual
                // assigned width must keep each of these cells on one line.
                if row_columns == 4 && item.lines > 1 {
                    candidate.rejected = Some(Rejection::TooManyLines);
                    break;
                }
            }
        }
        if candidate.rejected.is_none() && columns > 1 {
            let shortest = candidate
                .items
                .iter()
                .map(|i| i.height)
                .fold(f32::INFINITY, f32::min);
            let tallest = candidate.items.iter().map(|i| i.height).fold(0., f32::max);
            candidate.rejected = if candidate.items.iter().any(|i| i.overflow) {
                Some(Rejection::Overflow)
            } else if candidate
                .items
                .iter()
                // Entity cards include a separate authored title. Four body
                // lines plus that title are still bounded, scannable content;
                // overflow and measured height balance remain hard gates.
                .any(|i| i.lines > if labeled { 5 } else { 3 })
            {
                Some(Rejection::TooManyLines)
            } else if tallest > shortest * 1.5 + 0.001 {
                Some(Rejection::UnevenHeights)
            } else {
                None
            };
        }
        if candidate.rejected.is_none() {
            let mut start = 0;
            for &columns in &candidate.row_columns {
                let row = &candidate.items[start..(start + columns).min(count)];
                start += row.len();
                let width = if columns == 1 {
                    width
                } else {
                    span_width(canvas, (12 / columns) as u8).unwrap_or(1.)
                };
                let height = row.iter().map(|i| i.height).fold(0., f32::max);
                // Unoccupied final tracks preserve the collection's anchors;
                // they are not unequal item heights or stretched whitespace.
                let area = height * row.len() as f32;
                let used = row.iter().map(|i| i.height).sum::<f32>();
                candidate.rows.push(Penalties {
                    // The entity allowance includes its separate title line.
                    // Score the same line budget used by the hard fit gate.
                    discomfort: if columns > 1 {
                        row.iter()
                            .map(|i| {
                                i.lines.saturating_sub(if labeled { 5 } else { 3 }) as f32 / 2.
                            })
                            .fold(0., f32::max)
                    } else {
                        0.
                    },
                    // Rows are source-contiguous; nothing is detached or moved.
                    separation: 0.,
                    reading_jump: 0.,
                    // Natural card heights need not be equal. Small differences
                    // within the hard 1.5x fit limit should not outweigh a
                    // readable, compact grid just to save one wrapped line.
                    imbalance: ((area - used) / area.max(1.)).powi(2),
                    // Authored labels are already a strong grouping signal,
                    // so a compact grid adds less interpretive complexity.
                    complexity: (columns - 1) as f32 / columns as f32 * 0.03,
                    // List cells (unlike prose margins) offer useful space for
                    // peers. Intrinsic preferred widths come from actual fonts.
                    unused_width: row
                        .iter()
                        .map(|i| {
                            let comfortable = ((i.preferred_width - 2. * CARD_PADDING).max(0.)
                                / if labeled { 3. } else { 2. }
                                + 2. * CARD_PADDING)
                                .max(220.);
                            // Unlike sustained prose, a nominated independent
                            // feature group can use the full canvas for peers.
                            // Do not reward a stack for declaring that space
                            // unavailable. Hard fit/balance gates still decide
                            // whether any grid can compete at all.
                            (1. - comfortable / if columns == 1 { canvas } else { width })
                                .clamp(0., 1.)
                        })
                        .sum::<f32>()
                        / row.len() as f32,
                    change: previous.map_or(0., |old| {
                        if old.row_columns == candidate.row_columns {
                            0.
                        } else {
                            0.5
                        }
                    }),
                });
            }
        }
        candidates.push(candidate);
    }
    let is_previous = |candidate: &ListCandidate| {
        previous.is_some_and(|old| old.row_columns == candidate.row_columns)
    };
    let legal = |c: &&ListCandidate| c.rejected.is_none();
    let best = candidates.iter().filter(legal).min_by(|a, b| {
        a.cost()
            .total_cmp(&b.cost())
            .then_with(|| is_previous(b).cmp(&is_previous(a)))
            .then_with(|| a.columns.cmp(&b.columns))
            .then_with(|| b.row_columns.cmp(&a.row_columns))
    });
    let previous = candidates.iter().filter(legal).find(|c| is_previous(c));
    let winner = {
        match (best, previous) {
            (Some(best), Some(old))
                if old.cost() - best.cost() < old.cost().abs() * 0.1 + 0.001 =>
            {
                Some(old)
            }
            _ => best,
        }
    };
    let columns = winner.map_or(1, |c| c.columns);
    ListDecision {
        layout: if columns == 1 {
            ListLayout::List
        } else {
            ListLayout::Grid(columns)
        },
        row_columns: winner.map_or_else(|| vec![1; count], |c| c.row_columns.clone()),
        retained_previous: winner.is_some_and(is_previous),
        candidates,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn twelve_track_widths_cover_the_canvas_exactly() {
        for canvas in [360., 768., 1280., 1920.] {
            for spans in [
                &[12][..],
                &[6, 6],
                &[4, 8],
                &[8, 4],
                &[5, 7],
                &[7, 5],
                &[4, 4, 4],
            ] {
                let sum = spans
                    .iter()
                    .map(|s| span_width(canvas, *s).unwrap())
                    .sum::<f32>()
                    + (spans.len() - 1) as f32 * LAYOUT_GAP;
                assert!((sum - canvas).abs() < 0.001);
            }
        }
        assert!(span_width(f32::NAN, 6).is_none());
        assert!(span_width(300., 0).is_none());
    }

    #[test]
    fn measurements_reject_long_uneven_overflowing_and_unavailable_candidates() {
        let measured = |lines, height, overflow| {
            Some(ItemMeasurement {
                lines,
                height,
                overflow,
                preferred_width: 110.,
            })
        };
        assert_eq!(
            choose_list(6, 1100., PROSE_WIDTH, None, false, false, |_, _, _| {
                measured(1, 28., false)
            })
            .layout,
            ListLayout::Grid(3)
        );
        for reason in [
            Rejection::TooManyLines,
            Rejection::UnevenHeights,
            Rejection::Overflow,
            Rejection::MeasurementUnavailable,
        ] {
            let decision = choose_list(
                6,
                1100.,
                PROSE_WIDTH,
                None,
                false,
                false,
                |index, _, cards| {
                    if !cards || index != 0 {
                        return measured(1, 28., false);
                    }
                    match reason {
                        Rejection::TooManyLines => measured(6, 168., false),
                        Rejection::UnevenHeights => measured(2, 56., false),
                        Rejection::Overflow => measured(1, 28., true),
                        _ => None,
                    }
                },
            );
            assert_eq!(decision.layout, ListLayout::List);
            assert_eq!(decision.candidates[2].rejected, Some(reason));
        }
    }

    #[test]
    fn legal_previous_layout_is_stable_but_invalid_width_escapes_immediately() {
        let measure = |_, _, _| {
            Some(ItemMeasurement {
                lines: 1,
                height: 28.,
                preferred_width: 110.,
                overflow: false,
            })
        };
        let old = choose_list(6, 1100., PROSE_WIDTH, None, false, false, measure);
        let stable = choose_list(6, 1098., PROSE_WIDTH, Some(&old), false, false, measure);
        assert_eq!(old.layout, stable.layout);
        assert!(stable.retained_previous);
        let narrow = choose_list(6, 620., PROSE_WIDTH, Some(&old), false, false, measure);
        assert_eq!(narrow.layout, ListLayout::Grid(2));
        assert_eq!(
            narrow.candidates[2].rejected,
            Some(Rejection::ColumnTooNarrow)
        );
    }

    #[test]
    fn authored_labels_do_not_override_measured_imbalance() {
        let decision = choose_list(
            3,
            1100.,
            PROSE_WIDTH,
            None,
            true,
            false,
            |index, _, cards| {
                Some(ItemMeasurement {
                    lines: if cards { [2, 4, 3][index] } else { 1 },
                    height: if cards { [56., 112., 84.][index] } else { 28. },
                    preferred_width: 190.,
                    overflow: false,
                })
            },
        );
        assert_eq!(decision.layout, ListLayout::List);
        assert_eq!(
            decision.candidates[2].rejected,
            Some(Rejection::UnevenHeights)
        );
    }

    #[test]
    fn native_interface_measurements_prefer_fitting_three_by_two() {
        // Recorded loaded-font footprints for the six plan.md interfaces at
        // a 1314px canvas. Two columns avoid wrapping, but overextend the cells.
        let decision = choose_list(6, 1314., 548., None, true, false, |item, width, cards| {
            let lines = if width < 500. {
                [3, 2, 2, 3, 3, 2][item]
            } else {
                2
            };
            Some(ItemMeasurement {
                lines,
                height: lines as f32 * 24.,
                preferred_width: [464.336, 238.728, 278.128, 606.704, 588.456, 387.016][item],
                overflow: !cards,
            })
        });
        assert_eq!(decision.layout, ListLayout::Grid(3));
        assert_eq!(decision.row_columns, [3, 3]);
    }

    #[test]
    fn short_collections_keep_shared_column_anchors_in_partial_rows() {
        for count in [5, 6, 7, 8, 11] {
            let decision = choose_list(count, 1280., PROSE_WIDTH, None, true, false, |_, _, _| {
                Some(ItemMeasurement {
                    lines: 2,
                    height: 48.,
                    preferred_width: 280.,
                    overflow: false,
                })
            });
            assert_eq!(decision.layout, ListLayout::Grid(3), "count={count}");
            for item in 0..count {
                assert_eq!(
                    decision.placement(item),
                    Some((item / 3, item % 3, 3)),
                    "count={count}"
                );
            }
        }
    }

    #[test]
    fn uniform_grids_are_measured_bounded_and_stable() {
        for count in 2..=12 {
            let mut calls = 0;
            let measure = |_: usize, _: f32, _: bool| {
                Some(ItemMeasurement {
                    lines: 1,
                    height: 24.,
                    preferred_width: 220.,
                    overflow: false,
                })
            };
            let decision = choose_list(count, 1280., PROSE_WIDTH, None, false, false, |i, w, c| {
                calls += 1;
                measure(i, w, c)
            });
            assert!(
                calls <= count * 3,
                "count={count}: {calls} native measurements"
            );
            assert!(decision.is_valid(count, 1280., None));
            let columns = decision.row_columns[0];
            assert_eq!(decision.row_columns, vec![columns; count.div_ceil(columns)]);
            assert!(decision.row_columns.iter().all(|&n| (2..=3).contains(&n)));
            let stable = choose_list(
                count,
                1278.,
                PROSE_WIDTH,
                Some(&decision),
                false,
                false,
                measure,
            );
            assert_eq!(stable.row_columns, decision.row_columns);
            assert!(stable.retained_previous);
            let narrow = choose_list(
                count,
                360.,
                PROSE_WIDTH,
                Some(&decision),
                false,
                false,
                measure,
            );
            assert_eq!(narrow.layout, ListLayout::List);
        }
    }

    #[test]
    fn four_columns_require_single_line_cells_and_bounded_measurements() {
        for count in 2..=12 {
            let mut calls = 0;
            let decision = choose_list(count, 1280., PROSE_WIDTH, None, false, true, |_, _, _| {
                calls += 1;
                Some(ItemMeasurement {
                    lines: 1,
                    height: 24.,
                    preferred_width: 180.,
                    overflow: false,
                })
            });
            assert!(calls <= count * 4);
            assert!(decision.candidates.len() <= 40);
            assert!(decision.is_valid(count, 1280., None));
            if count % 4 == 0 {
                assert_eq!(decision.row_columns, vec![4; count / 4]);
            }
        }
        let decision = choose_list(8, 1280., PROSE_WIDTH, None, false, true, |_, width, _| {
            Some(ItemMeasurement {
                lines: if width < 350. { 2 } else { 1 },
                height: 24.,
                preferred_width: 360.,
                overflow: false,
            })
        });
        assert!(!decision.row_columns.contains(&4));
        assert!(
            decision
                .candidates
                .iter()
                .filter(|c| c.row_columns.contains(&4))
                .all(|c| c.rejected == Some(Rejection::TooManyLines))
        );
    }

    #[test]
    fn wider_items_select_wider_tracks_for_the_whole_collection() {
        // The final pair needs half-page tracks, so all items use that measure.
        // Earlier shorter items must retain the same anchors.
        let decision = choose_list(
            5,
            1280.,
            PROSE_WIDTH,
            None,
            false,
            false,
            |index, width, cards| {
                Some(ItemMeasurement {
                    lines: 1,
                    height: 24.,
                    preferred_width: 220.,
                    overflow: cards && index >= 3 && width < 500.,
                })
            },
        );
        assert_eq!(decision.row_columns, [2, 2, 2]);
        assert_eq!(decision.placement(3), Some((1, 1, 2)));
        assert_eq!(decision.placement(4), Some((2, 0, 2)));
        assert_eq!(
            decision
                .candidates
                .iter()
                .find(|c| c.columns == 3)
                .unwrap()
                .rejected,
            Some(Rejection::Overflow)
        );
    }
}
