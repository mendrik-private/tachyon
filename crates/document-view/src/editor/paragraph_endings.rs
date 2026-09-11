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
                    InlineStyle::Bold | InlineStyle::Italic | InlineStyle::Strikethrough
                )
            })
        })
}

pub(super) fn refine(
    text: &str,
    lines: &mut [Range<usize>],
    graphemes: &[usize],
    width: f32,
    mut measure: impl FnMut(Range<usize>) -> Option<f32>,
) {
    let count = lines.len();
    if count < 2 || !width.is_finite() || width <= 0. {
        return;
    }
    let previous = lines[count - 2].clone();
    let last = lines[count - 1].clone();
    if last.end - previous.start > 2048 || text[last.clone()].split_whitespace().count() != 1 {
        return;
    }
    let Some(last_width) = measure(last.clone()) else {
        return;
    };
    if last_width > width * 0.25 {
        return;
    }
    // Only ordinary spaces provide new boundaries. NBSP, tabs, CJK and
    // unbreakable tokens keep their native layout. Inspect at most 2 KiB and
    // measure at most three candidates, leaving at least two preceding words.
    let words = text[previous.clone()]
        .match_indices(|c: char| !c.is_whitespace())
        .filter_map(|(offset, _)| {
            (offset == 0 || text.as_bytes()[previous.start + offset - 1] == b' ')
                .then_some(previous.start + offset)
        })
        .collect::<Vec<_>>();
    let mut best = None;
    let mut imbalance = width - last_width;
    for moved in 2..=4 {
        let Some(index) = words.len().checked_sub(moved).filter(|index| *index >= 2) else {
            continue;
        };
        let boundary = words[index];
        if graphemes.binary_search(&boundary).is_err() {
            continue;
        }
        let Some(left) = measure(previous.start..boundary) else {
            continue;
        };
        let Some(right) = measure(boundary..last.end) else {
            continue;
        };
        let difference = (left - right).abs();
        if left <= width && right <= width && difference < imbalance {
            best = Some(boundary);
            imbalance = difference;
        }
    }
    if let Some(boundary) = best {
        lines[count - 2].end = boundary;
        lines[count - 1].start = boundary;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eligibility_preserves_authored_and_specialized_content() {
        for source in [
            "# A heading with a final word.",
            "A paragraph with an authored  \nline break.",
            "A paragraph with `literal code` inside.",
            "A paragraph with [a protected link](https://example.com).",
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
        assert!(calls <= 7);
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
                    cx.text_system().clone(), "Spline Sans Mineral".into(), zoom,
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
                assert!(cold.intrinsic_requests <= 7);
                assert!(cold.shaping_calls <= 8);
                assert!(projection.text()[refined.last().unwrap().clone()].split_whitespace().count() >= 3,
                    "an isolated short final word should gain its preceding words: width={width}, original={original:?}, refined={refined:?}");
                assert_eq!(refined.len(), original.len());
                assert_eq!(&refined[..refined.len() - 2], &original[..original.len() - 2]);
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
