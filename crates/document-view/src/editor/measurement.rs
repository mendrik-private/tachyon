//! Inert native font measurement shared by geometry preparation and editing.
//! It uses the same inline font runs as painting, and returns canonical byte
//! ranges. No focusable elements, resource requests, or editor transactions.

use super::*;

/// Hold the allocation as well as its identity: an allocator cannot recycle
/// this address while the key is retained. Equal IDs/revisions from another
/// document are intentionally not equivalent. Canonical nodes are immutable.
#[derive(Clone)]
pub(super) struct ContentIdentity(pub Arc<BlockNode>);

impl PartialEq for ContentIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for ContentIdentity {}
impl std::hash::Hash for ContentIdentity {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        std::ptr::hash(Arc::as_ptr(&self.0), state);
    }
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct MeasureKey {
    node: NodeId,
    revision: Revision,
    local_range: Range<usize>,
    width: u32,
    font_size: u32,
    runs: u64,
    refine_ending: bool,
    // IDs/revisions can repeat in another document. Retaining the exact input
    // also makes cache hits collision-safe without a second editable store.
    text: Arc<str>,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct IntrinsicKey {
    text: Arc<str>,
    font_size: u32,
    runs: u64,
}

#[derive(Clone, Hash, PartialEq, Eq)]
struct ListItemMeasureKey {
    list: ContentIdentity,
    node: NodeId,
    width: u32,
    cards: bool,
    math_edit: bool,
    metadata: bool,
    footnotes: crate::footnotes::Numbering,
}

pub(super) struct FontMeasurement {
    pub identity: Arc<()>,
    pub(super) geometry: Mutex<super::geometry_cache::GeometryCache>,
    fonts: Arc<gpui::TextSystem>,
    style: gpui::TextStyle,
    zoom: f32,
    prose_measures: [f32; 2],
    pub(super) code_digit_width: f32,
    cache: Mutex<BoundedLru<MeasureKey, Vec<Range<usize>>>>,
    intrinsic_cache: Mutex<BoundedLru<IntrinsicKey, f32>>,
    table_cache: Mutex<
        BoundedLru<
            (ContentIdentity, crate::footnotes::Numbering),
            crate::projection::TableMeasurements,
        >,
    >,
    item_cache: Mutex<BoundedLru<ListItemMeasureKey, crate::adaptive::candidates::ItemMeasurement>>,
    group_cache: Mutex<
        BoundedLru<super::arrangement::GroupMeasureKey, crate::adaptive::rows::GroupMeasurement>,
    >,
}

impl FontMeasurement {
    pub(super) fn command_language_width(&self, value: &str) -> f32 {
        let mut font = gpui::font("Spline Sans Mono Tachyon");
        font.weight = FontWeight::SEMIBOLD;
        let run = TextRun {
            len: value.len(),
            font,
            color: rgb(TachyonPalette::LIGHT.text).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = gpui::WindowTextSystem::new(self.fonts.clone()).shape_line(
            value.to_owned().into(),
            px(DocumentStyle::CAPTION_SIZE * self.zoom),
            &[run],
            None,
        );
        f32::from(line.width) / self.zoom
    }
    pub(super) fn footnote_advance(&self, number: usize, font_size: f32) -> f32 {
        let text = number.to_string();
        let run = TextRun {
            len: text.len(),
            font: gpui::font("Spline Sans Tachyon"),
            color: rgb(TachyonPalette::LIGHT.accent).into(),
            background_color: None,
            underline: None,
            strikethrough: None,
        };
        let line = gpui::WindowTextSystem::new(self.fonts.clone()).shape_line(
            text.into(),
            px(font_size * 0.7 * self.zoom),
            &[run],
            None,
        );
        f32::from(line.width) / self.zoom + font_size * 0.15
    }
    pub(super) fn cached_list_item(
        &self,
        projection: &TextProjection,
        node: NodeId,
        width: f32,
        cards: bool,
        measure: impl FnOnce() -> Option<crate::adaptive::candidates::ItemMeasurement>,
    ) -> Option<crate::adaptive::candidates::ItemMeasurement> {
        let segment = projection.segment_for_node(node)?;
        // Called only for the planner's bounded, flat 3–9-item list candidates.
        // The complete immutable list retains numbering and indentation context.
        let key = ListItemMeasureKey {
            list: ContentIdentity(projection.block_handle(segment.top_level_node_id)?.clone()),
            node,
            width: width.to_bits(),
            cards,
            math_edit: projection.preview_edit_node == Some(node),
            metadata: segment.context.metadata,
            footnotes: projection.footnotes.numbering.clone(),
        };
        diagnostics::count(|counts| counts.item_requests += 1);
        if let Some(value) = self
            .item_cache
            .lock()
            .ok()
            .and_then(|mut cache| cache.get(&key))
        {
            diagnostics::count(|counts| counts.item_cache_hits += 1);
            return Some(value);
        }
        let value = measure()?;
        diagnostics::count(|counts| counts.items_measured += 1);
        if let Ok(mut cache) = self.item_cache.lock() {
            cache.insert(key, value);
        }
        Some(value)
    }

    pub(super) fn cached_group(
        &self,
        key: Option<super::arrangement::GroupMeasureKey>,
        measure: impl FnOnce() -> Option<crate::adaptive::rows::GroupMeasurement>,
    ) -> Option<crate::adaptive::rows::GroupMeasurement> {
        diagnostics::count(|counts| counts.group_requests += 1);
        if let Some(value) = key.as_ref().and_then(|key| {
            self.group_cache
                .lock()
                .ok()
                .and_then(|mut cache| cache.get(key))
        }) {
            diagnostics::count(|counts| counts.group_cache_hits += 1);
            return Some(value);
        }
        // Never hold a cache lock while shaping or entering other caches.
        let value = measure()?;
        diagnostics::count(|counts| counts.groups_measured += 1);
        if let Some(key) = key
            && let Ok(mut cache) = self.group_cache.lock()
        {
            cache.insert(key, value);
        }
        Some(value)
    }

    pub(super) fn shape_unwrapped(
        &self,
        projection: &TextProjection,
        range: Range<usize>,
        font_size: f32,
    ) -> Option<ShapedLine> {
        let text = &projection.text()[range.clone()];
        if text.contains(['\n', '\r']) {
            return None;
        }
        let runs = styled_projection_runs(
            projection,
            &range,
            text.len(),
            &self.style,
            false,
            TachyonPalette::for_dark(false),
        );
        diagnostics::count(|counts| counts.shaping_calls += 1);
        Some(gpui::WindowTextSystem::new(self.fonts.clone()).shape_line(
            text.to_owned().into(),
            px(font_size),
            &runs,
            None,
        ))
    }

    /// Read every participating header/cell before declaring a table measured.
    /// Oversized/unsupported tables retain the explicit estimated stack path;
    /// partial sampling is never sufficient evidence to place them in a peer row.
    #[cfg(test)]
    pub fn measure_tables(&self, projection: &mut TextProjection) {
        self.measure_tables_in_scope(projection, None);
    }

    pub fn measure_tables_in_scope(
        &self,
        projection: &mut TextProjection,
        active: Option<&std::collections::HashSet<NodeId>>,
    ) {
        use crate::projection::TableMeasurements;
        let mut tables = HashMap::<NodeId, Vec<usize>>::new();
        for (index, segment) in projection.segments().iter().enumerate() {
            if let Some((id, _, _)) = segment.context.table_cell
                && projection.table_measurements(id).is_none()
                && !projection.table_layout_is_locked(id)
            {
                tables.entry(id).or_default().push(index);
            }
        }
        let table_attempts = tables.len() as u64;
        let mut cache_hits = 0;
        let measured = tables
            .into_iter()
            .filter_map(|(id, indexes)| {
                let key = (
                    ContentIdentity(projection.block_handle(id)?.clone()),
                    projection.footnotes.numbering.clone(),
                );
                if let Some(value) = self
                    .table_cache
                    .lock()
                    .ok()
                    .and_then(|mut cache| cache.get(&key))
                {
                    cache_hits += 1;
                    return Some((id, value));
                }
                if active.is_some_and(|active| !active.contains(&id)) {
                    return None;
                }
                let BlockNode::Table(table) = projection.block(id)? else {
                    return None;
                };
                if table.columns.is_empty() || table.columns.len() > 32 || indexes.len() > 512 {
                    return None;
                }
                let bytes = indexes
                    .iter()
                    .map(|i| projection.segments()[*i].projection_len())
                    .sum::<usize>();
                if bytes > 64 * 1024 {
                    return None;
                }
                let mut result = TableMeasurements {
                    minimum: vec![64.; table.columns.len()],
                    preferred: vec![64.; table.columns.len()],
                    reading_width: self.prose_measures().fit_width(f32::INFINITY, false, false),
                    record_headers: table_records::headers(projection, table, self),
                };
                for index in indexes {
                    let segment = &projection.segments()[index];
                    let (_, row, column) = segment.context.table_cell?;
                    let block = projection.block(segment.node_id)?;
                    if column >= table.columns.len()
                        || segment.projection_len() > 4096
                        || !matches!(
                            block,
                            BlockNode::Paragraph(_)
                                | BlockNode::Heading(_)
                                | BlockNode::CodeBlock(_)
                        )
                    {
                        return None;
                    }
                    let font_size = visual_line_style_for(
                        projection,
                        block,
                        segment,
                        &segment.projection_range(),
                        None,
                    )
                    .font_size;
                    let minimum_font_size = if row > 0
                        && column == 0
                        && result.record_headers.as_ref().is_some_and(|h| h.len() > 2)
                    {
                        table_records::TITLE_SIZE
                    } else {
                        font_size
                    };
                    // Container indentation is applied once to the table's
                    // origin/viewport; intrinsic columns own only cell padding.
                    let code = matches!(block, BlockNode::CodeBlock(_));
                    let text = &projection.text()[segment.projection_range()];
                    let inset = 24.
                        + table_insets(segment, projection).1
                        + if code { CODE_BLOCK_PADDING * 2. } else { 0. }
                        + code_gutter::width(block, text, Some(self)).unwrap_or(0.);
                    let mut valid = true;
                    for_each_display_line_range(text, |range| {
                        let range = segment.projection_start() + range.start
                            ..segment.projection_start() + range.end;
                        match self.line_width(projection, range, font_size) {
                            Some(width) => {
                                result.preferred[column] = result.preferred[column]
                                    .max(table_constraint_width(width, inset));
                                if code || segment.context.badge.is_some() {
                                    // Code lines and source-recognized short
                                    // status badges need their complete text,
                                    // not merely the longest word. Ordinary
                                    // prose stays flexible. Explicit widths
                                    // below still take precedence.
                                    result.minimum[column] = result.minimum[column]
                                        .max(table_constraint_width(width, inset));
                                }
                            }
                            None => valid = false,
                        }
                    });
                    // Unicode word boundaries avoid treating a whole CJK cell as
                    // one unbreakable Latin word. Inline styles use the exact font
                    // runs of each source slice. No byte-count width assumptions.
                    for (offset, word) in text
                        .split_word_bound_indices()
                        .filter(|(_, s)| !s.trim().is_empty())
                    {
                        let start = segment.projection_start() + offset;
                        result.minimum[column] =
                            result.minimum[column].max(table_constraint_width(
                                self.line_width(
                                    projection,
                                    start..start + word.len(),
                                    minimum_font_size,
                                )?,
                                inset,
                            ));
                    }
                    if !valid {
                        return None;
                    }
                }
                for (i, column) in table.columns.iter().enumerate() {
                    if let Some(width) = column.width {
                        result.minimum[i] = width.max(32.);
                        result.preferred[i] = width.max(32.);
                    } else {
                        result.preferred[i] = result.preferred[i].max(result.minimum[i]);
                    }
                }
                if let Ok(mut cache) = self.table_cache.lock() {
                    cache.insert(key, result.clone());
                }
                Some((id, result))
            })
            .collect::<Vec<_>>();
        diagnostics::count(|counts| {
            counts.table_attempts += table_attempts;
            counts.tables_measured += measured.len() as u64 - cache_hits;
            counts.table_cache_hits += cache_hits;
        });
        for (id, measured) in measured {
            projection.install_table_measurements(id, measured);
        }
    }

    /// Intrinsic width in document units, using exactly the paint font runs.
    /// Callers split explicit line breaks before measuring a line.
    pub fn line_width(
        &self,
        projection: &TextProjection,
        range: Range<usize>,
        font_size: f32,
    ) -> Option<f32> {
        diagnostics::count(|counts| counts.intrinsic_requests += 1);
        if let Some(segment) = projection.segment_for_range(&range)
            && inline_math::has_attachments(projection, segment.node_id)
            && let Some(lines) = inline_math::layout(
                projection,
                segment,
                range.clone(),
                f32::MAX,
                font_size,
                self,
            )
        {
            return lines.first().map(|line| line.width);
        }
        let text = &projection.text()[range.clone()];
        if text.contains('\n') || text.contains('\r') {
            return None;
        }
        let runs = styled_projection_runs(
            projection,
            &range,
            text.len(),
            &self.style,
            false,
            TachyonPalette::for_dark(false),
        );
        let mut hasher = DefaultHasher::new();
        for run in &runs {
            run.len.hash(&mut hasher);
            run.font.hash(&mut hasher);
        }
        let key = IntrinsicKey {
            text: text.into(),
            font_size: (font_size * self.zoom).to_bits(),
            runs: hasher.finish(),
        };
        if let Some(width) = self
            .intrinsic_cache
            .lock()
            .ok()
            .and_then(|mut cache| cache.get(&key))
        {
            diagnostics::count(|counts| counts.intrinsic_cache_hits += 1);
            return Some(width);
        }
        diagnostics::count(|counts| {
            counts.intrinsic_cache_misses += 1;
            counts.shaping_calls += 1;
        });
        let system = gpui::WindowTextSystem::new(self.fonts.clone());
        let line = system
            .shape_text(
                text.to_owned().into(),
                px(font_size * self.zoom),
                &runs,
                None,
                None,
            )
            .ok()?;
        let width = line
            .first()
            .map_or(0., |line| f32::from(line.unwrapped_layout.width))
            / self.zoom;
        if text.len() <= 16 * 1024
            && let Ok(mut cache) = self.intrinsic_cache.lock()
        {
            cache.insert(key, width);
        }
        Some(width)
    }

    pub fn matches(&self, family: &str, zoom: f32) -> bool {
        self.style.font_family.as_ref() == family && self.zoom.to_bits() == zoom.to_bits()
    }

    pub fn new(fonts: Arc<gpui::TextSystem>, family: gpui::SharedString, zoom: f32) -> Self {
        // Shape a fixed, ordinary-language calibration sample once per loaded
        // font environment, not once per paragraph. That keeps section width
        // stable while typing and avoids data-dependent width oscillation.
        const SAMPLE: &str = "Good writing gives ideas a clear shape and enough room for the reader to follow the argument.";
        let system = gpui::WindowTextSystem::new(fonts.clone());
        let prose_measures = [
            (family.clone(), DocumentStyle::REFERENCE_SIZE),
            ("Liberation Serif".into(), DocumentStyle::READING_SIZE),
        ]
        .map(|(family, size)| {
            let run = TextRun {
                len: SAMPLE.len(),
                font: gpui::font(family),
                color: rgb(TachyonPalette::LIGHT.text).into(),
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let line = system.shape_line(SAMPLE.into(), px(size * zoom), &[run], None);
            f32::from(line.width) / zoom / SAMPLE.chars().count() as f32
                * DocumentStyle::PROSE_CHARACTERS
        });
        let code_digit_width = ('0'..='9')
            .map(|digit| {
                let run = TextRun {
                    len: 1,
                    font: gpui::font("Spline Sans Mono Tachyon"),
                    color: rgb(TachyonPalette::LIGHT.secondary).into(),
                    background_color: None,
                    underline: None,
                    strikethrough: None,
                };
                f32::from(
                    system
                        .shape_line(
                            digit.to_string().into(),
                            px(DocumentStyle::CODE_SIZE * zoom),
                            &[run],
                            None,
                        )
                        .width,
                ) / zoom
            })
            .fold(0., f32::max);
        Self {
            identity: Arc::new(()),
            geometry: Mutex::new(super::geometry_cache::GeometryCache::new()),
            fonts,
            style: gpui::TextStyle {
                font_family: family,
                ..Default::default()
            },
            zoom,
            prose_measures,
            code_digit_width,
            cache: Mutex::new(BoundedLru::new(256)),
            intrinsic_cache: Mutex::new(BoundedLru::new(256)),
            table_cache: Mutex::new(BoundedLru::new(128)),
            item_cache: Mutex::new(BoundedLru::new(512)),
            group_cache: Mutex::new(BoundedLru::new(512)),
        }
    }

    pub fn prose_width(&self, narrative: bool, font_size: f32) -> f32 {
        let base_size = if narrative {
            DocumentStyle::READING_SIZE
        } else {
            DocumentStyle::REFERENCE_SIZE
        };
        self.prose_measures[usize::from(narrative)] * font_size / base_size
    }

    pub fn prose_measures(&self) -> crate::adaptive::ProseMeasures {
        crate::adaptive::ProseMeasures {
            reference: self.prose_width(false, DocumentStyle::REFERENCE_SIZE),
            narrative: self.prose_width(true, DocumentStyle::READING_SIZE),
            lead: self.prose_width(false, DocumentStyle::LEAD_SIZE),
        }
    }

    pub fn wrap(
        &self,
        projection: &TextProjection,
        segment: &crate::ProjectionSegment,
        range: Range<usize>,
        width: f32,
        font_size: f32,
    ) -> Option<Vec<Range<usize>>> {
        diagnostics::count(|counts| counts.wrap_requests += 1);
        if inline_math::has_attachments(projection, segment.node_id)
            && let Some(lines) =
                inline_math::layout(projection, segment, range.clone(), width, font_size, self)
        {
            return Some(lines.into_iter().map(|line| line.range).collect());
        }
        if range.is_empty() {
            return Some(vec![range]);
        }
        let text: Arc<str> = projection.text()[range.clone()].into();
        let runs = styled_projection_runs(
            projection,
            &range,
            text.len(),
            &self.style,
            false,
            TachyonPalette::for_dark(false),
        );
        let mut hasher = DefaultHasher::new();
        for run in &runs {
            run.len.hash(&mut hasher);
            run.font.hash(&mut hasher);
        }
        let key = MeasureKey {
            node: segment.node_id,
            revision: projection.node_revision(segment.node_id),
            local_range: range.start - segment.projection_start()
                ..range.end - segment.projection_start(),
            width: (width * self.zoom).to_bits(),
            font_size: (font_size * self.zoom).to_bits(),
            runs: hasher.finish(),
            text: text.clone(),
            refine_ending: paragraph_endings::eligible(projection, segment, &range),
        };
        if let Some(cached) = self.cache.lock().ok().and_then(|mut cache| cache.get(&key)) {
            diagnostics::count(|counts| counts.wrap_cache_hits += 1);
            return Some(
                cached
                    .into_iter()
                    .map(|local| range.start + local.start..range.start + local.end)
                    .collect(),
            );
        }
        // A short-lived layout cache prevents cold long-document measurement
        // from retaining every glyph in GPUI's per-window frame cache.
        diagnostics::count(|counts| counts.wrap_cache_misses += 1);
        if contains_strong_rtl(&text) {
            let local =
                self.wrap_bidi_source_order(projection, range.clone(), &text, width, font_size)?;
            if text.len() <= 16 * 1024
                && let Ok(mut cache) = self.cache.lock()
            {
                cache.insert(key, local.clone());
            }
            return Some(
                local
                    .into_iter()
                    .map(|local| range.start + local.start..range.start + local.end)
                    .collect(),
            );
        }
        diagnostics::count(|counts| counts.shaping_calls += 1);
        let system = gpui::WindowTextSystem::new(self.fonts.clone());
        let shaped = system
            .shape_text(
                text.to_string().into(),
                px(font_size * self.zoom),
                &runs,
                Some(px((width * self.zoom).max(1.))),
                None,
            )
            .ok()?;
        let line = shaped.first()?;
        let mut starts = vec![0];
        let graphemes = text
            .grapheme_indices(true)
            .map(|(offset, _)| offset)
            .collect::<Vec<_>>();
        for boundary in line.wrap_boundaries() {
            let index = line
                .unwrapped_layout
                .runs
                .get(boundary.run_ix)?
                .glyphs
                .get(boundary.glyph_ix)?
                .index;
            if index > *starts.last()? && index < text.len() {
                // Native shaping boundaries must not split an extended
                // grapheme cluster (emoji/combining marks included).
                let safe = graphemes[graphemes
                    .partition_point(|offset| *offset <= index)
                    .saturating_sub(1)];
                if safe > *starts.last()? {
                    starts.push(safe);
                }
            }
        }
        starts.push(text.len());
        let mut local = starts
            .windows(2)
            .map(|pair| pair[0]..pair[1])
            .collect::<Vec<_>>();
        if let Some(repaired) = line_breaks::repair(
            &text,
            &local,
            &graphemes,
            &line.unwrapped_layout,
            width,
            self.zoom,
            |local| {
                self.line_width(
                    projection,
                    range.start + local.start..range.start + local.end,
                    font_size,
                )
            },
        ) {
            local = repaired;
        }
        if key.refine_ending {
            paragraph_endings::refine(&text, &mut local, &graphemes, width, |local| {
                self.line_width(
                    projection,
                    range.start + local.start..range.start + local.end,
                    font_size,
                )
            });
        }
        if text.len() <= 16 * 1024
            && let Ok(mut cache) = self.cache.lock()
        {
            cache.insert(key, local.clone());
        }
        Some(
            local
                .into_iter()
                .map(|local| range.start + local.start..range.start + local.end)
                .collect(),
        )
    }

    /// GPUI's exposed wrap-boundary representation follows visual glyph order
    /// and therefore cannot be consumed as monotonically increasing source
    /// offsets for RTL paragraphs. Measure bounded logical candidates instead:
    /// source ranges stay canonical while every candidate is shaped by the
    /// same font/run path used for painting.
    fn wrap_bidi_source_order(
        &self,
        projection: &TextProjection,
        range: Range<usize>,
        text: &str,
        width: f32,
        font_size: f32,
    ) -> Option<Vec<Range<usize>>> {
        let grapheme_ends = text
            .grapheme_indices(true)
            .skip(1)
            .map(|(offset, _)| offset)
            .chain(std::iter::once(text.len()))
            .collect::<Vec<_>>();
        let preferred_ends = line_breaks::opportunities(text, &grapheme_ends);
        let mut lines = Vec::new();
        let mut start = 0;
        while start < text.len() {
            let mut best = start;
            let first = preferred_ends.partition_point(|end| *end <= start);
            for end in preferred_ends[first..].iter().copied() {
                let candidate = range.start + start..range.start + end;
                if self.line_width(projection, candidate, font_size)? <= width.max(1.) {
                    best = end;
                } else {
                    break;
                }
            }
            if best == start {
                let first = grapheme_ends.partition_point(|end| *end <= start);
                for end in grapheme_ends[first..].iter().copied() {
                    let candidate = range.start + start..range.start + end;
                    if self.line_width(projection, candidate, font_size)? <= width.max(1.) {
                        best = end;
                    } else {
                        break;
                    }
                }
                if best == start {
                    best = grapheme_ends.iter().copied().find(|end| *end > start)?;
                }
            }
            lines.push(start..best);
            start = best;
        }
        Some(lines)
    }
}

pub(super) fn contains_strong_rtl(text: &str) -> bool {
    text.chars().any(|character| {
        matches!(
            unicode_bidi::bidi_class(character),
            unicode_bidi::BidiClass::R | unicode_bidi::BidiClass::AL
        )
    })
}

// Round outward, leaving a subpixel guard for padding/track/zoom round trips.
// Otherwise an exactly fitted word can become microscopically too wide and
// the native wrapper moves its last glyph onto another line. Explicit authored
// column widths do not use this intrinsic sizing policy.
fn table_constraint_width(content: f32, inset: f32) -> f32 {
    (content + inset).ceil() + 1. / 64.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn punctuation_wrap_does_not_start_a_line_with_compound_slash(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = "A undo/redo";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            for width in 90..140 {
                let lines = fonts
                    .wrap(
                        &projection,
                        segment,
                        segment.projection_range(),
                        width as f32,
                        18.,
                    )
                    .unwrap();
                assert!(
                    lines
                        .iter()
                        .all(|r| !projection.text()[r.clone()].starts_with('/')),
                    "width {width}: {:?}",
                    lines
                        .iter()
                        .map(|r| &projection.text()[r.clone()])
                        .collect::<Vec<_>>()
                );
            }
        });
    }

    #[gpui::test]
    fn punctuation_wraps_fit_preserve_graphemes_styles_and_logical_source(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            for source in [
                "A undo/redo follows. Another read/write operation.",
                "A **stop!** Then (go) and [read](https://example.com).",
                "A `path/to/file` and 06/07/99 then more text.",
                "A no\u{a0}break and no\u{2060}break phrase.",
                "A e\u{301}/é and 👨‍👩‍👧‍👦/👩‍👩‍👧 family.",
                "文字（短い）文章、句読点。次の説明。",
                "مرحبا undo/redo عالم جديد ونص آخر.",
            ] {
                let document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let segment = &projection.segments()[0];
                let text = &projection.text()[segment.projection_range()];
                let graphemes = text
                    .grapheme_indices(true)
                    .map(|(i, _)| i)
                    .collect::<Vec<_>>();
                let legal = line_breaks::opportunities(text, &graphemes);
                for zoom in [1., 1.5, 2.] {
                    let fonts = FontMeasurement::new(
                        cx.text_system().clone(),
                        "Spline Sans Tachyon".into(),
                        zoom,
                    );
                    // Every legal unit fits by itself, so an emergency break
                    // cannot justify violating a punctuation/glue boundary.
                    let mut start = 0;
                    let mut minimum = 90_f32;
                    for &end in &legal {
                        minimum = minimum.max(
                            fonts
                                .line_width(
                                    &projection,
                                    segment.projection_start() + start
                                        ..segment.projection_start() + end,
                                    18.,
                                )
                                .unwrap()
                                .ceil()
                                + 1.,
                        );
                        start = end;
                    }
                    for width in [minimum, minimum + 17., minimum + 53.] {
                        let lines = fonts
                            .wrap(&projection, segment, segment.projection_range(), width, 18.)
                            .unwrap();
                        assert_eq!(
                            lines
                                .iter()
                                .map(|r| &projection.text()[r.clone()])
                                .collect::<String>(),
                            text
                        );
                        for line in &lines {
                            assert!(
                                legal.contains(&(line.end - segment.projection_start())),
                                "illegal break in {source:?}: {line:?} at {width}"
                            );
                            assert!(graphemes.contains(&(line.start - segment.projection_start())));
                            assert!(
                                fonts.line_width(&projection, line.clone(), 18.).unwrap()
                                    <= width + 0.01,
                                "overflow in {source:?} at {width}: {line:?} {:?}, measured {:?}",
                                &projection.text()[line.clone()],
                                fonts.line_width(&projection, line.clone(), 18.)
                            );
                        }
                        let scope = diagnostics::MeasurementScope::new();
                        assert_eq!(
                            fonts
                                .wrap(&projection, segment, segment.projection_range(), width, 18.)
                                .unwrap(),
                            lines
                        );
                        let warm = scope.take_stage();
                        assert_eq!(warm.shaping_calls, 0);
                        assert_eq!(warm.wrap_cache_hits, 1);
                    }
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn punctuation_emergency_wraps_preserve_oversized_graphemes_and_range_offsets(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = format!("# Prefix\n\n{}\n", "é/👩‍👩‍👧".repeat(64));
            let document = Document::from_markdown(source.as_str()).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[1];
            let text = &projection.text()[segment.projection_range()];
            assert!(segment.projection_start() > 0);
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            for width in [1., 18., 90.] {
                let lines = fonts
                    .wrap(&projection, segment, segment.projection_range(), width, 18.)
                    .unwrap();
                assert_eq!(
                    lines
                        .iter()
                        .map(|r| &projection.text()[r.clone()])
                        .collect::<String>(),
                    text
                );
                assert_eq!(lines.first().unwrap().start, segment.projection_start());
                assert_eq!(lines.last().unwrap().end, segment.projection_end());
                assert!(lines.windows(2).all(|pair| pair[0].end == pair[1].start));
                for line in &lines {
                    let part = &projection.text()[line.clone()];
                    assert!(!part.is_empty());
                    assert!(
                        text.grapheme_indices(true)
                            .any(|(i, _)| i + segment.projection_start() == line.start)
                    );
                    assert!(
                        fonts.line_width(&projection, line.clone(), 18.).unwrap() <= width + 0.01
                            || part.graphemes(true).count() == 1
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn diagnostic_counters_distinguish_cold_shapes_from_warm_cache_hits(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document =
                Document::from_markdown("A representative paragraph with several words.").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let scope = diagnostics::MeasurementScope::new();
            let cold_width = measurement
                .line_width(&projection, segment.projection_range(), 18.)
                .unwrap();
            let cold_wrap = measurement
                .wrap(&projection, segment, segment.projection_range(), 220., 18.)
                .unwrap();
            let cold = scope.take_stage();
            assert_eq!(cold.shaping_calls, 2);
            assert_eq!(
                (cold.wrap_cache_misses, cold.intrinsic_cache_misses),
                (1, 1)
            );
            assert_eq!(
                measurement.line_width(&projection, segment.projection_range(), 18.),
                Some(cold_width)
            );
            assert_eq!(
                measurement.wrap(&projection, segment, segment.projection_range(), 220., 18.),
                Some(cold_wrap)
            );
            let warm = scope.take_stage();
            assert_eq!(warm.shaping_calls, 0);
            assert_eq!((warm.wrap_cache_hits, warm.intrinsic_cache_hits), (1, 1));
            measurement
                .wrap(&projection, segment, segment.projection_range(), 221., 18.)
                .unwrap();
            assert_eq!(
                scope.take_stage().wrap_cache_misses,
                1,
                "a different exact width cannot report a cache hit"
            );
        });
    }

    #[test]
    fn table_constraint_survives_padding_and_track_round_trips() {
        // The bundled semibold "Retries" advance loses a few float bits when
        // cell padding is added and subtracted; the native wrapper then breaks
        // before its final glyph. Include scaled/column-local geometry too.
        let intrinsic = 51.646_f32;
        let column = table_constraint_width(intrinsic, 24.);
        for canvas in [360_f32, 960., 1280., 1920.] {
            for zoom in [0.7, 1., 1.3, 2.] {
                let fraction = column / canvas;
                let available = (canvas * zoom * fraction - 24. * zoom) / zoom;
                assert!(available >= intrinsic, "{available} < {intrinsic}");
            }
        }
    }

    #[gpui::test]
    fn measured_single_word_cells_do_not_wrap_at_their_intrinsic_width(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document = Document::from_markdown(
                "| Retries | Environment |\n| --- | --- |\n| 3 | Production |\n",
            )
            .unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            measurement.measure_tables(&mut projection);
            for segment in projection.segments() {
                let width = segment_text_width(segment, &projection, 1100.);
                let wraps = measurement
                    .wrap(
                        &projection,
                        segment,
                        segment.projection_range(),
                        width,
                        DocumentStyle::TABLE_SIZE,
                    )
                    .unwrap();
                assert_eq!(
                    wraps.len(),
                    1,
                    "intrinsic cell width must fit {:?}: {width}",
                    &projection.text()[segment.projection_range()]
                );
            }
        });
    }

    #[gpui::test]
    fn measured_table_short_status_keeps_its_complete_badge(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/76-entity-records.md");
            let document = Document::from_markdown(source).unwrap();
            for zoom in [1., 1.5, 2.] {
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), zoom);
                fonts.measure_tables(&mut projection);
                for width in [260., 420., 560., 641., 760., 1280.] {
                    let plan = build_measured_adaptive_plan(&projection, width, 1200., None, false, &fonts);
                    let lines = build_measured_visual_lines(&projection, &HashMap::new(), width, &plan, Some(&fonts));
                    for segment in projection.segments() {
                        let own = lines.iter().filter(|line| projection.segment_for_range(&line.projected_range()).unwrap().node_id == segment.node_id).collect::<Vec<_>>();
                        assert_eq!(own.iter().map(|l| &projection.text()[l.projected_range()]).collect::<String>(), projection.text()[segment.projection_range()]);
                        if &projection.text()[segment.projection_range()] == "In review" {
                            assert_eq!(own.len(), 1, "a short status must fit when its table can give it its natural width: {width}/{zoom}");
                            assert!(badge_range(segment, segment.context.badge.unwrap(), &own[0].projected_range()).is_some());
                        }
                    }
                    for root in projection.roots().filter(|r| matches!(r, BlockNode::Table(_))) {
                        let measured = projection.table_measurements(root.id()).unwrap();
                        let fitted = projection.fitted_table_widths(root.id(), width).unwrap();
                        for ((fit, low), high) in fitted.iter().zip(&measured.minimum).zip(&measured.preferred) {
                            assert!(fit >= low && fit <= high);
                        }
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn table_badge_growth_keeps_locked_widths_and_row_local_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/76-entity-records.md");
            for width in [641., 760., 1280.] {
                let mut document = Document::from_markdown(source).unwrap();
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Tachyon".into(),
                    1.,
                );
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut projection);
                let segment = projection
                    .segments()
                    .iter()
                    .find(|s| &projection.text()[s.projection_range()] == "In review")
                    .unwrap();
                let node = segment.node_id;
                let table = segment.context.table_cell.unwrap().0;
                projection.lock_table_for_node(Some(node));
                let widths = projection.fitted_table_widths(table, width).unwrap();
                let plan =
                    build_measured_adaptive_plan(&projection, width, 1200., None, false, &fonts);
                let mut lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                let mut order = visual_line_paint_order(&lines);
                let inserted = "Awaiting the complete review of all attached source material ";
                document
                    .apply(EditCommand::ReplaceText {
                        node_id: node,
                        range: 0..0,
                        text: inserted.into(),
                        typing: true,
                        selection_after: None,
                    })
                    .unwrap();
                assert_eq!(
                    document.snapshot().serialize().unwrap(),
                    source.replace("| In review |", &format!("| {inserted}In review |"))
                );
                refresh_arranged_text_node_geometry(
                    &mut projection,
                    &mut lines,
                    &mut order,
                    &plan,
                    TextRefreshRequest {
                        snapshot: &document.snapshot(),
                        node_id: node,
                        image_dimensions: &HashMap::new(),
                        layout_width: width,
                        zoom_factor: 1.,
                        measurement: Some(&fonts),
                    },
                )
                .expect("status growth retains the existing row-local edit path");
                assert_eq!(
                    projection.fitted_table_widths(table, width).unwrap(),
                    widths
                );
                let full = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                assert_eq!(lines.len(), full.len());
                for (local, rebuilt) in lines.iter().zip(&full) {
                    assert_eq!(local.projected_range(), rebuilt.projected_range());
                    assert!((local.y - rebuilt.y).abs() < 0.01);
                    assert_eq!(local.x_fraction, rebuilt.x_fraction);
                    assert_eq!(local.width_fraction, rebuilt.width_fraction);
                    assert_eq!(local.table_row_height, rebuilt.table_row_height);
                }
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn table_badge_constraints_require_semantics_and_honor_explicit_widths(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            for (header, value, atomic) in [
                ("Status", "In review", true),
                ("State", "**In review**", true),
                ("Classification", "Internal use only", true),
                ("Outcome", "In review", false),
                ("Status", "This status is described in a complete sentence rather than a short label.", false),
            ] {
                let source = format!("| {header} | Description |\n| --- | --- |\n| {value} | A longer description can wrap into the remaining table width without breaking a short semantic label. |\n");
                let document = Document::from_markdown(source.as_str()).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut projection);
                let table = projection.roots().next().unwrap().id();
                let measured = projection.table_measurements(table).unwrap();
                let segment = projection.segments().iter().find(|s| s.context.table_cell == Some((table, 1, 0))).unwrap();
                assert_eq!(segment.context.badge.is_some(), atomic);
                if atomic {
                    let complete = fonts.line_width(&projection, segment.projection_range(), DocumentStyle::TABLE_SIZE).unwrap();
                    assert!(measured.minimum[0] >= complete + 24.);
                } else {
                    assert!(measured.minimum[0] < measured.preferred[0], "ordinary multiword text must remain flexible");
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
            let explicit = concat!("<!-- tachyon-table:v1 {\"border\":\"LogicalPixel\",\"widths\":[64.0,300.0]} -->\n",
                "| Status | Description |\n| --- | --- |\n| In review | Explicit author widths take precedence. |\n");
            let document = Document::from_markdown(explicit).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            fonts.measure_tables(&mut projection);
            let table = projection.roots().next().unwrap().id();
            assert_eq!(projection.fitted_table_widths(table, 1000.).unwrap(), vec![64., 300.]);
            let segment = projection.segments().iter().find(|s| s.context.table_cell == Some((table, 1, 0))).unwrap();
            let width = segment_text_width(segment, &projection, 1000.);
            assert!(fonts.wrap(&projection, segment, segment.projection_range(), width, DocumentStyle::TABLE_SIZE).unwrap().len() > 1, "an explicitly narrow cell must still wrap readable source text");
            assert_eq!(document.snapshot().serialize().unwrap(), explicit);
        });
    }

    #[gpui::test]
    fn table_constraints_include_last_row_and_preserve_unbreakable_words(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = format!("| Key | Value |\n| --- | --- |\n{}| configuration_schema_revision_identifier | last |\n", "| id | short |\n".repeat(100));
            let document = Document::from_markdown(source.as_str()).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let id = projection.roots().next().unwrap().id();
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            measurement.measure_tables(&mut projection);
            let measured = projection.table_measurements(id).unwrap();
            let last = projection.segments().iter().find(|s| &projection.text()[s.projection_range()] == "configuration_schema_revision_identifier").unwrap();
            let required = measurement.line_width(&projection, last.projection_range(), DocumentStyle::TABLE_SIZE).unwrap() + 24.;
            assert!(measured.minimum[0] >= required - 0.01);
            assert!(measured.minimum[0] > measured.minimum[1] * 2.);
            let fitted = projection.fitted_table_widths(id, 240.).unwrap();
            assert!(fitted[0] >= required - 0.01, "narrow columns must overflow locally, not crush identifiers");
            assert!(fitted.iter().sum::<f32>() > 240.);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn oversized_table_measurement_is_not_mistaken_for_a_complete_sample(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = format!(
                "| Key | Value |\n| --- | --- |\n{}",
                "| id | short |\n".repeat(300)
            );
            let document = Document::from_markdown(source.as_str()).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let id = projection.roots().next().unwrap().id();
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            measurement.measure_tables(&mut projection);
            assert!(projection.table_measurements(id).is_none());
            assert!(
                projection.fitted_table_widths(id, 600.).is_some(),
                "complete source-order fallback remains available"
            );
        });
    }

    #[gpui::test]
    fn intrinsic_width_cache_distinguishes_exact_text_font_runs_and_size(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document =
                Document::from_markdown("iiiiiiii\n\nWWWWWWWW\n\n**WWWWWWWW**\n").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let ranges = projection
                .segments()
                .iter()
                .map(|s| s.projection_range())
                .collect::<Vec<_>>();
            let thin = measurement
                .line_width(&projection, ranges[0].clone(), 18.)
                .unwrap();
            let wide = measurement
                .line_width(&projection, ranges[1].clone(), 18.)
                .unwrap();
            // GPUI's deterministic test platform uses NoopTextSystem, whose
            // equal-length strings have equal synthetic widths. They must
            // still occupy different cache entries; production glyph widths
            // are validated separately with the native Wayland fixture.
            assert!(thin > 0. && wide > 0.);
            assert_eq!(measurement.intrinsic_cache.lock().unwrap().entries.len(), 2);
            assert_eq!(
                wide,
                measurement
                    .line_width(&projection, ranges[1].clone(), 18.)
                    .unwrap()
            );
            measurement
                .line_width(&projection, ranges[2].clone(), 18.)
                .unwrap();
            let larger = measurement
                .line_width(&projection, ranges[1].clone(), 36.)
                .unwrap();
            assert!(larger > wide * 1.9);
            assert_eq!(measurement.intrinsic_cache.lock().unwrap().entries.len(), 4);
        });
    }

    #[gpui::test]
    fn measured_wraps_preserve_text_graphemes_and_exact_width_cache_keys(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document = Document::from_markdown("# Atlas product specification\n\nA **bold** label and `identifier` with e\u{301} and 👨‍👩‍👧‍👦.\n").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            for segment in projection.segments() {
                let mut previous = None;
                for width in [70., 70.25, 320., 760.] {
                    let ranges = measurement.wrap(&projection, segment, segment.projection_range(), width, 18.).unwrap();
                    assert_eq!(ranges.first().unwrap().start, segment.projection_start());
                    assert_eq!(ranges.last().unwrap().end, segment.projection_end());
                    assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start));
                    for range in &ranges {
                        assert!(projection.text()[segment.projection_range()].grapheme_indices(true)
                            .any(|(offset, _)| segment.projection_start() + offset == range.start));
                    }
                    assert_eq!(ranges, measurement.wrap(&projection, segment, segment.projection_range(), width, 18.).unwrap());
                    if let Some(count) = previous { assert!(ranges.len() <= count); }
                    previous = Some(ranges.len());
                }
            }
            assert_eq!(measurement.cache.lock().unwrap().entries.len(), 8);
        });
    }

    #[gpui::test]
    fn rtl_wraps_remain_logical_contiguous_and_fit_the_measured_width(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "مرحبا بالعالم يحافظ التخطيط التلقائي على ترتيب المصدر وموضع القراءة";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let ranges = measurement
                .wrap(&projection, segment, segment.projection_range(), 180., 18.)
                .unwrap();
            assert!(ranges.len() > 1);
            assert_eq!(ranges.first().unwrap().start, segment.projection_start());
            assert_eq!(ranges.last().unwrap().end, segment.projection_end());
            assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start));
            assert!(ranges.iter().all(|range| {
                projection.text().is_char_boundary(range.start)
                    && projection.text().is_char_boundary(range.end)
                    && measurement
                        .line_width(&projection, range.clone(), 18.)
                        .unwrap()
                        <= 180.01
            }));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn geometry_uses_shaped_width_instead_of_a_character_budget(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown("# specification\n").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            let runs = styled_projection_runs(
                &projection,
                &segment.projection_range(),
                projection.text().len(),
                &measurement.style,
                false,
                TachyonPalette::for_dark(false),
            );
            let native = gpui::WindowTextSystem::new(cx.text_system().clone()).shape_line(
                projection.text().to_owned().into(),
                px(DocumentStyle::heading(1).0),
                &runs,
                None,
            );
            let width = f32::from(native.width()) + 8.1;
            let plan = AdaptivePlan::build(&projection, width, None, false);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                width,
                &plan,
                Some(&measurement),
            );
            assert_eq!(
                lines.len(),
                1,
                "a word fitting at the actual font width must stay whole"
            );
            assert_eq!(lines[0].projected_range(), segment.projection_range());
        });
    }
}
