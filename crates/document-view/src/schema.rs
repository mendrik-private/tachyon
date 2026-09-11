//! Lossless structural views of explicitly identified JSON Schema documents.
//! This is not a schema evaluator: even unknown keywords and reference URIs are
//! displayed in authored order, never followed or interpreted as valid/invalid.
use std::fmt;

use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::value::RawValue;

use crate::diagram::{DiagramError, Figure, outline_svg, svg_options};

const MAX_SOURCE: usize = 8192;
const MAX_ROWS: usize = 64;
const MAX_DEPTH: usize = 12;
const INSET: f32 = 16.;
const INDENT: f32 = 20.;
const GAP: f32 = 24.;
const ROW: f32 = 26.;
const HEADER: f32 = 36.;

// serde_json::Value would sort keys by default and normalize number spellings.
// Borrow raw values and keep the map traversal instead. No feature-wide map
// policy change, second canonical model, or numeric precision loss is needed.
struct Members<'a>(Vec<(String, &'a RawValue)>);

impl<'de> Deserialize<'de> for Members<'de> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Ordered;
        impl<'de> Visitor<'de> for Ordered {
            type Value = Members<'de>;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("a JSON object")
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut members = Vec::new();
                while let Some(member) = map.next_entry()? {
                    members.push(member);
                }
                Ok(Members(members))
            }
        }
        d.deserialize_map(Ordered)
    }
}

#[derive(Debug)]
struct Row<'a> {
    label: String,
    value: &'a str,
    parent: Option<usize>,
    depth: usize,
}

fn rows(source: &str) -> Result<Vec<Row<'_>>, DiagramError> {
    if source.len() > MAX_SOURCE {
        return Err(DiagramError::TooComplex);
    }
    let root: Members<'_> = serde_json::from_str(source).map_err(|_| DiagramError::Invalid)?;
    // Requiring an authored dialect keeps ordinary configuration JSON in its
    // familiar code pane. No inferred schema from `type` or `properties` keys.
    let dialects: Vec<_> = root.0.iter().filter(|(key, _)| key == "$schema").collect();
    let [(_, dialect)] = dialects.as_slice() else {
        return Err(DiagramError::Unsupported);
    };
    let dialect: String = serde_json::from_str(dialect.get()).map_err(|_| DiagramError::Invalid)?;
    if !matches!(
        dialect.as_str(),
        "https://json-schema.org/draft/2020-12/schema"
            | "https://json-schema.org/draft/2019-09/schema"
            | "http://json-schema.org/draft-07/schema#"
            | "http://json-schema.org/draft-06/schema#"
            | "http://json-schema.org/draft-04/schema#"
    ) {
        return Err(DiagramError::Unsupported);
    }
    let mut output = Vec::new();
    for (key, value) in root.0 {
        append(&mut output, quoted(&key), value, None, 0)?;
    }
    Ok(output)
}

fn quoted(text: &str) -> String {
    serde_json::to_string(text).expect("serializing a string cannot fail")
}

fn append<'a>(
    output: &mut Vec<Row<'a>>,
    label: String,
    raw: &'a RawValue,
    parent: Option<usize>,
    depth: usize,
) -> Result<(), DiagramError> {
    if output.len() >= MAX_ROWS || depth > MAX_DEPTH {
        return Err(DiagramError::TooComplex);
    }
    let value = raw.get();
    let index = output.len();
    let container = match value.as_bytes().first() {
        Some(b'{') => Some("object"),
        Some(b'[') => Some("array"),
        _ => None,
    };
    output.push(Row {
        label,
        value: container.unwrap_or(value),
        parent,
        depth,
    });
    match container {
        Some("object") => {
            let members: Members<'_> =
                serde_json::from_str(value).map_err(|_| DiagramError::Invalid)?;
            if members.0.is_empty() {
                output[index].value = "{}";
            }
            for (key, child) in members.0 {
                append(output, quoted(&key), child, Some(index), depth + 1)?;
            }
        }
        Some("array") => {
            let children: Vec<&RawValue> =
                serde_json::from_str(value).map_err(|_| DiagramError::Invalid)?;
            if children.is_empty() {
                output[index].value = "[]";
            }
            for (i, child) in children.into_iter().enumerate() {
                append(output, format!("[{i}]"), child, Some(index), depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn xml(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn text(id: &str, content: &str, x: f32, y: f32, color: u32) -> String {
    format!(
        "<text id=\"{id}\" x=\"{x}\" y=\"{y}\" fill=\"#{color:06x}\">{}</text>",
        xml(content)
    )
}

fn svg(body: &str, width: f32, height: f32) -> String {
    format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\"><g font-family=\"Spline Sans Mono Mineral\" font-size=\"14\" xml:space=\"preserve\">{body}</g></svg>"
    )
}

pub(super) fn render(source: &str, dark: bool) -> Result<Figure, DiagramError> {
    let rows = rows(source)?;
    let palette = crate::MineralPalette::for_dark(dark);
    let height = HEADER + rows.len() as f32 * ROW + INSET;
    let mut labels = String::new();
    for (i, row) in rows.iter().enumerate() {
        let y = HEADER + i as f32 * ROW + 18.;
        labels.push_str(&text(
            &format!("k{i}"),
            &row.label,
            INSET + row.depth as f32 * INDENT,
            y,
            palette.text,
        ));
        labels.push_str(&text(&format!("v{i}"), row.value, 0., y, palette.secondary));
    }
    // Shape with the very same bundled font and SVG pipeline that will paint.
    // A byte/character-count guess must not clip CJK, wide glyphs, or escapes.
    let measured = usvg::Tree::from_str(&svg(&labels, 4096., height), &svg_options())
        .map_err(|_| DiagramError::Invalid)?;
    let mut leading = 0_f32;
    let mut value_width = 0_f32;
    for i in 0..rows.len() {
        let key = measured
            .node_by_id(&format!("k{i}"))
            .ok_or(DiagramError::Invalid)?;
        let value = measured
            .node_by_id(&format!("v{i}"))
            .ok_or(DiagramError::Invalid)?;
        leading = leading.max(key.abs_bounding_box().right());
        value_width = value_width.max(value.abs_bounding_box().width());
    }
    let value_x = leading.ceil() + GAP;
    let width = (value_x + value_width.ceil() + INSET).max(320.);
    if width > 1600. || width * height > 2_000_000. {
        return Err(DiagramError::TooLarge);
    }
    let mut body = format!(
        "<rect x=\"0.5\" y=\"0.5\" width=\"{}\" height=\"{}\" rx=\"4\" fill=\"#{:06x}\" stroke=\"#{:06x}\"/>",
        width - 1.,
        height - 1.,
        palette.page,
        palette.border
    );
    body.push_str(&text(
        "title",
        "Schema structure",
        INSET,
        23.,
        palette.secondary,
    ));
    body.push_str(&format!(
        "<path d=\"M0 {HEADER} H{width}\" fill=\"none\" stroke=\"#{:06x}\"/>",
        palette.border
    ));
    let mut description = "JSON Schema structure. Source order; no schema validation. ".to_owned();
    for (i, row) in rows.iter().enumerate() {
        let y = HEADER + i as f32 * ROW + 18.;
        let x = INSET + row.depth as f32 * INDENT;
        if let Some(parent) = row.parent {
            let guide_x = x - INDENT + 4.;
            let start_y = HEADER + parent as f32 * ROW + 23.;
            let end_y = y - 5.;
            body.push_str(&format!(
                "<path d=\"M{guide_x} {start_y} V{end_y} H{}\" fill=\"none\" stroke=\"#{:06x}\"/>",
                x - 5.,
                palette.border
            ));
        }
        body.push_str(&text(&format!("k{i}"), &row.label, x, y, palette.text));
        body.push_str(&text(
            &format!("v{i}"),
            row.value,
            value_x,
            y,
            palette.secondary,
        ));
        description.push_str(&format!(
            "Level {}: {} = {}. ",
            row.depth + 1,
            row.label,
            row.value
        ));
    }
    outline_svg(&svg(&body, width, height), description)
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str = r#"{"$schema":"https://json-schema.org/draft/2020-12/schema","properties":{"z":{"type":"string"},"a":{"type":"number","default":1.00e+5}},"required":["z"],"extra":{},"other":[]}"#;

    #[test]
    fn schema_rows_keep_all_members_nesting_order_and_number_spelling() {
        let rows = rows(SOURCE).unwrap();
        let pairs: Vec<_> = rows
            .iter()
            .map(|r| (r.label.as_str(), r.value, r.depth))
            .collect();
        assert_eq!(
            pairs,
            vec![
                (
                    "\"$schema\"",
                    "\"https://json-schema.org/draft/2020-12/schema\"",
                    0
                ),
                ("\"properties\"", "object", 0),
                ("\"z\"", "object", 1),
                ("\"type\"", "\"string\"", 2),
                ("\"a\"", "object", 1),
                ("\"type\"", "\"number\"", 2),
                ("\"default\"", "1.00e+5", 2),
                ("\"required\"", "array", 0),
                ("[0]", "\"z\"", 1),
                ("\"extra\"", "{}", 0),
                ("\"other\"", "[]", 0),
            ]
        );
        assert_eq!(rows[3].parent, Some(2));
        assert_eq!(rows[4].parent, Some(1));
    }

    #[test]
    fn schema_rendering_is_measured_deterministic_inert_and_theme_aware() {
        let a = render(SOURCE, false).unwrap();
        let b = render(SOURCE, false).unwrap();
        let dark = render(SOURCE, true).unwrap();
        assert_eq!(a.image.bytes, b.image.bytes);
        assert_ne!(a.image.bytes, dark.image.bytes);
        assert_eq!((a.width, a.height), (dark.width, dark.height));
        assert!((320. ..=1600.).contains(&a.width));
        assert_eq!(
            a.height,
            HEADER + rows(SOURCE).unwrap().len() as f32 * ROW + INSET
        );
        let output = std::str::from_utf8(&a.image.bytes).unwrap();
        for tag in ["<text", "<image", "href=", "<script", "<foreignObject"] {
            assert!(!output.contains(tag), "{tag}");
        }
        assert!(a.description.contains("Level 3: \"default\" = 1.00e+5."));
        assert!(render(&SOURCE.replace("1.00e+5", "\"<script>&\\n\\\"\""), false).is_ok());
    }

    #[test]
    fn schema_detection_is_explicit_and_work_is_bounded() {
        for source in [
            "{}",
            "{\"type\":\"object\"}",
            "{\"$schema\":\"https://example.org/schema\"}",
            "{\"$schema\":false}",
            "{",
            "null",
        ] {
            assert!(rows(source).is_err(), "{source}");
        }
        assert!(rows(&" ".repeat(MAX_SOURCE + 1)).is_err());
        let many = format!(
            "{},\"large\":[{}]}}",
            &SOURCE[..SOURCE.len() - 1],
            vec!["0"; MAX_ROWS].join(",")
        );
        assert_eq!(rows(&many).unwrap_err(), DiagramError::TooComplex);
        let deep = format!(
            "{},\"deep\":{}0{}}}",
            &SOURCE[..SOURCE.len() - 1],
            "[".repeat(MAX_DEPTH + 1),
            "]".repeat(MAX_DEPTH + 1)
        );
        assert_eq!(rows(&deep).unwrap_err(), DiagramError::TooComplex);
        let duplicate = SOURCE.replace("\"extra\":{}", "\"extra\":{},\"extra\":false");
        assert_eq!(
            rows(&duplicate)
                .unwrap()
                .iter()
                .filter(|r| r.label == "\"extra\"")
                .count(),
            2
        );
        let wide = SOURCE.replace("1.00e+5", &quoted(&"w".repeat(300)));
        assert!(matches!(render(&wide, false), Err(DiagramError::TooLarge)));
        let duplicate_dialect = SOURCE.replace(
            "\"extra\":{}",
            "\"$schema\":\"https://json-schema.org/draft/2020-12/schema\"",
        );
        assert_eq!(
            rows(&duplicate_dialect).unwrap_err(),
            DiagramError::Unsupported
        );
    }
}
