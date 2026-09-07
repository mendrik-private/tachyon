//! Source-indexed inline math. Geometry is prepared offscreen; paint only
//! substitutes measured advances and uses retained, inert formula images.
use super::*;

#[derive(Clone)]
pub(super) struct Attachment {
    pub range: Range<usize>,
    pub light: Arc<crate::math::Formula>,
    pub dark: Arc<crate::math::Formula>,
    pub x: f32,
}

#[derive(Clone)]
pub(super) struct InlineLine {
    pub range: Range<usize>,
    pub attachments: Vec<Attachment>,
    pub font_size: f32,
    pub width: f32,
    pub ascent: f32,
    pub descent: f32,
}

pub(super) fn has_math(projection: &TextProjection, node: NodeId) -> bool {
    projection
        .block(node)
        .and_then(BlockNode::text)
        .is_some_and(|text| {
            text.runs().iter().any(|run| {
                run.styles
                    .iter()
                    .any(|style| matches!(style, InlineStyle::Math { .. }))
            })
        })
}

pub(super) fn layout(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    range: Range<usize>,
    width: f32,
    font_size: f32,
    measurement: &FontMeasurement,
) -> Option<Vec<InlineLine>> {
    if projection.math_edit_node == Some(segment.node_id)
        || range.len() > 16 * 1024
        || range.is_empty()
    {
        return None;
    }
    let rich = projection.block(segment.node_id)?.text()?;
    // The current glyph mapper is logical-LTR. Preserve the canonical native
    // text fallback until mixed-direction inline-object ordering is supported.
    if projection.text()[range.clone()].chars().any(|ch| matches!(ch, '\u{0590}'..='\u{08ff}' | '\u{fb1d}'..='\u{fdff}' | '\u{fe70}'..='\u{fefc}')) {
        return None;
    }
    let mut attachments = Vec::new();
    let factor = font_size / crate::math::EM;
    for run in rich.runs() {
        if !run
            .styles
            .iter()
            .any(|style| matches!(style, InlineStyle::Math { .. }))
        {
            continue;
        }
        if run.range.start < segment.node_range.start || run.range.end > segment.node_range.end {
            continue;
        }
        let source_range = segment.projection_range.start + run.range.start
            - segment.node_range.start
            ..segment.projection_range.start + run.range.end - segment.node_range.start;
        if source_range.start < range.start || source_range.end > range.end {
            continue;
        }
        let source = &projection.text()[source_range.clone()];
        let Ok(light) = crate::math::inline_formula(source, MineralPalette::for_dark(false).text)
        else {
            continue;
        };
        if light.width * factor + font_size * (2. / 9.) > width {
            continue;
        }
        let dark = crate::math::inline_formula(source, MineralPalette::for_dark(true).text).ok()?;
        attachments.push(Attachment {
            range: source_range,
            light,
            dark,
            x: 0.,
        });
        if attachments.len() > 64 {
            return None;
        }
    }
    if attachments.is_empty() {
        return None;
    }
    let raw = measurement.shape_unwrapped(projection, range.clone(), font_size)?;
    let composed = compose(raw, &range, &attachments, font_size);
    let text = &projection.text()[range.clone()];
    let is_inside_formula = |offset: usize| {
        attachments
            .iter()
            .any(|a| a.range.start < offset && offset < a.range.end)
    };
    let mut boundaries = text
        .grapheme_indices(true)
        .map(|(i, _)| range.start + i)
        .filter(|i| !is_inside_formula(*i))
        .collect::<Vec<_>>();
    boundaries.push(range.end);
    let mut soft = text
        .split_word_bound_indices()
        .map(|(i, word)| range.start + i + word.len())
        .filter(|i| !is_inside_formula(*i))
        .collect::<HashSet<_>>();
    for attachment in &attachments {
        soft.insert(attachment.range.start);
        soft.insert(attachment.range.end);
    }
    let mut output = Vec::new();
    let mut first = 0;
    while first + 1 < boundaries.len() {
        let start = boundaries[first];
        let x = f32::from(composed.x_for_index(start - range.start));
        let mut end = first + 1;
        let mut preferred = None;
        while end < boundaries.len() {
            let advance = f32::from(composed.x_for_index(boundaries[end] - range.start)) - x;
            if advance > width + 0.01 {
                break;
            }
            if soft.contains(&boundaries[end]) {
                preferred = Some(end);
            }
            end += 1;
        }
        let last = if end == boundaries.len() {
            end - 1
        } else {
            preferred.unwrap_or(end.saturating_sub(1).max(first + 1))
        };
        let line_range = start..boundaries[last];
        let mut line_attachments = attachments
            .iter()
            .filter(|a| a.range.start >= start && a.range.end <= line_range.end)
            .cloned()
            .collect::<Vec<_>>();
        // Shape again at the final source boundaries, retaining real kerning
        // within each text fragment and exact source indices for hit testing.
        let raw = measurement.shape_unwrapped(projection, line_range.clone(), font_size)?;
        let shaped = compose(raw, &line_range, &line_attachments, font_size);
        for attachment in &mut line_attachments {
            attachment.x = f32::from(shaped.x_for_index(attachment.range.start - start));
            // Retained geometry moves with its visual line when earlier text
            // changes; never retain absolute document offsets inside it.
            attachment.range = attachment.range.start - start..attachment.range.end - start;
        }
        output.push(InlineLine {
            range: line_range,
            attachments: line_attachments,
            font_size,
            width: shaped.width().into(),
            ascent: shaped.ascent.into(),
            descent: shaped.descent.into(),
        });
        first = last;
    }
    Some(output)
}

/// Keep every canonical glyph index, including indices inside TeX. Invisible
/// source glyph advances occupy the formula's measured box, so native caret,
/// selection and hit testing continue to speak original UTF-8 byte offsets.
pub(super) fn compose(
    mut line: ShapedLine,
    range: &Range<usize>,
    attachments: &[Attachment],
    font_size: f32,
) -> ShapedLine {
    if attachments.is_empty() {
        return line;
    }
    let factor = font_size / crate::math::EM;
    let spans = attachments
        .iter()
        .map(|attachment| {
            let start = attachment.range.start - range.start;
            let end = attachment.range.end - range.start;
            let x = f32::from(line.x_for_index(start));
            let old_width = f32::from(line.x_for_index(end)) - x;
            (
                start..end,
                x,
                old_width,
                attachment.light.width * factor + font_size * (2. / 9.),
            )
        })
        .collect::<Vec<_>>();
    let mut runs = line.runs.clone();
    for run in &mut runs {
        for glyph in &mut run.glyphs {
            let mut shift = 0.;
            let original = f32::from(glyph.position.x);
            for (source, start_x, old_width, advance) in &spans {
                if glyph.index >= source.end {
                    shift += advance - old_width;
                } else if glyph.index >= source.start {
                    let progress = if *old_width > 0. {
                        ((original - start_x) / old_width).clamp(0., 1.)
                    } else {
                        0.
                    };
                    shift += progress * (advance - old_width);
                    break;
                } else {
                    break;
                }
            }
            glyph.position.x += px(shift);
        }
    }
    let ascent = attachments
        .iter()
        .map(|a| a.light.baseline * factor)
        .fold(f32::from(line.ascent), f32::max);
    let descent = attachments
        .iter()
        .map(|a| (a.light.height - a.light.baseline) * factor)
        .fold(f32::from(line.descent), f32::max);
    let width = f32::from(line.width())
        + spans
            .iter()
            .map(|(_, _, old, advance)| advance - old)
            .sum::<f32>();
    *line = Arc::new(gpui::LineLayout {
        font_size: px(font_size),
        width: px(width),
        ascent: px(ascent),
        descent: px(descent),
        runs,
        len: line.len(),
    });
    line
}

pub(super) fn hide_sources(
    runs: Vec<TextRun>,
    range: &Range<usize>,
    attachments: &[Attachment],
) -> Vec<TextRun> {
    let mut output = Vec::new();
    let mut start = range.start;
    for run in runs {
        let end = start + run.len;
        let mut cuts = vec![start, end];
        for a in attachments {
            cuts.extend(
                [a.range.start, a.range.end]
                    .into_iter()
                    .filter(|i| *i > start && *i < end),
            );
        }
        cuts.sort_unstable();
        cuts.dedup();
        for pair in cuts.windows(2) {
            let mut piece = run.clone();
            piece.len = pair[1] - pair[0];
            if attachments
                .iter()
                .any(|a| a.range.start <= pair[0] && pair[1] <= a.range.end)
            {
                piece.color.a = 0.;
                piece.background_color = None;
                piece.underline = None;
                piece.strikethrough = None;
            }
            output.push(piece);
        }
        start = end;
    }
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn retained_math_offsets_survive_edits_before_the_paragraph(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let original = "# Title\n\nBefore $x^2$ after.\n\nClosing.\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(original).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.refresh_projection();
                let old = editor
                    .visual_lines
                    .iter()
                    .find_map(|line| line.inline_math.clone())
                    .unwrap();
                let old_image = old.attachments[0].light.image.id;
                editor.replace_range(0..0, "Prefix ", true, window, cx);
                let line = editor
                    .visual_lines
                    .iter()
                    .find(|line| line.inline_math.is_some())
                    .unwrap();
                let retained = line.inline_math.as_ref().unwrap();
                assert_eq!(old_image, retained.attachments[0].light.image.id);
                let raw = editor
                    .measurement
                    .shape_unwrapped(&editor.projection, line.range.clone(), line.style.font_size)
                    .unwrap();
                let shaped = compose(
                    raw,
                    &(0..line.range.len()),
                    &retained.attachments,
                    line.style.font_size,
                );
                assert_eq!(shaped.len(), line.range.len());
                assert_eq!(
                    &editor.projection.text()[line.range.start + retained.attachments[0].range.start
                        ..line.range.start + retained.attachments[0].range.end],
                    "x^2"
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
                let offset = editor.projection.text().find("x^2").unwrap();
                editor.move_to(offset + 1, window, cx);
                assert!(
                    !editor
                        .visual_lines
                        .iter()
                        .any(|line| line.inline_math.is_some())
                );
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
                editor.replace_range(offset + 1..offset + 1, "y", true, window, cx);
                assert!(
                    editor
                        .document
                        .snapshot()
                        .serialize()
                        .unwrap()
                        .contains("$xy^2$")
                );
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
                editor.move_to(0, window, cx);
                assert!(
                    editor
                        .visual_lines
                        .iter()
                        .any(|line| line.inline_math.is_some())
                );
                assert_eq!(editor.document.snapshot().serialize().unwrap(), original);
            });
        });
    }

    #[gpui::test]
    fn inline_math_wraps_as_atoms_without_changing_source_ranges(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = "# Title\n\nBefore $\\frac{a+b}{c+d}$ and $x^2$ after.\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[1];
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            for width in [140., 360., 760., 1200.] {
                let lines = layout(
                    &projection,
                    segment,
                    segment.projection_range.clone(),
                    width,
                    18.,
                    &measurement,
                )
                .unwrap();
                assert_eq!(
                    lines.first().unwrap().range.start,
                    segment.projection_range.start
                );
                assert_eq!(
                    lines.last().unwrap().range.end,
                    segment.projection_range.end
                );
                assert!(
                    lines
                        .windows(2)
                        .all(|pair| pair[0].range.end == pair[1].range.start)
                );
                assert_eq!(
                    lines
                        .iter()
                        .map(|line| line.attachments.len())
                        .sum::<usize>(),
                    2
                );
                for line in &lines {
                    assert!(line.width <= width + 0.1, "{} > {width}", line.width);
                    for attachment in &line.attachments {
                        assert!(attachment.range.end <= line.range.len());
                        assert!(
                            attachment.light.height * 18. / crate::math::EM
                                <= line.ascent + line.descent
                        );
                    }
                    let raw = measurement
                        .shape_unwrapped(&projection, line.range.clone(), 18.)
                        .unwrap();
                    let composed = compose(raw, &(0..line.range.len()), &line.attachments, 18.);
                    assert_eq!(composed.len(), line.range.len());
                    assert!((f32::from(composed.width()) - line.width).abs() < 0.01);
                    for attachment in &line.attachments {
                        assert!(
                            (f32::from(composed.x_for_index(attachment.range.start))
                                - attachment.x)
                                .abs()
                                < 0.01
                        );
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn editing_math_uses_canonical_source_and_leaves_other_paragraphs_rendered(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let mut document =
                Document::from_markdown("# Title\n\nFirst $x^2$.\n\nSecond $y^2$.\n").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let node = projection.segments()[1].node_id;
            document
                .apply(EditCommand::SetSelection(Selection::Text(
                    TextSelection::caret(DocumentPosition::new(node, 7, Affinity::Downstream)),
                )))
                .unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            assert_eq!(projection.math_edit_node, Some(node));
            let first = &projection.segments()[1];
            let second = &projection.segments()[2];
            assert!(
                layout(
                    &projection,
                    first,
                    first.projection_range.clone(),
                    760.,
                    18.,
                    &measurement
                )
                .is_none()
            );
            assert!(
                layout(
                    &projection,
                    second,
                    second.projection_range.clone(),
                    760.,
                    18.,
                    &measurement
                )
                .is_some()
            );
        });
    }
}
