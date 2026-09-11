//! Source-preserving repair of native word-wrap boundaries. Word boundaries
//! are not line-break opportunities: notably, `/`, closing punctuation and
//! non-breaking glue must not become ordinary leading break positions.
use std::ops::Range;

pub(super) fn opportunities(text: &str, graphemes: &[usize]) -> Vec<usize> {
    unicode_linebreak::linebreaks(text)
        .map(|(offset, _)| offset)
        .filter(|offset| *offset == text.len() || graphemes.binary_search(offset).is_ok())
        .collect()
}

/// Keep legal native wraps untouched. Otherwise use the already-shaped LTR
/// advances to nominate legal breaks and verify changed lines with the same
/// standalone shaping used by painting. No new glyph layout for each word.
/// Emergency breaks are permitted only when no legal opportunity fits; even
/// an oversized single grapheme must make progress without splitting it.
pub(super) fn repair(
    text: &str,
    native: &[Range<usize>],
    graphemes: &[usize],
    layout: &gpui::LineLayout,
    width: f32,
    zoom: f32,
    mut measure: impl FnMut(Range<usize>) -> Option<f32>,
) -> Option<Vec<Range<usize>>> {
    if native.len() < 2 || !width.is_finite() || text.contains(['\n', '\r']) {
        return None;
    }
    let legal = opportunities(text, graphemes);
    // A monotonic glyph cursor avoids LineLayout::x_for_index's full scan for
    // every grapheme. Strong RTL paragraphs use the logical shaping path.
    let mut glyphs = layout.runs.iter().flat_map(|run| &run.glyphs).peekable();
    let points = graphemes
        .iter()
        .copied()
        .chain(std::iter::once(text.len()))
        .map(|offset| {
            while glyphs.peek().is_some_and(|glyph| glyph.index < offset) {
                glyphs.next();
            }
            let x = glyphs.peek().map_or(layout.width, |glyph| glyph.position.x);
            (offset, f32::from(x) / zoom)
        })
        .collect::<Vec<_>>();
    if points.windows(2).any(|pair| pair[0].1 > pair[1].1) {
        return None;
    }
    let width = width.max(1.);
    // Snapping a native boundary backward out of an emoji cluster can make
    // the *next* native line overflow, even if its new boundary is legal.
    if native.iter().all(|line| {
        let start = points.partition_point(|point| point.0 < line.start);
        let end = points.partition_point(|point| point.0 < line.end);
        legal.binary_search(&line.end).is_ok() && points[end].1 - points[start].1 <= width
    }) {
        return None;
    }
    let mut start_ix = 0;
    let mut lines = Vec::new();
    while start_ix + 1 < points.len() {
        let (start, x) = points[start_ix];
        let fit_ix = points
            .partition_point(|point| point.1 <= x + width)
            .saturating_sub(1)
            .max(start_ix + 1);
        let mut legal_ix = legal.partition_point(|end| *end <= points[fit_ix].0);
        let mut end = start;
        while legal_ix > 0 && legal[legal_ix - 1] > start {
            let candidate = legal[legal_ix - 1];
            if measure(start..candidate)? <= width {
                end = candidate;
                break;
            }
            legal_ix -= 1;
        }
        if end == start {
            let mut end_ix = fit_ix;
            while end_ix > start_ix + 1 && measure(start..points[end_ix].0)? > width {
                end_ix -= 1;
            }
            end = points[end_ix].0;
        }
        lines.push(start..end);
        start_ix = points.binary_search_by_key(&end, |point| point.0).ok()?;
    }
    Some(lines)
}
