use crate::{DocumentError, DocumentSnapshot};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum StaticPageSize {
    #[default]
    A4Portrait,
    A4Landscape,
    LetterPortrait,
    LetterLandscape,
}

impl StaticPageSize {
    fn css(self) -> &'static str {
        match self {
            Self::A4Portrait => "A4 portrait",
            Self::A4Landscape => "A4 landscape",
            Self::LetterPortrait => "Letter portrait",
            Self::LetterLandscape => "Letter landscape",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StaticHtmlOptions {
    pub title: String,
    pub language: String,
    pub page_size: StaticPageSize,
}

impl Default for StaticHtmlOptions {
    fn default() -> Self {
        Self {
            title: "Document".into(),
            language: "en".into(),
            page_size: StaticPageSize::default(),
        }
    }
}

impl DocumentSnapshot {
    /// Export a standalone, inert document surface suitable for browser print
    /// or a paged-HTML renderer. The canonical snapshot remains unchanged.
    pub fn export_static_html(&self, options: &StaticHtmlOptions) -> Result<String, DocumentError> {
        let body = crate::markdown::static_html_blocks(self.blocks())?;
        let title = escape_html(&options.title);
        let language = normalized_language(&options.language);
        let page_size = options.page_size.css();
        Ok(format!(
            "<!doctype html>\n<html lang=\"{language}\">\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n<title>{title}</title>\n<style>\n{STYLE}\n@page {{ size: {page_size}; margin: 18mm 16mm 20mm; @top-left {{ content: string(document-title); }} @bottom-right {{ content: counter(page) \" / \" counter(pages); }} }}\n</style>\n</head>\n<body>\n<main><article aria-label=\"{title}\">\n{body}</article></main>\n</body>\n</html>\n"
        ))
    }
}

fn normalized_language(language: &str) -> &str {
    let valid = !language.is_empty()
        && language.len() <= 64
        && language
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-');
    if valid { language } else { "und" }
}

fn escape_html(value: &str) -> String {
    let mut output = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '&' => output.push_str("&amp;"),
            '<' => output.push_str("&lt;"),
            '>' => output.push_str("&gt;"),
            '"' => output.push_str("&quot;"),
            '\'' => output.push_str("&#39;"),
            _ => output.push(character),
        }
    }
    output
}

const STYLE: &str = r#":root { color-scheme: light; --paper:#fbfaf6; --ink:#17212b; --muted:#52616d; --rule:#9ba8b2; --accent:#245f8f; --panel:#f1f3f2; }
* { box-sizing: border-box; }
html { background:#d9dde0; }
body { margin:0; color:var(--ink); background:transparent; font:11pt/1.55 "Spline Sans", "Noto Sans", sans-serif; }
main { margin:24px auto; width:min(210mm, calc(100% - 32px)); }
article { min-height:297mm; padding:18mm 16mm 20mm; background:var(--paper); }
h1,h2,h3,h4,h5,h6 { margin:1.4em 0 .45em; color:#172019; font-family:"Liberation Serif", Georgia, serif; line-height:1.2; break-after:avoid-page; }
h1:first-of-type { margin-top:0; string-set:document-title content(text); }
p { margin:.45em 0 .9em; orphans:3; widows:3; }
a { color:var(--accent); text-decoration-thickness:.08em; text-underline-offset:.15em; }
blockquote,aside { margin:1em 0; padding:.65em .9em; border-left:2px solid var(--accent); background:var(--panel); break-inside:avoid-page; }
aside[data-alert] > strong:first-child::before { content:attr(data-alert) " · "; text-transform:capitalize; color:var(--accent); }
ul,ol { padding-left:1.6em; }
li { break-inside:avoid-page; }
.task-list { padding-left:0; list-style:none; }
.task-item > input + p { display:inline; }
input[type=checkbox] { appearance:none; width:.85em; height:.85em; margin:0 .45em 0 0; border:1px solid var(--muted); vertical-align:-.05em; }
input[type=checkbox]:checked::after { content:"✓"; display:block; color:var(--accent); font:bold .8em/1 sans-serif; transform:translate(.04em,-.08em); }
pre { position:relative; margin:1em 0; padding:2em .9em .8em; overflow-wrap:anywhere; white-space:pre-wrap; border:1px solid var(--rule); background:#20292c; color:#f0eee5; font:9pt/1.5 "Spline Sans Mono", monospace; }
pre[data-language]::before { content:attr(data-language); position:absolute; top:.35em; right:.7em; color:#a9c8dc; font:8pt/1.2 sans-serif; }
code { font-family:"Spline Sans Mono", monospace; }
table { margin:1em 0; border-collapse:collapse; max-width:100%; break-inside:auto; }
table:not(.authored-widths) { width:auto; }
table.authored-widths { max-width:none; table-layout:fixed; }
thead { display:table-header-group; }
tr { break-inside:avoid-page; }
th,td { padding:.35em .55em; border:1px solid var(--rule); vertical-align:top; }
th { background:var(--panel); text-align:start; }
img,svg { max-width:100%; height:auto; break-inside:avoid-page; }
figure,section[role=doc-footnote] { break-inside:avoid-page; }
details:not([open]) > :not(summary) { display:block; }
summary { font-weight:600; }
hr { margin:1.5em 0; border:0; border-top:1px solid var(--rule); }
@media print { html,body,main,article { width:auto; min-height:0; margin:0; padding:0; background:white; } nav,button,[role=toolbar] { display:none !important; } a { color:inherit; } }
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Document;

    #[test]
    fn static_export_is_inert_semantic_and_preserves_authored_table_widths() {
        let source = "# Report & review\n\n- [x] Done\n\n<!-- mineral-table:v1 {\"border\":\"LogicalPixel\",\"widths\":[160,320]} -->\n| Key | Value |\n| --- | --- |\n| Mode | Local |\n\n```rust\nfn main() {}\n```\n\n<script>alert('bad')</script>\n";
        let document = Document::from_markdown(source).unwrap();
        let before = document.snapshot().serialize().unwrap();
        let html = document
            .snapshot()
            .export_static_html(&StaticHtmlOptions {
                title: "A <report> & notes".into(),
                language: "en-GB".into(),
                page_size: StaticPageSize::A4Landscape,
            })
            .unwrap();
        assert!(html.starts_with("<!doctype html>\n<html lang=\"en-GB\">"));
        assert!(html.contains("<title>A &lt;report&gt; &amp; notes</title>"));
        assert!(html.contains("<h1 id=\"report--review\">Report &amp; review</h1>"));
        assert!(html.contains("@page { size: A4 landscape"));
        assert!(html.contains("<thead>"));
        assert!(html.contains("class=\"authored-widths\" style=\"width:480px\""));
        assert!(html.contains("<col style=\"width:160px\">"));
        assert!(html.contains("<col style=\"width:320px\">"));
        assert!(html.contains("<pre data-language=\"rust\">"));
        assert!(html.contains("type=\"checkbox\" checked disabled"));
        assert!(html.contains("<ul class=\"task-list\">"));
        assert!(html.contains("<li class=\"task-item\">"));
        assert!(!html.contains("<script"));
        assert!(!html.contains("Copy"));
        assert_eq!(document.snapshot().serialize().unwrap(), before);
    }

    #[test]
    fn invalid_document_language_falls_back_without_attribute_injection() {
        let html = Document::empty()
            .snapshot()
            .export_static_html(&StaticHtmlOptions {
                language: "en\" onload=bad".into(),
                ..StaticHtmlOptions::default()
            })
            .unwrap();
        assert!(html.contains("<html lang=\"und\">"));
        assert!(!html.contains("onload"));
    }

    #[test]
    fn static_heading_ids_match_gfm_fragments_and_disambiguate_duplicates() {
        let html = Document::from_markdown(
            "# Café **guide**\n\n## Café guide\n\n[Second](#caf%C3%A9-guide-1)\n",
        )
        .unwrap()
        .snapshot()
        .export_static_html(&StaticHtmlOptions::default())
        .unwrap();
        assert!(html.contains("<h1 id=\"café-guide\">"), "{html}");
        assert!(html.contains("<h2 id=\"café-guide-1\">"), "{html}");
        assert!(html.contains("href=\"#caf%C3%A9-guide-1\""), "{html}");
    }
}
