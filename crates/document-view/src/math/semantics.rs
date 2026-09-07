//! Read-only MathML from the exact AST used for formula layout. Unsupported
//! constructs retain source-only accessibility rather than a partial equation.
use latex_rust::{AtomKind, Delimiter, EnvRow, IntegralKind, MathNode, MatrixStyle, TextStyle};

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct MathMarkup {
    pub tag: &'static str,
    pub text: String,
    pub children: Vec<Self>,
}

impl MathMarkup {
    fn token(tag: &'static str, text: impl Into<String>) -> Self {
        Self {
            tag,
            text: text.into(),
            children: Vec::new(),
        }
    }

    fn group(tag: &'static str, children: Vec<Self>) -> Self {
        Self {
            tag,
            text: String::new(),
            children,
        }
    }

    pub fn xml(&self) -> String {
        let mut xml = format!("<{}>", self.tag);
        for ch in self.text.chars() {
            match ch {
                '&' => xml.push_str("&amp;"),
                '<' => xml.push_str("&lt;"),
                '>' => xml.push_str("&gt;"),
                _ => xml.push(ch),
            }
        }
        for child in &self.children {
            xml.push_str(&child.xml());
        }
        xml.push_str(&format!("</{}>", self.tag));
        xml
    }
}

pub(super) fn from_ast(ast: &MathNode) -> Option<MathMarkup> {
    fn valid_text(node: &MathMarkup) -> bool {
        node.text.chars().all(|ch| matches!(ch, '\t' | '\n' | '\r' | '\u{20}'..='\u{d7ff}' | '\u{e000}'..='\u{fffd}' | '\u{10000}'..='\u{10ffff}'))
            && node.children.iter().all(valid_text)
    }
    let markup = convert(ast, &mut 1024, 0)?;
    valid_text(&markup).then_some(markup)
}

fn atom(ch: char, kind: AtomKind) -> MathMarkup {
    let tag = if ch.is_ascii_digit() {
        "mn"
    } else if kind == AtomKind::Ord && ch.is_alphabetic() {
        "mi"
    } else {
        "mo"
    };
    MathMarkup::token(tag, ch.to_string())
}

fn delimiter(delimiter: &Delimiter) -> Option<MathMarkup> {
    let text = match delimiter {
        Delimiter::Empty => return Some(MathMarkup::group("mrow", Vec::new())),
        Delimiter::Char(ch) => *ch,
        Delimiter::Named(name) => latex_rust::glyph_char(name).or(match name.as_str() {
            "{" => Some('{'),
            "}" => Some('}'),
            "|" => Some('|'),
            _ => None,
        })?,
    };
    Some(MathMarkup::token("mo", text.to_string()))
}

fn limits(
    base: MathMarkup,
    lower: Option<MathMarkup>,
    upper: Option<MathMarkup>,
    under_over: bool,
) -> MathMarkup {
    match (lower, upper) {
        (Some(lo), Some(hi)) => MathMarkup::group(
            if under_over { "munderover" } else { "msubsup" },
            vec![base, lo, hi],
        ),
        (Some(lo), None) => {
            MathMarkup::group(if under_over { "munder" } else { "msub" }, vec![base, lo])
        }
        (None, Some(hi)) => {
            MathMarkup::group(if under_over { "mover" } else { "msup" }, vec![base, hi])
        }
        (None, None) => base,
    }
}

fn convert(ast: &MathNode, budget: &mut usize, depth: usize) -> Option<MathMarkup> {
    *budget = budget.checked_sub(1)?;
    if depth > 32 {
        return None;
    }
    let mut child = |node: &MathNode| convert(node, budget, depth + 1);
    use MathNode::*;
    Some(match ast {
        Atom(ch, kind) => atom(*ch, *kind),
        Symbol(name) => atom(
            latex_rust::glyph_char(name)?,
            latex_rust::symbol_atom_kind(name),
        ),
        Fraction(top, bottom) => MathMarkup::group("mfrac", vec![child(top)?, child(bottom)?]),
        Radical(None, body) => MathMarkup::group("msqrt", vec![child(body)?]),
        Radical(Some(index), body) => MathMarkup::group("mroot", vec![child(body)?, child(index)?]),
        Superscript(base, script) => MathMarkup::group("msup", vec![child(base)?, child(script)?]),
        Subscript(base, script) => MathMarkup::group("msub", vec![child(base)?, child(script)?]),
        SubSup(base, sub, sup) => {
            MathMarkup::group("msubsup", vec![child(base)?, child(sub)?, child(sup)?])
        }
        Delimited(left, body, right) => MathMarkup::group(
            "mrow",
            vec![delimiter(left)?, child(body)?, delimiter(right)?],
        ),
        SizedDelim(value, _, _) => delimiter(value)?,
        Row(nodes) => {
            let mut children = Vec::<MathMarkup>::new();
            for (index, node) in nodes.iter().enumerate() {
                let mut next = child(node)?;
                if next.tag == "mo"
                    && next.text == "."
                    && matches!(nodes.get(index + 1), Some(Atom(ch, _)) if ch.is_ascii_digit())
                    && children
                        .last()
                        .is_none_or(|previous| previous.tag != "mn" || !previous.text.contains('.'))
                {
                    next.tag = "mn";
                }
                // Digits in one TeX row represent one number, not a product
                // of separately announced one-digit MathML tokens.
                if next.tag == "mn"
                    && let Some(previous) = children.last_mut()
                    && previous.tag == "mn"
                {
                    previous.text.push_str(&next.text);
                } else {
                    children.push(next);
                }
            }
            MathMarkup::group("mrow", children)
        }
        Sum(lo, hi) | Product(lo, hi) => {
            let base = MathMarkup::token("mo", if matches!(ast, Sum(..)) { "∑" } else { "∏" });
            let lo = match lo {
                Some(lo) => Some(child(lo)?),
                None => None,
            };
            let hi = match hi {
                Some(hi) => Some(child(hi)?),
                None => None,
            };
            limits(base, lo, hi, true)
        }
        Integral(kind, lo, hi) => {
            let glyph = match kind {
                IntegralKind::Int => "∫",
                IntegralKind::Iint => "∬",
                IntegralKind::Iiint => "∭",
                IntegralKind::Oint => "∮",
                IntegralKind::Oiint => "∯",
            };
            let lo = match lo {
                Some(lo) => Some(child(lo)?),
                None => None,
            };
            let hi = match hi {
                Some(hi) => Some(child(hi)?),
                None => None,
            };
            limits(MathMarkup::token("mo", glyph), lo, hi, false)
        }
        Limit(lo) => limits(
            MathMarkup::token("mo", "lim"),
            match lo {
                Some(lo) => Some(child(lo)?),
                None => None,
            },
            None,
            true,
        ),
        OverUnder(base, over, under) => {
            let base = child(base)?;
            let under = match under {
                Some(value) => Some(child(value)?),
                None => None,
            };
            let over = match over {
                Some(value) => Some(child(value)?),
                None => None,
            };
            limits(base, under, over, true)
        }
        Text(text, TextStyle::Text) => MathMarkup::token("mtext", text.clone()),
        Text(text, style) => MathMarkup::token(
            "mi",
            text.chars()
                .map(|ch| latex_rust::styled_char(ch, *style))
                .collect::<String>(),
        ),
        Operator(text, _) => MathMarkup::token("mo", text.clone()),
        Space(_) | Strut(..) | Label(_) | NoNumber => MathMarkup::group("mrow", Vec::new()),
        Color(_, body) | TextColor(_, body) | ColorBox(_, body) | FColorBox(_, _, body) => {
            child(body)?
        }
        Phantom(_, body) => MathMarkup::group("mphantom", vec![child(body)?]),
        Matrix(style, spec, rows) if spec.is_empty() => {
            let mut output = Vec::new();
            for row in rows {
                let EnvRow::Cells {
                    cells,
                    number: latex_rust::EqNumber::Default | latex_rust::EqNumber::Suppress,
                    ..
                } = row
                else {
                    return None;
                };
                output.push(MathMarkup::group(
                    "mtr",
                    cells
                        .iter()
                        .map(|cell| Some(MathMarkup::group("mtd", vec![child(cell)?])))
                        .collect::<Option<Vec<_>>>()?,
                ));
            }
            let table = MathMarkup::group("mtable", output);
            let (left, right) = match style {
                MatrixStyle::Matrix => return Some(table),
                MatrixStyle::Pmatrix => ("(", ")"),
                MatrixStyle::Bmatrix => ("[", "]"),
                MatrixStyle::BBmatrix => ("{", "}"),
                MatrixStyle::Vmatrix => ("|", "|"),
                MatrixStyle::VVmatrix => ("‖", "‖"),
                MatrixStyle::Cases => ("{", ""),
                _ => return None,
            };
            let mut children = vec![MathMarkup::token("mo", left), table];
            if !right.is_empty() {
                children.push(MathMarkup::token("mo", right));
            }
            MathMarkup::group("mrow", children)
        }
        // Do not claim a complete semantic translation for decorations,
        // references or numbered alignment structures not modeled above.
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fractions_roots_scripts_and_numbers_keep_their_tree() {
        let tree = from_ast(&latex_rust::parse(r"\frac{37}{\sqrt[3]{x_2^4}}").unwrap()).unwrap();
        let xml = tree.xml();
        let document = roxmltree::Document::parse(&xml).unwrap();
        let fraction = document
            .descendants()
            .find(|node| node.has_tag_name("mfrac"))
            .unwrap();
        assert!(fraction.children().any(|node| {
            node.descendants()
                .any(|n| n.has_tag_name("mn") && n.text() == Some("37"))
        }));
        let root = document
            .descendants()
            .find(|node| node.has_tag_name("mroot"))
            .unwrap();
        let children = root
            .children()
            .filter(|node| node.is_element())
            .collect::<Vec<_>>();
        assert!(
            children[0]
                .descendants()
                .any(|node| node.has_tag_name("msubsup"))
        );
        assert!(
            children[1]
                .descendants()
                .any(|node| node.text() == Some("3"))
        );
    }

    #[test]
    fn matrices_and_xml_text_preserve_cells_without_markup_injection() {
        let matrix =
            from_ast(&latex_rust::parse(r"\begin{pmatrix}17&23\\31&41\end{pmatrix}").unwrap())
                .unwrap()
                .xml();
        let xml = roxmltree::Document::parse(&matrix).unwrap();
        assert_eq!(
            xml.descendants()
                .filter(|node| node.has_tag_name("mtr"))
                .count(),
            2
        );
        assert_eq!(
            xml.descendants()
                .filter(|node| node.has_tag_name("mtd"))
                .count(),
            4
        );
        let escaped = MathMarkup::token("mtext", "<untrusted>&").xml();
        assert_eq!(
            roxmltree::Document::parse(&escaped)
                .unwrap()
                .root_element()
                .text(),
            Some("<untrusted>&")
        );
        assert!(from_ast(&MathNode::Ref("unknown".into())).is_none());
        assert!(from_ast(&MathNode::Text("bad\0text".into(), TextStyle::Text)).is_none());
        let decimals = from_ast(&latex_rust::parse("3.14+.5").unwrap())
            .unwrap()
            .xml();
        let parsed = roxmltree::Document::parse(&decimals).unwrap();
        assert_eq!(
            parsed
                .descendants()
                .filter(|node| node.has_tag_name("mn"))
                .filter_map(|node| node.text())
                .collect::<Vec<_>>(),
            ["3.14", ".5"]
        );
    }
}
