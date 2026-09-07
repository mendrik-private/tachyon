//! Inert formula previews. The canonical code node remains the editable source.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};

use document_core::BlockNode;
use gpui::{Image, ImageFormat};
use latex_rust::{Color, Dim, MathFont, MathStyle, SvgOptions};

pub(crate) mod semantics;

pub(crate) const EM: f32 = 24.;
const MAX_SOURCE_BYTES: usize = 2048;
const MAX_TOKENS: usize = 256;
const MAX_CACHE_ENTRIES: usize = 64;
pub(crate) const PREVIEW_GAP: f32 = 24.;

pub(crate) struct Formula {
    pub image: Arc<Image>,
    pub width: f32,
    pub height: f32,
    pub baseline: f32,
    pub semantics: Option<Arc<semantics::MathMarkup>>,
}

/// Prepared with the owning source node; independent of global LRU eviction.
pub(crate) struct BlockFormula {
    pub light: Arc<Formula>,
    pub dark: Arc<Formula>,
}

impl BlockFormula {
    pub fn extent(&self) -> f32 {
        self.light.height + PREVIEW_GAP
    }

    pub fn for_dark(&self, dark: bool) -> &Arc<Formula> {
        if dark { &self.dark } else { &self.light }
    }
}

pub(crate) fn prepare_block(block: &BlockNode) -> Option<Arc<BlockFormula>> {
    Some(Arc::new(BlockFormula {
        light: block_preview(block, crate::MineralPalette::LIGHT.text)?,
        dark: block_preview(block, crate::MineralPalette::for_dark(true).text)?,
    }))
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum FormulaError {
    TooComplex,
    Invalid(String),
    TooLarge,
}

struct Entry {
    source: String,
    color: u32,
    inline: bool,
    formula: Result<Arc<Formula>, FormulaError>,
}

static CACHE: OnceLock<Mutex<VecDeque<Entry>>> = OnceLock::new();

#[cfg(test)]
thread_local! {
    static FORMULA_REQUESTS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
pub(crate) fn test_formula_requests() -> usize {
    FORMULA_REQUESTS.get()
}

pub(crate) fn is_math(block: &BlockNode) -> bool {
    matches!(block, BlockNode::CodeBlock(code)
        if code.language.as_deref().is_some_and(|language| language.eq_ignore_ascii_case("math")))
}

pub(crate) fn block_preview(block: &BlockNode, color: u32) -> Option<Arc<Formula>> {
    let BlockNode::CodeBlock(code) = block else {
        return None;
    };
    if !is_math(block) || code.content.len() > MAX_SOURCE_BYTES {
        return None;
    }
    formula(&code.content.as_string(), color).ok()
}

fn formula(source: &str, color: u32) -> Result<Arc<Formula>, FormulaError> {
    cached_formula(source, color, false)
}

pub(crate) fn inline_formula(source: &str, color: u32) -> Result<Arc<Formula>, FormulaError> {
    cached_formula(source, color, true)
}

fn cached_formula(source: &str, color: u32, inline: bool) -> Result<Arc<Formula>, FormulaError> {
    #[cfg(test)]
    FORMULA_REQUESTS.set(FORMULA_REQUESTS.get() + 1);
    if source.len() > MAX_SOURCE_BYTES {
        return Err(FormulaError::TooComplex);
    }
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    {
        let mut entries = cache
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(index) = entries.iter().position(|entry| {
            entry.source == source && entry.color == color && entry.inline == inline
        }) {
            let entry = entries
                .remove(index)
                .expect("position came from this cache");
            let result = entry.formula.clone();
            entries.push_front(entry);
            return result;
        }
    }
    // Do not hold a shared lock while parsing or outlining glyphs. Failed
    // formulas are cached too, so malformed input is not retried every frame.
    let result = render_style(source, color, inline).map(Arc::new);
    let mut entries = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    entries.push_front(Entry {
        source: source.into(),
        color,
        inline,
        formula: result.clone(),
    });
    entries.truncate(MAX_CACHE_ENTRIES);
    result
}

#[cfg(test)]
fn render(source: &str, color: u32) -> Result<Formula, FormulaError> {
    render_style(source, color, false)
}

fn render_style(source: &str, color: u32, inline: bool) -> Result<Formula, FormulaError> {
    let invalid = |error: latex_rust::Error| FormulaError::Invalid(error.to_string());
    if source.len() > MAX_SOURCE_BYTES {
        return Err(FormulaError::TooComplex);
    }
    let tokens = latex_rust::tokenize(source).map_err(|error| invalid(error.into()))?;
    if tokens.len() > MAX_TOKENS {
        return Err(FormulaError::TooComplex);
    }
    // Bound recursive grouping before entering the renderer. The total token
    // bound also limits implicit groups (unbraced roots and scripts).
    let mut depth = 0usize;
    for ch in source.chars() {
        if ch == '{' {
            depth += 1;
        }
        if depth > 24 {
            return Err(FormulaError::TooComplex);
        }
        if ch == '}' {
            depth = depth.saturating_sub(1);
        }
    }
    let ast = latex_rust::parse(source).map_err(|error| invalid(error.into()))?;
    let font =
        MathFont::stix_two_math().map_err(|error| FormulaError::Invalid(error.to_string()))?;
    let tree = latex_rust::layout(
        &ast,
        &font,
        if inline {
            MathStyle::Text
        } else {
            MathStyle::Display
        },
    )
    .map_err(invalid)?;
    let width = f32::from_bits(tree.width.to_ieee32_bits()) * EM;
    let baseline = f32::from_bits(tree.height.to_ieee32_bits()) * EM;
    let height = baseline + f32::from_bits(tree.depth.to_ieee32_bits()) * EM;
    if !width.is_finite()
        || !height.is_finite()
        || !baseline.is_finite()
        || width <= 0.
        || height <= 0.
        || width > 4096.
        || height > 1024.
        || !(0. ..=height).contains(&baseline)
    {
        return Err(FormulaError::TooLarge);
    }
    let options = SvgOptions {
        // 18 points == 24 logical pixels in SVG's default 96 dpi units.
        font_size_pt: Dim::from_i64(18),
        color: Color::rgb((color >> 16) as u8, (color >> 8) as u8, color as u8),
        display: !inline,
    };
    let svg = latex_rust::render_svg(&tree, &font, &options).map_err(invalid)?;
    if svg.len() > 512 * 1024 {
        return Err(FormulaError::TooComplex);
    }
    Ok(Formula {
        image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
        width,
        height,
        baseline,
        semantics: semantics::from_ast(&ast).map(Arc::new),
    })
}

#[cfg(test)]
fn layout_formula(
    ast: &latex_rust::MathNode,
    font: &MathFont,
) -> Result<latex_rust::MathBox, latex_rust::Error> {
    latex_rust::layout(ast, font, MathStyle::Display)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inline_math_uses_text_style_and_a_distinct_retained_cache_entry() {
        let source = r"\sum_{i=1}^{n} i";
        let inline = inline_formula(source, 0).unwrap();
        let display = formula(source, 0).unwrap();
        assert!(!Arc::ptr_eq(&inline, &display));
        assert!(display.height > inline.height);
        assert!(Arc::ptr_eq(&inline, &inline_formula(source, 0).unwrap()));
    }

    #[test]
    fn real_formula_paths_and_metrics_are_cached_without_source_changes() {
        let source = r"\frac{-b \pm \sqrt{b^2-4ac}}{2a}";
        let first = formula(source, 0x202020).unwrap();
        let second = formula(source, 0x202020).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let svg = std::str::from_utf8(&first.image.bytes).unwrap();
        assert!(svg.contains("<path"));
        assert!(first.width > 60. && first.height > EM);
        assert!(first.baseline > 0. && first.baseline < first.height);
        assert!(!svg.contains("<script") && !svg.contains("href="));
        let recolored = formula(source, 0xffffff).unwrap();
        assert_ne!(recolored.image.id, first.image.id);
        assert_eq!(recolored.width, first.width);
    }

    #[test]
    fn matrix_rows_are_centered_inside_their_delimiters() {
        use latex_rust::{BoxContent, MathBox};
        fn unwrapped(tree: &MathBox) -> &MathBox {
            if let BoxContent::Overlap(children) = &tree.content
                && children.len() == 1
            {
                return unwrapped(&children[0]);
            }
            tree
        }
        fn check(tree: &MathBox) -> usize {
            match &tree.content {
                BoxContent::HList(children) => {
                    if children.len() == 3
                        && matches!(
                            unwrapped(&children[0]).content,
                            BoxContent::Glyph {
                                ch: '(' | '[' | '{' | '|' | '‖',
                                ..
                            }
                        )
                        && matches!(unwrapped(&children[1]).content, BoxContent::VList(_))
                    {
                        let left = &children[0];
                        let rows = &children[1];
                        let center = |node: &MathBox| {
                            f32::from_bits(
                                ((&node.height - &node.depth) / Dim::from_i64(2) + &node.shift)
                                    .to_ieee32_bits(),
                            )
                        };
                        assert!(
                            (center(left) - center(rows)).abs() < 0.001,
                            "matrix rows and delimiters must share their visual center: delimiter={}, rows={}",
                            center(left),
                            center(rows)
                        );
                        return 1;
                    }
                    children.iter().map(check).sum()
                }
                BoxContent::VList(children) | BoxContent::Overlap(children) => {
                    children.iter().map(check).sum()
                }
                BoxContent::Color(_, inner)
                | BoxContent::BackColor(_, inner)
                | BoxContent::Frame { inner, .. } => check(inner),
                _ => 0,
            }
        }
        let font = MathFont::stix_two_math().unwrap();
        for environment in [
            "pmatrix", "bmatrix", "Bmatrix", "vmatrix", "Vmatrix", "cases",
        ] {
            let source = format!(r"\begin{{{environment}}}a & b \\ c & d\end{{{environment}}}");
            for source in [
                source.clone(),
                format!(r"\frac{{{source}}}{{2}}"),
                format!(r"x^{{{source}}}"),
            ] {
                let ast = latex_rust::parse(&source).unwrap();
                let tree = layout_formula(&ast, &font).unwrap();
                assert_eq!(check(&tree), 1, "{source}");
                assert!(render(&source, 0).is_ok(), "{source}");
            }
        }
    }

    #[test]
    fn scripts_use_the_measured_glyph_scale() {
        let preview = formula("x^2", 0).unwrap();
        let svg = std::str::from_utf8(&preview.image.bytes).unwrap();
        let parsed = roxmltree::Document::parse(svg).unwrap();
        let scales = parsed
            .descendants()
            .filter(|node| node.has_tag_name("path"))
            .map(|node| {
                node.attribute("transform")
                    .unwrap()
                    .split("scale(")
                    .nth(1)
                    .unwrap()
                    .split_whitespace()
                    .next()
                    .unwrap()
                    .parse::<f32>()
                    .unwrap()
            })
            .collect::<Vec<_>>();
        assert_eq!(scales.len(), 2);
        assert!(
            (scales[1] / scales[0] - 0.7).abs() < 0.001,
            "script outline must match its 70% measurement: {scales:?}"
        );
    }

    #[test]
    fn invalid_and_excessive_formulas_keep_a_source_fallback() {
        assert!(formula(r"\notARealCommand{x}", 0).is_err());
        assert!(formula(r"\frac{", 0).is_err());
        assert!(matches!(
            formula(&"x".repeat(2049), 0),
            Err(FormulaError::TooComplex)
        ));
        assert!(formula(&format!("{}x{}", "{".repeat(25), "}".repeat(25)), 0).is_err());
        assert!(formula(r"\rule{999999em}{1em}", 0).is_err());
    }
}
