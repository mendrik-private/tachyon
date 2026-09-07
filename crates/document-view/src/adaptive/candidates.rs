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
    pub width: f32,
    pub items: Vec<ItemMeasurement>,
    /// One normalized breakdown per internal row, including the stack's rows.
    pub rows: Vec<Penalties>,
    pub rejected: Option<Rejection>,
}

impl ListCandidate {
    pub fn cost(&self) -> f32 {
        self.rows.iter().map(|row| row.weighted()).sum()
    }
}

#[derive(Clone, Debug, serde::Serialize)]
pub(crate) struct ListDecision {
    pub layout: ListLayout,
    pub candidates: Vec<ListCandidate>,
    pub retained_previous: bool,
}

impl ListDecision {
    pub fn is_valid(&self, count: usize, canvas: f32, previous: Option<ListLayout>) -> bool {
        if self.retained_previous && previous != Some(self.layout) {
            return false;
        }
        let ListLayout::Grid(columns) = self.layout else {
            return self.layout == ListLayout::List;
        };
        self.candidates.iter().any(|candidate| {
            candidate.columns == columns
                && candidate.rejected.is_none()
                && candidate.items.len() == count
                && candidate.rows.len() == count.div_ceil(columns)
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
    previous: Option<ListLayout>,
    mut measure: impl FnMut(usize, f32, bool) -> Option<ItemMeasurement>,
) -> ListDecision {
    let mut candidates = Vec::with_capacity(3);
    for columns in 1..=3 {
        // Stack uses the renderer's bounded prose measure. Counting the
        // intentional outside margin as intrinsic content would spuriously
        // make a stack cheaper as the canvas grows beyond that measure.
        let width = if columns == 1 {
            canvas.clamp(1., PROSE_WIDTH)
        } else {
            span_width(canvas, (12 / columns) as u8).unwrap_or(1.)
        };
        let mut candidate = ListCandidate {
            columns,
            width,
            items: Vec::with_capacity(count),
            rows: Vec::new(),
            rejected: None,
        };
        // Reserve a comfortable text column in addition to marker/card insets.
        // The subsequent native measurements, not this guard, decide fit.
        if columns > 1 && width - 2. * CARD_PADDING - 32. < 180. {
            candidate.rejected = Some(Rejection::ColumnTooNarrow);
        } else {
            for index in 0..count {
                let Some(item) = measure(index, width, columns > 1) else {
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
            } else if candidate.items.iter().any(|i| i.lines > 5) {
                Some(Rejection::TooManyLines)
            } else if tallest > shortest * 1.6 + 0.001 {
                Some(Rejection::UnevenHeights)
            } else {
                None
            };
        }
        if candidate.rejected.is_none() {
            for row in candidate.items.chunks(columns) {
                let height = row.iter().map(|i| i.height).fold(0., f32::max);
                let area = height * columns as f32;
                let used = row.iter().map(|i| i.height).sum::<f32>();
                candidate.rows.push(Penalties {
                    // More than three lines in an internal card is legal but
                    // less comfortable. Stack line wrapping is intentional.
                    discomfort: if columns > 1 {
                        row.iter()
                            .map(|i| i.lines.saturating_sub(3) as f32 / 2.)
                            .fold(0., f32::max)
                    } else {
                        0.
                    },
                    // Rows are source-contiguous; nothing is detached or moved.
                    separation: 0.,
                    reading_jump: 0.,
                    imbalance: (area - used) / area.max(1.),
                    complexity: (columns - 1) as f32 / columns as f32,
                    // List cells (unlike prose margins) offer useful space for
                    // peers. Intrinsic preferred widths come from actual fonts.
                    unused_width: row
                        .iter()
                        .map(|i| (1. - i.preferred_width / width).clamp(0., 1.))
                        .sum::<f32>()
                        / columns as f32,
                    change: previous.map_or(0., |old| {
                        let old_columns = if let ListLayout::Grid(n) = old { n } else { 1 };
                        old_columns.abs_diff(columns) as f32 / 2.
                    }),
                });
            }
        }
        candidates.push(candidate);
    }
    let old_columns = previous.map(|old| if let ListLayout::Grid(n) = old { n } else { 1 });
    let legal = |c: &&ListCandidate| c.rejected.is_none();
    let best = candidates.iter().filter(legal).min_by(|a, b| {
        a.cost()
            .total_cmp(&b.cost())
            .then_with(|| (Some(b.columns) == old_columns).cmp(&(Some(a.columns) == old_columns)))
            .then_with(|| a.columns.cmp(&b.columns))
    });
    let previous = candidates
        .iter()
        .filter(legal)
        .find(|c| Some(c.columns) == old_columns);
    let winner = match (best, previous) {
        (Some(best), Some(old)) if old.cost() - best.cost() < old.cost().abs() * 0.1 + 0.001 => {
            Some(old)
        }
        _ => best,
    };
    let columns = winner.map_or(1, |c| c.columns);
    ListDecision {
        layout: if columns == 1 {
            ListLayout::List
        } else {
            ListLayout::Grid(columns)
        },
        retained_previous: Some(columns) == old_columns,
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
            choose_list(6, 1100., None, |_, _, _| measured(1, 28., false)).layout,
            ListLayout::Grid(3)
        );
        for reason in [
            Rejection::TooManyLines,
            Rejection::UnevenHeights,
            Rejection::Overflow,
            Rejection::MeasurementUnavailable,
        ] {
            let decision = choose_list(6, 1100., None, |index, _, cards| {
                if !cards || index != 0 {
                    return measured(1, 28., false);
                }
                match reason {
                    Rejection::TooManyLines => measured(6, 168., false),
                    Rejection::UnevenHeights => measured(2, 56., false),
                    Rejection::Overflow => measured(1, 28., true),
                    _ => None,
                }
            });
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
        let old = choose_list(6, 1100., None, measure);
        let stable = choose_list(6, 1098., Some(old.layout), measure);
        assert_eq!(old.layout, stable.layout);
        assert!(stable.retained_previous);
        let narrow = choose_list(6, 620., Some(old.layout), measure);
        assert_eq!(narrow.layout, ListLayout::Grid(2));
        assert_eq!(
            narrow.candidates[2].rejected,
            Some(Rejection::ColumnTooNarrow)
        );
    }
}
