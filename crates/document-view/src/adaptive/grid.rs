//! One document-wide column grid. Unequal splits, label rails and reading
//! measures snap to its lines so nearby elements share vertical edges. It is
//! a pure function of the layout canvas, so retained (edit-locked) geometry
//! stays stable. Equal splits keep exact twelve-track geometry: on a divisible
//! grid that geometry already lies on grid lines.
use super::{LAYOUT_GAP, candidates::span_width};

/// Narrowest column a grid may use; narrower canvases try fewer columns.
pub(crate) const GRID_MIN_COLUMN: f32 = 120.;
const GRID_COLUMNS: [usize; 3] = [8, 6, 4];
const EPSILON: f32 = 0.01;
/// Fraction of a column pitch around the midpoint between two gutters.
const AMBIGUOUS_SPLIT: f32 = 0.1;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct DocumentGrid {
    pub columns: usize,
    pub column: f32,
}

impl DocumentGrid {
    /// Coordinates are relative to the document content origin. A single
    /// column means no grid: every caller keeps its exact geometry.
    pub fn new(canvas: f32) -> Self {
        let canvas = if canvas.is_finite() {
            canvas.max(0.)
        } else {
            0.
        };
        GRID_COLUMNS
            .into_iter()
            .map(|columns| Self {
                columns,
                column: (canvas - (columns - 1) as f32 * LAYOUT_GAP) / columns as f32,
            })
            .find(|grid| grid.column >= GRID_MIN_COLUMN)
            .unwrap_or(Self {
                columns: 1,
                column: canvas,
            })
    }

    pub fn active(self) -> bool {
        self.columns > 1
    }

    pub fn pitch(self) -> f32 {
        self.column + LAYOUT_GAP
    }

    pub fn start(self, index: usize) -> f32 {
        index as f32 * self.pitch()
    }

    pub fn end(self, index: usize) -> f32 {
        self.start(index) + self.column
    }

    /// Whole columns inside a local canvas that begins on a column start.
    pub fn columns_within(self, width: f32) -> usize {
        (0..self.columns)
            .take_while(|&index| self.end(index) <= width + EPSILON)
            .count()
    }

    /// Label column for text starting `offset` after a column start: it ends
    /// on the first column end covering `extent`, so the body starts on the
    /// next column start exactly one gutter later. Returns the label width.
    pub fn label_column(self, offset: f32, extent: f32) -> Option<f32> {
        if !self.active() {
            return None;
        }
        (0..self.columns.saturating_sub(1))
            .map(|index| self.end(index))
            .find(|end| *end + EPSILON >= offset + extent)
            .map(|end| end - offset)
    }

    /// Largest column end inside `[minimum, maximum]`, measured from a column
    /// start; `None` keeps the caller's unsnapped measure.
    pub fn end_within(self, minimum: f32, maximum: f32) -> Option<f32> {
        if !self.active() {
            return None;
        }
        (0..self.columns)
            .rev()
            .map(|index| self.end(index))
            .find(|end| *end <= maximum + EPSILON && *end + EPSILON >= minimum)
    }

    /// A bounded local canvas for snapped splits: the last column end inside
    /// `width`, or `width` itself when fewer than two columns would remain.
    pub fn fit_down(self, width: f32) -> f32 {
        let columns = self.columns_within(width);
        if self.active() && columns >= 2 {
            self.end(columns - 1)
        } else {
            width
        }
    }

    /// The column end nearest `width` (at least two columns, at most
    /// `limit`); ties take the wider canvas.
    pub fn fit_nearest(self, width: f32, limit: f32) -> f32 {
        if !self.active() {
            return width;
        }
        (1..self.columns)
            .map(|index| self.end(index))
            .filter(|end| *end <= limit + EPSILON)
            .fold(None::<f32>, |best, end| match best {
                Some(best) if (best - width).abs() + EPSILON < (end - width).abs() => Some(best),
                _ => Some(end),
            })
            .unwrap_or(width)
    }

    /// The first column end at least `width`, bounded by `limit`.
    pub fn fit_up(self, width: f32, limit: f32) -> f32 {
        if !self.active() {
            return width;
        }
        (1..self.columns)
            .map(|index| self.end(index))
            .find(|end| *end + EPSILON >= width && *end <= limit + EPSILON)
            .unwrap_or(width)
    }

    /// Interior gutter for an unequal two-part split at twelve-track boundary
    /// `boundary`, inside `columns` whole grid columns: the nearest gutter,
    /// keeping at least one column per part. A boundary that falls (nearly)
    /// midway between two gutters has no meaningful nearest line; `None`
    /// keeps that template's exact geometry instead of an arbitrary choice.
    fn split(self, columns: usize, local: f32, boundary: u8) -> Option<usize> {
        if columns < 2 {
            return None;
        }
        let target = f32::from(boundary) * (local + LAYOUT_GAP) / 12. / self.pitch();
        if (target.fract() - 0.5).abs() < AMBIGUOUS_SPLIT {
            return None;
        }
        Some((target.round() as usize).clamp(1, columns - 1))
    }
}

/// True when a placement belongs to an unequal two-part split, the only
/// family that moves onto grid lines. Equal splits keep exact geometry.
fn snappable(track_start: u8, span: u8, parts: usize) -> bool {
    parts == 2
        && span != 6
        && (1..12).contains(&span)
        && (track_start == 0 || track_start + span == 12)
}

/// `(left, width)` of a twelve-track placement in a local canvas whose origin
/// is the document content origin. `grid` is the document canvas whose grid
/// the placement snaps to; `None` keeps exact twelve-track geometry. Every
/// measurement and paint path converts spans through this one function.
pub(crate) fn track_geometry(
    grid: Option<f32>,
    canvas: f32,
    track_start: u8,
    span: u8,
    parts: usize,
) -> Option<(f32, f32)> {
    let width = span_width(canvas, span)?;
    if let Some(document) = grid.filter(|_| snappable(track_start, span, parts)) {
        let grid = DocumentGrid::new(document);
        let columns = grid.columns_within(canvas.min(document + EPSILON));
        let boundary = if track_start == 0 { span } else { track_start };
        if grid.active()
            && let Some(split) = grid.split(columns, canvas, boundary)
        {
            return Some(if track_start == 0 {
                (0., grid.end(split - 1))
            } else {
                (grid.start(split), grid.end(columns - 1) - grid.start(split))
            });
        }
    }
    let left = if track_start == 0 {
        0.
    } else {
        span_width(canvas, track_start)? + LAYOUT_GAP
    };
    Some((left, width))
}

/// Widths of every part of a template, in source order.
pub(crate) fn template_widths(grid: Option<f32>, canvas: f32, spans: &[u8]) -> Vec<f32> {
    let mut start = 0;
    spans
        .iter()
        .map(|&span| {
            let width = track_geometry(grid, canvas, start, span, spans.len()).map_or(0., |g| g.1);
            start += span;
            width
        })
        .collect()
}

/// Local canvas for a template: unequal splits end on a column end so both
/// parts are grid aligned; other templates keep the requested width.
pub(crate) fn template_canvas(grid: Option<f32>, canvas: f32, spans: &[u8]) -> f32 {
    match grid {
        Some(document) if spans.len() == 2 && spans[0] != spans[1] => {
            let grid = DocumentGrid::new(document);
            let fitted = grid.fit_down(canvas.min(document));
            grid.split(grid.columns_within(fitted), fitted, spans[0])
                .map_or(canvas, |_| fitted)
        }
        _ => canvas,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::rows::TEMPLATES;

    #[test]
    fn column_count_follows_the_available_whitespace() {
        for (canvas, columns) in [(1556., 8), (1632., 8), (1200., 6), (800., 4), (500., 1)] {
            let grid = DocumentGrid::new(canvas);
            assert_eq!(grid.columns, columns, "{canvas}");
            assert!((grid.end(grid.columns - 1) - canvas).abs() < 0.01);
            if grid.active() {
                assert!(grid.column >= GRID_MIN_COLUMN);
            }
        }
        assert_eq!(DocumentGrid::new(f32::NAN).columns, 1);
        let grid = DocumentGrid::new(1556.);
        assert!((grid.column - 152.5).abs() < 0.01);
        assert_eq!(grid.start(0), 0.);
        assert!((grid.start(1) - 200.5).abs() < 0.01);
        assert!((grid.end(1) - (200.5 + 152.5)).abs() < 0.01);
        assert!((grid.start(3) - grid.end(2) - LAYOUT_GAP).abs() < 0.01);
        assert_eq!(grid.columns_within(1556.), 8);
        assert_eq!(grid.columns_within(grid.end(2) + 30.), 3);
        assert_eq!(grid.label_column(24., 100.), Some(grid.end(0) - 24.));
        assert_eq!(grid.label_column(24., 200.), Some(grid.end(1) - 24.));
        assert_eq!(grid.end_within(600., 760.), Some(grid.end(3)));
        assert_eq!(grid.end_within(620., 640.), None);
        assert_eq!(DocumentGrid::new(500.).label_column(0., 10.), None);
    }

    #[test]
    fn unequal_templates_snap_to_grid_lines_and_equal_templates_stay_equal() {
        let mut canvas = 600.;
        let mut snapped = 0;
        while canvas <= 2400. {
            let grid = DocumentGrid::new(canvas);
            for spans in TEMPLATES {
                let local = template_canvas(Some(canvas), canvas, spans);
                assert!(local <= canvas + 0.01);
                let mut start = 0;
                let mut previous_right = None::<f32>;
                let mut widths = Vec::new();
                let mut on_grid = true;
                for &span in *spans {
                    let (left, width) =
                        track_geometry(Some(canvas), local, start, span, spans.len()).unwrap();
                    start += span;
                    assert!(width > 0. && left >= 0. && left + width <= canvas + 0.01);
                    if let Some(right) = previous_right {
                        assert!(
                            (left - right - LAYOUT_GAP).abs() < 0.01,
                            "{spans:?} {canvas}"
                        );
                    }
                    previous_right = Some(left + width);
                    widths.push(width);
                    on_grid &= (0..grid.columns).any(|i| (grid.start(i) - left).abs() < 0.01)
                        && (0..grid.columns).any(|i| (grid.end(i) - left - width).abs() < 0.01)
                        && width + 0.01 >= grid.column;
                }
                let unequal = spans.len() == 2 && spans[0] != spans[1];
                if grid.active() && unequal {
                    // Either every part lies on grid lines, or the split was
                    // ambiguous and the exact twelve-track geometry remains.
                    let exact = template_widths(None, canvas, spans);
                    let is_exact = widths.iter().zip(&exact).all(|(a, b)| (a - b).abs() < 0.01);
                    assert!(on_grid || is_exact, "{spans:?} at {canvas}: {widths:?}");
                    if on_grid && !is_exact {
                        snapped += 1;
                    }
                    if grid.columns == 8 {
                        assert!(on_grid, "eight columns resolve every split: {spans:?}");
                    }
                }
                if spans.iter().all(|span| *span == spans[0]) {
                    assert!(widths.iter().all(|w| (w - widths[0]).abs() < 0.01));
                    assert!((widths[0] - span_width(canvas, spans[0]).unwrap()).abs() < 0.01);
                }
            }
            canvas += 37.;
        }
        assert!(snapped > 0);
    }

    #[test]
    fn ambiguous_splits_keep_exact_tracks() {
        // Six columns: [5, 7] and [7, 5] fall exactly between two gutters,
        // while [4, 8] lies on a grid line.
        let canvas = 1200.;
        let grid = DocumentGrid::new(canvas);
        assert_eq!(grid.columns, 6);
        for spans in [[5, 7], [7, 5]] {
            assert_eq!(
                template_widths(Some(canvas), canvas, &spans),
                template_widths(None, canvas, &spans)
            );
            assert_eq!(template_canvas(Some(canvas), canvas, &spans), canvas);
        }
        assert!((template_widths(Some(canvas), canvas, &[4, 8])[0] - grid.end(1)).abs() < 0.01);
        // Eight columns: [5, 7] moves its split to the nearest gutter.
        let wide = DocumentGrid::new(1556.);
        let widths = template_widths(Some(1556.), 1556., &[5, 7]);
        assert!((widths[0] - wide.end(2)).abs() < 0.01);
        assert!((widths[1] - (1556. - wide.start(3))).abs() < 0.01);
    }
}
