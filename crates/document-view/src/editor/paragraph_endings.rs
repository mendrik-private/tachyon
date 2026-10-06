//! Bounded final-line refinement, shared by measurement and editor geometry.
use super::*;

pub(super) fn eligible(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    range: &Range<usize>,
) -> bool {
    let context = &segment.context;
    if context.table_cell.is_some()
        || context.metadata
        || context.figure_text.is_some()
        || context.bibliography.is_some()
        || context.definition.is_some()
        || context.resource_title_end.is_some()
    {
        return false;
    }
    let Some(BlockNode::Paragraph(paragraph)) = projection.block(segment.node_id) else {
        return false;
    };
    let Some(canonical) = projection.segment_for_node(segment.node_id) else {
        return false;
    };
    let canonical_range = &canonical.projection_range();
    if canonical_range.len() > 16 * 1024 || range.end != canonical_range.end {
        return false;
    }
    // Preserve authored hard breaks and specialized inline break behavior.
    // Markdown soft breaks have already become ordinary prose spaces.
    // A semantic eligibility bit participates in the wrap cache identity.
    !projection.text()[canonical_range.clone()].contains(['\n', '\r'])
        && paragraph.content.runs().iter().all(|run| {
            run.styles.iter().all(|style| {
                matches!(
                    style,
                    InlineStyle::Bold
                        | InlineStyle::Italic
                        | InlineStyle::Strikethrough
                        | InlineStyle::Code
                        | InlineStyle::Link(_)
                )
            })
        })
}

/// Distinct line measurements one paragraph ending may request.
const MEASURE_BUDGET: usize = 48;
/// Words one boundary may pass to the following line. With a three-line
/// window the worst case stays at 6 + 6 * 6 + 6 = 48 distinct measurements.
const MAX_MOVED: usize = 5;
/// Final lines narrower than this share of the width read as stranded.
const SHORT_ENDING: f32 = 0.3;

/// Re-breaks the last (at most three) lines when the paragraph would end in a
/// stranded phrase. The shortfall is spread over the window by minimizing the
/// squared slack of its non-final lines plus a strong short-ending penalty, so
/// the rag stays even instead of collapsing on the penultimate line alone.
/// Line count, contiguity and every line before the window are preserved.
pub(super) fn refine(
    text: &str,
    lines: &mut [Range<usize>],
    graphemes: &[usize],
    width: f32,
    measure: impl FnMut(Range<usize>) -> Option<f32>,
) {
    let count = lines.len();
    if count < 2 || !width.is_finite() || width <= 0. {
        return;
    }
    let first = count - count.min(3);
    let window = &lines[first..];
    let last = window[window.len() - 1].clone();
    if last.end - window[0].start > 2048 || !(1..=3).contains(&word_count(text, &last)) {
        return;
    }
    let mut search = Search {
        text,
        window,
        width,
        measurements: Vec::new(),
        measure,
        options: Vec::new(),
        chosen: Vec::new(),
        best: (f32::INFINITY, Vec::new()),
    };
    match search.measure(last.clone()) {
        Ok(Some(last_width)) if last_width <= width * 0.25 => {}
        _ => return,
    }
    // Only ordinary spaces provide new boundaries. NBSP, tabs, CJK and
    // unbreakable tokens keep their native layout; protected inline spans are
    // already absent from `graphemes`. The first window line keeps two words.
    search.options = window
        .windows(2)
        .enumerate()
        .map(|(index, pair)| {
            let line = &pair[0];
            let starts = text[line.clone()]
                .match_indices(|c: char| !c.is_whitespace())
                .filter_map(|(offset, _)| {
                    (offset == 0 || text.as_bytes()[line.start + offset - 1] == b' ')
                        .then_some(line.start + offset)
                })
                .collect::<Vec<_>>();
            let movable = starts
                .get(if index == 0 { 2 } else { 1 }..)
                .unwrap_or_default()
                .iter()
                .copied()
                .filter(|start| graphemes.binary_search(start).is_ok())
                .collect::<Vec<_>>();
            std::iter::once(pair[1].start)
                .chain(movable.into_iter().rev().take(MAX_MOVED))
                .collect()
        })
        .collect();
    // The native breaks set the bar a re-break must strictly beat.
    let native = window[1..]
        .iter()
        .map(|line| line.start)
        .collect::<Vec<_>>();
    let Ok(Some(current)) = search.cost(&native) else {
        return;
    };
    search.best = (current, native.clone());
    if search.descend(0, window[0].start, 0.).is_err() || search.best.1 == native {
        return;
    }
    for (offset, boundary) in search.best.1.into_iter().enumerate() {
        lines[first + offset].end = boundary;
        lines[first + offset + 1].start = boundary;
    }
}

fn word_count(text: &str, range: &Range<usize>) -> usize {
    text[range.clone()].split_whitespace().count()
}

/// The measurement budget was spent before the search completed.
struct Exhausted;

struct Search<'a, F> {
    text: &'a str,
    window: &'a [Range<usize>],
    width: f32,
    measurements: Vec<(Range<usize>, Option<f32>)>,
    measure: F,
    /// Candidate starts per window line after the first, latest first.
    options: Vec<Vec<usize>>,
    chosen: Vec<usize>,
    best: (f32, Vec<usize>),
}

impl<F: FnMut(Range<usize>) -> Option<f32>> Search<'_, F> {
    fn measure(&mut self, range: Range<usize>) -> Result<Option<f32>, Exhausted> {
        if let Some((_, width)) = self.measurements.iter().find(|(r, _)| *r == range) {
            return Ok(*width);
        }
        if self.measurements.len() >= MEASURE_BUDGET {
            return Err(Exhausted);
        }
        let width = (self.measure)(range.clone());
        self.measurements.push((range, width));
        Ok(width)
    }

    /// Measured width of a line that may be laid out: native lines are kept
    /// as shaped, new lines must fit.
    fn fitting(&mut self, range: Range<usize>) -> Result<Option<f32>, Exhausted> {
        let native = self.window.contains(&range);
        Ok(self
            .measure(range)?
            .filter(|measured| native || *measured <= self.width))
    }

    fn slack(&self, line: f32) -> f32 {
        ((self.width - line).max(0.) / self.width).powi(2)
    }

    fn ending(&self, ending: f32, range: &Range<usize>) -> f32 {
        let mut penalty = 0.;
        if ending < self.width * SHORT_ENDING {
            penalty += 1. + SHORT_ENDING - ending / self.width;
        }
        if word_count(self.text, range) < 2 {
            penalty += 1.;
        }
        penalty
    }

    /// Objective for complete window boundaries, or None if a line does not
    /// measure or fit.
    fn cost(&mut self, boundaries: &[usize]) -> Result<Option<f32>, Exhausted> {
        let mut start = self.window[0].start;
        let mut cost = 0.;
        for &boundary in boundaries {
            let Some(line) = self.fitting(start..boundary)? else {
                return Ok(None);
            };
            cost += self.slack(line);
            start = boundary;
        }
        let range = start..self.window[self.window.len() - 1].end;
        Ok(self
            .fitting(range.clone())?
            .map(|ending| cost + self.ending(ending, &range)))
    }

    /// Chooses the start of window line `level + 1`. Candidates run from the
    /// native break towards earlier words, so the current line only shrinks
    /// and the remainder only grows; both permit early exits.
    fn descend(&mut self, level: usize, start: usize, cost: f32) -> Result<(), Exhausted> {
        let end = self.window[self.window.len() - 1].end;
        for index in 0..self.options[level].len() {
            let boundary = self.options[level][index];
            if boundary <= start {
                break;
            }
            let Some(line) = self.fitting(start..boundary)? else {
                continue;
            };
            let cost = cost + self.slack(line);
            if cost >= self.best.0 {
                break;
            }
            self.chosen.push(boundary);
            if level + 1 < self.options.len() {
                let result = self.descend(level + 1, boundary, cost);
                self.chosen.pop();
                result?;
                continue;
            }
            let ending = self.fitting(boundary..end)?;
            let penalty = ending.map(|ending| self.ending(ending, &(boundary..end)));
            if let Some(penalty) = penalty
                && cost + penalty < self.best.0
            {
                self.best = (cost + penalty, self.chosen.clone());
            }
            self.chosen.pop();
            // A longer ending never fits again, and a settled one only costs
            // the current line more slack.
            if penalty.is_none_or(|penalty| penalty == 0.) {
                break;
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_phrases_gain_words_without_changing_source() {
        for ending in ["me.", "for me.", "just for me."] {
            let text = format!("One two three four five six seven {ending}");
            let start = text.find(ending).unwrap();
            let mut lines = vec![0..start, start..text.len()];
            let boundaries = text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            refine(&text, &mut lines, &boundaries, 48., |range| {
                Some(text[range].chars().count() as f32)
            });
            assert!(lines[1].start < start);
            assert_eq!(
                lines
                    .iter()
                    .map(|range| &text[range.clone()])
                    .collect::<String>(),
                text
            );
        }
    }

    #[test]
    fn inline_code_matches_body_font_and_uses_theme_syntax_color() {
        let document =
            Document::from_markdown("A typed `Result FsError Catalog` gives a contract.").unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let segment = &projection.segments()[0];
        let range = segment.projection_range();
        assert!(eligible(&projection, segment, &range));
        for palette in [TachyonPalette::LIGHT, TachyonPalette::DARK] {
            let runs = styled_projection_runs(
                &projection,
                &range,
                range.len(),
                &gpui::TextStyle::default(),
                false,
                palette,
            );
            assert!(runs.len() >= 3);
            assert_eq!(runs[0].font, runs[1].font);
            assert_eq!(runs[1].color, rgb(palette.syntax_number).into());
            assert_ne!(runs[0].color, runs[1].color);
        }
    }

    #[test]
    fn eligibility_preserves_authored_and_specialized_content() {
        for source in [
            "# A heading with a final word.",
            "A paragraph with an authored  \nline break.",
            "A paragraph with $x^2$ inside.",
            "A paragraph with ![an image](image.png) inside.",
            "```text\nA literal block with a final word.\n```",
            "| Column |\n| --- |\n| A cell with a final word. |",
        ] {
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for segment in projection.segments() {
                assert!(
                    !eligible(&projection, segment, &segment.projection_range()),
                    "{source}"
                );
            }
        }
        let document = Document::from_markdown("A **styled** paragraph with an ending.").unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let segment = &projection.segments()[0];
        assert!(eligible(&projection, segment, &segment.projection_range()));
        assert!(!eligible(
            &projection,
            segment,
            &(segment.projection_start()..segment.projection_end() - 1)
        ));
        let large = "A large paragraph. ".repeat(1024);
        let document = Document::from_markdown(large.as_str()).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let segment = &projection.segments()[0];
        assert!(!eligible(&projection, segment, &segment.projection_range()));
    }

    #[gpui::test]
    fn runt_refinement_keeps_inline_spans_intact(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for span in ["`Result FsError Catalog`", "[a useful contract](https://example.com)"] {
                let source = format!("The explanation keeps its evidence together with {span} and the original document just for me.");
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let segment = &projection.segments()[0];
                let rich = projection.block(segment.node_id).unwrap().text().unwrap();
                let mut unrefined = segment.clone();
                unrefined.context.metadata = true;
                let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
                for width in (200..600).step_by(20) {
                    let original = fonts.wrap(&projection, &unrefined, segment.projection_range(), width as f32, 18.).unwrap();
                    let refined = fonts.wrap(&projection, segment, segment.projection_range(), width as f32, 18.).unwrap();
                    assert_eq!(refined.len(), original.len());
                    for line in &refined {
                        if original.iter().any(|old| old.start == line.start) { continue; }
                        let offset = line.start - segment.projection_start();
                        assert!(!rich.runs().iter().any(|run| run.styles.iter().any(|style| matches!(style, InlineStyle::Code | InlineStyle::Link(_))) && run.range.start < offset && offset < run.range.end));
                    }
                    assert_eq!(refined.iter().map(|r| &projection.text()[r.clone()]).collect::<String>(), projection.text()[segment.projection_range()]);
                }
            }
        });
    }

    #[test]
    fn refinement_is_bounded_contiguous_and_grapheme_safe() {
        let text = "One two cafe\u{301} four five six end.";
        let last = text.find("end.").unwrap();
        let original = vec![0..last, last..text.len()];
        let mut lines = original.clone();
        let graphemes = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let mut calls = 0;
        refine(text, &mut lines, &graphemes, 30., |r| {
            calls += 1;
            Some(text[r].graphemes(true).count() as f32)
        });
        assert_ne!(lines, original);
        assert!(calls <= 9);
        assert_eq!(lines[0].end, lines[1].start);
        assert!(graphemes.contains(&lines[1].start));
        assert_eq!(
            lines.iter().map(|r| &text[r.clone()]).collect::<String>(),
            text
        );
        for text in [
            "One\u{a0}two\u{a0}three\u{a0}four\u{a0}five end.",
            "One\ttwo\tthree\tfour\tfive end.",
            "漢字漢字漢字漢字漢字 end.",
        ] {
            let last = text.find("end.").unwrap();
            let original = vec![0..last, last..text.len()];
            let mut lines = original.clone();
            let graphemes = text
                .grapheme_indices(true)
                .map(|(i, _)| i)
                .collect::<Vec<_>>();
            refine(text, &mut lines, &graphemes, 100., |r| {
                Some(text[r].chars().count() as f32)
            });
            assert_eq!(lines, original, "do not introduce unsafe word boundaries");
        }
        let text = format!("{}end.", "word ".repeat(500));
        let mut lines = vec![0..text.len() - 4, text.len() - 4..text.len()];
        refine(&text, &mut lines, &[], 3000., |_| {
            panic!("oversized tails are not measured")
        });
    }

    /// Native-like greedy wrap at ordinary spaces; a line's width includes its
    /// trailing space, matching how `refine` measures source ranges.
    fn greedy(text: &str, width: usize) -> Vec<Range<usize>> {
        let mut lines = vec![Range { start: 0, end: 0 }];
        for (offset, _) in text.match_indices(' ') {
            let line = lines.last_mut().unwrap();
            if offset + 1 - line.start > width && line.end > line.start {
                let start = line.end;
                lines.push(start..offset + 1);
            } else {
                line.end = offset + 1;
            }
        }
        let line = lines.last_mut().unwrap();
        if text.len() - line.start > width && line.end > line.start {
            let start = line.end;
            lines.push(start..text.len());
        } else {
            line.end = text.len();
        }
        lines
    }

    #[test]
    fn stranded_endings_spread_the_shortfall_over_the_last_lines() {
        let text = "alpha beta gamma delta epsilon zeta eta \
                    theta iota kappa lambda mu nu xi pi tau \
                    upsilon phi chi psi omega alef bet dal end.";
        let original = greedy(text, 40);
        let widths = original.iter().map(|r| r.len()).collect::<Vec<_>>();
        assert_eq!(widths, [40, 40, 39, 4], "greedy rag ≈ [1, 1, .97, .1]·W");
        let graphemes = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .collect::<Vec<_>>();
        let mut lines = original.clone();
        let mut calls = 0;
        refine(text, &mut lines, &graphemes, 40., |r| {
            calls += 1;
            Some(r.len() as f32)
        });
        assert!(calls <= MEASURE_BUDGET, "{calls} measurements");
        assert_eq!(lines.len(), original.len());
        assert_eq!(lines[0], original[0], "lines before the window stay");
        // The old two-line balance left a ~0.5·W penultimate line here.
        let widths = lines.iter().map(|r| r.len()).collect::<Vec<_>>();
        assert!(widths[1] >= 30 && widths[2] >= 30, "even rag: {widths:?}");
        assert!(widths[3] >= 12, "reasonable ending: {widths:?}");
        assert!(text[lines[3].clone()].split_whitespace().count() >= 2);
        assert!(widths.iter().all(|width| *width <= 40));
        assert_eq!(
            lines.iter().map(|r| &text[r.clone()]).collect::<String>(),
            text
        );
    }

    #[test]
    fn rebreaking_is_bounded_and_respects_allowed_boundaries() {
        let text = "The committed values are resolved on demand without building \
                    a graph sized table and the reports keep `inline code spans` \
                    together while errors stay inside the callee instead of at \
                    the offending call site so readers can follow it.";
        let protected = text.find('`').unwrap()..text.rfind('`').unwrap() + 1;
        let graphemes = text
            .grapheme_indices(true)
            .map(|(i, _)| i)
            .filter(|i| !(protected.start < *i && *i < protected.end))
            .collect::<Vec<_>>();
        let mut refined = 0;
        for width in 16..90 {
            let original = greedy(text, width);
            let mut lines = original.clone();
            let mut calls = 0;
            refine(text, &mut lines, &graphemes, width as f32, |r| {
                calls += 1;
                Some(text[r].chars().count() as f32)
            });
            assert!(calls <= MEASURE_BUDGET, "width={width}: {calls} calls");
            assert_eq!(lines.len(), original.len());
            let window = original.len() - original.len().min(3);
            assert_eq!(lines[..window], original[..window], "width={width}");
            assert_eq!(lines[0].start, 0);
            assert_eq!(lines.last().unwrap().end, text.len());
            for (pair, native) in lines.windows(2).zip(original.windows(2)) {
                assert_eq!(pair[0].end, pair[1].start);
                assert!(pair[0].start < pair[0].end);
                let boundary = pair[1].start;
                if boundary != native[1].start {
                    assert!(graphemes.contains(&boundary), "width={width}");
                    assert_eq!(text.as_bytes()[boundary - 1], b' ');
                }
            }
            for (line, native) in lines.iter().zip(&original) {
                assert!(line == native || line.len() <= width, "width={width}");
            }
            refined += usize::from(lines != original);
        }
        assert!(refined > 0, "exercise stranded endings");
    }

    #[gpui::test]
    fn justified_prose_does_not_pull_words_down_to_lengthen_the_last_line(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "The explanation keeps its evidence and qualifications together so another reader can understand the original document.";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            let mut unbalanced = segment.clone();
            unbalanced.context.metadata = true;
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.)
                .with_typography(typography::Options { justify: true, hyphenate: false });
            let mut short_endings = 0;
            for width in (300..700).step_by(5) {
                let expected = fonts.wrap(&projection, &unbalanced, segment.projection_range(), width as f32, 18.).unwrap();
                let actual = fonts.wrap(&projection, segment, segment.projection_range(), width as f32, 18.).unwrap();
                assert_eq!(actual, expected, "justification must not move words off the preceding line at width={width}");
                let last = actual.last().unwrap();
                short_endings += usize::from(projection.text()[last.clone()].split_whitespace().count() == 1);
                assert!(!typography::continues(&projection, segment, last, None));
            }
            assert!(short_endings > 0, "exercise naturally short final lines");
        });
    }

    #[gpui::test]
    fn typography_toggles_preserve_source_and_only_balance_ragged_prose(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "The explanation keeps its evidence and qualifications together so another reader can understand the original document.";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            for justify in [false, true] {
                for hyphenate in [false, true] {
                    let fonts = FontMeasurement::new(
                        cx.text_system().clone(), "Public Sans Tachyon".into(), 1.,
                    ).with_typography(typography::Options { justify, hyphenate });
                    for width in (300..700).step_by(5) {
                        let lines = fonts.wrap(&projection, segment, segment.projection_range(), width as f32, 18.).unwrap();
                        let last = lines.last().unwrap();
                        if !justify && lines.len() > 1 && fonts.line_width(&projection, last.clone(), 18.).unwrap() <= width as f32 * 0.25 {
                            assert!(projection.text()[last.clone()].split_whitespace().count() > 1,
                                "stranded ending at width={width}, justify={justify}, hyphenate={hyphenate}: {:?}", &projection.text()[last.clone()]);
                        }
                        assert_eq!(lines.iter().map(|range| &projection.text()[range.clone()]).collect::<String>(), source);
                    }
                }
            }
        });
    }

    #[gpui::test]
    fn native_paragraph_endings_balance_without_changing_source_or_line_count(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "The explanation keeps its evidence and qualifications together so another reader can understand the original document.";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            // A metadata context provides exactly the original native wrap, but
            // must not reuse a paragraph-refined cache entry (or vice versa).
            let mut native = segment.clone();
            native.context.metadata = true;
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(), "Public Sans Tachyon".into(), zoom,
                );
                let witness = (300..700).find_map(|width| {
                    let lines = fonts.wrap(&projection, &native, segment.projection_range(), width as f32, 18.)?;
                    let last = lines.last()?;
                    (lines.len() >= 2 && projection.text()[last.clone()].trim() == "document."
                        && fonts.line_width(&projection, last.clone(), 18.)? <= width as f32 * 0.25)
                        .then_some((width as f32, lines))
                }).expect("native single-word ending witness");
                let (width, original) = witness;
                let cold_scope = diagnostics::MeasurementScope::new();
                let refined = fonts.wrap(&projection, segment, segment.projection_range(), width, 18.).unwrap();
                let cold = cold_scope.take_stage();
                assert!(cold.intrinsic_requests <= 9);
                assert!(cold.shaping_calls <= 10);
                let ending = refined.last().unwrap().clone();
                assert!(projection.text()[ending.clone()].split_whitespace().count() >= 2
                    && fonts.line_width(&projection, ending, 18.).unwrap() >= width * SHORT_ENDING,
                    "an isolated short final word should gain its preceding words: width={width}, original={original:?}, refined={refined:?}");
                assert_eq!(refined.len(), original.len());
                let window = original.len() - original.len().min(3);
                assert_eq!(&refined[..window], &original[..window]);
                assert_eq!(refined.iter().map(|r| &projection.text()[r.clone()]).collect::<String>(), source);
                for line in &refined {
                    assert!(fonts.line_width(&projection, line.clone(), 18.).unwrap() <= width);
                }
                let scope = diagnostics::MeasurementScope::new();
                assert_eq!(fonts.wrap(&projection, segment, segment.projection_range(), width, 18.).unwrap(), refined);
                assert_eq!(fonts.wrap(&projection, &native, segment.projection_range(), width, 18.).unwrap(), original);
                let warm = scope.take_stage();
                assert_eq!(warm.wrap_cache_hits, 2);
                assert_eq!(warm.intrinsic_requests, 0);
                assert_eq!(warm.shaping_calls, 0);
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }
}
