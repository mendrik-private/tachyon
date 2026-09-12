//! View-only prose typography. All break positions remain source byte offsets.
use super::*;
use hyphenation::{Hyphenator as _, Language, Load as _, Standard};
use std::sync::OnceLock;

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) struct Options {
    pub justify: bool,
    pub hyphenate: bool,
}

pub(super) fn eligible(projection: &TextProjection, segment: &crate::ProjectionSegment) -> bool {
    matches!(
        projection.block(segment.node_id),
        Some(BlockNode::Paragraph(_))
    ) && !segment.context.metadata
        && segment.context.table_cell.is_none()
        && segment.context.image_source.is_none()
        && segment.context.figure_text.is_none()
        && segment.context.resource_title_end.is_none()
        && !segment
            .context
            .definition
            .is_some_and(|(_, role)| role == document_core::DefinitionKind::Term)
        && !inline_math::has_attachments(projection, segment.node_id)
}

fn language(text: &str) -> Option<Language> {
    let info = whatlang::detect(text)?;
    if !info.is_reliable() {
        return None;
    }
    Some(match info.lang().code() {
        "eng" => Language::EnglishUS,
        "fin" => Language::Finnish,
        "swe" => Language::Swedish,
        "deu" => Language::German1996,
        "fra" => Language::French,
        "spa" => Language::Spanish,
        "ita" => Language::Italian,
        "nld" => Language::Dutch,
        "por" => Language::Portuguese,
        "dan" => Language::Danish,
        "nob" => Language::NorwegianBokmal,
        "est" => Language::Estonian,
        "lav" => Language::Latvian,
        "lit" => Language::Lithuanian,
        "pol" => Language::Polish,
        "ces" => Language::Czech,
        "slk" => Language::Slovak,
        "slv" => Language::Slovenian,
        "hrv" => Language::Croatian,
        "hun" => Language::Hungarian,
        "ron" => Language::Romanian,
        "bul" => Language::Bulgarian,
        "rus" => Language::Russian,
        "ukr" => Language::Ukrainian,
        "ell" => Language::GreekMono,
        "tur" => Language::Turkish,
        "afr" => Language::Afrikaans,
        "bel" => Language::Belarusian,
        "cat" => Language::Catalan,
        "epo" => Language::Esperanto,
        "kat" => Language::Georgian,
        "hye" => Language::Armenian,
        "ind" => Language::Indonesian,
        "mkd" => Language::Macedonian,
        _ => return None,
    })
}

fn dictionary(language: Language) -> Option<Arc<Standard>> {
    static DICTIONARIES: OnceLock<Mutex<HashMap<Language, Arc<Standard>>>> = OnceLock::new();
    let mut dictionaries = DICTIONARIES.get_or_init(Mutex::default).lock().ok()?;
    if let Some(dictionary) = dictionaries.get(&language) {
        return Some(dictionary.clone());
    }
    let dictionary = Arc::new(Standard::from_embedded(language).ok()?);
    dictionaries.insert(language, dictionary.clone());
    Some(dictionary)
}

/// Detect prose only, then exclude code, links, URLs and nonbreaking glue from
/// dictionary candidates. Uncertain/unsupported languages retain normal wrapping.
pub(super) fn breaks(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
) -> Vec<usize> {
    if !eligible(projection, segment) {
        return Vec::new();
    }
    let range = segment.projection_range();
    let text = &projection.text()[range.clone()];
    if text.len() > 16 * 1024 || measurement::contains_strong_rtl(text) {
        return Vec::new();
    }
    let Some(rich) = projection.block(segment.node_id).and_then(BlockNode::text) else {
        return Vec::new();
    };
    let excluded = rich
        .runs()
        .iter()
        .filter(|run| {
            run.styles.iter().any(|style| {
                !matches!(
                    style,
                    InlineStyle::Bold | InlineStyle::Italic | InlineStyle::Strikethrough
                )
            })
        })
        .map(|run| run.range.clone())
        .collect::<Vec<_>>();
    let words = text
        .unicode_word_indices()
        .filter(|(start, word)| {
            word.chars().all(char::is_alphabetic)
                && !excluded.iter().any(|span| {
                    span.start < segment.node_range.start + start + word.len()
                        && span.end > segment.node_range.start + *start
                })
                && {
                    let token_start = text[..*start]
                        .rfind(char::is_whitespace)
                        .map_or(0, |i| i + text[i..].chars().next().unwrap().len_utf8());
                    let token_end = text[*start..]
                        .find(char::is_whitespace)
                        .map_or(text.len(), |i| start + i);
                    !text[token_start..token_end].contains(['/', '@', '_', ':'])
                }
        })
        .collect::<Vec<_>>();
    let sample = words
        .iter()
        .map(|(_, word)| *word)
        .collect::<Vec<_>>()
        .join(" ");
    let Some(dictionary) = language(&sample).and_then(dictionary) else {
        return Vec::new();
    };
    let mut breaks = Vec::new();
    for (start, word) in words {
        // Do not defeat an author's explicit nonbreaking spaces or word joiners.
        if text[..start].ends_with(['\u{a0}', '\u{202f}', '\u{2060}'])
            || text[start + word.len()..].starts_with(['\u{a0}', '\u{202f}', '\u{2060}'])
        {
            continue;
        }
        breaks.extend(
            dictionary
                .hyphenate(word)
                .breaks
                .into_iter()
                .filter(|offset| {
                    word.is_char_boundary(*offset)
                        && word[..*offset].chars().count() >= 3
                        && word[*offset..].chars().count() >= 3
                })
                .map(|offset| range.start + start + offset),
        );
    }
    breaks
}

pub(super) fn with_suffix(text: &str, runs: &mut [TextRun], hyphen: bool) -> String {
    let mut display = text.to_owned();
    if hyphen && let Some(last) = runs.last_mut() {
        display.push('-');
        last.len += 1;
    }
    display
}

/// A label and its description are separate text flows even when they share
/// one source paragraph. Whitespace after the last visible word is not a
/// continuation, and an authored hard break ends the current line's flow.
pub(super) fn continues(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    range: &Range<usize>,
    part: Option<label_rows::Part>,
) -> bool {
    if matches!(
        part,
        Some(label_rows::Part::Label | label_rows::Part::StackedLabel)
    ) {
        return false;
    }
    let text = projection.text();
    if text[range.clone()].ends_with(['\n', '\r']) {
        return false;
    }
    text.get(range.end..segment.projection_range().end)
        .and_then(|tail| tail.split(['\n', '\r']).next())
        .is_some_and(|tail| !tail.trim().is_empty())
}

/// Expand ordinary inter-word spaces in glyph geometry, preserving source text,
/// decoration runs and the positions used by selection, links and IME.
pub(super) fn justify(mut line: ShapedLine, width: Pixels) -> ShapedLine {
    if measurement::contains_strong_rtl(&line.text) {
        return line;
    }
    let end = line.text.trim_end_matches(' ').len();
    let spaces = line.text[..end]
        .match_indices(' ')
        .filter_map(|(i, _)| (i > 0 && !line.text[..i].ends_with(' ')).then_some(i))
        .collect::<Vec<_>>();
    if spaces.is_empty() {
        return line;
    }
    let content_width = if end < line.text.len() {
        line.x_for_index(end)
    } else {
        line.width()
    };
    let extra = f32::from(width - content_width);
    if !extra.is_finite() || extra <= 0. {
        return line;
    }
    let step = extra / spaces.len() as f32;
    // Each gap may grow to at most twice its natural advance. If filling the
    // measure would exceed that limit, preserve the entire left-aligned line.
    let maximum_added_space = spaces
        .iter()
        .map(|space| f32::from(line.x_for_index(space + 1) - line.x_for_index(*space)))
        .fold(f32::INFINITY, f32::min);
    if step > maximum_added_space || maximum_added_space <= 0. {
        return line;
    }
    let mut runs = line.runs.clone();
    for glyph in runs.iter_mut().flat_map(|run| &mut run.glyphs) {
        glyph.position.x += px(step * spaces.partition_point(|space| *space < glyph.index) as f32);
    }
    *line = Arc::new(gpui::LineLayout {
        font_size: line.font_size,
        width: line.width() + px(extra),
        ascent: line.ascent,
        descent: line.descent,
        runs,
        len: line.len(),
    });
    line
}

/// Greedy wrapping over Unicode and dictionary opportunities. Whole-paragraph
/// advances nominate candidates; standalone shaping verifies the actual line
/// including its discretionary hyphen. Emergency breaks preserve graphemes.
pub(super) fn wrap(
    text: &str,
    layout: &gpui::LineLayout,
    hyphens: &[usize],
    width: f32,
    zoom: f32,
    mut measure: impl FnMut(Range<usize>, bool) -> Option<f32>,
) -> Option<Vec<Range<usize>>> {
    if hyphens.is_empty() || !width.is_finite() || text.contains(['\n', '\r']) {
        return None;
    }
    let graphemes = text
        .grapheme_indices(true)
        .map(|(i, _)| i)
        .chain(std::iter::once(text.len()))
        .collect::<Vec<_>>();
    let mut legal = line_breaks::opportunities(text, &graphemes);
    legal.extend(
        hyphens
            .iter()
            .copied()
            .filter(|i| graphemes.binary_search(i).is_ok()),
    );
    legal.sort_unstable();
    legal.dedup();
    let mut glyphs = layout.runs.iter().flat_map(|run| &run.glyphs).peekable();
    let points = graphemes
        .iter()
        .copied()
        .map(|i| {
            while glyphs.peek().is_some_and(|g| g.index < i) {
                glyphs.next();
            }
            let x = glyphs.peek().map_or(layout.width, |g| g.position.x);
            (i, f32::from(x) / zoom)
        })
        .collect::<Vec<_>>();
    if points.windows(2).any(|pair| pair[0].1 > pair[1].1) {
        return None;
    }
    let mut result = Vec::new();
    let mut start_ix = 0;
    let mut consecutive_hyphens = 0;
    let width = width.max(1.);
    while start_ix + 1 < points.len() {
        let (start, x) = points[start_ix];
        let fit = points
            .partition_point(|p| p.1 <= x + width)
            .saturating_sub(1)
            .max(start_ix + 1);
        let mut candidate = legal.partition_point(|end| *end <= points[fit].0);
        let mut end = start;
        while candidate > 0 && legal[candidate - 1] > start {
            let next = legal[candidate - 1];
            if consecutive_hyphens >= 2
                && (hyphens.binary_search(&next).is_ok() || text[..next].trim_end().ends_with('-'))
            {
                candidate -= 1;
                continue;
            }
            if measure(start..next, hyphens.binary_search(&next).is_ok())? <= width {
                end = next;
                break;
            }
            candidate -= 1;
        }
        if end == start {
            let mut index = fit;
            while index > start_ix + 1
                && ((consecutive_hyphens >= 2
                    && (hyphens.binary_search(&points[index].0).is_ok()
                        || text[..points[index].0].trim_end().ends_with('-')))
                    || measure(
                        start..points[index].0,
                        hyphens.binary_search(&points[index].0).is_ok(),
                    )? > width)
            {
                index -= 1;
            }
            end = points[index].0;
        }
        result.push(start..end);
        consecutive_hyphens =
            if hyphens.binary_search(&end).is_ok() || text[start..end].trim_end().ends_with('-') {
                consecutive_hyphens + 1
            } else {
                0
            };
        start_ix = points.binary_search_by_key(&end, |p| p.0).ok()?;
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENGLISH: &str = "The international organization provides comprehensive documentation and extraordinary opportunities for understanding typography and communication.";

    #[test]
    fn typography_detects_prose_and_preserves_protected_content() {
        assert_eq!(language(ENGLISH), Some(Language::EnglishUS));
        assert_eq!(
            language(
                "Suomen kielessä on paljon pitkiä yhdyssanoja. Automaattinen tavutus helpottaa tekstin lukemista ja parantaa kappaleiden ulkoasua."
            ),
            Some(Language::Finnish)
        );
        assert!(language("xy").is_none());
        for protected in [
            format!("# {ENGLISH}\n"),
            format!("```\n{ENGLISH}\n```\n"),
            format!("| Heading |\n| --- |\n| {ENGLISH} |\n"),
        ] {
            let document = Document::from_markdown(protected.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            assert!(
                projection
                    .segments()
                    .iter()
                    .all(|segment| breaks(&projection, segment).is_empty())
            );
        }
        let source = format!(
            "{ENGLISH} `internationalization` [internationalization](https://example.com) https://example.com/internationalization\n"
        );
        let document = Document::from_markdown(source.as_str()).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let segment = &projection.segments()[0];
        let candidates = breaks(&projection, segment);
        assert!(!candidates.is_empty());
        for &boundary in &candidates {
            let (start, word) = projection
                .text()
                .unicode_word_indices()
                .find(|(start, word)| *start < boundary && boundary < start + word.len())
                .unwrap();
            assert!(word[..boundary - start].chars().count() >= 3);
            assert!(word[boundary - start..].chars().count() >= 3);
        }
        let first_protected = projection.text().find("internationalization").unwrap();
        assert!(candidates.iter().all(|offset| *offset < first_protected));
        assert_eq!(document.snapshot().serialize().unwrap(), source);
    }

    #[gpui::test]
    fn typography_wraps_with_measured_hyphens_and_canonical_ranges(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            for source in [ENGLISH, "Suomen kielessä on paljon pitkiä yhdyssanoja. Automaattinen tavutus helpottaa tekstin lukemista ja parantaa kappaleiden ulkoasua."] {
                let document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let segment = &projection.segments()[0];
                for zoom in [0.75, 1., 2.] {
                    let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), zoom)
                        .with_typography(Options { justify: true, hyphenate: true });
                    let candidates = fonts.hyphen_breaks(&projection, segment);
                    assert!(!candidates.is_empty());
                    let mut hyphenated_lines = 0;
                    for width in [110., 180., 260.] {
                        let lines = fonts.wrap(&projection, segment, segment.projection_range(), width, 18.).unwrap();
                        let reconstructed = lines.iter().map(|r| &projection.text()[r.clone()]).collect::<String>();
                        assert_eq!(reconstructed, &projection.text()[segment.projection_range()]);
                        let mut streak = 0;
                        for range in &lines {
                            let hyphen = candidates.binary_search(&range.end).is_ok();
                            streak = if hyphen { streak + 1 } else { 0 };
                            assert!(streak <= 2, "stacked hyphens at width={width}, zoom={zoom}");
                            hyphenated_lines += usize::from(hyphen);
                            assert!(fonts.hyphenated_width(&projection, range.clone(), 18., hyphen).unwrap() <= width + 0.01);
                        }
                        assert_eq!(fonts.wrap(&projection, segment, segment.projection_range(), width, 18.).unwrap(), lines);
                    }
                    assert!(hyphenated_lines > 0);
                }
            }
        });
    }

    #[gpui::test]
    fn typography_justification_shares_caret_geometry_and_keeps_suffix_virtual(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let text = "Some extraordinary words ";
            let mut runs = vec![TextRun {
                len: text.len(),
                font: gpui::font("Public Sans Tachyon"),
                color: rgb(0).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
            let system = gpui::WindowTextSystem::new(cx.text_system().clone());
            let natural = system.shape_line(text.to_owned().into(), px(18.), &runs, None);
            let natural_space = natural.x_for_index(5) - natural.x_for_index(4);
            let width = natural.x_for_index(text.trim_end().len()) + natural_space;
            let line = justify(natural.clone(), width);
            assert_eq!(line.text.as_ref(), text);
            assert!((f32::from(line.x_for_index(text.trim_end().len()) - width)).abs() < 0.01);
            let word = text.find("words").unwrap();
            assert!(shaped_x_for_index(&line, word) > shaped_x_for_index(&natural, word));
            assert_eq!(
                shaped_index_for_x(&line, shaped_x_for_index(&line, word)),
                word
            );
            let source = "extraordi";
            runs[0].len = source.len();
            let display = with_suffix(source, &mut runs, true);
            let mut hyphenated = system
                .shape_line(display.into(), px(18.), &runs, None)
                .with_len(source.len());
            hyphenated.text = source.to_owned().into();
            assert_eq!(hyphenated.len(), source.len());
            assert!(
                shaped_caret_stops(&hyphenated)
                    .iter()
                    .all(|stop| stop.offset <= source.len())
            );
            assert_eq!(
                shaped_index_for_x(&hyphenated, hyphenated.width()),
                source.len()
            );
        });
    }

    #[gpui::test]
    fn typography_short_list_lines_keep_natural_spacing(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let source = "- Short label: Two words\n- A much longer label: More words\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        for _ in 0..5 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        let before = cx.update(|_, cx| {
            editor
                .read(cx)
                .painted_lines
                .iter()
                .filter(|line| line.layout.text.contains(' '))
                .map(|line| (line.layout.text.to_string(), f32::from(line.layout.width())))
                .collect::<HashMap<_, _>>()
        });
        assert!(!before.is_empty());
        cx.update(|_, cx| editor.update(cx, |editor, cx| editor.toggle_body_justification(cx)));
        for _ in 0..5 {
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            cx.run_until_parked();
        }
        cx.update(|_, cx| {
            let editor = editor.read(cx);
            for line in &editor.painted_lines {
                if let Some(width) = before.get(line.layout.text.as_ref()) {
                    assert!(
                        (f32::from(line.layout.width()) - width).abs() < 0.1,
                        "short list line {:?} stretched from {width} to {:?}",
                        line.layout.text,
                        line.layout.width()
                    );
                }
            }
        });
    }

    #[test]
    fn typography_final_visible_lines_do_not_continue_through_whitespace_or_hard_breaks() {
        for source in [
            "- Two words   \n",
            "- Two words  \n  Next line\n",
            "- Two words\n",
            "- Two words\n\n  Next paragraph\n",
        ] {
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            let start = segment.projection_start();
            assert!(
                !continues(
                    &projection,
                    segment,
                    &(start..start + "Two words".len()),
                    None
                ),
                "{source:?}"
            );
        }
        let document = Document::from_markdown("- Two words followed by more words\n").unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let segment = &projection.segments()[0];
        let start = segment.projection_start();
        assert!(continues(
            &projection,
            segment,
            &(start..start + "Two words ".len()),
            None
        ));
        assert!(!continues(
            &projection,
            segment,
            &(start..start + "Two words ".len()),
            Some(label_rows::Part::StackedLabel)
        ));
    }

    #[gpui::test]
    fn typography_excessive_word_spacing_stays_left_aligned(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let text = "Two words";
            let runs = [TextRun {
                len: text.len(),
                font: gpui::font("Public Sans Tachyon"),
                color: rgb(0).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            }];
            let natural = gpui::WindowTextSystem::new(cx.text_system().clone()).shape_line(
                text.into(),
                px(18.),
                &runs,
                None,
            );
            let result = justify(natural.clone(), natural.width() + px(300.));
            assert_eq!(
                result.width(),
                natural.width(),
                "excessive gaps must retain left alignment"
            );
        });
    }

    #[gpui::test]
    fn typography_toggles_preserve_selection_source_and_clipboard(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let source = format!("{ENGLISH}\n");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.set_selection(0..ENGLISH.len(), false, window, cx);
                let before = editor.selected_byte_range();
                editor.toggle_body_justification(cx);
                editor.toggle_hyphenation(cx);
                assert!(editor.body_justified() && editor.hyphenation_enabled());
                assert_eq!(editor.selected_byte_range(), before);
                editor.copy(&Copy, window, cx);
                assert_eq!(cx.read_from_clipboard().unwrap().text().unwrap(), ENGLISH);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                editor.toggle_body_justification(cx);
                assert!(!editor.body_justified() && editor.hyphenation_enabled());
                editor.toggle_hyphenation(cx);
                assert!(!editor.hyphenation_enabled());
            })
        });
    }
}
