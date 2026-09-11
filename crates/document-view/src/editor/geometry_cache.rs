//! Source-local renderer geometry, before adaptive placement and external gaps.
//! No canonical content ownership, active DOMs, input handlers or glyph caches.
use super::*;
use std::{collections::VecDeque, sync::Weak};

const MAX_ENTRIES: usize = 8192;
const MAX_ACCOUNTED_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENTRY_BYTES: usize = 1024 * 1024;
const MAX_SEGMENT_LINES: usize = 256;

#[derive(Clone)]
struct LeafIdentity(Weak<BlockNode>);

impl PartialEq for LeafIdentity {
    fn eq(&self, other: &Self) -> bool {
        Weak::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for LeafIdentity {}

#[derive(Clone, PartialEq, Eq)]
struct Key {
    // A weak allocation identity cannot be recycled while this key exists,
    // but does not keep a deleted document's potentially huge source alive.
    leaf: LeafIdentity,
    context: crate::ProjectionContext,
    container_edges: Option<crate::projection::ContainerEdges>,
    node_range: Range<usize>,
    width: u32,
    text_width: u32,
    table_columns: Option<usize>,
    table_records: bool,
    record_header: Option<LeafIdentity>,
    table_insets: (u32, u32),
    image_height: Option<u32>,
    font_override: Option<u32>,
    math_edit: bool,
    expanded_code_tail: bool,
    command_strip_lock: Option<Option<u32>>,
    html_disclosures: crate::html::DisclosureOverrides,
    html_images: crate::html::images::ImageKey,
    starts_document: bool,
    breaks: Vec<usize>,
}

impl std::hash::Hash for Key {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.leaf.0.as_ptr().hash(state);
        self.width.hash(state);
        self.text_width.hash(state);
        self.table_insets.hash(state);
        self.node_range.hash(state);
        self.container_edges.hash(state);
        self.table_columns.hash(state);
        self.table_records.hash(state);
        self.record_header
            .as_ref()
            .map(|h| h.0.as_ptr())
            .hash(state);
        self.image_height.hash(state);
        self.font_override.hash(state);
        self.math_edit.hash(state);
        self.expanded_code_tail.hash(state);
        self.command_strip_lock.hash(state);
        self.html_disclosures.hash(state);
        self.html_images.hash(state);
        self.starts_document.hash(state);
        self.breaks.hash(state);
        let c = &self.context;
        c.narrative.hash(state);
        c.metric.hash(state);
        c.color_role.hash(state);
        c.color_rgba.hash(state);
        c.badge.hash(state);
        c.bibliography.hash(state);
        c.figure_text.hash(state);
        c.resource_title_end.hash(state);
        c.metadata.hash(state);
        c.list_depth.hash(state);
        c.ordered_list_depth.hash(state);
        c.list_ancestors.hash(state);
        c.outline_metrics.hash(state);
        c.list_item_label.hash(state);
        c.list_item_container.hash(state);
        c.list_parent_label.hash(state);
        c.list_branch_start.hash(state);
        c.compact_outline.hash(state);
        c.list_marker.hash(state);
        c.task_checked.hash(state);
        c.task_list_depth.hash(state);
        c.quote_depth.hash(state);
        c.quote.hash(state);
        c.quote_ancestors.hash(state);
        c.quote_pull.hash(state);
        c.quote_attribution.hash(state);
        c.quote_before_attribution.hash(state);
        c.quote_first.hash(state);
        c.quote_last.hash(state);
        c.alert
            .as_ref()
            .map(|(id, kind)| (id, std::mem::discriminant(kind)))
            .hash(state);
        if let Some((_, document_core::AlertKind::Other(label))) = &c.alert {
            label.hash(state);
        }
        c.alert_first.hash(state);
        c.alert_last.hash(state);
        c.table_cell.hash(state);
        c.table_header.hash(state);
        c.table_property_key.hash(state);
        c.table_border
            .map(|border| std::mem::discriminant(&border))
            .hash(state);
        c.image_source.hash(state);
        c.preserved_source.hash(state);
    }
}

impl Key {
    fn new(
        projection: &TextProjection,
        segment: &crate::ProjectionSegment,
        images: &NodeImageDimensions,
        width: f32,
        breaks: &[usize],
        font: Option<f32>,
    ) -> Option<Self> {
        let context = &segment.context;
        let context_bytes = projection
            .container_edges(segment.node_id)
            .map_or(0, |edges| {
                (edges.starts.len() + edges.ends.len()) * std::mem::size_of::<NodeId>()
            })
            + context.list_marker.as_ref().map_or(0, String::len)
            + context.image_source.as_ref().map_or(0, String::len)
            + match &context.alert {
                Some((_, document_core::AlertKind::Other(label))) => label.len(),
                _ => 0,
            };
        if context_bytes > 4096 || breaks.len() > 64 || segment.projection_len() > 16 * 1024 {
            return None;
        }
        let block = projection.block_handle(segment.node_id)?;
        Some(Self {
            leaf: LeafIdentity(Arc::downgrade(block)),
            context: context.as_ref().clone(),
            container_edges: projection.container_edges(segment.node_id).cloned(),
            node_range: segment.node_range.clone(),
            width: width.to_bits(),
            text_width: segment_text_width(segment, projection, width).to_bits(),
            table_insets: {
                let (outer, inner) = table_insets(segment, projection);
                (outer.to_bits(), inner.to_bits())
            },
            table_columns: context.table_cell.and_then(|(id, _, _)| {
                match projection.block(id)? {
                    BlockNode::Table(table) => Some(table.columns.len().max(1)),
                    _ => None,
                }
            }),
            table_records: table_records::is_cell(projection, segment, width),
            record_header: table_records::field(projection, segment, width)
                .and_then(|(id, _, _)| projection.block_handle(id))
                .map(|block| LeafIdentity(Arc::downgrade(block))),
            image_height: image_reserved_height(projection, block, segment, images, width)
                .map(f32::to_bits),
            font_override: font.map(f32::to_bits),
            math_edit: projection.preview_edit_node == Some(segment.node_id),
            expanded_code_tail: projection.expanded_code_tail == Some(segment.node_id),
            command_strip_lock: projection
                .command_strip_lock
                .filter(|(id, _)| *id == segment.node_id)
                .map(|(_, width)| width),
            html_disclosures: projection
                .html_disclosure_overrides(segment.node_id)
                .cloned()
                .unwrap_or_default(),
            html_images: projection
                .html_images(segment.node_id)
                .map(|images| images.key.clone())
                .unwrap_or_default(),
            starts_document: segment.projection_start() == 0,
            breaks: breaks
                .iter()
                .filter(|offset| segment.projection_range().contains(offset))
                .map(|offset| offset - segment.projection_start())
                .collect(),
        })
    }

    fn accounted_bytes(&self, lines: &[VisualLineSpec]) -> usize {
        let key_bytes = std::mem::size_of::<Self>()
            + self.container_edges.as_ref().map_or(0, |edges| {
                (edges.starts.len() + edges.ends.len()) * std::mem::size_of::<NodeId>()
            })
            + self.html_disclosures.len() * 32
            + self
                .html_images
                .iter()
                .map(|(source, ..)| source.len() + 32)
                .sum::<usize>()
            + self.breaks.len() * std::mem::size_of::<usize>()
            + self.context.list_marker.as_ref().map_or(0, String::len)
            + self.context.image_source.as_ref().map_or(0, String::len)
            + match &self.context.alert {
                Some((_, document_core::AlertKind::Other(label))) => label.len(),
                _ => 0,
            };
        let mut bytes = key_bytes * 2
            + std::mem::size_of::<BlockNode>()
            + 2 * std::mem::size_of::<usize>()
            + std::mem::size_of_val(lines);
        // Conservatively charge referenced previews per entry, even when
        // several lines share a raster also retained by the visible document.
        let pixels = |width: f32, height: f32| (width.ceil() * height.ceil() * 4.) as usize;
        for line in lines {
            if let Some(label) = &line.record_label {
                bytes = bytes
                    .saturating_add(std::mem::size_of::<table_records::Label>())
                    .saturating_add(std::mem::size_of_val(label.ranges.as_slice()));
            }
            if let Some(math) = &line.display_math {
                for formula in [&math.light, &math.dark] {
                    bytes = bytes
                        .saturating_add(std::mem::size_of::<crate::math::Formula>())
                        .saturating_add(formula.image.bytes.len())
                        .saturating_add(pixels(formula.width, formula.height));
                }
            }
            if let Some(diagram) = &line.diagram {
                for figure in [&diagram.light, &diagram.dark] {
                    bytes = bytes
                        .saturating_add(figure.image.bytes.len())
                        .saturating_add(figure.description.len())
                        .saturating_add(pixels(figure.width, figure.height));
                }
            }
            if let Some(html) = &line.html_preview {
                bytes = bytes
                    .saturating_add(html.source.len())
                    .saturating_add(html.text.len())
                    .saturating_add(html.editable_text.len())
                    .saturating_add(html.text_ranges.len() * std::mem::size_of::<Range<usize>>())
                    .saturating_add(
                        html.text_hits.len() * std::mem::size_of::<crate::html::HtmlTextHit>(),
                    )
                    .saturating_add(pixels(html.width, html.height));
                for link in &html.links {
                    bytes = bytes
                        .saturating_add(std::mem::size_of::<crate::html::HtmlLink>())
                        .saturating_add(link.target.len())
                        .saturating_add(std::mem::size_of_val(link.bounds.as_slice()));
                }
                for disclosure in &html.disclosures {
                    bytes = bytes
                        .saturating_add(std::mem::size_of::<crate::html::HtmlDisclosure>())
                        .saturating_add(disclosure.label.len());
                }
                for anchor in &html.anchors {
                    bytes = bytes
                        .saturating_add(std::mem::size_of::<crate::html::HtmlAnchor>())
                        .saturating_add(anchor.name.len())
                        .saturating_add(std::mem::size_of_val(anchor.closed_ancestors.as_slice()));
                }
            }
            if let Some(math) = &line.inline_math {
                bytes = bytes
                    .saturating_add(std::mem::size_of::<inline_math::InlineLine>())
                    .saturating_add(
                        math.attachments.len() * std::mem::size_of::<inline_math::Attachment>(),
                    );
                for attachment in &math.attachments {
                    bytes = bytes.saturating_add(match &attachment.content {
                        inline_math::Content::Formula { light, dark } => {
                            pixels(light.width, light.height)
                                .saturating_add(pixels(dark.width, dark.height))
                        }
                        inline_math::Content::Reference { label, .. } => label.len(),
                    });
                }
            }
        }
        bytes
    }
}

/// Only source-local extension lines belonging to one published generation.
/// Raster/semantic payloads are shared with that generation's final lines; no
/// previous-generation map or canonical source owner is retained by this map.
#[derive(Default)]
pub(super) struct RetainedExtensions {
    entries: HashMap<(Key, bool), Arc<[VisualLineSpec]>>,
}

pub(super) struct ExtensionReuse<'a> {
    previous: Option<&'a RetainedExtensions>,
    pub next: RetainedExtensions,
}

impl<'a> ExtensionReuse<'a> {
    pub fn new(previous: Option<&'a RetainedExtensions>) -> Self {
        Self {
            previous,
            next: RetainedExtensions::default(),
        }
    }

    // Mirrors the segment renderer's inputs; placement and zoom are applied
    // afterwards by the ordinary arrangement pass, never retained here.
    #[allow(clippy::too_many_arguments)]
    pub fn segment(
        &mut self,
        projection: &TextProjection,
        segment: &crate::ProjectionSegment,
        images: &NodeImageDimensions,
        width: f32,
        breaks: &[usize],
        measurement: Option<&FontMeasurement>,
        font: Option<f32>,
    ) -> Vec<VisualLineSpec> {
        diagnostics::count(|counts| counts.retained_extension_requests += 1);
        // Valid HTML returns its complete Blitz geometry before consulting
        // the text measurement provider. Only such rendered HTML is retained
        // below; fallback source lines must still use their ordinary renderer.
        // Math includes editable source lines, so its measurement mode matters.
        let measured_source = measurement.is_some()
            && !matches!(
                projection.block(segment.node_id),
                Some(BlockNode::PreservedSource { .. })
            );
        let key = Key::new(projection, segment, images, width, breaks, font)
            .map(|key| (key, measured_source));
        if let Some(key) = key.as_ref()
            && let Some(cached) = self.previous.and_then(|p| p.entries.get(key))
        {
            let segment = projection
                .segment_for_node(segment.node_id)
                .unwrap_or(segment);
            let lines = cached
                .iter()
                .map(|line| {
                    let mut line = line.clone();
                    line.rebind_from_cache(segment)?;
                    Some(line)
                })
                .collect::<Option<Vec<_>>>();
            if let Some(lines) = lines {
                self.next.entries.insert(key.clone(), cached.clone());
                diagnostics::count(|counts| counts.retained_extension_hits += 1);
                return lines;
            }
        }
        let lines = build_visual_lines_for_segment(
            projection,
            segment,
            images,
            width,
            breaks,
            measurement,
            font,
        );
        // Unsupported extensions use the normal text cache. Display math
        // retains its exact source lines alongside the formula for editing.
        if lines.len() <= MAX_SEGMENT_LINES
            && lines.first().is_some_and(|line| {
                line.html_preview.is_some() || line.display_math.is_some() || line.diagram.is_some()
            })
            && let Some(key) = key
        {
            let coordinate_segment = projection
                .segment_for_node(segment.node_id)
                .unwrap_or(segment);
            let mut cached = lines.clone();
            if cached.iter_mut().all(|line| {
                line.normalize_for_cache(coordinate_segment.projection_local_start())
                    .is_some()
            }) {
                self.next.entries.insert(key, cached.into());
            }
        }
        lines
    }
}

pub(super) struct GeometryCache {
    entries: HashMap<Key, Arc<[VisualLineSpec]>>,
    order: VecDeque<(Key, usize)>,
    accounted_bytes: usize,
}

impl GeometryCache {
    pub(super) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: VecDeque::new(),
            accounted_bytes: 0,
        }
    }

    fn insert(&mut self, key: Key, mut lines: Vec<VisualLineSpec>, projection_local_start: usize) {
        if lines.is_empty() || lines.len() > MAX_SEGMENT_LINES || self.entries.contains_key(&key) {
            return;
        }
        if !lines
            .iter_mut()
            .all(|line| line.normalize_for_cache(projection_local_start).is_some())
        {
            return;
        }
        let bytes = key.accounted_bytes(&lines);
        if bytes > MAX_ENTRY_BYTES {
            return;
        }
        // FIFO admission bounds eviction work amortized O(1), unlike scanning
        // thousands of timestamps for every cold node in a giant document.
        while self.entries.len() >= MAX_ENTRIES
            || self.accounted_bytes + bytes > MAX_ACCOUNTED_BYTES
        {
            let Some((old, cost)) = self.order.pop_front() else {
                break;
            };
            self.entries.remove(&old);
            self.accounted_bytes -= cost;
            diagnostics::count(|counts| counts.geometry_cache_evictions += 1);
        }
        self.accounted_bytes += bytes;
        self.order.push_back((key.clone(), bytes));
        self.entries.insert(key, lines.into());
    }
}

impl FontMeasurement {
    pub(super) fn segment_geometry(
        &self,
        projection: &TextProjection,
        segment: &crate::ProjectionSegment,
        images: &NodeImageDimensions,
        width: f32,
        breaks: &[usize],
        font: Option<f32>,
    ) -> Vec<VisualLineSpec> {
        diagnostics::count(|counts| counts.geometry_requests += 1);
        let key = Key::new(projection, segment, images, width, breaks, font);
        let cached = key.as_ref().and_then(|key| {
            self.geometry
                .lock()
                .ok()
                .and_then(|cache| cache.entries.get(key).cloned())
        });
        if let Some(cached) = cached {
            let segment = projection
                .segment_for_node(segment.node_id)
                .unwrap_or(segment);
            let lines = cached
                .iter()
                .map(|line| {
                    let mut line = line.clone();
                    line.rebind_from_cache(segment)?;
                    Some(line)
                })
                .collect::<Option<Vec<_>>>();
            if let Some(lines) = lines {
                diagnostics::count(|counts| counts.geometry_cache_hits += 1);
                return lines;
            }
        }
        diagnostics::count(|counts| counts.segments_laid_out += 1);
        let lines = build_visual_lines_for_segment_uncached(
            projection,
            segment,
            images,
            width,
            breaks,
            Some(self),
            font,
        );
        if lines.len() <= MAX_SEGMENT_LINES
            && let Some(key) = key
            && let Ok(mut cache) = self.geometry.lock()
        {
            let coordinate_segment = projection
                .segment_for_node(segment.node_id)
                .unwrap_or(segment);
            cache.insert(
                key,
                lines.clone(),
                coordinate_segment.projection_local_start(),
            );
        }
        lines
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn geometry_budget_accounts_for_retained_display_formula_payloads() {
        let document = Document::from_markdown("```math\n\\frac{1}{2}\n```\n").unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let segment = &projection.segments()[0];
        let key = Key::new(&projection, segment, &HashMap::new(), 760., &[], None).unwrap();
        let mut lines = build_visual_lines_for_segment_uncached(
            &projection,
            segment,
            &HashMap::new(),
            760.,
            &[],
            None,
            None,
        );
        let with_preview = key.accounted_bytes(&lines);
        let preview = lines[0].display_math.take().unwrap();
        let payload = preview.light.image.bytes.len() + preview.dark.image.bytes.len();
        assert!(payload > 0);
        assert!(with_preview >= key.accounted_bytes(&lines) + payload);
    }

    #[gpui::test]
    fn geometry_cache_bounds_entries_payload_and_does_not_retain_source(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document = Document::from_markdown("A small paragraph.").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            let leaf = projection.block_handle(segment.node_id).unwrap().clone();
            let strong_count = Arc::strong_count(&leaf);
            let key = Key::new(&projection, segment, &HashMap::new(), 760., &[], None).unwrap();
            let weak = key.leaf.0.clone();
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let lines = build_visual_lines_for_segment_uncached(
                &projection,
                segment,
                &HashMap::new(),
                760.,
                &[],
                Some(&measurement),
                None,
            );
            let projection_local_start = segment.projection_local_start();
            let mut cache = GeometryCache::new();
            for index in 0..MAX_ENTRIES + 10 {
                let mut key = key.clone();
                key.width = (index as f32).to_bits();
                cache.insert(key, lines.clone(), projection_local_start);
                assert!(cache.entries.len() <= MAX_ENTRIES);
                assert!(cache.accounted_bytes <= MAX_ACCOUNTED_BYTES);
                assert_eq!(cache.entries.len(), cache.order.len());
            }
            assert_eq!(cache.entries.len(), MAX_ENTRIES);
            assert_eq!(Arc::strong_count(&leaf), strong_count);
            drop(leaf);
            drop(projection);
            drop(document);
            assert!(
                weak.upgrade().is_none(),
                "geometry must not keep deleted source payload alive"
            );
            let large = vec![lines[0].clone(); MAX_SEGMENT_LINES];
            for index in 0..2000 {
                let mut key = key.clone();
                key.width = ((index + MAX_ENTRIES + 20) as f32).to_bits();
                cache.insert(key, large.clone(), projection_local_start);
                assert!(cache.accounted_bytes <= MAX_ACCOUNTED_BYTES);
                assert_eq!(cache.entries.len(), cache.order.len());
            }
            assert!(
                cache.entries.len() < 2000,
                "payload budget, not only entry count, must evict"
            );
        });
    }

    #[gpui::test]
    fn oversized_geometry_bypasses_cache_without_losing_lines(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = format!("```text\n{}```\n", "line\n".repeat(MAX_SEGMENT_LINES + 30));
            let document = Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let counts = diagnostics::MeasurementScope::new();
            for _ in 0..2 {
                let lines = build_visual_lines_for_segment(
                    &projection,
                    segment,
                    &HashMap::new(),
                    760.,
                    &[],
                    Some(&measurement),
                    None,
                );
                assert!(lines.len() > MAX_SEGMENT_LINES);
                assert_eq!(
                    lines.first().unwrap().projected_start(),
                    segment.projection_start()
                );
                assert_eq!(
                    lines.last().unwrap().projected_end() + 1,
                    segment.projection_end()
                );
                // The source newline is retained, without an idle caret-only row.
                assert_eq!(lines.len(), MAX_SEGMENT_LINES + 30);
                assert_eq!(
                    &projection.text()
                        [lines.last().unwrap().projected_end()..segment.projection_end()],
                    "\n"
                );
            }
            assert_eq!(counts.take_stage().geometry_cache_hits, 0);
            assert!(measurement.geometry.lock().unwrap().entries.is_empty());
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn geometry_cache_rebinds_segment_local_ranges_after_an_earlier_edit(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let mut document = Document::from_markdown("prefix\n\ntarget words").unwrap();
            let snapshot = document.snapshot();
            let prefix = snapshot.blocks().get(0).unwrap().id();
            let target = snapshot.blocks().get(1).unwrap().id();
            let projection = TextProjection::from_snapshot(&snapshot);
            let segment = projection.segment_for_node(target).unwrap();
            assert!(segment.projection_local_start() > 0);
            let original_local = segment.projection_local_start();
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let counts = diagnostics::MeasurementScope::new();
            let before = measurement.segment_geometry(
                &projection,
                segment,
                &HashMap::new(),
                760.,
                &[],
                None,
            );
            assert_eq!(counts.take_stage().geometry_cache_hits, 0);

            let transaction = document
                .apply(EditCommand::ReplaceText {
                    node_id: prefix,
                    range: 6..6,
                    text: " expanded".into(),
                    selection_after: None,
                    typing: true,
                })
                .unwrap();
            let projection = TextProjection::from_snapshot(&transaction.snapshot);
            let segment = projection.segment_for_node(target).unwrap();
            assert_eq!(
                segment.projection_local_start(),
                original_local + " expanded".len()
            );
            let after = measurement.segment_geometry(
                &projection,
                segment,
                &HashMap::new(),
                760.,
                &[],
                None,
            );
            assert_eq!(counts.take_stage().geometry_cache_hits, 1);
            assert_eq!(before.len(), after.len());
            for (before, after) in before.iter().zip(&after) {
                assert_eq!(
                    after.projected_range(),
                    before.projected_start() + " expanded".len()
                        ..before.projected_end() + " expanded".len()
                );
                assert_eq!(
                    &projection.text()[after.projected_range()],
                    &transaction
                        .snapshot
                        .node(target)
                        .unwrap()
                        .text()
                        .unwrap()
                        .as_string()[after.projected_start()
                        - segment.projection_start()
                        ..after.projected_end() - segment.projection_start()]
                );
            }
        });
    }
}
