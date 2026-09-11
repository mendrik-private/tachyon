//! Bounded, inert Blitz layout/paint. The DOM is temporary rendering state;
//! canonical HTML bytes and editor identity remain in document-core.
use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, OnceLock},
};

use anyrender::ImageRenderer;
use anyrender_vello_cpu::VelloCpuImageRenderer;
use blitz_dom::{DocumentConfig, StyleThreading};
use blitz_html::HtmlDocument;
use blitz_traits::{
    net::DummyNetProvider,
    shell::{ColorScheme, Viewport},
};
use document_core::{BlockNode, inert_html_fragment};
use gpui::{Image, ImageFormat};
use unicode_segmentation::UnicodeSegmentation;

use crate::MineralPalette;

mod accessibility;
mod fonts;
pub(crate) mod images;

#[derive(Debug)]
pub(crate) struct HtmlPreview {
    pub source: Arc<str>,
    pub image: Arc<Image>,
    pub width: f32,
    pub height: f32,
    pub text: String,
    /// Readable source-order text from the resolved, currently visible DOM.
    /// Unlike copy/conversion text, this excludes closed and CSS-hidden bodies.
    pub accessible_text: String,
    pub can_convert: bool,
    /// Retained native text geometry, in unscaled fragment coordinates. No DOM
    /// or shaping work is performed in pointer or scrolling handlers.
    pub text_hits: Vec<HtmlTextHit>,
    /// Legal user-perceived character edges, cached from the exact text leaves.
    /// Shaping clusters may split a combining accent or a joined emoji.
    caret_stops: Vec<HtmlCaretStop>,
    /// Immutable semantic conversion text, used only for temporary selection.
    pub editable_text: String,
    pub text_ranges: Vec<std::ops::Range<usize>>,
    pub links: Vec<HtmlLink>,
    pub disclosures: Vec<HtmlDisclosure>,
    pub anchors: Vec<HtmlAnchor>,
    #[cfg(test)]
    glyph_counts: (usize, usize),
}

#[derive(Clone, Debug)]
pub(crate) struct HtmlAnchor {
    pub name: String,
    pub y: Option<f32>,
    pub text_byte: Option<usize>,
    /// Only closed containing bodies, outermost first. A summary does not
    /// require opening its own details element.
    pub closed_ancestors: Vec<usize>,
}

pub(crate) type DisclosureOverrides = BTreeMap<usize, bool>;

/// A view-only choice, valid only for these exact canonical source bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DisclosureState {
    pub source: Arc<str>,
    pub overrides: DisclosureOverrides,
}

#[derive(Debug)]
pub(crate) struct HtmlDisclosure {
    pub ordinal: usize,
    pub authored_open: bool,
    pub open: bool,
    pub label: String,
    pub bounds: [f32; 4],
}

#[derive(Debug)]
pub(crate) struct HtmlLink {
    /// Present only when the complete link text has exact conversion/caret
    /// correspondence. Opaque Blitz geometry remains clickable but never
    /// becomes an inferred editable range.
    pub range: Option<std::ops::Range<usize>>,
    pub target: String,
    pub bounds: Vec<[f32; 4]>,
}

#[derive(Clone, Debug)]
pub(crate) struct HtmlTextHit {
    // Used only to bind anchor carets while this preview's temporary DOM lives.
    dom_node: blitz_dom::NodeId,
    pub bounds: [f32; 4],
    pub left: document_core::HtmlTextPosition,
    pub right: document_core::HtmlTextPosition,
}

#[derive(Debug)]
struct HtmlCaretStop {
    position: document_core::HtmlTextPosition,
    bounds: [f32; 4],
}

impl HtmlPreview {
    pub fn link_at(&self, x: f32, y: f32) -> Option<&HtmlLink> {
        self.links.iter().find(|link| {
            link.bounds
                .iter()
                .any(|b| x >= b[0] && x < b[2] && y >= b[1] && y < b[3])
        })
    }

    pub fn link_at_byte(&self, byte: usize) -> Option<&HtmlLink> {
        self.links.iter().find(|link| {
            link.range
                .as_ref()
                .is_some_and(|range| range.contains(&byte) || range.end == byte)
        })
    }

    pub fn byte_for_position(&self, position: document_core::HtmlTextPosition) -> Option<usize> {
        let range = self.text_ranges.get(position.text_node)?;
        let offset = range.start.checked_add(position.byte_offset)?;
        (offset <= range.end && self.editable_text.is_char_boundary(offset)).then_some(offset)
    }

    pub fn position_for_byte(&self, byte: usize) -> Option<document_core::HtmlTextPosition> {
        let index = self
            .text_ranges
            .partition_point(|range| range.start <= byte)
            .checked_sub(1)?;
        let range = &self.text_ranges[index];
        (byte <= range.end && self.editable_text.is_char_boundary(byte)).then_some(
            document_core::HtmlTextPosition {
                text_node: index,
                byte_offset: byte - range.start,
            },
        )
    }

    pub fn caret_bounds(&self, byte: usize) -> Option<[f32; 4]> {
        self.caret_stops
            .iter()
            .filter_map(|stop| {
                let [x, top, _, bottom] = stop.bounds;
                Some((
                    self.byte_for_position(stop.position)?.abs_diff(byte),
                    [x, top, x + 1.5, bottom],
                ))
            })
            .min_by_key(|(distance, _)| *distance)
            .map(|(_, bounds)| bounds)
    }

    /// Logical range of the measured line containing this legal caret.
    pub fn line_range(&self, byte: usize) -> Option<std::ops::Range<usize>> {
        let caret = self.caret_bounds(byte)?;
        let mut range = byte..byte;
        for stop in &self.caret_stops {
            if (stop.bounds[1] - caret[1]).abs() < 1. {
                let byte = self.byte_for_position(stop.position)?;
                range.start = range.start.min(byte);
                range.end = range.end.max(byte);
            }
        }
        Some(range)
    }

    /// Nearest legal grapheme edge on the next measured row, or the first/
    /// last visible row when entering a fragment from an adjacent block.
    pub fn vertical_position(
        &self,
        from: Option<usize>,
        direction: isize,
        x: f32,
    ) -> Option<document_core::HtmlTextPosition> {
        let y = from
            .and_then(|byte| self.caret_bounds(byte))
            .map(|caret| caret[1]);
        let row = self
            .caret_stops
            .iter()
            .filter(|stop| y.is_none_or(|y| (stop.bounds[1] - y) * direction as f32 > 1.))
            .min_by(|a, b| {
                (a.bounds[1] * direction as f32).total_cmp(&(b.bounds[1] * direction as f32))
            })?
            .bounds[1];
        self.caret_stops
            .iter()
            .filter(|stop| (stop.bounds[1] - row).abs() < 1.)
            .min_by(|a, b| (a.bounds[0] - x).abs().total_cmp(&(b.bounds[0] - x).abs()))
            .map(|stop| stop.position)
    }

    pub fn text_position_at(&self, x: f32, y: f32) -> Option<document_core::HtmlTextPosition> {
        if !(x >= 0. && x <= self.width && y >= 0. && y <= self.height) {
            return None;
        }
        // Whitespace has no glyph in the verified map. Snap to the nearest
        // retained glyph edge, just as a text editor does in a line's blank
        // area. Falling through here would edit unrelated surrounding Markdown.
        let distance = |stop: &HtmlCaretStop| {
            let [left, top, right, bottom] = stop.bounds;
            (x - x.clamp(left, right)).powi(2) + (y - y.clamp(top, bottom)).powi(2)
        };
        self.caret_stops
            .iter()
            .min_by(|a, b| distance(a).total_cmp(&distance(b)))
            .map(|stop| stop.position)
    }
}

fn caret_stops(hits: &[HtmlTextHit], texts: &[String]) -> Vec<HtmlCaretStop> {
    let boundaries: Vec<Vec<_>> = texts
        .iter()
        .map(|text| {
            text.grapheme_indices(true)
                .map(|(offset, _)| offset)
                .chain(std::iter::once(text.len()))
                .collect()
        })
        .collect();
    hits.iter()
        .flat_map(|hit| {
            [(hit.left, hit.bounds[0]), (hit.right, hit.bounds[2])]
                .into_iter()
                .filter_map(|(position, x)| {
                    boundaries
                        .get(position.text_node)?
                        .binary_search(&position.byte_offset)
                        .ok()?;
                    Some(HtmlCaretStop {
                        position,
                        bounds: [x, hit.bounds[1], x, hit.bounds[3]],
                    })
                })
        })
        .collect()
}

/// Accept only a complete, source-order correspondence. Matching a substring
/// would silently misaddress repeated paragraphs, hidden text or generated CSS
/// content. Whitespace may collapse in HTML; caret addresses remain byte
/// offsets in the canonical conversion, not offsets in a normalized copy.
fn text_hits(
    doc: &HtmlDocument,
    texts: &[String],
    leaves: &[document_core::HtmlConversionLeaf],
) -> Option<Vec<HtmlTextHit>> {
    use document_core::HtmlTextPosition;
    let canonical: Vec<_> = texts
        .iter()
        .enumerate()
        .filter(|(ordinal, _)| {
            leaves
                .get(*ordinal)
                .is_some_and(|leaf| !leaf.image_description)
        })
        .flat_map(|(text_node, text)| {
            text.char_indices()
                .filter(|(_, ch)| !ch.is_whitespace())
                .map(move |(byte_offset, ch)| {
                    (
                        ch,
                        HtmlTextPosition {
                            text_node,
                            byte_offset,
                        },
                        HtmlTextPosition {
                            text_node,
                            byte_offset: byte_offset + ch.len_utf8(),
                        },
                    )
                })
        })
        .collect();
    if canonical.is_empty() || canonical.len() > 4096 {
        return None;
    }
    let mut stack = vec![doc.root_node().id];
    let mut visited = std::collections::HashSet::new();
    let mut roots = Vec::new();
    let mut rendered = String::new();
    while let Some(id) = stack.pop() {
        if !visited.insert(id) {
            continue;
        }
        if visited.len() > 2048 {
            return None;
        }
        let node = doc.get_node(id)?;
        // Absolute axis-aligned boxes cannot describe authored transforms.
        // Keep explicit whole-fragment conversion for these uncommon cases.
        if node.transform().is_some() {
            return None;
        }
        if node.flags.is_inline_root()
            && let Some(layout) = node
                .element_data()
                .and_then(|element| element.inline_layout_data.as_ref())
            && !layout.text.trim().is_empty()
        {
            // Blitz prepends an inside marker to a summary's inline text.
            // Remove only the prefix identified by its actual layout metadata,
            // never an arbitrary arrow that could belong to authored content.
            let prefix = match node.element_data().and_then(|element| {
                (element.name.local.as_ref() == "summary")
                    .then_some(element.list_item_data.as_deref())
                    .flatten()
            }) {
                Some(blitz_dom::node::ListItemLayout {
                    marker,
                    position: blitz_dom::node::ListItemLayoutPosition::Inside,
                }) => match marker {
                    // Parley may collapse the trailing marker space. Leave
                    // any remaining whitespace to the normal mapping below.
                    blitz_dom::node::Marker::Char(ch) => ch.to_string(),
                    blitz_dom::node::Marker::String(text) => text.trim_end().to_owned(),
                },
                _ => String::new(),
            };
            rendered.extend(
                layout
                    .text
                    .strip_prefix(&prefix)?
                    .chars()
                    .filter(|ch| !ch.is_whitespace()),
            );
            roots.push((node, prefix.len()));
        }
        if let Some(children) = node.layout_children.borrow().as_ref() {
            stack.extend(children.iter().rev().copied());
        }
    }
    if !rendered.chars().eq(canonical.iter().map(|entry| entry.0)) {
        return None;
    }
    let mut hits = Vec::new();
    let mut base = 0;
    for (node, prefix_len) in roots {
        let inline = node.element_data()?.inline_layout_data.as_ref()?;
        let layout = &inline.layout;
        let scale = layout.scale();
        let frame = node.final_layout();
        let origin = node.absolute_position(
            frame.border.left + frame.padding.left,
            frame.border.top + frame.padding.top,
        );
        let clip = text_clip_bounds(doc, node)?;
        let mut offsets = vec![0; inline.text.len() + 1];
        let mut count = base;
        for (index, ch) in inline.text.char_indices() {
            offsets[index..index + ch.len_utf8()].fill(count);
            count += usize::from(index >= prefix_len && !ch.is_whitespace());
        }
        offsets[inline.text.len()] = count;
        for line in layout.lines() {
            // Inline boxes have separate positioning and may interleave text.
            // Do not approximate their contribution by summing glyph advances.
            if line.len() != line.runs().count() {
                return None;
            }
            let metrics = line.metrics();
            let mut x = metrics.offset;
            for run in line.runs() {
                for cluster in run.visual_clusters() {
                    let range = cluster.text_range();
                    let start = *offsets.get(range.start)?;
                    let end = *offsets.get(range.end)?;
                    let next_x = x + cluster.advance();
                    if start < end {
                        let before = canonical.get(start)?.1;
                        let after = canonical.get(end - 1)?.2;
                        let bounds = [
                            origin.x + x / scale,
                            origin.y + (metrics.baseline - metrics.ascent) / scale,
                            origin.x + next_x / scale,
                            origin.y + (metrics.baseline + metrics.descent) / scale,
                        ];
                        if bounds[0] < clip[0]
                            || bounds[1] < clip[1]
                            || bounds[2] > clip[2]
                            || bounds[3] > clip[3]
                        {
                            return None;
                        }
                        // Validate against Blitz's actual hit testing too: this
                        // rejects occlusion/clipping and incompatible geometry.
                        let hit =
                            doc.hit((bounds[0] + bounds[2]) * 0.5, (bounds[1] + bounds[3]) * 0.5)?;
                        let hit_node = doc.get_node(hit.node_id)?;
                        if hit_node.id != node.id
                            && hit_node.inline_root_ancestor().map(|root| root.id) != Some(node.id)
                        {
                            return None;
                        }
                        hits.push(HtmlTextHit {
                            dom_node: hit_node.id,
                            bounds,
                            left: if cluster.is_rtl() { after } else { before },
                            right: if cluster.is_rtl() { before } else { after },
                        });
                    }
                    x = next_x;
                }
            }
        }
        base = count;
    }
    Some(hits)
}

/// Blitz's point hit test does not enforce overflow clipping. Retained text
/// boxes must fit the resolved ancestor clips too; otherwise keep explicit
/// conversion rather than exposing an invisible direct-edit caret. This is
/// computed once per inline root during preview creation, never on scrolling.
fn text_clip_bounds<'a>(doc: &'a HtmlDocument, mut node: &'a blitz_dom::Node) -> Option<[f32; 4]> {
    let mut clip = [
        f32::NEG_INFINITY,
        f32::NEG_INFINITY,
        f32::INFINITY,
        f32::INFINITY,
    ];
    for _ in 0..64 {
        let overflow = node.style().overflow;
        // Taffy's default is Visible. Match the pinned Blitz painter, which
        // currently clips both axes when either overflow axis is non-visible.
        if overflow.x != Default::default() || overflow.y != Default::default() {
            let frame = node.final_layout();
            let origin = node.absolute_position(0., 0.);
            clip[0] = clip[0].max(origin.x + frame.border.left);
            clip[1] = clip[1].max(origin.y + frame.border.top);
            clip[2] = clip[2].min(origin.x + frame.size.width - frame.border.right);
            clip[3] = clip[3].min(origin.y + frame.size.height - frame.border.bottom);
        }
        let Some(parent) = node.layout_parent.get() else {
            return Some(clip);
        };
        node = doc.get_node(parent)?;
    }
    None
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum HtmlError {
    Unsupported,
    TooLarge,
    TableNeedsWidth(u32),
    Encoding(String),
}

struct Entry {
    source: Arc<str>,
    width: u32,
    overrides: DisclosureOverrides,
    images: images::ImageKey,
    result: Result<Arc<HtmlPreview>, HtmlError>,
}

static CACHE: OnceLock<Mutex<VecDeque<Entry>>> = OnceLock::new();
const MAX_PIXELS: u32 = 4 * 1024 * 1024;
const RASTER_SCALE: u32 = 2;

pub(crate) fn block_preview(
    block: &BlockNode,
    width: f32,
    overrides: &DisclosureOverrides,
) -> Option<Arc<HtmlPreview>> {
    block_preview_with_images(block, width, overrides, &Default::default())
}

pub(crate) fn block_preview_with_images(
    block: &BlockNode,
    width: f32,
    overrides: &DisclosureOverrides,
    resources: &images::ImageResources,
) -> Option<Arc<HtmlPreview>> {
    let BlockNode::PreservedSource { source, .. } = block else {
        return None;
    };
    if !source.trim_start().starts_with('<')
        || !width.is_finite()
        || !(1. ..=1920.).contains(&width)
        || source.len() > 32768
    {
        return None;
    }
    // Never round upwards across a line-wrap-sensitive boundary.
    let width = width.floor() as u32;
    let image_key = if resources.is_empty() {
        Vec::new()
    } else {
        images::key(&inert_html_fragment(source)?, resources).ok()?
    };
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    {
        let mut entries = cache.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(index) = entries.iter().position(|entry| {
            entry.width == width
                && entry.source == *source
                && entry.overrides == *overrides
                && entry.images == image_key
        }) {
            let entry = entries.remove(index)?;
            let result = entry.result.clone().ok();
            entries.push_front(entry);
            return result;
        }
    }
    let result = render_with_images(source, width, overrides, resources).map(Arc::new);
    let mut entries = cache.lock().unwrap_or_else(|p| p.into_inner());
    entries.push_front(Entry {
        source: source.clone(),
        width,
        overrides: overrides.clone(),
        images: image_key,
        result: result.clone(),
    });
    entries.truncate(8);
    while entries
        .iter()
        .map(|entry| {
            entry.result.as_ref().map_or(0, |preview| {
                preview.image.bytes.len()
                    + preview.accessible_text.len()
                    + std::mem::size_of_val(preview.caret_stops.as_slice())
                    + preview
                        .anchors
                        .iter()
                        .map(|anchor| {
                            std::mem::size_of::<HtmlAnchor>()
                                + anchor.name.len()
                                + std::mem::size_of_val(anchor.closed_ancestors.as_slice())
                        })
                        .sum::<usize>()
            })
        })
        .sum::<usize>()
        > 16 * 1024 * 1024
    {
        entries.pop_back();
    }
    result.ok()
}

#[cfg(test)]
fn render(source: &str, width: u32) -> Result<HtmlPreview, HtmlError> {
    render_with_disclosures(source, width, &DisclosureOverrides::new())
}

#[cfg(test)]
fn render_with_disclosures(
    source: &str,
    width: u32,
    overrides: &DisclosureOverrides,
) -> Result<HtmlPreview, HtmlError> {
    render_with_images(source, width, overrides, &Default::default())
}

fn render_with_images(
    source: &str,
    width: u32,
    overrides: &DisclosureOverrides,
    resources: &images::ImageResources,
) -> Result<HtmlPreview, HtmlError> {
    let mut candidate_width = width;
    // Negotiate a bounded technical-block width before publishing any pixels.
    // The native editor owns overflow; no nested DOM scroll state or font
    // shrinking is introduced, and measurement is never done while scrolling.
    for _ in 0..3 {
        match render_at_width(source, candidate_width, overrides, resources) {
            Err(HtmlError::TableNeedsWidth(required)) if required <= 1920 => {
                candidate_width = required;
            }
            result => return result,
        }
    }
    Err(HtmlError::TooLarge)
}

fn render_at_width(
    source: &str,
    width: u32,
    overrides: &DisclosureOverrides,
    resources: &images::ImageResources,
) -> Result<HtmlPreview, HtmlError> {
    let fragment = inert_html_fragment(source).ok_or(HtmlError::Unsupported)?;
    images::key(&fragment, resources)?;
    let palette = MineralPalette::LIGHT;
    let fonts = fonts::context();
    let css = format!(
        "html {{ background: #{:06x} !important; color: #{:06x}; font: 16px/1.5 'Spline Sans Mineral', sans-serif; }}\n\
         body {{ margin: 0; padding: 0; }}\n\
         p {{ margin: 0 0 16px; }}\n\
         table {{ border-collapse: separate; border-spacing: 0; margin: 0 0 16px; border: solid #{:06x}; border-width: 1px 0 0 1px; }}\n\
         th, td {{ padding: 10px 12px; border: solid #{:06x}; border-width: 0 1px 1px 0; vertical-align: top; }}\n\
         th {{ background: #{:06x}; text-align: inherit; }}\n\
         [align='left' i] {{ text-align: left; }}\n\
         [align='center' i] {{ text-align: center; }}\n\
         [align='right' i] {{ text-align: right; }}\n\
         th > :last-child, td > :last-child, body > table:last-child {{ margin-bottom: 0; }}\n\
         img {{ max-width: 100%; height: auto; }}\n\
         details {{ padding: 16px 24px; border: 1px solid #{:06x}; border-radius: 4px; }}\n\
         details + details {{ margin-top: 16px; }}\n\
         summary {{ font-weight: bold; min-height: 1.6em; }}\n\
         details[open] > summary {{ margin-bottom: 8px; }}\n\
         details > p:last-child {{ margin-bottom: 0; }}\n\
         a {{ color: #{:06x}; }}\n\
         code, pre, kbd {{ font-family: 'Spline Sans Mono Mineral', monospace; }}\n\
         * {{ animation: none !important; transition: none !important; }}",
        palette.page,
        palette.text,
        palette.border,
        palette.border,
        palette.panel,
        palette.border,
        palette.accent
    );
    let mut doc = HtmlDocument::from_html(
        fragment.html(),
        DocumentConfig {
            viewport: Some(Viewport::new(
                width * RASTER_SCALE,
                RASTER_SCALE,
                RASTER_SCALE as f32,
                ColorScheme::Light,
            )),
            ua_stylesheets: Some(vec![blitz_dom::DEFAULT_CSS.to_owned(), css]),
            font_ctx: Some(fonts),
            // Explicitly deny ALL authored resources, including file:, data:,
            // font and CSS URLs. Installed-font fallback is a separate trusted
            // platform service. No shell/navigation/subdocument provider exists.
            net_provider: Some(Arc::new(DummyNetProvider)),
            style_threading: StyleThreading::Sequential,
            ..DocumentConfig::default()
        },
    );
    images::install(&mut doc, &fragment, resources)?;
    // Enumerate authored DOM nodes before layout creates anonymous boxes. IDs
    // are temporary; the retained ordinal is scoped to exact source bytes.
    let mut stack = vec![doc.root_node().id];
    let mut authored = Vec::new();
    let mut authored_anchors = Vec::new();
    let mut authored_links = Vec::new();
    while let Some(id) = stack.pop() {
        let node = doc.get_node(id).ok_or(HtmlError::Unsupported)?;
        stack.extend(node.children.iter().rev().copied());
        if let Some(name) = node.attr("data-mineral-anchor".into())
            && !name.is_empty()
        {
            authored_anchors.push((id, name.to_owned()));
        }
        if let Some(ordinal) = node
            .attr("data-mineral-link".into())
            .and_then(|ordinal| ordinal.parse::<usize>().ok())
            && let Some(target) = fragment.link_targets().get(ordinal)
            && !target.is_empty()
        {
            authored_links.push((id, target.to_owned()));
        }
        if node
            .element_data()
            .is_some_and(|e| e.name.local.as_ref() == "details")
        {
            let summary = node.children.iter().copied().find(|id| {
                doc.get_node(*id).is_some_and(|n| {
                    n.element_data()
                        .is_some_and(|e| e.name.local.as_ref() == "summary")
                })
            });
            let label = summary
                .and_then(|id| doc.get_node(id))
                .map_or_else(|| "Details".to_owned(), |node| node.text_content());
            authored.push((id, summary, node.attr("open".into()).is_some(), label));
        }
    }
    for (id, summary, _, label) in &mut authored {
        if summary.is_none() {
            // A missing summary has a user-agent legend, not inferred content.
            // Add it only to this inert rendering DOM; canonical/copy text and
            // source ordinals were collected before these temporary UI nodes.
            let first_child = doc
                .get_node(*id)
                .and_then(|node| node.children.first().copied());
            let mut dom = doc.mutate();
            let legend = dom.create_element(blitz_dom::qual_name!("summary", html), Vec::new());
            let text = dom.create_text_node("Details");
            dom.append_children(legend, &[text]);
            if let Some(first_child) = first_child {
                dom.insert_nodes_before(first_child, &[legend]);
            } else {
                dom.append_children(*id, &[legend]);
            }
            *summary = Some(legend);
        } else if label.trim().is_empty() {
            // Blitz constructs no inline line for an empty summary, dropping
            // its inside marker. A paint-only zero-width leaf creates that
            // line; the retained accessible label is the authored label above.
            let mut dom = doc.mutate();
            let spacer = dom.create_text_node("\u{200b}");
            dom.append_children(summary.unwrap(), &[spacer]);
        }
    }
    for (ordinal, &(id, _, open, _)) in authored.iter().enumerate() {
        let visible = overrides.get(&ordinal).copied().unwrap_or(open);
        if visible != open {
            doc.toggle_details_open(id);
        }
        if !visible {
            // Blitz's closed-details rule hides element children. Text nodes
            // cannot match a CSS selector, so suppress their paint-only value
            // as well. Each render starts from exact source; open state restores
            // the original text without a canonical transaction or wrapper.
            let text_children = doc
                .get_node(id)
                .ok_or(HtmlError::Unsupported)?
                .children
                .iter()
                .copied()
                .filter(|child| {
                    doc.get_node(*child)
                        .is_some_and(|node| matches!(&node.data, blitz_dom::NodeData::Text(_)))
                })
                .collect::<Vec<_>>();
            let mut dom = doc.mutate();
            for child in text_children {
                dom.set_node_text(child, "");
            }
        }
    }
    doc.resolve(0.);
    let body = doc.find_body_node().ok_or(HtmlError::Unsupported)?;
    // Collapsed child margins can move the body away from the document
    // origin. Raster painting uses document coordinates, not body-local ones.
    let height = (body.absolute_position(0., 0.).y
        + body
            .final_layout()
            .size
            .height
            .max(body.final_layout().scrollable_overflow_rect.bottom))
    .ceil();
    let content_width = body
        .final_layout()
        .size
        .width
        .max(body.final_layout().scrollable_overflow_rect.right);
    if !height.is_finite() || height <= 0. || height > 2048. || !content_width.is_finite() {
        return Err(HtmlError::TooLarge);
    }
    if content_width > width as f32 + 1. {
        return if fragment.html().contains("<table>") || fragment.html().contains("<table ") {
            Err(HtmlError::TableNeedsWidth(content_width.ceil() as u32))
        } else {
            Err(HtmlError::TooLarge)
        };
    }
    let physical_width = width * RASTER_SCALE;
    let physical_height = height as u32 * RASTER_SCALE;
    if physical_width * physical_height > MAX_PIXELS {
        return Err(HtmlError::TooLarge);
    }
    // Resize viewport height before painting, but do not let viewport-relative
    // authored geometry silently change. Such a fragment uses source fallback.
    doc.set_viewport(Viewport::new(
        width * RASTER_SCALE,
        height as u32 * RASTER_SCALE,
        RASTER_SCALE as f32,
        ColorScheme::Light,
    ));
    doc.resolve(0.);
    let body = doc.find_body_node().ok_or(HtmlError::Unsupported)?;
    let resolved_height = body.absolute_position(0., 0.).y
        + body
            .final_layout()
            .size
            .height
            .max(body.final_layout().scrollable_overflow_rect.bottom);
    if (resolved_height.ceil() - height).abs() > 1. {
        return Err(HtmlError::Unsupported);
    }
    let mut renderer = VelloCpuImageRenderer::new(physical_width, physical_height);
    let mut pixels = Vec::new();
    renderer.render_to_vec(
        |scene| {
            blitz_paint::paint_scene(
                scene,
                &mut doc,
                RASTER_SCALE as f64,
                physical_width,
                physical_height,
                0,
                0,
            )
        },
        &mut pixels,
    );
    // Root background is opaque, so premultiplied Vello pixels are also valid
    // straight-alpha PNG pixels. Refuse unexpected transparency rather than
    // producing dark antialiased edges from a mismatched alpha convention.
    if pixels
        .as_chunks::<4>()
        .0
        .iter()
        .any(|pixel| pixel[3] != 255)
    {
        return Err(HtmlError::Unsupported);
    }
    let mut png = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut png, physical_width, physical_height);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder
            .write_header()
            .map_err(|e| HtmlError::Encoding(e.to_string()))?;
        writer
            .write_image_data(&pixels)
            .map_err(|e| HtmlError::Encoding(e.to_string()))?;
    }
    let leaves = document_core::editable_html_leaves(source);
    let texts = leaves.as_ref().map(|leaves| {
        leaves
            .iter()
            .map(|leaf| leaf.text.as_string())
            .collect::<Vec<_>>()
    });
    let can_convert = texts.is_some();
    let text_hits = texts
        .as_deref()
        .and_then(|texts| text_hits(&doc, texts, leaves.as_deref().unwrap_or_default()))
        .unwrap_or_default();
    let caret_stops = caret_stops(&text_hits, texts.as_deref().unwrap_or_default());
    let mut editable_text = String::new();
    let mut text_ranges = Vec::new();
    if !text_hits.is_empty() {
        for text in texts.unwrap_or_default() {
            if !text_ranges.is_empty() {
                editable_text.push('\n');
            }
            let start = editable_text.len();
            editable_text.push_str(&text);
            text_ranges.push(start..editable_text.len());
        }
    }
    let mut preview = HtmlPreview {
        source: Arc::from(source),
        image: Arc::new(Image::from_bytes(ImageFormat::Png, png)),
        width: width as f32,
        height,
        text: fragment.text().to_owned(),
        accessible_text: accessibility::visible_text(&doc),
        can_convert,
        text_hits,
        caret_stops,
        editable_text,
        text_ranges,
        links: Vec::new(),
        disclosures: Vec::new(),
        anchors: Vec::new(),
        #[cfg(test)]
        glyph_counts: {
            let mut total = 0;
            let mut missing = 0;
            doc.visit(|_, node| {
                if let Some(inline) = node
                    .element_data()
                    .and_then(|element| element.inline_layout_data.as_ref())
                {
                    for line in inline.layout.lines() {
                        for run in line.runs() {
                            for cluster in run.visual_clusters() {
                                for glyph in cluster.glyphs() {
                                    total += 1;
                                    missing += usize::from(glyph.id == 0);
                                }
                            }
                        }
                    }
                }
            });
            (total, missing)
        },
    };
    for (id, name) in authored_anchors {
        let mut child = id;
        let mut ancestor = Some(id);
        let mut closed_ancestors = Vec::new();
        let mut transformed = false;
        while let Some(current) = ancestor {
            let node = doc.get_node(current).ok_or(HtmlError::Unsupported)?;
            transformed |= node.transform().is_some();
            if current != id
                && let Some((ordinal, (_, summary, open, _))) = authored
                    .iter()
                    .enumerate()
                    .find(|(_, entry)| entry.0 == current)
                && *summary != Some(child)
                && !overrides.get(&ordinal).copied().unwrap_or(*open)
            {
                closed_ancestors.push(ordinal);
            }
            child = current;
            ancestor = node.parent;
        }
        closed_ancestors.reverse();
        let y = if !transformed && closed_ancestors.is_empty() {
            // Native Blitz fragment rectangles also cover wrapped inline IDs;
            // do not guess their location from a parent block or repeated text.
            doc.node_client_rects(id)
                .into_iter()
                .filter(|rect| {
                    rect.x.is_finite()
                        && rect.y.is_finite()
                        && rect.width.is_finite()
                        && rect.height.is_finite()
                        && rect.height > 0.
                        && rect.width >= 0.
                        && rect.y >= 0.
                        && rect.y < height as f64
                })
                .map(|rect| rect.y as f32)
                .min_by(f32::total_cmp)
        } else {
            None
        };
        if transformed {
            closed_ancestors.clear();
        }
        let text_byte = y.and_then(|_| {
            preview
                .text_hits
                .iter()
                .filter(|hit| {
                    let mut current = Some(hit.dom_node);
                    while let Some(node) = current {
                        if node == id {
                            return true;
                        }
                        current = doc.get_node(node).and_then(|node| node.parent);
                    }
                    false
                })
                .filter_map(|hit| {
                    Some(
                        preview
                            .byte_for_position(hit.left)?
                            .min(preview.byte_for_position(hit.right)?),
                    )
                })
                .min()
        });
        preview.anchors.push(HtmlAnchor {
            name,
            y,
            text_byte,
            closed_ancestors,
        });
    }
    for (ordinal, &(id, summary, authored_open, ref label)) in authored.iter().enumerate() {
        let Some(summary) = summary.and_then(|id| doc.get_node(id)) else {
            continue;
        };
        let frame = summary.final_layout();
        let position = summary.absolute_position(0., 0.);
        let bounds = [
            position.x,
            position.y,
            position.x + frame.size.width,
            position.y + frame.size.height,
        ];
        if bounds.iter().any(|v| !v.is_finite())
            || frame.size.width <= 0.
            || frame.size.height <= 0.
            || bounds[0] < 0.
            || bounds[1] < 0.
            || bounds[2] > width as f32 + 1.
            || bounds[3] > height + 1.
        {
            continue;
        }
        // Never place a rectangular native control over hidden, transformed,
        // or occluded summary content. Unknown geometry retains static output.
        let mut ancestor = Some(summary.id);
        let mut transformed = false;
        while let Some(id) = ancestor {
            let Some(node) = doc.get_node(id) else {
                break;
            };
            transformed |= node.transform().is_some();
            ancestor = node.parent;
        }
        if transformed {
            continue;
        }
        let mut hit = doc
            .hit((bounds[0] + bounds[2]) * 0.5, (bounds[1] + bounds[3]) * 0.5)
            .map(|hit| hit.node_id);
        let mut matches = false;
        while let Some(id) = hit {
            if id == summary.id {
                matches = true;
                break;
            }
            hit = doc.get_node(id).and_then(|node| node.parent);
        }
        if !matches {
            continue;
        }
        preview.disclosures.push(HtmlDisclosure {
            ordinal,
            authored_open,
            open: doc
                .get_node(id)
                .is_some_and(|n| n.attr("open".into()).is_some()),
            label: label.clone(),
            bounds,
        });
    }
    // Only the already verified, complete source-to-glyph correspondence may
    // create editable link ranges. Never infer links by matching repeated labels.
    for (ordinal, leaf) in leaves.unwrap_or_default().iter().enumerate() {
        let leaf = &leaf.text;
        let Some(range) = preview.text_ranges.get(ordinal) else {
            continue;
        };
        for run in leaf.runs() {
            let Some(target) = run.styles.iter().find_map(|style| match style {
                document_core::InlineStyle::Link(target) => Some(&target.0),
                _ => None,
            }) else {
                continue;
            };
            let bounds = preview
                .text_hits
                .iter()
                .filter(|hit| {
                    hit.left.text_node == ordinal
                        && hit.right.text_node == ordinal
                        && run.range.start <= hit.left.byte_offset.min(hit.right.byte_offset)
                        && run.range.end >= hit.left.byte_offset.max(hit.right.byte_offset)
                })
                .map(|hit| hit.bounds)
                .collect::<Vec<_>>();
            if bounds.is_empty() {
                continue;
            }
            let absolute = range.start + run.range.start..range.start + run.range.end;
            if let Some(last) = preview.links.last_mut()
                && last.target == *target
                && last
                    .range
                    .as_ref()
                    .is_some_and(|range| range.end == absolute.start)
            {
                last.range.as_mut().unwrap().end = absolute.end;
                last.bounds.extend(bounds);
            } else {
                preview.links.push(HtmlLink {
                    range: Some(absolute),
                    target: target.clone(),
                    bounds,
                });
            }
        }
    }
    // An otherwise opaque fragment may still expose exact, inert Blitz link
    // boxes (for example, a spanning HTML table). Retain those pointer regions
    // without inventing text offsets or making the fragment convertible. A
    // transformed, hidden or occluded anchor has no trustworthy native box.
    for (id, target) in authored_links {
        let represented_by_text = preview.text_hits.iter().any(|hit| {
            let mut current = Some(hit.dom_node);
            while let Some(node) = current {
                if node == id {
                    return true;
                }
                current = doc.get_node(node).and_then(|node| node.parent);
            }
            false
        });
        if represented_by_text {
            continue;
        }
        let mut current = Some(id);
        let mut transformed = false;
        while let Some(node_id) = current {
            let Some(node) = doc.get_node(node_id) else {
                transformed = true;
                break;
            };
            transformed |= node.transform().is_some();
            current = node.parent;
        }
        if transformed {
            continue;
        }
        let bounds = doc
            .node_client_rects(id)
            .into_iter()
            .filter_map(|rect| {
                let values = [
                    rect.x as f32,
                    rect.y as f32,
                    (rect.x + rect.width) as f32,
                    (rect.y + rect.height) as f32,
                ];
                if values.iter().any(|value| !value.is_finite())
                    || rect.width <= 0.
                    || rect.height <= 0.
                    || values[0] < 0.
                    || values[1] < 0.
                    || values[2] > width as f32 + 1.
                    || values[3] > height + 1.
                {
                    return None;
                }
                let mut hit = doc
                    .hit((values[0] + values[2]) * 0.5, (values[1] + values[3]) * 0.5)
                    .map(|hit| hit.node_id);
                while let Some(node_id) = hit {
                    if node_id == id {
                        return Some(values);
                    }
                    hit = doc.get_node(node_id).and_then(|node| node.parent);
                }
                None
            })
            .collect::<Vec<_>>();
        if !bounds.is_empty() {
            preview.links.push(HtmlLink {
                range: None,
                target,
                bounds,
            });
        }
    }
    for link in &mut preview.links {
        link.bounds
            .sort_by(|a, b| a[1].total_cmp(&b[1]).then_with(|| a[0].total_cmp(&b[0])));
        let mut merged: Vec<[f32; 4]> = Vec::new();
        for rect in &link.bounds {
            if let Some(last) = merged.last_mut()
                && (last[1] - rect[1]).abs() < 0.5
                && (last[3] - rect[3]).abs() < 0.5
                && rect[0] - last[2] <= (rect[3] - rect[1]) * 0.8
            {
                last[2] = last[2].max(rect[2]);
            } else {
                merged.push(*rect);
            }
        }
        link.bounds = merged;
    }
    Ok(preview)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collapsed_top_margin_keeps_final_text_inside_the_raster() {
        for source in [
            "<ul><li>Other</li></ul>",
            "<div><ul><li>One<ul><li>Nested</li></ul></li><li>Other</li></ul></div>",
            "<div style='margin-top:48px'><p>Other</p></div>",
            "<div style='margin-top:0'><p>Other</p></div>",
        ] {
            for width in [200, 600] {
                let preview = render(source, width).unwrap();
                let byte = preview.editable_text.find("Other").unwrap();
                let position = preview.position_for_byte(byte).unwrap();
                let bounds = preview
                    .caret_stops
                    .iter()
                    .find(|stop| stop.position == position)
                    .expect("final item needs its own visible caret geometry")
                    .bounds;
                assert!(
                    bounds[3] <= preview.height,
                    "final item {bounds:?} exceeds raster height {} for {source}",
                    preview.height
                );
            }
        }
    }

    #[test]
    fn keyboard_rows_preserve_legal_grapheme_edges_and_measured_wrapping() {
        let preview = render(
            &format!(
                "<div><p>{}</p></div>",
                "cafe\u{301} words to wrap. ".repeat(12)
            ),
            220,
        )
        .unwrap();
        let mut byte = preview
            .byte_for_position(preview.vertical_position(None, 1, 0.).unwrap())
            .unwrap();
        let first = byte;
        let mut rows = 0;
        loop {
            let range = preview.line_range(byte).unwrap();
            assert!(range.start <= byte && byte <= range.end);
            for edge in [range.start, range.end] {
                assert!(
                    preview
                        .caret_stops
                        .iter()
                        .any(|stop| preview.byte_for_position(stop.position) == Some(edge))
                );
            }
            rows += 1;
            let Some(next) = preview.vertical_position(Some(byte), 1, 0.) else {
                break;
            };
            let next = preview.byte_for_position(next).unwrap();
            assert!(
                preview.caret_bounds(next).unwrap()[1] > preview.caret_bounds(byte).unwrap()[1]
            );
            assert!(next > byte);
            byte = next;
        }
        assert!(rows > 4, "fixture must exercise real wrapping");
        assert_eq!(
            preview.byte_for_position(preview.vertical_position(None, -1, 0.).unwrap()),
            Some(byte)
        );
        for _ in 1..rows {
            byte = preview
                .byte_for_position(preview.vertical_position(Some(byte), -1, 0.).unwrap())
                .unwrap();
        }
        assert_eq!(byte, first);
        assert!(preview.vertical_position(Some(byte), -1, 0.).is_none());
    }

    #[test]
    fn narrow_nested_table_negotiates_width_without_discarding_cells() {
        let document = document_core::Document::from_markdown(include_str!(
            "../../../performance/layout-fixtures/37-nested-html-tables.md"
        ))
        .unwrap();
        let snapshot = document.snapshot();
        let block = snapshot
            .blocks()
            .iter()
            .find(|block| matches!(block.as_ref(), BlockNode::PreservedSource { .. }))
            .unwrap();
        let preview = block_preview(block, 296., &DisclosureOverrides::new())
            .expect("wide table preview instead of plain-text fallback");
        assert!(preview.width > 296. && preview.width <= 1920.);
        for marker in [
            "Configuration",
            "Nested key",
            "Retry count",
            "17",
            "Keep all evidence",
            "Independent outer neighbor.",
        ] {
            assert_eq!(preview.accessible_text.matches(marker).count(), 1);
        }
    }

    #[test]
    fn html_table_cells_have_insets_and_preserve_authored_alignment() {
        let source = |attributes: &str| {
            format!("<table style='width:300px'><tr><td {attributes}>Value</td></tr></table>")
        };
        let left = render(&source(""), 320).unwrap();
        let right = render(&source("align='RIGHT'"), 320).unwrap();
        let styled = render(&source("align='right' style='text-align:left'"), 320).unwrap();
        let bounds = |preview: &HtmlPreview| preview.text_hits.first().unwrap().bounds;
        assert!(
            bounds(&left)[0] >= 12.,
            "cell text needs a horizontal inset"
        );
        assert!(bounds(&left)[1] >= 10., "cell text needs a vertical inset");
        assert!(
            bounds(&right)[0] > 200.,
            "right-aligned cell must use its available width"
        );
        assert!(
            (bounds(&styled)[0] - bounds(&left)[0]).abs() < 1.,
            "authored CSS must override the presentational hint"
        );
        assert!(
            left.height >= 44.,
            "padding must contribute to measured height"
        );
        for preview in [&left, &right, &styled] {
            assert!(preview.can_convert);
            assert_eq!(preview.accessible_text.trim(), "Value");
        }
    }

    #[test]
    fn spanning_html_table_renders_complete_text_without_edit_hit_targets() {
        let preview = render("<table><tr><th colspan='2'>Group</th><th>Owner</th></tr><tr><td rowspan='2'>Review</td><td>Draft</td><td>Ada</td></tr><tr><td>Sign-off</td><td>Lin</td></tr></table>", 700).unwrap();
        let mut remaining = preview.accessible_text.as_str();
        for marker in [
            "Group", "Owner", "Review", "Draft", "Ada", "Sign-off", "Lin",
        ] {
            assert_eq!(preview.accessible_text.matches(marker).count(), 1);
            remaining = remaining.split_once(marker).unwrap().1;
        }
        assert!(!preview.can_convert);
        assert!(preview.text_hits.is_empty());
        assert!(preview.height > 100.);
    }

    #[test]
    fn combining_accent_retains_verified_html_text_hits() {
        // The second Parley cluster occupies half the ligature advance but has
        // no glyph of its own. It must still resolve to the paragraph's text.
        let preview = render("<p>e\u{301}</p>", 300).unwrap();
        assert!(preview.can_convert);
        assert!(
            !preview.text_hits.is_empty(),
            "a glyphless ligature continuation must not disable fragment editing"
        );
    }

    #[test]
    fn html_pointer_caret_never_splits_a_combining_grapheme() {
        let preview = render("<p>e\u{301}</p>", 300).unwrap();
        let first = preview.text_hits.first().unwrap();
        let last = preview.text_hits.last().unwrap();
        let left = first.bounds[0];
        let right = last.bounds[2];
        let y = (first.bounds[1] + first.bounds[3]) * 0.5;
        for (fraction, expected) in [(0.25, 0), (0.75, 3)] {
            assert_eq!(
                preview.text_position_at(left + (right - left) * fraction, y),
                Some(document_core::HtmlTextPosition {
                    text_node: 0,
                    byte_offset: expected
                }),
                "clicks must choose an edge of the complete accented grapheme"
            );
        }
        let letters = render("<p>fi</p>", 300).unwrap();
        assert!(
            letters
                .caret_stops
                .iter()
                .any(|stop| stop.position.byte_offset == 1),
            "font ligatures must retain boundaries between independent graphemes"
        );
    }

    #[test]
    #[ignore = "native font integration: requires installed CJK, Arabic and emoji fonts"]
    fn installed_font_fallback_renders_multilingual_html_without_notdef_glyphs() {
        // Integration precondition: system fonts covering these scripts. The
        // native validation host provides Noto CJK, Arabic and Color Emoji.
        let source = include_str!("../../../performance/layout-fixtures/29-multilingual-html.md")
            .lines()
            .find(|line| line.starts_with("<div"))
            .unwrap();
        let preview = render(source, 760).unwrap();
        assert!(
            preview.glyph_counts.0 > 100,
            "the native layout must contain shaped glyphs"
        );
        assert_eq!(
            preview.glyph_counts.1, 0,
            "multilingual text must not be painted as missing-glyph boxes"
        );
        assert!(preview.accessible_text.contains("東京から京都へ"));
        assert_eq!(&*preview.source, source);
        assert!(
            !preview.text_hits.is_empty(),
            "mixed-script HTML must retain verified native text hit regions"
        );
    }

    #[test]
    #[ignore = "native font integration: requires installed Noto Color Emoji bitmap font"]
    fn installed_color_emoji_is_painted_not_only_shaped() {
        let preview = render("<p>🦀 👩‍💻</p>", 300).unwrap();
        assert!(preview.glyph_counts.0 >= 2);
        assert_eq!(preview.glyph_counts.1, 0);
        let offsets: std::collections::BTreeSet<_> = preview
            .caret_stops
            .iter()
            .map(|stop| stop.position.byte_offset)
            .collect();
        assert_eq!(
            offsets,
            [0, 4, 5, 16].into_iter().collect(),
            "joined emoji have only whole-grapheme caret edges"
        );
        let decoder = png::Decoder::new(std::io::Cursor::new(&preview.image.bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        assert_eq!(info.color_type, png::ColorType::Rgba);
        let colored = pixels[..info.buffer_size()]
            .as_chunks::<4>()
            .0
            .iter()
            .filter(|pixel| {
                let rgb = &pixel[..3];
                rgb.iter().max().unwrap() - rgb.iter().min().unwrap() > 60
            })
            .count();
        assert!(
            colored > 20,
            "emoji must have actual colored ink, not blank glyph advances: {colored}"
        );
    }

    #[test]
    fn authored_anchor_geometry_tracks_inline_boxes_and_closed_ancestors() {
        let source = "<details><summary id='outer'>Outer</summary><details><summary id='inner'>Inner</summary><p id='café'>Deep</p></details><details><summary>Unrelated</summary>Keep closed</details></details><p id='repeat'>First</p><p id='repeat'>Second<br><span id='inline'>Wrapped inline target with enough text to occupy multiple lines.</span></p>";
        let closed = render(source, 360).unwrap();
        let anchor = |name: &str| closed.anchors.iter().find(|a| a.name == name).unwrap();
        assert!(anchor("outer").y.is_some());
        assert!(anchor("outer").closed_ancestors.is_empty());
        assert_eq!(anchor("inner").closed_ancestors, [0]);
        assert_eq!(anchor("café").closed_ancestors, [0, 1]);
        assert!(anchor("café").y.is_none());
        let repeats: Vec<_> = closed
            .anchors
            .iter()
            .filter(|a| a.name == "repeat")
            .collect();
        assert_eq!(repeats.len(), 2);
        assert!(repeats[0].y.unwrap() < repeats[1].y.unwrap());
        assert!(anchor("inline").y.unwrap() > repeats[1].y.unwrap());
        let opened = render_with_disclosures(source, 360, &[(0, true), (1, true)].into()).unwrap();
        let deep = opened.anchors.iter().find(|a| a.name == "café").unwrap();
        assert!(deep.y.is_some());
        assert!(deep.closed_ancestors.is_empty());
        assert!(
            !opened
                .disclosures
                .iter()
                .find(|d| d.ordinal == 2)
                .unwrap()
                .open
        );
        assert_eq!(&*opened.source, source);
        assert_eq!(opened.text, closed.text);
        let hidden = render(
            "<div style='display:none'><p id='hidden'>Hidden</p></div><p>Visible</p>",
            360,
        )
        .unwrap();
        assert!(hidden.anchors[0].y.is_none());
    }

    #[test]
    fn html_links_use_verified_source_ranges_and_never_nearby_whitespace() {
        let source = "<div style='padding:16px'><p>Same <a href='https://example.test/one?a=1&amp;b=2'><strong>café</strong> link</a></p><p>Same <a href='mailto:person@example.test'>café link</a></p></div>";
        for width in [180, 640] {
            let preview = render(source, width).unwrap();
            assert_eq!(preview.links.len(), 2);
            assert_eq!(preview.links[0].target, "https://example.test/one?a=1&b=2");
            assert_eq!(preview.links[1].target, "mailto:person@example.test");
            assert!(preview.link_at(1., 1.).is_none());
            for link in &preview.links {
                assert_eq!(
                    &preview.editable_text[link.range.clone().unwrap()],
                    "café link"
                );
                for b in &link.bounds {
                    assert_eq!(
                        preview
                            .link_at((b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5)
                            .unwrap()
                            .target,
                        link.target
                    );
                }
            }
        }
        for source in [
            "<div style='display:none'><a href='https://example.test'>Hidden</a></div><p>Visible</p>",
            "<div style='width:300px;transform:translateX(10px)'><a href='https://example.test'>Moved</a></div>",
        ] {
            assert!(render(source, 640).unwrap().links.is_empty());
        }
        for target in [
            "javascript:alert(1)",
            "data:text/html,x",
            "file:///tmp/x",
            "custom:thing",
            "../other.md",
            "https:\n//example.test",
            "",
        ] {
            assert!(
                document_core::resolve_link(target, None).is_err(),
                "{target}"
            );
        }
        for target in [
            "https://example.test",
            "HTTP://example.test",
            "mailto:person@example.test",
            "#chapter",
        ] {
            assert!(document_core::resolve_link(target, None).is_ok());
        }
    }

    #[test]
    fn opaque_links_use_exact_blitz_boxes_without_inventing_editable_ranges() {
        let source = "<table><tr><th colspan='2'><a href='https://example.test/spec'>Opaque link</a></th></tr><tr><td>A</td><td>B</td></tr></table>";
        let preview = render(source, 420).unwrap();
        assert!(!preview.can_convert);
        assert!(preview.text_hits.is_empty());
        assert_eq!(preview.links.len(), 1);
        let link = &preview.links[0];
        assert_eq!(link.target, "https://example.test/spec");
        assert_eq!(link.range, None);
        assert!(!link.bounds.is_empty());
        for bounds in &link.bounds {
            assert_eq!(
                preview
                    .link_at((bounds[0] + bounds[2]) * 0.5, (bounds[1] + bounds[3]) * 0.5)
                    .map(|link| link.target.as_str()),
                Some("https://example.test/spec")
            );
        }
        assert!(preview.link_at_byte(0).is_none());
        assert_eq!(preview.source.as_ref(), source);
    }

    #[test]
    fn retained_blitz_hits_address_the_correct_duplicate_and_unicode_cluster() {
        let source = "<div style='padding:16px;border:2px solid #256f50'><p>Repeat <strong>café</strong></p><p>Repeat <strong>café</strong></p></div>";
        for width in [180, 640] {
            let preview = render(source, width).unwrap();
            assert!(
                !preview.text_hits.is_empty(),
                "no retained text geometry at {width}"
            );
            for ordinal in [0, 1] {
                let hit = preview
                    .text_hits
                    .iter()
                    .find(|hit| hit.left.text_node == ordinal && hit.left.byte_offset == 10)
                    .unwrap();
                let [left, top, right, bottom] = hit.bounds;
                assert_eq!(
                    preview.text_position_at(left + (right - left) * 0.1, (top + bottom) * 0.5),
                    Some(document_core::HtmlTextPosition {
                        text_node: ordinal,
                        byte_offset: 10
                    })
                );
                assert_eq!(
                    preview.text_position_at(left + (right - left) * 0.9, (top + bottom) * 0.5),
                    Some(document_core::HtmlTextPosition {
                        text_node: ordinal,
                        byte_offset: 12
                    })
                );
            }
        }
    }

    #[test]
    fn native_html_fixture_has_italic_and_link_edit_targets() {
        let source = include_str!("../../../performance/layout-fixtures/10-html-fragments.md");
        let doc = document_core::Document::from_markdown(source).unwrap();
        let snapshot = doc.snapshot();
        let block = snapshot.blocks().iter().find(|block| matches!(block.as_ref(), BlockNode::PreservedSource { source, .. } if source.contains("Six independent"))).unwrap();
        let preview = block_preview(block, 950., &DisclosureOverrides::new()).unwrap();
        assert!(!preview.text_hits.is_empty());
        assert!(
            preview
                .text_hits
                .iter()
                .any(|hit| hit.left.text_node == 1 && hit.left.byte_offset > 30)
        );
    }

    #[test]
    fn html_whitespace_clicks_stay_in_the_rendered_text() {
        let preview = render(
            "<div style='padding:16px'><p>ordinary rich text</p></div>",
            640,
        )
        .unwrap();
        let before = preview
            .text_hits
            .iter()
            .find(|hit| hit.right.byte_offset == 8)
            .unwrap();
        let after = preview
            .text_hits
            .iter()
            .find(|hit| hit.left.byte_offset == 9)
            .unwrap();
        let y = (before.bounds[1] + before.bounds[3]) * 0.5;
        let gap = after.bounds[0] - before.bounds[2];
        assert!(gap > 1.);
        for fraction in [0.1, 0.9] {
            let hit = preview
                .text_position_at(before.bounds[2] + gap * fraction, y)
                .expect("space is part of the text line");
            assert_eq!(hit.text_node, 0);
            assert_eq!(hit.byte_offset, if fraction < 0.5 { 8 } else { 9 });
        }
        assert!(preview.text_position_at(-1., y).is_none());
        assert!(preview.text_position_at(641., y).is_none());
        assert!(preview.text_position_at(40., -1.).is_none());
        assert!(preview.text_position_at(40., preview.height + 1.).is_none());
    }

    #[test]
    fn html_edit_hits_never_guess_hidden_or_transformed_text_correspondence() {
        for source in [
            "<div><p style='display:none'>Hidden</p><p>Visible</p></div>",
            "<div style='transform:translateX(10px)'><p>Moved</p></div>",
            "<details><summary>Closed</summary><p>Hidden</p></details>",
            "<div style='position:relative'><p>e\u{301}</p><div style='position:absolute;inset:0;background:white'></div></div>",
            "<div style='width:1px;overflow:hidden'><p>e\u{301}</p></div>",
        ] {
            assert!(
                render(source, 640).unwrap().text_hits.is_empty(),
                "{source}"
            );
        }
        assert!(
            !render(
                "<div style='overflow:hidden;padding:20px'><p>e\u{301}</p></div>",
                300
            )
            .unwrap()
            .text_hits
            .is_empty(),
            "fully visible text inside a clipping box remains editable"
        );
    }

    #[test]
    fn image_descriptions_do_not_break_verified_text_editing_before_and_after_figures() {
        use blitz_dom::node::RasterImageData;
        let source = "<div><p>Before text.</p><div><img src='a.png' alt='Not painted'><img src='a.png' alt='Not painted either'></div><p>After text.</p></div>";
        let resources = [(
            "a.png".into(),
            RasterImageData::new(1, 1, Arc::new(vec![255, 0, 0, 255])),
        )]
        .into();
        let preview = render_with_images(source, 400, &Default::default(), &resources).unwrap();
        assert!(preview.text_hits.iter().any(|hit| hit.left.text_node == 0));
        assert!(preview.text_hits.iter().any(|hit| hit.left.text_node == 3));
        assert!(
            preview
                .text_hits
                .iter()
                .all(|hit| !matches!(hit.left.text_node, 1 | 2))
        );
        assert_eq!(
            &preview.editable_text[preview.text_ranges[3].clone()],
            "After text."
        );
    }

    #[test]
    fn loaded_html_images_paint_in_blitz_and_keep_alt_out_of_visible_text() {
        use blitz_dom::node::RasterImageData;
        let source = "<div style='height:80px;display:flex;gap:8px'><img src='red.png' alt='Red diagram' width='120' height='60'><img src='blue.png' alt='Blue diagram' width='60' height='60'></div>";
        let resources = images::ImageResources::from([
            (
                "red.png".into(),
                RasterImageData::new(2, 1, Arc::new([255, 0, 0, 255].repeat(2))),
            ),
            (
                "blue.png".into(),
                RasterImageData::new(1, 1, Arc::new(vec![0, 0, 255, 255])),
            ),
        ]);
        assert_eq!(render(source, 240).unwrap_err(), HtmlError::Unsupported);
        let preview = render_with_images(source, 240, &Default::default(), &resources).unwrap();
        assert!(preview.can_convert);
        assert!(preview.accessible_text.contains("Red diagram"));
        assert!(preview.accessible_text.contains("Blue diagram"));
        assert!(
            preview.text_hits.is_empty(),
            "non-painted alt text is not a direct edit target"
        );
        let mut reader = png::Decoder::new(std::io::Cursor::new(&preview.image.bytes))
            .read_info()
            .unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        let pixel = |x: usize, y: usize| &pixels[(y * info.width as usize + x) * 4..][..4];
        assert_eq!(pixel(60, 60), [255, 0, 0, 255]);
        assert_eq!(pixel(300, 60), [0, 0, 255, 255]);
        assert_eq!(preview.height, 80.);
        let mut incomplete = resources.clone();
        incomplete.remove("blue.png");
        assert_eq!(
            render_with_images(source, 240, &Default::default(), &incomplete).unwrap_err(),
            HtmlError::Unsupported
        );
    }

    #[test]
    fn html_image_cache_keys_track_loaded_pixels_not_just_dimensions_or_source() {
        use blitz_dom::node::RasterImageData;
        let document = document_core::Document::from_markdown(
            "<div><img src='local.png' alt='Chart' width='40' height='40'></div>",
        )
        .unwrap();
        let snapshot = document.snapshot();
        let block = snapshot.blocks().iter().next().unwrap();
        let overrides = DisclosureOverrides::new();
        assert!(block_preview(block, 240., &overrides).is_none());
        let mut resources = images::ImageResources::from([(
            "local.png".into(),
            RasterImageData::new(1, 1, Arc::new(vec![255, 0, 0, 255])),
        )]);
        let first = block_preview_with_images(block, 240., &overrides, &resources).unwrap();
        let same = block_preview_with_images(block, 240., &overrides, &resources).unwrap();
        assert!(Arc::ptr_eq(&first, &same));
        resources.insert(
            "local.png".into(),
            RasterImageData::new(1, 1, Arc::new(vec![0, 0, 255, 255])),
        );
        let changed = block_preview_with_images(block, 240., &overrides, &resources).unwrap();
        assert!(!Arc::ptr_eq(&first, &changed));
        assert_ne!(first.image.bytes, changed.image.bytes);
        assert!(
            block_preview(block, 240., &overrides).is_none(),
            "a different document with no loaded resource must not borrow cached pixels"
        );
        assert_eq!(
            snapshot.serialize().unwrap(),
            document.snapshot().serialize().unwrap()
        );
    }

    #[test]
    fn malformed_and_oversized_html_rasters_are_rejected_before_paint() {
        use blitz_dom::node::RasterImageData;
        let fragment = inert_html_fragment("<img src='local.png'>").unwrap();
        for (width, height, pixels) in [
            (0, 1, vec![]),
            (1, 1, vec![0; 3]),
            (u32::MAX, u32::MAX, vec![]),
            (16385, 1, vec![]),
        ] {
            let resources = [(
                "local.png".into(),
                RasterImageData::new(width, height, Arc::new(pixels)),
            )]
            .into();
            assert_eq!(images::key(&fragment, &resources), Err(HtmlError::TooLarge));
        }
        let repeated = inert_html_fragment(&"<img src='local.png'>".repeat(3)).unwrap();
        let resources = [(
            "local.png".into(),
            RasterImageData::new(2048, 2048, Arc::new(vec![0; 2048 * 2048 * 4])),
        )]
        .into();
        assert!(images::key(&fragment, &resources).is_ok());
        assert_eq!(images::key(&repeated, &resources), Err(HtmlError::TooLarge));
    }

    #[test]
    fn blitz_reflows_styled_html_and_renders_real_pixels() {
        let source = "<div style='padding:16px;border:2px solid #256f50;border-radius:12px;background:#dcebe1'><p>Six independent ideas should occupy a readable composition without changing the authored content.</p><strong>A styled HTML fragment</strong></div>";
        let wide = render(source, 640).unwrap();
        let narrow = render(source, 240).unwrap();
        assert!(
            narrow.height > wide.height,
            "Blitz must reflow at the exact available width"
        );
        assert_eq!(wide.width, 640.);
        assert!(wide.text.contains("A styled HTML fragment"));
        assert_eq!(&wide.image.bytes[..8], b"\x89PNG\r\n\x1a\n");
        let decoder = png::Decoder::new(std::io::Cursor::new(&wide.image.bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[..3] == [0x25, 0x6f, 0x50]),
            "authored green border must be painted"
        );
    }

    #[test]
    fn implicit_disclosure_summaries_are_ui_only_and_source_bound() {
        let source = "<details><p>Outer body</p><details><p>Inner body</p></details></details><details open><p>Sibling body</p></details>";
        let initial = render(source, 640).unwrap();
        assert_eq!(
            initial
                .disclosures
                .iter()
                .map(|d| (d.ordinal, d.open))
                .collect::<Vec<_>>(),
            vec![(0, false), (2, true)]
        );
        assert!(initial.disclosures.iter().all(|d| d.label == "Details"));
        assert!(
            !initial.text.contains("Details"),
            "UI labels must not enter copied text"
        );
        let outer = render_with_disclosures(source, 640, &[(0, true)].into()).unwrap();
        let both = render_with_disclosures(source, 640, &[(0, true), (1, true)].into()).unwrap();
        assert!(initial.height < outer.height && outer.height < both.height);
        assert_eq!(both.source.as_ref(), source);
        assert_eq!(both.text, initial.text);
        assert_eq!(both.disclosures.len(), 3);
        let empty = render("<details></details>", 400).unwrap();
        assert_eq!(empty.disclosures.len(), 1);
        assert_eq!(empty.disclosures[0].label, "Details");
        // An authored empty summary is not the same as a missing summary.
        let authored = render("<details><summary></summary><p>Body</p></details>", 400).unwrap();
        assert_eq!(authored.disclosures.len(), 1);
        assert!(authored.disclosures[0].label.is_empty());
        let decoder = png::Decoder::new(std::io::Cursor::new(&authored.image.bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        reader.next_frame(&mut pixels).unwrap();
        let color = MineralPalette::LIGHT.text.to_be_bytes();
        assert!(
            pixels
                .as_chunks::<4>()
                .0
                .iter()
                .any(|p| p[..3] == color[1..]),
            "an authored empty summary must still paint its disclosure marker"
        );
    }

    #[test]
    fn open_disclosure_body_edits_use_complete_source_order_correspondence() {
        let source =
            "<details open><summary>Repeat</summary><p>Repeat <strong>café</strong></p></details>";
        for width in [260, 640] {
            let preview = render(source, width).unwrap();
            assert!(preview.can_convert);
            assert_eq!(preview.editable_text, "Repeat\nRepeat café");
            assert!(
                preview
                    .text_hits
                    .iter()
                    .any(|hit| hit.left.text_node == 1 && hit.left.byte_offset == 7)
            );
            assert_eq!(preview.source.as_ref(), source);
            let closed = render_with_disclosures(source, width, &[(0, false)].into()).unwrap();
            assert!(
                closed.can_convert,
                "explicit conversion must retain hidden bodies"
            );
            assert!(
                closed.text_hits.is_empty(),
                "never guess targets from a partial visible match"
            );
        }
        let arrow = render(
            "<details open><summary>▾ Authored arrow</summary><p>Body</p></details>",
            400,
        )
        .unwrap();
        assert_eq!(arrow.editable_text, "▾ Authored arrow\nBody");
        assert!(arrow.text_hits.iter().any(|hit| hit.left.text_node == 0
            && hit.left.byte_offset == 0
            && hit.right.byte_offset == '▾'.len_utf8()));
        let nested = render("<details open><summary>Outer</summary><details open><summary>Inner</summary><p>Deep body</p></details></details>", 400).unwrap();
        assert_eq!(nested.editable_text, "Outer\nInner\nDeep body");
        assert!(nested.text_hits.iter().any(|hit| hit.left.text_node == 2));
        for source in [
            "<details open><summary style='transform:translateX(5px)'>Title</summary><p>Body</p></details>",
            "<details open><summary>Title</summary><p style='display:none'>Hidden</p><p>Body</p></details>",
        ] {
            assert!(render(source, 400).unwrap().text_hits.is_empty());
        }
    }

    #[test]
    fn closed_disclosures_hide_direct_text_without_changing_copy_text() {
        for source in [
            "<details>Visible only when opened.</details>",
            "<details>Before<summary>Details</summary>After</details>",
        ] {
            let closed = render(source, 400).unwrap();
            let open = render_with_disclosures(source, 400, &[(0, true)].into()).unwrap();
            let empty = render("<details><summary>Details</summary></details>", 400).unwrap();
            assert_eq!(
                closed.height, empty.height,
                "collapsed text must not leak: {source}"
            );
            assert!(open.height > closed.height);
            assert_eq!(open.source.as_ref(), source);
            assert_eq!(open.text, closed.text);
        }
    }

    #[test]
    fn disclosures_toggle_nested_source_ordinals_without_mutating_source() {
        let source = "<details><summary>Repeat</summary><p>Outer body</p><details open><summary>Repeat</summary><p>Inner body</p></details></details><details open><summary>Repeat</summary><p>Sibling body</p></details>";
        let initial = render(source, 640).unwrap();
        assert_eq!(
            initial
                .disclosures
                .iter()
                .map(|d| (d.ordinal, d.open))
                .collect::<Vec<_>>(),
            vec![(0, false), (2, true)]
        );
        let open = render_with_disclosures(source, 640, &[(0, true)].into()).unwrap();
        assert!(open.height > initial.height);
        assert_eq!(
            open.disclosures
                .iter()
                .map(|d| (d.ordinal, d.open))
                .collect::<Vec<_>>(),
            vec![(0, true), (1, true), (2, true)]
        );
        assert!(open.disclosures.iter().all(|d| d.label == "Repeat"));
        let inner_closed =
            render_with_disclosures(source, 640, &[(0, true), (1, false)].into()).unwrap();
        assert!(inner_closed.height < open.height);
        assert!(inner_closed.height > initial.height);
        assert_eq!(inner_closed.source.as_ref(), source);
        assert!(inner_closed.disclosures[1].authored_open);
        assert!(!inner_closed.disclosures[1].open);
        for source in [
            "<details style='transform:translateX(5px)'><summary>Shifted</summary><p>Body</p></details>",
            "<details><summary style='display:none'>Hidden</summary><p>Body</p></details><p>Visible</p>",
        ] {
            assert!(render(source, 640).unwrap().disclosures.is_empty());
        }
    }

    #[test]
    fn disclosure_spacing_separates_groups_and_attaches_body_without_double_insets() {
        let single = "<details><summary>Title</summary><p>Body</p></details>";
        for width in [280, 640, 1200] {
            let closed = render(single, width).unwrap();
            let pair = render(&single.repeat(2), width).unwrap();
            let gap = pair.height - 2. * closed.height;
            assert!((gap - 16.).abs() <= 1., "sibling panel gap: {gap}");

            let opened = single.replacen("<details>", "<details open>", 1);
            let preview = render(&opened, width).unwrap();
            let no_inner_gap = render(
                &opened.replace("<summary>", "<summary style='margin-bottom:0'>"),
                width,
            )
            .unwrap();
            assert!((preview.height - no_inner_gap.height - 8.).abs() <= 1.);
            let no_trailing_margin =
                render(&opened.replace("<p>", "<p style='margin-bottom:0'>"), width).unwrap();
            assert_eq!(
                preview.height, no_trailing_margin.height,
                "the final paragraph must use the panel inset, not a second trailing gap"
            );
            assert_eq!(&*preview.source, opened);
            assert_eq!(render(single, width).unwrap().height, closed.height);

            let authored_pair = format!(
                "{single}{}",
                single.replace("<details>", "<details style='margin-top:30px'>")
            );
            let authored = render(&authored_pair, width).unwrap();
            assert!(
                (authored.height - pair.height - 14.).abs() <= 1.,
                "authored spacing must override user-agent defaults"
            );
            assert_eq!(&*authored.source, authored_pair);
        }
    }

    #[test]
    fn authored_details_and_oversize_fallback_are_measured() {
        let closed = render(
            "<details><summary>Summary</summary><p>Disclosure body</p></details>",
            400,
        )
        .unwrap();
        let open = render(
            "<details open><summary>Summary</summary><p>Disclosure body</p></details>",
            400,
        )
        .unwrap();
        assert!(open.height > closed.height);
        assert_eq!(
            render("<div style='height:100000px'>Large</div>", 400).unwrap_err(),
            HtmlError::TooLarge
        );
    }

    #[test]
    fn raster_scale_preserves_body_glyph_size() {
        let preview = render(
            "<div style='height:60px;color:black;background:white'>MMMM</div>",
            240,
        )
        .unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(&preview.image.bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        let rows = pixels
            .chunks_exact(info.width as usize * 4)
            .enumerate()
            .filter_map(|(y, row)| {
                row.as_chunks::<4>()
                    .0
                    .iter()
                    .any(|p| p[0] < 64 && p[1] < 64 && p[2] < 64)
                    .then_some(y)
            })
            .collect::<Vec<_>>();
        let glyph_height = rows.last().unwrap() - rows.first().unwrap() + 1;
        assert!(
            glyph_height >= 22,
            "18px body glyphs at 2x raster scale must not become 9px: {glyph_height}"
        );
    }

    #[test]
    fn complete_html_border_fits_the_reported_preview_height() {
        let preview = render(
            "<div style='box-sizing:border-box;height:80px;border:2px solid #256f50'>Panel</div>",
            240,
        )
        .unwrap();
        let decoder = png::Decoder::new(std::io::Cursor::new(&preview.image.bytes));
        let mut reader = decoder.read_info().unwrap();
        let mut pixels = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut pixels).unwrap();
        let offset = ((info.height - 2) * info.width + info.width / 2) as usize * 4;
        assert_eq!(
            &pixels[offset..offset + 3],
            &[0x25, 0x6f, 0x50],
            "bottom border must not be clipped by body margins"
        );
    }
}
