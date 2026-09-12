//! Measured, source-owned property and entity records. Comparisons stay tables.
use super::*;

pub(super) const INSET: f32 = 24.;
pub(super) const LABEL_GAP: f32 = 8.;
pub(super) const RECORD_GAP: f32 = crate::projection::RecordLayout::GAP;
pub(super) const TITLE_SIZE: f32 = 18.;
pub(super) const TITLE_LEADING: f32 = 24.;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Geometry {
    pub top: f32,
    pub height: f32,
}

#[derive(Clone)]
pub(super) struct Label {
    pub header: NodeId,
    /// Header-local ranges survive edits before the original table header.
    pub ranges: Vec<Range<usize>>,
    pub width: f32,
    pub height: f32,
    pub inline: bool,
}

/// Repeated labels are presentation of an existing column header, never a
/// second editable paragraph or a new source/clipboard/accessibility node.
pub(super) fn field(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    width: f32,
) -> Option<(NodeId, f32, bool)> {
    let (id, row, column) = segment.context.table_cell?;
    let measured = projection.table_measurements(id)?;
    let headers = measured.record_headers.as_ref()?;
    if row == 0 || column == 0 || headers.len() < 3 || !projection.uses_record_layout(id, width) {
        return None;
    }
    let BlockNode::Table(table) = projection.block(id)? else {
        return None;
    };
    let header = table.rows.first()?.cells.get(column)?.blocks.get(0)?.id();
    let rail = headers.iter().skip(1).copied().fold(0., f32::max) - 24.;
    let available = projection.record_layout(id, width)?.width - 2. * INSET;
    let inline = available - rail - 16. >= (measured.minimum[column] - 24.).max(160.);
    Some((header, if inline { rail } else { available }, inline))
}

pub(super) fn body_inset(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    width: f32,
) -> f32 {
    field(projection, segment, width)
        .filter(|(_, _, inline)| *inline)
        .map_or(0., |(_, rail, _)| rail + 16.)
}

pub(super) fn label(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    width: f32,
    fonts: &FontMeasurement,
) -> Option<Arc<Label>> {
    let (header, width, inline) = field(projection, segment, width)?;
    let header_segment = projection.segment_for_node(header)?;
    let ranges = fonts.wrap(
        projection,
        header_segment,
        header_segment.projection_range(),
        width,
        DocumentStyle::TABLE_SIZE,
    )?;
    let height = ranges.len() as f32 * DocumentStyle::TABLE_LEADING;
    Some(Arc::new(Label {
        header,
        ranges: ranges
            .into_iter()
            .map(|r| {
                r.start - header_segment.projection_start()
                    ..r.end - header_segment.projection_start()
            })
            .collect(),
        width,
        height,
        inline,
    }))
}

pub(super) fn paint_labels(
    editor: &RichDocumentEditor,
    spec: &VisualLineSpec,
    value_bounds: Bounds<Pixels>,
    palette: TachyonPalette,
    window: &mut Window,
) -> Vec<PaintedLine> {
    let Some(label) = &spec.record_label else {
        return Vec::new();
    };
    let Some(header) = editor.projection.segment_for_node(label.header) else {
        return Vec::new();
    };
    let snapshot = editor.document.snapshot();
    let zoom = editor.zoom_factor;
    let x = value_bounds.left() - px(spec.inset) + px(INSET * zoom);
    let y = value_bounds.top()
        - if label.inline {
            px(0.)
        } else {
            px(label.height + LABEL_GAP * zoom)
        };
    label
        .ranges
        .iter()
        .enumerate()
        .filter_map(|(index, local)| {
            let range = header.projection_start().checked_add(local.start)?
                ..header.projection_start().checked_add(local.end)?;
            let text = editor.projection.text().get(range.clone())?;
            let mut runs = styled_projection_runs(
                &editor.projection,
                &range,
                text.len(),
                &window.text_style(),
                false,
                palette,
            );
            for run in &mut runs {
                run.color = rgb(palette.secondary).into();
            }
            let font_size = DocumentStyle::TABLE_SIZE * zoom;
            let key = shape_cache_key(ShapeCacheInput {
                snapshot: &snapshot,
                segment: Some(header),
                range: &range,
                runs: &runs,
                palette,
                font_size,
                width: label.width,
                scale: window.scale_factor(),
                marked: None,
            });
            let cached = editor.shaped_line_cache.borrow_mut().get(&key);
            let layout = cached.unwrap_or_else(|| {
                let shaped = window.text_system().shape_line(
                    text.to_owned().into(),
                    px(font_size),
                    &runs,
                    None,
                );
                editor
                    .shaped_line_cache
                    .borrow_mut()
                    .insert(key, shaped.clone());
                shaped
            });
            Some(PaintedLine {
                range: spec.projected_start()..spec.projected_start(),
                layout,
                bounds: Bounds::new(
                    point(
                        x,
                        y + px(index as f32 * DocumentStyle::TABLE_LEADING * zoom),
                    ),
                    size(px(label.width), px(DocumentStyle::TABLE_LEADING * zoom)),
                ),
                line_height: px(DocumentStyle::TABLE_LEADING * zoom),
                horizontal_owner: None,
                content_mask: Some(window.content_mask()),
                alignment: ColumnAlignment::Left,
            })
        })
        .collect()
}

/// Headers and every row are checked once during native table measurement.
/// No source scanning or semantic guessing is performed while scrolling.
pub(super) fn headers(
    projection: &TextProjection,
    table: &document_core::Table,
    fonts: &FontMeasurement,
) -> Option<Vec<f32>> {
    if !(crate::projection::property_header_labels(table)
        || crate::projection::entity_header_labels(table))
        || table.row_count() < 2
        || table.border != document_core::TableBorder::LogicalPixel
        || table.columns.iter().any(|c| c.width.is_some())
    {
        return None;
    }
    let mut widths = vec![0.; table.column_count()];
    let mut keys = HashSet::new();
    for (row, record) in table.rows.iter().enumerate() {
        if record.cells.len() != widths.len() {
            return None;
        }
        for (column, cell) in record.cells.iter().enumerate() {
            if cell.blocks.len() != 1 {
                return None;
            }
            let BlockNode::Paragraph(paragraph) = cell.blocks.get(0)?.as_ref() else {
                return None;
            };
            let segment = projection.segment_for_node(paragraph.id)?;
            if segment.top_level_node_id != table.id {
                return None;
            }
            let text = &projection.text()[segment.projection_range()];
            if text.len() > 4096
                || inline_math::has_math(projection, paragraph.id)
                || measurement::contains_strong_rtl(text)
            {
                return None;
            }
            if row == 0 {
                widths[column] = fonts
                    .line_width(
                        projection,
                        segment.projection_range(),
                        DocumentStyle::TABLE_SIZE,
                    )?
                    .ceil()
                    + 24.;
            } else if column == 0 && (text.trim().is_empty() || !keys.insert(text.trim())) {
                return None;
            }
        }
    }
    Some(widths)
}

pub(super) fn is_cell(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    width: f32,
) -> bool {
    segment
        .context
        .table_cell
        .is_some_and(|(id, row, _)| row > 0 && projection.uses_record_layout(id, width))
}

/// Keep the original row/cell topology, but place each label above its value.
/// Geometry stores distinct cell bounds for selection and table commands.
pub(super) fn position(lines: &mut [VisualLineSpec], region: FlowRegion, y: f32) -> f32 {
    let mut cursor = y;
    let mut start = 0;
    while start < lines.len() {
        let column = lines[start].table_cell.unwrap().2;
        let end = lines[start..]
            .iter()
            .position(|l| l.table_cell.unwrap().2 != column)
            .map_or(lines.len(), |n| start + n);
        let cell_top = cursor;
        let entities = lines[start].table_cell.unwrap().3 > 2;
        let label = lines[start].record_label.clone();
        let extra = label
            .as_ref()
            .filter(|label| !label.inline)
            .map_or(0., |label| label.height + LABEL_GAP);
        let body_inset = label
            .as_ref()
            .filter(|label| label.inline)
            .map_or(0., |label| label.width + 16.);
        let last_column = column + 1 == lines[start].table_cell.unwrap().3;
        for (index, line) in lines[start..end].iter_mut().enumerate() {
            line.style.space_above = if index == 0 {
                if column == 0 { INSET } else { extra }
            } else {
                0.
            };
            line.style.space_below = if start + index + 1 == end {
                if last_column {
                    INSET
                } else if entities {
                    16.
                } else {
                    LABEL_GAP
                }
            } else {
                0.
            };
            cursor += line.style.space_above;
            line.y = cursor;
            line.x_fraction = region.left / region.document;
            line.width_fraction = region.width / region.document;
            line.inset = INSET + body_inset;
            line.table_cell_first = index == 0;
            cursor += line.style.line_height + line.style.space_below;
        }
        if let Some(label) = &label {
            let below = lines[end - 1].style.space_below;
            let extra_height = (cell_top + label.height + below - cursor).max(0.);
            lines[end - 1].style.space_below += extra_height;
            cursor += extra_height;
        }
        for line in &mut lines[start..end] {
            line.table_row_y = cell_top;
            line.table_row_height = cursor - cell_top;
        }
        start = end;
    }
    for line in lines {
        line.table_record = Some(Geometry {
            top: y,
            height: cursor - y,
        });
    }
    cursor
}

#[cfg(test)]
mod tests {
    use super::*;
    const SOURCE: &str =
        include_str!("../../../../performance/layout-fixtures/75-property-records.md");
    const ENTITIES: &str =
        include_str!("../../../../performance/layout-fixtures/76-entity-records.md");

    #[gpui::test]
    fn overflowing_entity_directories_pair_complete_records(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/122-paired-records.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let table = projection
                .roots()
                .find(|root| matches!(root, BlockNode::Table(_)))
                .unwrap()
                .id();
            let width = 1200.;
            assert!(
                projection.uses_record_layout(table, width),
                "overflowing independent directories need a readable record composition"
            );
            let plan = build_measured_adaptive_plan(&projection, width, 1800., None, false, &fonts);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                width,
                &plan,
                Some(&fonts),
            );
            let records = (1..=3)
                .map(|row| {
                    lines
                        .iter()
                        .find(|line| {
                            line.table_cell
                                .is_some_and(|(id, r, c, _)| id == table && r == row && c == 0)
                        })
                        .unwrap()
                })
                .collect::<Vec<_>>();
            assert_eq!(records[0].y, records[1].y);
            assert!(records[1].x_fraction > records[0].x_fraction);
            assert_eq!(
                records[0].width_fraction, records[2].width_fraction,
                "an unmatched final record retains the same width"
            );
            assert!(
                records[2].table_record.unwrap().top
                    >= records[..2]
                        .iter()
                        .map(|line| {
                            let record = line.table_record.unwrap();
                            record.top + record.height
                        })
                        .fold(0., f32::max)
                        + RECORD_GAP
            );
            assert_eq!(
                lines
                    .iter()
                    .map(|line| &projection.text()[line.projected_range()])
                    .collect::<String>(),
                projection
                    .segments()
                    .iter()
                    .map(|s| &projection.text()[s.projection_range()])
                    .collect::<String>()
            );
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn paired_records_stack_when_their_measured_content_cannot_fit(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/122-paired-records.md");
            for zoom in [1., 1.5, 2.] {
                let document = Document::from_markdown(source).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    zoom,
                );
                fonts.measure_tables(&mut projection);
                let tables = projection
                    .roots()
                    .filter(|root| matches!(root, BlockNode::Table(_)))
                    .map(BlockNode::id)
                    .collect::<Vec<_>>();
                let table = tables[0];
                for (width, columns) in [(1200., Some(2)), (800., Some(1)), (240., None)] {
                    assert_eq!(
                        projection
                            .record_layout(table, width)
                            .map(|layout| layout.columns),
                        columns
                    );
                    assert!(
                        !projection.uses_record_layout(tables[1], width),
                        "numeric comparisons keep their columns"
                    );
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1800.,
                        None,
                        false,
                        &fonts,
                    );
                    let lines = build_measured_visual_lines(
                        &projection,
                        &HashMap::new(),
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    if columns.is_some() {
                        for line in &lines {
                            if line.table_record.is_some() {
                                assert!(
                                    line.x_fraction >= 0.
                                        && line.x_fraction + line.width_fraction <= 1.001
                                );
                                let measured = fonts
                                    .line_width(
                                        &projection,
                                        line.projected_range(),
                                        line.style.font_size,
                                    )
                                    .unwrap();
                                assert!(
                                    measured + line.inset + INSET
                                        <= line.width_fraction * width + 0.5,
                                    "complete record glyphs fit their own column"
                                );
                            }
                        }
                        let schema = projection.fitted_table_row_widths(table, 0, width).unwrap();
                        assert!(
                            schema.iter().sum::<f32>() <= width,
                            "the retained schema must not use overflowing value widths"
                        );
                    }
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn paired_record_growth_repositions_the_complete_band_and_undo_is_exact(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/122-paired-records.md");
            let mut document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            fonts.measure_tables(&mut projection);
            let segment = projection
                .segments()
                .iter()
                .find(|s| {
                    projection.text()[s.projection_range()].starts_with("Collects unresolved")
                })
                .unwrap();
            let node = segment.node_id;
            let length = segment.projection_len();
            let table = segment.context.table_cell.unwrap().0;
            projection.lock_table_for_node(Some(node));
            let plan = build_measured_adaptive_plan(&projection, 1200., 1800., None, false, &fonts);
            let before = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1200.,
                &plan,
                Some(&fonts),
            );
            let record_top = |lines: &[VisualLineSpec], row| {
                lines
                    .iter()
                    .find(|line| {
                        line.table_cell
                            .is_some_and(|(id, r, c, _)| id == table && r == row && c == 0)
                    })
                    .unwrap()
                    .table_record
                    .unwrap()
                    .top
            };
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: length..length,
                    text: " Retained context remains with this record.".repeat(24),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            let mut updated = TextProjection::from_snapshot(&document.snapshot());
            updated.retain_table_layout_lock(&projection, Some(node));
            fonts.measure_tables(&mut updated);
            let locked = arrangement::build_edit_locked_adaptive_plan(
                &updated,
                1200.,
                1800.,
                Some(&plan),
                false,
                &fonts,
                Some(node),
            );
            let lines = build_measured_visual_lines(
                &updated,
                &HashMap::new(),
                1200.,
                &locked,
                Some(&fonts),
            );
            assert_eq!(
                updated.record_layout(table, 1200.),
                projection.record_layout(table, 1200.)
            );
            assert_eq!(record_top(&lines, 1), record_top(&before, 1));
            assert_eq!(record_top(&lines, 2), record_top(&lines, 1));
            assert!(record_top(&lines, 3) > record_top(&before, 3));
            let grown = lines
                .iter()
                .find(|line| {
                    line.table_cell
                        .is_some_and(|(id, r, _, _)| id == table && r == 2)
                })
                .unwrap()
                .table_record
                .unwrap();
            assert!(record_top(&lines, 3) >= grown.top + grown.height + RECORD_GAP);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn entity_record_labels_are_source_owned_and_use_measured_rails_or_stacks(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let compact = "| Name | Description | Owner |\n| --- | --- | --- |\n| Atlas | A sustained explanation describes this independent service and the context in which it should be used. | Research |\n";
            for (source, width, expected_inline) in [(ENTITIES, 500., true), (compact, 310., false)] {
                let document = Document::from_markdown(source).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut projection);
                let plan = build_measured_adaptive_plan(&projection, width, 1200., None, false, &fonts);
                let lines = build_measured_visual_lines(&projection, &HashMap::new(), width, &plan, Some(&fonts));
                let mut labels = 0;
                for line in &lines {
                    let segment = projection.segment_for_range(&line.projected_range()).unwrap();
                    if let Some(label) = &line.record_label {
                        labels += 1;
                        assert_eq!(label.inline, expected_inline);
                        let header = projection.segment_for_node(label.header).unwrap();
                        assert!(header.context.table_header);
                        assert_eq!(header.context.table_cell.unwrap().2, segment.context.table_cell.unwrap().2);
                        let text = &projection.text()[header.projection_range()];
                        assert_eq!(label.ranges.iter().map(|r| &text[r.clone()]).collect::<String>(), text);
                        for range in &label.ranges {
                            assert!(fonts.line_width(&projection, header.projection_start()+range.start..header.projection_start()+range.end, DocumentStyle::TABLE_SIZE).unwrap() <= label.width+0.5);
                        }
                        if label.inline { assert_eq!(line.inset, INSET + label.width + 16.); }
                        else { assert_eq!(line.inset, INSET); assert_eq!(line.style.space_above, label.height + LABEL_GAP); }
                    }
                    if line.table_record.is_some() {
                        let measured = fonts.line_width(&projection, line.projected_range(), line.style.font_size).unwrap();
                        assert!(measured <= width - line.inset - INSET + 0.5, "record text must fit its actual inset");
                        if segment.context.table_cell.unwrap().2 == 0 {
                            assert_eq!(line.style.font_size, TITLE_SIZE);
                            assert_eq!(line.style.line_height, TITLE_LEADING);
                        }
                    }
                }
                assert!(labels >= 2);
                for segment in projection.segments() {
                    assert_eq!(lines.iter().filter(|l| projection.segment_for_range(&l.projected_range()).unwrap().node_id == segment.node_id)
                        .map(|l| &projection.text()[l.projected_range()]).collect::<String>(), projection.text()[segment.projection_range()]);
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn editing_entity_header_republishes_all_labels_without_stale_cached_ranges(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let mut document = Document::from_markdown(ENTITIES).unwrap();
            let mut previous = TextProjection::from_snapshot(&document.snapshot());
            fonts.measure_tables(&mut previous);
            let header = previous
                .segments()
                .iter()
                .find(|s| &previous.text()[s.projection_range()] == "Purpose")
                .unwrap()
                .node_id;
            let table = previous
                .segment_for_node(header)
                .unwrap()
                .context
                .table_cell
                .unwrap()
                .0;
            previous.lock_table_for_node(Some(header));
            let plan = build_measured_adaptive_plan(&previous, 420., 1200., None, false, &fonts);
            let before =
                build_measured_visual_lines(&previous, &HashMap::new(), 420., &plan, Some(&fonts));
            let old_count = before
                .iter()
                .filter(|line| {
                    line.record_label
                        .as_ref()
                        .is_some_and(|label| label.header == header)
                })
                .count();
            assert_eq!(old_count, 4);
            let changed = "Purpose and operational responsibility";
            document
                .apply(EditCommand::ReplaceText {
                    node_id: header,
                    range: 0..7,
                    text: changed.into(),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            assert_eq!(
                document.snapshot().serialize().unwrap(),
                ENTITIES.replace("| Purpose |", "| Purpose and operational responsibility |")
            );
            assert!(
                previous
                    .refresh_text_node(&document.snapshot(), header)
                    .is_none()
            );
            let mut updated = TextProjection::from_snapshot(&document.snapshot());
            updated.retain_table_layout_lock(&previous, Some(header));
            fonts.measure_tables(&mut updated);
            assert!(updated.uses_record_layout(table, 420.));
            assert!(
                updated
                    .segments()
                    .iter()
                    .filter(|s| s
                        .context
                        .table_cell
                        .is_some_and(|(id, row, column)| id == table && row > 0 && column == 0))
                    .all(|s| s.context.table_property_key),
                "focused header edits retain entity-name typography"
            );
            let after =
                build_measured_visual_lines(&updated, &HashMap::new(), 420., &plan, Some(&fonts));
            let repeated = after
                .iter()
                .filter_map(|line| line.record_label.as_ref())
                .filter(|label| label.header == header)
                .collect::<Vec<_>>();
            assert_eq!(repeated.len(), old_count);
            for label in repeated {
                assert_eq!(
                    label
                        .ranges
                        .iter()
                        .map(|r| &changed[r.clone()])
                        .collect::<String>(),
                    changed
                );
                assert!(label.height > DocumentStyle::TABLE_LEADING);
            }
            // Exercise the worker publication path as well as the immediate
            // projection refresh. Native idle reflow must retain the same type.
            let (published, _, _) = PreparedDocumentView::prepare_snapshot_with_images(
                &document.snapshot(),
                &HashMap::new(),
                None,
                ReflowViewport {
                    published_geometry: None,
                    width: 420.,
                    height: 1200.,
                    zoom: 1.,
                    preview_edit_node: None,
                    expanded_code_tail: None,
                    editing_node: Some(header),
                    table_layout_lock: updated.table_layout_lock.clone(),
                    html_disclosures: Arc::default(),
                    html_loaded_images: Arc::default(),
                    trace_mode: LayoutTraceMode::Off,
                    visible_roots: None,
                    resource_generation: 0,
                },
                &plan,
                &fonts,
            );
            let names = published
                .projection
                .segments()
                .iter()
                .filter(|s| {
                    s.context
                        .table_cell
                        .is_some_and(|(id, row, column)| id == table && row > 0 && column == 0)
                })
                .collect::<Vec<_>>();
            assert_eq!(names.len(), 4);
            assert!(
                names.iter().all(|s| s.context.table_property_key),
                "background publication must retain entity-name typography after header editing"
            );
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), ENTITIES);
        });
    }

    #[gpui::test]
    fn independent_entity_rows_become_records_without_transposing_comparisons(
        cx: &mut gpui::TestAppContext,
    ) {
        let source = include_str!("../../../../performance/layout-fixtures/76-entity-records.md");
        cx.update(|cx| {
            let document = Document::from_markdown(source).unwrap();
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            fonts.measure_tables(&mut projection);
            let tables = projection
                .roots()
                .filter_map(|b| match b {
                    BlockNode::Table(t) => Some(t.id),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(tables.len(), 3);
            // A 600px window used to have 544px of content. Reduced outer
            // margins now provide 576px; test the content boundary itself.
            for width in [420., 526., 544., 559., 560., 576., 760., 1280.] {
                assert_eq!(
                    projection.uses_record_layout(tables[0], width),
                    width < 560.,
                    "independent entities should receive complete labeled records when cramped"
                );
                assert!(
                    !projection.uses_record_layout(tables[1], width),
                    "numeric comparisons keep their columns"
                );
                assert!(
                    !projection.uses_record_layout(tables[2], width),
                    "already compact rows remain tables"
                );
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn property_records_stack_only_when_measured_columns_are_cramped(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let document = Document::from_markdown(SOURCE).unwrap();
            for width in [280., 420., 760., 1280.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Public Sans Tachyon".into(),
                    1.,
                );
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut projection);
                let plan =
                    build_measured_adaptive_plan(&projection, width, 1000., None, false, &fonts);
                let lines = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                let key = projection
                    .segments()
                    .iter()
                    .find(|s| &projection.text()[s.projection_range()] == "Evidence retention")
                    .unwrap();
                let value = projection
                    .segments()
                    .iter()
                    .find(|s| {
                        projection.text()[s.projection_range()]
                            .starts_with("Keep original messages")
                    })
                    .unwrap();
                let key_line = lines
                    .iter()
                    .find(|l| l.projected_start() == key.projection_start())
                    .unwrap();
                let value_line = lines
                    .iter()
                    .find(|l| l.projected_start() == value.projection_start())
                    .unwrap();
                if width < 560. {
                    assert_eq!(
                        key_line.x_fraction, value_line.x_fraction,
                        "property labels and values must stack on the same leading edge"
                    );
                    assert!(value_line.y > key_line.y);
                    assert!(
                        (value_line.y - key_line.y - key_line.style.line_height - LABEL_GAP).abs()
                            < 0.01
                    );
                    assert_eq!(key_line.inset, INSET);
                    assert_eq!(value_line.inset, INSET);
                    let bounds = key_line.table_record.unwrap();
                    assert_eq!(key_line.y - bounds.top, INSET);
                    let last = lines
                        .iter()
                        .rfind(|l| {
                            projection
                                .segment_for_range(&l.projected_range())
                                .unwrap()
                                .node_id
                                == value.node_id
                        })
                        .unwrap();
                    assert!(
                        (bounds.top + bounds.height - last.y - last.style.line_height - INSET)
                            .abs()
                            < 0.01
                    );
                    for line in lines.iter().filter(|l| l.table_record.is_some()) {
                        assert!(
                            fonts
                                .line_width(
                                    &projection,
                                    line.projected_range(),
                                    line.style.font_size
                                )
                                .unwrap()
                                <= width - 2. * INSET + 0.5
                        );
                    }
                } else {
                    assert_eq!(key_line.y, value_line.y);
                    assert!(value_line.x_fraction > key_line.x_fraction);
                }
                let tables = projection
                    .roots()
                    .filter_map(|r| {
                        if let BlockNode::Table(t) = r {
                            Some(t.id)
                        } else {
                            None
                        }
                    })
                    .collect::<Vec<_>>();
                assert_eq!(tables.len(), 3);
                for id in &tables[1..] {
                    assert!(
                        !projection.uses_record_layout(*id, width),
                        "comparison and already compact tables must not become records"
                    );
                    assert!(
                        lines
                            .iter()
                            .filter(|l| l.table_cell.is_some_and(|(t, _, _, _)| t == *id))
                            .all(|l| l.table_record.is_none())
                    );
                }
                for segment in projection.segments() {
                    let own = lines
                        .iter()
                        .filter(|l| {
                            projection
                                .segment_for_range(&l.projected_range())
                                .unwrap()
                                .node_id
                                == segment.node_id
                        })
                        .collect::<Vec<_>>();
                    assert_eq!(
                        own.iter()
                            .map(|l| &projection.text()[l.projected_range()])
                            .collect::<String>(),
                        projection.text()[segment.projection_range()]
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }

    #[gpui::test]
    fn property_record_edits_preserve_the_frozen_mode_and_row_geometry(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let mut document = Document::from_markdown(SOURCE).unwrap();
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            fonts.measure_tables(&mut projection);
            let plan = build_measured_adaptive_plan(&projection, 420., 900., None, false, &fonts);
            let mut lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                420.,
                &plan,
                Some(&fonts),
            );
            let mut order = visual_line_paint_order(&lines);
            let segment = projection
                .segments()
                .iter()
                .find(|s| {
                    projection.text()[s.projection_range()].starts_with("Keep original messages")
                })
                .unwrap();
            let node = segment.node_id;
            let table = segment.context.table_cell.unwrap().0;
            document
                .apply(EditCommand::ReplaceText {
                    node_id: node,
                    range: 0..0,
                    text: "Carefully ".into(),
                    typing: true,
                    selection_after: None,
                })
                .unwrap();
            assert_eq!(
                document.snapshot().serialize().unwrap(),
                SOURCE.replacen(
                    "Keep original messages",
                    "Carefully Keep original messages",
                    1
                )
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
                    layout_width: 420.,
                    zoom_factor: 1.,
                    measurement: Some(&fonts),
                },
            )
            .expect("ordinary record value typing must use the row-local path");
            assert!(projection.uses_record_layout(table, 420.));
            let full = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                420.,
                &plan,
                Some(&fonts),
            );
            assert_eq!(lines.len(), full.len());
            for (local, rebuilt) in lines.iter().zip(&full) {
                assert_eq!(local.projected_range(), rebuilt.projected_range());
                assert_eq!(local.table_record, rebuilt.table_record);
                assert!((local.y - rebuilt.y).abs() < 0.01);
                assert_eq!(local.table_row_y, rebuilt.table_row_y);
                assert_eq!(local.table_row_height, rebuilt.table_row_height);
            }
            let header = projection
                .segments()
                .iter()
                .find(|s| s.context.table_cell == Some((table, 0, 0)))
                .unwrap()
                .node_id;
            projection.lock_table_for_node(Some(header));
            document
                .apply(EditCommand::ReplaceText {
                    node_id: header,
                    range: 0..8,
                    text: "Other".into(),
                    typing: false,
                    selection_after: None,
                })
                .unwrap();
            let mut updated = TextProjection::from_snapshot(&document.snapshot());
            updated.retain_table_layout_lock(&projection, Some(header));
            fonts.measure_tables(&mut updated);
            assert!(
                updated.uses_record_layout(table, 420.),
                "editing a header cannot reshuffle its table"
            );
            updated.table_layout_lock = None;
            fonts.measure_tables(&mut updated);
            assert!(
                !updated.uses_record_layout(table, 420.),
                "blur must release obsolete property semantics"
            );
            document.undo().unwrap();
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        });
    }

    #[gpui::test]
    fn property_record_eligibility_rejects_ambiguous_or_unfittable_content(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for source in [
                "| Option | Result |\n| --- | --- |\n| A | A very long descriptive result repeated for comparison across options. |\n| B | Another result. |\n".to_string(),
                "| Property | Description |\n| --- | --- |\n| Duplicate | A long description that would otherwise use the record fallback on a narrow screen. |\n| Duplicate | Another description. |\n".to_string(),
                format!("| Property | Value |\n| --- | --- |\n| Token | {} |\n", "unbreakable".repeat(60)),
                "> | Property | Description |\n> | --- | --- |\n> | Nested | An explicitly quoted property table stays in the quoted table's original layout. |\n".to_string(),
            ] {
                let document = Document::from_markdown(source.as_str()).unwrap();
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                fonts.measure_tables(&mut projection);
                for segment in projection.segments() {
                    if let Some((table, _, _)) = segment.context.table_cell {
                        assert!(!projection.uses_record_layout(table, 280.), "{source}");
                    }
                }
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }
}
