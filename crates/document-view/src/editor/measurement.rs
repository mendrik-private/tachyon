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
}

pub(super) struct FontMeasurement {
    pub identity: Arc<()>,
    pub(super) geometry: Mutex<super::geometry_cache::GeometryCache>,
    fonts: Arc<gpui::TextSystem>,
    style: gpui::TextStyle,
    zoom: f32,
    cache: Mutex<BoundedLru<MeasureKey, Vec<Range<usize>>>>,
    intrinsic_cache: Mutex<BoundedLru<IntrinsicKey, f32>>,
    table_cache: Mutex<BoundedLru<ContentIdentity, crate::projection::TableMeasurements>>,
    item_cache: Mutex<BoundedLru<ListItemMeasureKey, crate::adaptive::candidates::ItemMeasurement>>,
    group_cache: Mutex<
        BoundedLru<super::arrangement::GroupMeasureKey, crate::adaptive::rows::GroupMeasurement>,
    >,
}

impl FontMeasurement {
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
            math_edit: projection.math_edit_node == Some(node),
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
            MineralPalette::for_dark(false),
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
                let key = ContentIdentity(projection.block_handle(id)?.clone());
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
                    .map(|i| projection.segments()[*i].projection_range.len())
                    .sum::<usize>();
                if bytes > 64 * 1024 {
                    return None;
                }
                let mut result = TableMeasurements {
                    minimum: vec![64.; table.columns.len()],
                    preferred: vec![64.; table.columns.len()],
                };
                for index in indexes {
                    let segment = &projection.segments()[index];
                    let (_, _, column) = segment.context.table_cell?;
                    let block = projection.block(segment.node_id)?;
                    if column >= table.columns.len()
                        || segment.projection_range.len() > 4096
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
                        &segment.projection_range,
                        None,
                    )
                    .font_size;
                    // Container indentation is applied once to the table's
                    // origin/viewport; intrinsic columns own only cell padding.
                    let code = matches!(block, BlockNode::CodeBlock(_));
                    let inset = 24.
                        + table_insets(segment, projection).1
                        + if code { CODE_BLOCK_PADDING * 2. } else { 0. };
                    let text = &projection.text()[segment.projection_range.clone()];
                    let mut valid = true;
                    for_each_display_line_range(text, |range| {
                        let range = segment.projection_range.start + range.start
                            ..segment.projection_range.start + range.end;
                        match self.line_width(projection, range, font_size) {
                            Some(width) => {
                                result.preferred[column] = result.preferred[column]
                                    .max(table_constraint_width(width, inset));
                                if code {
                                    // Unwrapped code needs its complete line,
                                    // not merely the longest word. Any excess
                                    // belongs to contained table scrolling.
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
                        let start = segment.projection_range.start + offset;
                        result.minimum[column] =
                            result.minimum[column].max(table_constraint_width(
                                self.line_width(projection, start..start + word.len(), font_size)?,
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
            && inline_math::has_math(projection, segment.node_id)
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
            MineralPalette::for_dark(false),
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
        Self {
            identity: Arc::new(()),
            geometry: Mutex::new(super::geometry_cache::GeometryCache::new()),
            fonts,
            style: gpui::TextStyle {
                font_family: family,
                ..Default::default()
            },
            zoom,
            cache: Mutex::new(BoundedLru::new(256)),
            intrinsic_cache: Mutex::new(BoundedLru::new(256)),
            table_cache: Mutex::new(BoundedLru::new(128)),
            item_cache: Mutex::new(BoundedLru::new(512)),
            group_cache: Mutex::new(BoundedLru::new(512)),
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
        if inline_math::has_math(projection, segment.node_id)
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
            MineralPalette::for_dark(false),
        );
        let mut hasher = DefaultHasher::new();
        for run in &runs {
            run.len.hash(&mut hasher);
            run.font.hash(&mut hasher);
        }
        let key = MeasureKey {
            node: segment.node_id,
            revision: projection.node_revision(segment.node_id),
            local_range: range.start - segment.projection_range.start
                ..range.end - segment.projection_range.start,
            width: (width * self.zoom).to_bits(),
            font_size: (font_size * self.zoom).to_bits(),
            runs: hasher.finish(),
            text: text.clone(),
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
        diagnostics::count(|counts| {
            counts.wrap_cache_misses += 1;
            counts.shaping_calls += 1;
        });
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
        let local = starts
            .windows(2)
            .map(|pair| pair[0]..pair[1])
            .collect::<Vec<_>>();
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
    fn diagnostic_counters_distinguish_cold_shapes_from_warm_cache_hits(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document =
                Document::from_markdown("A representative paragraph with several words.").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let scope = diagnostics::MeasurementScope::new();
            let cold_width = measurement
                .line_width(&projection, segment.projection_range.clone(), 18.)
                .unwrap();
            let cold_wrap = measurement
                .wrap(
                    &projection,
                    segment,
                    segment.projection_range.clone(),
                    220.,
                    18.,
                )
                .unwrap();
            let cold = scope.take_stage();
            assert_eq!(cold.shaping_calls, 2);
            assert_eq!(
                (cold.wrap_cache_misses, cold.intrinsic_cache_misses),
                (1, 1)
            );
            assert_eq!(
                measurement.line_width(&projection, segment.projection_range.clone(), 18.),
                Some(cold_width)
            );
            assert_eq!(
                measurement.wrap(
                    &projection,
                    segment,
                    segment.projection_range.clone(),
                    220.,
                    18.
                ),
                Some(cold_wrap)
            );
            let warm = scope.take_stage();
            assert_eq!(warm.shaping_calls, 0);
            assert_eq!((warm.wrap_cache_hits, warm.intrinsic_cache_hits), (1, 1));
            measurement
                .wrap(
                    &projection,
                    segment,
                    segment.projection_range.clone(),
                    221.,
                    18.,
                )
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
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            measurement.measure_tables(&mut projection);
            for segment in projection.segments() {
                let width = segment_text_width(segment, &projection, 1100.);
                let wraps = measurement
                    .wrap(
                        &projection,
                        segment,
                        segment.projection_range.clone(),
                        width,
                        15.5,
                    )
                    .unwrap();
                assert_eq!(
                    wraps.len(),
                    1,
                    "intrinsic cell width must fit {:?}: {width}",
                    &projection.text()[segment.projection_range.clone()]
                );
            }
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            measurement.measure_tables(&mut projection);
            let measured = projection.table_measurements(id).unwrap();
            let last = projection.segments().iter().find(|s| &projection.text()[s.projection_range.clone()] == "configuration_schema_revision_identifier").unwrap();
            let required = measurement.line_width(&projection, last.projection_range.clone(), 15.5).unwrap() + 24.;
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
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
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
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let ranges = projection
                .segments()
                .iter()
                .map(|s| s.projection_range.clone())
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
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            for segment in projection.segments() {
                let mut previous = None;
                for width in [70., 70.25, 320., 760.] {
                    let ranges = measurement.wrap(&projection, segment, segment.projection_range.clone(), width, 18.).unwrap();
                    assert_eq!(ranges.first().unwrap().start, segment.projection_range.start);
                    assert_eq!(ranges.last().unwrap().end, segment.projection_range.end);
                    assert!(ranges.windows(2).all(|pair| pair[0].end == pair[1].start));
                    for range in &ranges {
                        assert!(projection.text()[segment.projection_range.clone()].grapheme_indices(true)
                            .any(|(offset, _)| segment.projection_range.start + offset == range.start));
                    }
                    assert_eq!(ranges, measurement.wrap(&projection, segment, segment.projection_range.clone(), width, 18.).unwrap());
                    if let Some(count) = previous { assert!(ranges.len() <= count); }
                    previous = Some(ranges.len());
                }
            }
            assert_eq!(measurement.cache.lock().unwrap().entries.len(), 8);
        });
    }

    #[gpui::test]
    fn geometry_uses_shaped_width_instead_of_a_character_budget(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let document = Document::from_markdown("# specification\n").unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let segment = &projection.segments()[0];
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let runs = styled_projection_runs(
                &projection,
                &segment.projection_range,
                projection.text().len(),
                &measurement.style,
                false,
                MineralPalette::for_dark(false),
            );
            let native = gpui::WindowTextSystem::new(cx.text_system().clone()).shape_line(
                projection.text().to_owned().into(),
                px(44.),
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
            assert_eq!(lines[0].range, segment.projection_range);
        });
    }
}
