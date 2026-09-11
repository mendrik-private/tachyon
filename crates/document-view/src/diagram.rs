//! Source-backed, inert technical figures. Parsing, layout and glyph outlining
//! happen during preparation, never during document paint or scroll.
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex, OnceLock},
};

use document_core::BlockNode;
use gpui::{Image, ImageFormat};
use mermaid_rs_renderer::{DiagramKind, Graph, LayoutConfig, Theme};

const MAX_SOURCE: usize = 8192;
const MAX_NODES: usize = 48;
const MAX_EDGES: usize = 96;
const MAX_CACHE: usize = 16;
pub(crate) const GAP: f32 = 24.;

pub(crate) struct Figure {
    pub image: Arc<Image>,
    pub width: f32,
    pub height: f32,
    pub description: String,
}

pub(crate) struct BlockDiagram {
    pub light: Arc<Figure>,
    pub dark: Arc<Figure>,
    pub source_visible: bool,
    kind: Kind,
}

impl BlockDiagram {
    pub fn centered(&self) -> bool {
        self.kind == Kind::Mermaid
    }

    pub fn accessible_name(&self) -> &'static str {
        match self.kind {
            Kind::Mermaid => "Rendered diagram",
            Kind::Schema => "Rendered schema tree",
        }
    }

    pub fn edit_hint(&self) -> &'static str {
        match self.kind {
            Kind::Mermaid => "Edit diagram · Enter. Pan · Left / Right",
            Kind::Schema => "Edit schema · Enter. Pan · Left / Right",
        }
    }

    pub fn extent(&self) -> f32 {
        if self.source_visible {
            self.light.height + GAP
        } else {
            0.
        }
    }

    pub fn for_dark(&self, dark: bool) -> &Arc<Figure> {
        if dark { &self.dark } else { &self.light }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DiagramError {
    TooComplex,
    Unsupported,
    Invalid,
    TooLarge,
}

type Cached = Result<Arc<Figure>, DiagramError>;
struct Entry {
    source: String,
    dark: bool,
    kind: Kind,
    result: Cached,
}
static CACHE: OnceLock<Mutex<VecDeque<Entry>>> = OnceLock::new();

pub(crate) fn is_diagram(block: &BlockNode) -> bool {
    kind(block).is_some()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Mermaid,
    Schema,
}

fn kind(block: &BlockNode) -> Option<Kind> {
    let BlockNode::CodeBlock(code) = block else {
        return None;
    };
    let language = code.language.as_deref()?;
    if language.eq_ignore_ascii_case("mermaid") {
        Some(Kind::Mermaid)
    } else if language.eq_ignore_ascii_case("json")
        && code.content.len() <= MAX_SOURCE
        && code.content.as_string().contains("\"$schema\"")
    {
        Some(Kind::Schema)
    } else {
        None
    }
}

pub(crate) fn prepare_block(block: &BlockNode, source_visible: bool) -> Option<Arc<BlockDiagram>> {
    let BlockNode::CodeBlock(code) = block else {
        return None;
    };
    let kind = kind(block)?;
    if code.content.len() > MAX_SOURCE {
        return None;
    }
    let source = code.content.as_string();
    Some(Arc::new(BlockDiagram {
        kind,
        light: cached(&source, false, kind).ok()?,
        dark: cached(&source, true, kind).ok()?,
        source_visible,
    }))
}

fn cached(source: &str, dark: bool, kind: Kind) -> Cached {
    if source.len() > MAX_SOURCE {
        return Err(DiagramError::TooComplex);
    }
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    {
        let mut entries = cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(index) = entries
            .iter()
            .position(|e| e.source == source && e.dark == dark && e.kind == kind)
        {
            let entry = entries.remove(index).expect("index belongs to this cache");
            let result = entry.result.clone();
            entries.push_front(entry);
            return result;
        }
    }
    let result = match kind {
        Kind::Mermaid => render(source, dark),
        Kind::Schema => crate::schema::render(source, dark),
    }
    .map(Arc::new);
    let mut entries = cache.lock().unwrap_or_else(|p| p.into_inner());
    entries.push_front(Entry {
        source: source.into(),
        dark,
        kind,
        result: result.clone(),
    });
    entries.truncate(MAX_CACHE);
    result
}

fn parse(source: &str) -> Result<Graph, DiagramError> {
    if source.len() > MAX_SOURCE {
        return Err(DiagramError::TooComplex);
    }
    validate_delimiters(source)?;
    let parsed =
        mermaid_rs_renderer::parse_mermaid_strict(source).map_err(|_| DiagramError::Invalid)?;
    let graph = parsed.graph;
    // Other Mermaid families and authored style/interaction overrides remain
    // readable source until they have their own tested semantic adapter. Never
    // silently drop a callback, image, style declaration or custom layout.
    if graph.kind != DiagramKind::Flowchart
        || parsed.init_config.is_some()
        || !graph.node_links.is_empty()
        || !graph.class_defs.is_empty()
        || !graph.node_styles.is_empty()
        || !graph.edge_styles.is_empty()
        || graph.edge_style_default.is_some()
        || !graph.subgraph_styles.is_empty()
        || graph.nodes.values().any(|n| n.icon.is_some())
    {
        return Err(DiagramError::Unsupported);
    }
    if graph.nodes.is_empty() {
        return Err(DiagramError::Invalid);
    }
    if graph
        .edges
        .iter()
        .any(|e| !graph.nodes.contains_key(&e.from) || !graph.nodes.contains_key(&e.to))
    {
        return Err(DiagramError::Invalid);
    }
    if graph.nodes.len() > MAX_NODES
        || graph.edges.len() > MAX_EDGES
        || graph.subgraphs.len() > 12
        || graph.nodes.values().any(|n| n.label.len() > 1024)
    {
        return Err(DiagramError::TooComplex);
    }
    Ok(graph)
}

// The pinned strict parser accepts an unfinished `A[label` as a plain node.
// Reject unfinished shapes rather than displaying a misleading partial graph.
fn validate_delimiters(source: &str) -> Result<(), DiagramError> {
    let mut stack = Vec::new();
    let mut quoted = false;
    for line in source.lines() {
        if line.trim_start().starts_with("%%") {
            continue;
        }
        for c in line.chars() {
            if c == '"' {
                quoted = !quoted;
                continue;
            }
            if quoted {
                continue;
            }
            match c {
                '[' | '(' | '{' => stack.push(c),
                ']' | ')' | '}' => {
                    let expected = match c {
                        ']' => '[',
                        ')' => '(',
                        _ => '{',
                    };
                    if stack.pop() != Some(expected) {
                        return Err(DiagramError::Invalid);
                    }
                }
                _ => {}
            }
        }
    }
    if quoted || !stack.is_empty() {
        Err(DiagramError::Invalid)
    } else {
        Ok(())
    }
}

fn graph_description(graph: &Graph, source: &str) -> String {
    let mut output = String::new();
    let mut multiline = false;
    for line in source.lines().map(str::trim) {
        if multiline {
            if line == "}" {
                multiline = false;
            } else {
                output.push_str(line);
                output.push(' ');
            }
        } else if let Some(text) = line
            .strip_prefix("accTitle:")
            .or_else(|| line.strip_prefix("accDescr:"))
        {
            output.push_str(text.trim());
            output.push(' ');
        } else if line == "accDescr {" {
            multiline = true;
        }
    }
    output.push_str("Flowchart. Nodes in source order: ");
    let mut nodes: Vec<_> = graph.nodes.values().collect();
    nodes.sort_by_key(|n| {
        (
            graph.node_order.get(&n.id).copied().unwrap_or(usize::MAX),
            &n.id,
        )
    });
    for (index, node) in nodes.iter().enumerate() {
        if index > 0 {
            output.push_str("; ");
        }
        output.push_str(&node.label);
    }
    output.push('.');
    for edge in &graph.edges {
        output.push(' ');
        output.push_str(&graph.nodes[&edge.from].label);
        output.push_str(if edge.arrow_start && edge.arrow_end {
            " connects both ways with "
        } else if edge.arrow_start {
            " receives from "
        } else if edge.arrow_end || edge.directed {
            " leads to "
        } else {
            " connects with "
        });
        output.push_str(&graph.nodes[&edge.to].label);
        if let Some(label) = &edge.label {
            output.push_str(" — ");
            output.push_str(label);
        }
        output.push('.');
    }
    output
}

fn fonts() -> Arc<usvg::fontdb::Database> {
    static FONTS: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONTS
        .get_or_init(|| {
            let mut db = usvg::fontdb::Database::new();
            db.load_font_data(
                include_bytes!("../../../assets/fonts/SplineSans-Tachyon-Regular.ttf").to_vec(),
            );
            db.load_font_data(
                include_bytes!("../../../assets/fonts/NotoSans-Tachyon-Regular.ttf").to_vec(),
            );
            db.load_font_data(
                include_bytes!("../../../assets/fonts/SplineSansMono-Tachyon-Regular.ttf").to_vec(),
            );
            db.set_sans_serif_family("Spline Sans Tachyon");
            Arc::new(db)
        })
        .clone()
}

fn render(source: &str, dark: bool) -> Result<Figure, DiagramError> {
    let graph = parse(source)?;
    let palette = crate::TachyonPalette::for_dark(dark);
    let hex = |color: u32| format!("#{color:06x}");
    let theme = Theme {
        font_family: "Spline Sans Tachyon".into(),
        font_size: 14.,
        primary_color: hex(palette.panel),
        primary_text_color: hex(palette.text),
        primary_border_color: hex(palette.accent),
        line_color: hex(palette.secondary),
        secondary_color: hex(palette.panel),
        tertiary_color: hex(palette.page),
        edge_label_background: hex(palette.page),
        cluster_background: hex(palette.panel),
        cluster_border: hex(palette.border),
        background: hex(palette.page),
        text_color: hex(palette.text),
        ..Theme::modern()
    };
    let config = LayoutConfig {
        node_spacing: 24.,
        rank_spacing: 48.,
        node_padding_x: 24.,
        node_padding_y: 16.,
        label_line_height: 1.5,
        ..LayoutConfig::default()
    };
    let layout = mermaid_rs_renderer::compute_layout(&graph, &theme, &config);
    let dimensions = mermaid_rs_renderer::measure_svg_dimensions(&layout, &config, None);
    if !dimensions.width.is_finite()
        || !dimensions.height.is_finite()
        || dimensions.width <= 0.
        || dimensions.height <= 0.
        || dimensions.width > 4096.
        || dimensions.height > 4096.
        || dimensions.width * dimensions.height > 2_000_000.
    {
        return Err(DiagramError::TooLarge);
    }
    let svg = mermaid_rs_renderer::render_svg(&layout, &theme, &config);
    // No network, file, embedded image or executable HTML resolver is permitted.
    // Text becomes deterministic bundled-font outlines before entering GPUI's
    // image cache; its SVG loader need not know our application-only fonts.
    outline_svg(&svg, graph_description(&graph, source))
}

pub(super) fn svg_options() -> usvg::Options<'static> {
    usvg::Options {
        fontdb: fonts(),
        font_family: "Spline Sans Tachyon".into(),
        image_href_resolver: usvg::ImageHrefResolver {
            resolve_data: Box::new(|_, _, _| None),
            resolve_string: Box::new(|_, _| None),
        },
        ..usvg::Options::default()
    }
}

pub(super) fn outline_svg(svg: &str, description: String) -> Result<Figure, DiagramError> {
    let tree = usvg::Tree::from_str(svg, &svg_options()).map_err(|_| DiagramError::Invalid)?;
    let outlined = tree.to_string(&usvg::WriteOptions::default());
    Ok(Figure {
        image: Arc::new(Image::from_bytes(ImageFormat::Svg, outlined.into_bytes())),
        width: tree.size().width(),
        height: tree.size().height(),
        description,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    const FLOW: &str = "flowchart LR\naccTitle: Review route\nA[Input] --> B[Process]\nB --> C{Decision}\nC -->|Yes| D[Output A]\nC -->|No| E[Output B]\n";

    #[test]
    fn flowchart_retains_topology_and_source_order_alternative() {
        let graph = parse(FLOW).unwrap();
        assert_eq!(graph.nodes.len(), 5);
        assert_eq!(graph.edges.len(), 4);
        let description = graph_description(&graph, FLOW);
        assert!(description.contains("Input; Process; Decision; Output A; Output B"));
        assert!(description.contains("Decision leads to Output A — Yes."));
        assert!(description.contains("Decision leads to Output B — No."));
    }

    #[test]
    fn diagrams_are_bounded_inert_outlined_and_deterministic() {
        let a = render(FLOW, false).unwrap();
        let b = render(FLOW, false).unwrap();
        assert_eq!(a.image.bytes, b.image.bytes);
        let svg = std::str::from_utf8(&a.image.bytes).unwrap();
        assert!(svg.contains("<path"));
        for forbidden in [
            "<text",
            "<script",
            "<image",
            "<foreignObject",
            "<filter",
            "href=",
        ] {
            assert!(!svg.contains(forbidden), "{forbidden}");
        }
        assert!(a.width > 400. && a.height > 100.);
        assert!(render(FLOW, true).is_ok());
        assert!(matches!(
            parse(&"x".repeat(MAX_SOURCE + 1)),
            Err(DiagramError::TooComplex)
        ));
    }

    #[test]
    fn unsupported_or_interactive_diagrams_keep_source_fallback() {
        for source in [
            "sequenceDiagram\nA->>B: Hi",
            "flowchart LR\nA-->B\nclick A \"https://example.com\"",
            "flowchart LR\nA[Unclosed",
            "%%{init: {'theme':'dark'}}%%\nflowchart LR\nA-->B",
        ] {
            assert!(parse(source).is_err(), "{source}");
        }
    }
}
