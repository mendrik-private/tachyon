//! A pressure-triggered tree, with explicit source-backed parent context.
use super::*;
use std::borrow::Cow;

pub(super) const HEADER: f32 = 24.;
pub(super) const INSET: f32 = 64.;

pub(super) fn prepare(
    segment: &crate::ProjectionSegment,
    canvas: f32,
) -> Cow<'_, crate::ProjectionSegment> {
    let compact = segment
        .context
        .outline_metrics
        .is_some_and(|(depth, inset)| depth > 3 && canvas - (inset as f32) < 320.);
    if !compact {
        return Cow::Borrowed(segment);
    }
    let mut segment = segment.clone();
    segment.context.compact_outline = true;
    Cow::Owned(segment)
}

pub(super) fn has_header(segment: &crate::ProjectionSegment) -> bool {
    segment.context.compact_outline
        && segment.context.list_depth > 2
        && segment.context.list_branch_start
        && segment.context.list_marker.is_some()
        && segment.context.list_parent_label.is_some()
}

fn parent_excerpt(source: &str) -> String {
    // Bound bytes as well as graphemes: a single adversarial combining cluster
    // can otherwise make a seemingly bounded caption scan an entire paragraph.
    let mut end = source.len().min(1024);
    while !source.is_char_boundary(end) {
        end -= 1;
    }
    let mut result = source[..end]
        .grapheme_indices(true)
        .take(96)
        .take_while(|(start, part)| end == source.len() || start + part.len() < end)
        .map(|(_, part)| part)
        .collect::<String>();
    if result.len() < source.len() {
        result.push('…');
    }
    result.replace(['\n', '\r'], " ")
}

fn header_bounds(line: &VisualLineSpec, code: bool, width: f32, zoom: f32) -> Bounds<Pixels> {
    let code = code && !line.rendered_code_preview();
    let inset = line.inset - if code { CODE_BLOCK_PADDING * zoom } else { 0. };
    let chrome = if code {
        CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING + line.preview_extent()
    } else {
        0.
    };
    Bounds::new(
        point(
            px(line.x_fraction * width + inset),
            px(line.y - (HEADER + chrome) * zoom),
        ),
        size(
            px((line.width_fraction * width - inset - 8. * zoom).max(1.)),
            px(HEADER * zoom),
        ),
    )
}

pub(super) fn leading_container_bounds(
    segment: &crate::ProjectionSegment,
    line: &VisualLineSpec,
    projection: &TextProjection,
    components: &ComponentIndex,
    width: f32,
    zoom: f32,
) -> Option<Bounds<Pixels>> {
    let owner = segment.context.list_item_container?;
    if segment.context.list_marker.is_none() || line.projected_start() != segment.projection_start()
    {
        return None;
    }
    let component = components.get(&owner)?;
    match projection.block(owner)? {
        BlockNode::BlockQuote { .. } => quotes::panel(
            component,
            Bounds::new(point(px(0.), px(0.)), size(px(width), px(0.))),
            zoom,
            false,
        ),
        BlockNode::Alert { .. } => Some(Bounds::from_corners(
            point(
                px(component.left_fraction * width - ALERT_CONTENT_INSET * zoom),
                px(component.top - ALERT_HEADER_HEIGHT * zoom),
            ),
            point(
                px(component.right_fraction * width - 8. * zoom),
                px(component.bottom + ALERT_BOTTOM_PADDING * zoom),
            ),
        )),
        BlockNode::Table(_) => {
            let (left, available) = table_viewport_geometry(line, projection, width, zoom);
            Some(Bounds::new(
                point(px(left), px(line.table_row_y)),
                size(px(available), px(component.bottom - line.table_row_y)),
            ))
        }
        _ => None,
    }
}

impl RichDocumentEditor {
    pub(super) fn render_tree_context(
        &self,
        visible: &[usize],
        palette: MineralPalette,
    ) -> Vec<AnyElement> {
        visible
            .iter()
            .filter_map(|index| {
                let line = &self.visual_lines[*index];
                let segment = self.projection.segment_for_range(&line.projected_range())?;
                if !line.compact_tree
                    || segment.context.list_depth <= 2
                    || !segment.context.list_branch_start
                    || segment.context.list_marker.is_none()
                    || line.projected_start() != segment.projection_start()
                {
                    return None;
                }
                let parent = self
                    .projection
                    .segment_for_node(segment.context.list_parent_label?)?;
                // Read only a bounded prefix of the already-projected parent. The
                // complete canonical label remains in the document/accessibility tree.
                let source = &self.projection.text()[parent.projection_range()];
                let excerpt = parent_excerpt(source);
                let excerpt = if excerpt.trim().is_empty() {
                    match self.projection.block(parent.node_id) {
                        Some(BlockNode::CodeBlock(_)) => "code block",
                        Some(BlockNode::Image(_)) => "image",
                        Some(BlockNode::Heading(_)) => "heading",
                        _ => "an untitled item",
                    }
                } else {
                    &excerpt
                };
                let label = format!("Level {} · Within {}", segment.context.list_depth, excerpt);
                let bounds = leading_container_bounds(
                    segment,
                    line,
                    &self.projection,
                    &self.components,
                    self.layout_width,
                    self.zoom_factor,
                )
                .map(|bounds| {
                    Bounds::new(
                        point(bounds.left(), bounds.top() - px(HEADER * self.zoom_factor)),
                        size(bounds.size.width, px(HEADER * self.zoom_factor)),
                    )
                })
                .unwrap_or_else(|| {
                    header_bounds(
                        line,
                        matches!(
                            self.projection.block(segment.node_id),
                            Some(BlockNode::CodeBlock(_))
                        ),
                        self.layout_width,
                        self.zoom_factor,
                    )
                });
                Some(
                    div()
                        .absolute()
                        .left(bounds.left())
                        .top(bounds.top())
                        .w(bounds.size.width)
                        .h(bounds.size.height)
                        .text_size(px(DocumentStyle::CAPTION_SIZE * self.zoom_factor))
                        .line_height(px(DocumentStyle::CAPTION_LEADING * self.zoom_factor))
                        .text_color(rgb(palette.secondary))
                        .text_ellipsis()
                        .child(label)
                        .into_any_element(),
                )
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn unlabeled_branches_keep_rows_and_readable_width(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/109-unlabeled-tree.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Mineral".into(),
                    zoom,
                );
                for width in [360., 620., 1600.] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1000.,
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
                    let mut blank_rows = 0;
                    for segment in projection
                        .segments()
                        .iter()
                        .filter(|s| s.context.list_depth > 2)
                    {
                        let line = lines
                            .iter()
                            .find(|l| l.projected_start() == segment.projection_start())
                            .unwrap();
                        assert_eq!(
                            line.compact_tree,
                            width == 360.,
                            "blank parents must not disable width recovery"
                        );
                        assert_eq!(segment.context.list_item_label, Some(segment.node_id));
                        if segment.projection_range().is_empty() {
                            blank_rows += 1;
                            assert!(segment.context.list_marker.is_some());
                            assert!(line.style.line_height >= DocumentStyle::REFERENCE_LEADING);
                        }
                        let prepared = prepare(segment, width);
                        let available =
                            segment_text_width(&prepared, &projection, line.width_fraction * width);
                        if width == 360. {
                            assert!((available - (width - INSET - 8.)).abs() < 0.01);
                        }
                        for line in lines.iter().filter(|l| {
                            projection
                                .segment_for_range(&l.projected_range())
                                .unwrap()
                                .node_id
                                == segment.node_id
                        }) {
                            assert!(
                                fonts
                                    .line_width(
                                        &projection,
                                        line.projected_range(),
                                        line.style.font_size
                                    )
                                    .unwrap()
                                    <= available + 0.5
                            );
                        }
                    }
                    assert_eq!(
                        blank_rows, 3,
                        "two blank parents and an empty sibling must each have a row"
                    );
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn container_led_branches_use_readable_tree_geometry(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/96-container-first-tree.md");
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Mineral".into(),
                    zoom,
                );
                fonts.measure_tables(&mut projection);
                for width in [360., 620., 1600.] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1000.,
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
                    let mut scaled = lines.clone();
                    scale_visual_lines(&mut scaled, zoom);
                    let components = component_geometry(
                        &projection,
                        &scaled,
                        width * zoom,
                        zoom,
                        &visual_line_paint_order(&scaled),
                    );
                    for segment in projection
                        .segments()
                        .iter()
                        .filter(|s| s.context.list_depth >= 7)
                    {
                        let line = lines
                            .iter()
                            .find(|l| l.projected_start() == segment.projection_start())
                            .unwrap();
                        assert_eq!(
                            line.compact_tree,
                            width == 360.,
                            "container-first item must not disable its tree"
                        );
                        let scaled_line = scaled
                            .iter()
                            .find(|l| l.projected_start() == segment.projection_start())
                            .unwrap();
                        if let Some(panel) = leading_container_bounds(
                            segment,
                            scaled_line,
                            &projection,
                            &components,
                            width * zoom,
                            zoom,
                        ) {
                            assert_eq!(segment.context.list_depth, 7);
                            assert!(panel.left() >= px(0.) && panel.right() <= px(width * zoom));
                            assert!(panel.top() <= px(scaled_line.y));
                            if width == 360. {
                                // Caption is outside component chrome, with its
                                // own reserved height above the first row/body.
                                let previous = lines
                                    .iter()
                                    .rev()
                                    .find(|l| l.projected_end() <= segment.projection_start())
                                    .unwrap();
                                assert!(
                                    panel.top() - px(HEADER * zoom)
                                        >= px((previous.y + previous.style.line_height) * zoom)
                                );
                            }
                        }
                        if segment.context.table_cell.is_some() {
                            assert!(
                                (line.y - line.table_row_y - 10.).abs() < 0.01,
                                "tree caption must not inflate the first table cell"
                            );
                            assert!(
                                (line.table_row_height - line.style.line_height - 20.).abs() < 0.01
                            );
                        }
                        if segment.context.list_depth == 8 {
                            let parent = projection
                                .segment_for_node(segment.context.list_parent_label.unwrap())
                                .unwrap();
                            assert!(projection.text()[parent.projection_range()].starts_with(
                                match &projection.text()[segment.projection_range()] {
                                    text if text.starts_with("Quoted") => "The observer",
                                    text if text.starts_with("Note-led") => "Keep the original",
                                    _ => "Reading",
                                }
                            ));
                            let prepared = prepare(segment, width);
                            let available = segment_text_width(
                                &prepared,
                                &projection,
                                line.width_fraction * width,
                            );
                            if width == 360. {
                                // 64px hierarchy rail plus the ordinary 8px
                                // trailing inset, not another level of padding.
                                assert!((available - (width - INSET - 8.)).abs() < 0.01);
                            }
                            for line in lines.iter().filter(|l| {
                                projection
                                    .segment_for_range(&l.projected_range())
                                    .unwrap()
                                    .node_id
                                    == segment.node_id
                            }) {
                                assert!(
                                    fonts
                                        .line_width(
                                            &projection,
                                            line.projected_range(),
                                            line.style.font_size
                                        )
                                        .unwrap()
                                        <= available + 0.5
                                );
                            }
                        }
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn source_led_branches_keep_their_authored_start(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/95-code-first-tree.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Mineral".into(),
                    zoom,
                );
                let images = projection
                    .segments()
                    .iter()
                    .filter_map(|s| {
                        s.context
                            .image_source
                            .as_ref()
                            .map(|source| (s.node_id, (source.clone(), (960, 360))))
                    })
                    .collect();
                for width in [360., 620., 1600.] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1000.,
                        None,
                        false,
                        &fonts,
                    );
                    let lines = build_measured_visual_lines(
                        &projection,
                        &images,
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    let mut scaled = lines.clone();
                    scale_visual_lines(&mut scaled, zoom);
                    let canvas =
                        Bounds::new(point(px(20.), px(30.)), size(px(width * zoom), px(10000.)));
                    for segment in projection
                        .segments()
                        .iter()
                        .filter(|s| s.context.list_depth > 2)
                    {
                        let line = lines
                            .iter()
                            .find(|l| l.projected_start() == segment.projection_start())
                            .unwrap();
                        assert_eq!(
                            line.compact_tree,
                            width == 360.,
                            "non-paragraph starts must not disable the whole tree"
                        );
                        assert_eq!(segment.context.list_item_label, Some(segment.node_id));
                        let parent = projection
                            .segment_for_node(segment.context.list_parent_label.unwrap())
                            .unwrap();
                        assert!(!projection.text()[parent.projection_range()].is_empty());
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
                                .collect::<String>()
                                .replace('\n', ""),
                            projection.text()[segment.projection_range()].replace('\n', "")
                        );
                        let code = matches!(
                            projection.block(segment.node_id),
                            Some(BlockNode::CodeBlock(_))
                        );
                        let scaled_line = scaled
                            .iter()
                            .find(|l| l.projected_start() == line.projected_start())
                            .unwrap();
                        if code {
                            let caption = header_bounds(scaled_line, true, width * zoom, zoom);
                            let pane_top =
                                (line.y - CODE_HEADER_HEIGHT - CODE_BLOCK_PADDING) * zoom;
                            assert!((f32::from(caption.bottom()) - pane_top).abs() < 0.01);
                            let marker_line = code_item_marker_line(scaled_line, canvas, zoom);
                            let marker = list_marker_bounds(marker_line, false, false, None, zoom);
                            assert!(
                                marker.right() < canvas.left() + caption.left(),
                                "code marker must be outside the pane"
                            );
                        } else if segment.context.image_source.is_none() {
                            let prepared = prepare(segment, width);
                            let available = segment_text_width(
                                &prepared,
                                &projection,
                                line.width_fraction * width,
                            );
                            for line in own {
                                assert!(
                                    fonts
                                        .line_width(
                                            &projection,
                                            line.projected_range(),
                                            line.style.font_size
                                        )
                                        .unwrap()
                                        <= available + 0.5
                                );
                            }
                        }
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn enclosed_trees_keep_readable_branches_and_source_context(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source =
                include_str!("../../../../performance/layout-fixtures/94-enclosed-trees.md");
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            for zoom in [1., 1.5, 2.] {
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Mineral".into(),
                    zoom,
                );
                for width in [360., 520., 570., 620., 1600.] {
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1000.,
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
                    let mut scaled = lines.clone();
                    scale_visual_lines(&mut scaled, zoom);
                    let components = component_geometry(
                        &projection,
                        &scaled,
                        width * zoom,
                        zoom,
                        &visual_line_paint_order(&scaled),
                    );
                    let canvas = Bounds::new(
                        point(px(0.), px(0.)),
                        size(px(width * zoom), px(10000. * zoom)),
                    );
                    for segment in projection
                        .segments()
                        .iter()
                        .filter(|s| s.context.list_depth == 8)
                    {
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
                        let line = own[0];
                        assert_eq!(
                            line.compact_tree,
                            width
                                < if segment.context.alert.is_some() {
                                    584.
                                } else {
                                    560.
                                },
                            "outer quote/alert must not exclude compact layout"
                        );
                        let adjusted = prepare(segment, width);
                        let available =
                            segment_text_width(&adjusted, &projection, line.width_fraction * width);
                        if width == 360. {
                            assert!(
                                available >= 224.,
                                "enclosure must retain usable prose: {available}"
                            );
                        }
                        for line in &own {
                            let actual = fonts
                                .line_width(
                                    &projection,
                                    line.projected_range(),
                                    line.style.font_size,
                                )
                                .unwrap();
                            assert!(
                                actual <= available + 0.5,
                                "native line must fit its enclosed measure: {actual} > {available}"
                            );
                        }
                        assert_eq!(
                            own.iter()
                                .map(|l| &projection.text()[l.projected_range()])
                                .collect::<String>(),
                            projection.text()[segment.projection_range()]
                        );
                        assert!(segment.context.list_parent_label.is_some());
                        let owner = segment
                            .context
                            .quote
                            .or_else(|| segment.context.alert.as_ref().map(|(id, _)| *id))
                            .unwrap();
                        let component = components.get(&owner).unwrap();
                        let panel = if segment.context.quote.is_some() {
                            quotes::panel(component, canvas, zoom, false).unwrap()
                        } else {
                            Bounds::from_corners(
                                point(
                                    px(component.left_fraction * width * zoom
                                        - ALERT_CONTENT_INSET * zoom),
                                    px(component.top - ALERT_HEADER_HEIGHT * zoom),
                                ),
                                point(
                                    px(component.right_fraction * width * zoom - 8. * zoom),
                                    px(component.bottom + ALERT_BOTTOM_PADDING * zoom),
                                ),
                            )
                        };
                        assert!(panel.left() >= px(0.) && panel.right() <= px(width * zoom));
                        for line in &own {
                            let left = (line.x_fraction * width + line.inset) * zoom;
                            let actual = fonts
                                .line_width(
                                    &projection,
                                    line.projected_range(),
                                    line.style.font_size,
                                )
                                .unwrap()
                                * zoom;
                            assert!(px(left) >= panel.left() && px(left + actual) <= panel.right());
                            assert!(px(line.y * zoom) >= panel.top());
                            assert!(px((line.y + line.style.line_height) * zoom) <= panel.bottom());
                        }
                        if line.compact_tree {
                            assert!(
                                px((line.y - HEADER) * zoom) >= panel.top(),
                                "parent caption must remain inside its source enclosure"
                            );
                        }
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn mixed_branch_components_share_compact_ancestry(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = include_str!("../../../../performance/layout-fixtures/93-mixed-tree.md");
            let document = Document::from_markdown(source).unwrap();
            for zoom in [1., 1.5, 2.] {
                let mut projection = TextProjection::from_snapshot(&document.snapshot());
                let fonts = FontMeasurement::new(
                    cx.text_system().clone(),
                    "Spline Sans Mineral".into(),
                    zoom,
                );
                fonts.measure_tables(&mut projection);
                let images = projection
                    .segments()
                    .iter()
                    .filter_map(|s| {
                        s.context
                            .image_source
                            .as_ref()
                            .map(|source| (s.node_id, (source.clone(), (960, 360))))
                    })
                    .collect();
                for width in [360., 620., 1600.] {
                    let compact = width < 512.;
                    let ancestry = if compact { INSET } else { 192. };
                    let plan = build_measured_adaptive_plan(
                        &projection,
                        width,
                        1000.,
                        None,
                        false,
                        &fonts,
                    );
                    let lines = build_measured_visual_lines(
                        &projection,
                        &images,
                        width,
                        &plan,
                        Some(&fonts),
                    );
                    for segment in projection
                        .segments()
                        .iter()
                        .filter(|s| s.context.list_depth == 8)
                    {
                        let line = lines
                            .iter()
                            .find(|l| l.projected_start() == segment.projection_start())
                            .unwrap();
                        assert_eq!(
                            line.compact_tree,
                            compact,
                            "mixed branch cannot reject compact geometry: {:?}",
                            projection.block(segment.node_id).unwrap()
                        );
                        if segment
                            .context
                            .table_cell
                            .is_some_and(|(_, _, column)| column == 0)
                        {
                            assert!(
                                (line.x_fraction * width - ancestry).abs() < 0.01,
                                "the table must pay ancestry once at its compact outer edge: {}",
                                line.x_fraction * width
                            );
                            let (left, available) =
                                table_viewport_geometry(line, &projection, width * zoom, zoom);
                            assert!((left - ancestry * zoom).abs() < 0.01);
                            assert!((available - (width - ancestry - 24.) * zoom).abs() < 0.01);
                            assert!(
                                line.table_record.is_none(),
                                "nested tables retain their grid topology"
                            );
                        }
                        if segment.context.image_source.is_some() {
                            let image_width = (width - ancestry).min(960.);
                            assert!(
                                (line.style.line_height - image_width * 360. / 960.).abs() < 0.01,
                                "the figure must fit its remaining branch width"
                            );
                            assert!(
                                (line.width_fraction * width - line.inset - image_width).abs()
                                    < 0.01,
                                "painted image span must match its reserved aspect ratio"
                            );
                        }
                    }
                }
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[test]
    fn tree_context_tracks_real_parents_and_branch_returns() {
        let document = Document::from_markdown(include_str!(
            "../../../../performance/layout-fixtures/91-nested-reading-measures.md"
        ))
        .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let find = |prefix: &str| {
            projection
                .segments()
                .iter()
                .find(|s| projection.text()[s.projection_range()].starts_with(prefix))
                .unwrap()
        };
        for (child, parent, starts) in [
            ("Calibration notes", "Instrument readings", true),
            ("Comparison notes", "Instrument readings", false),
            ("Afternoon observations", "Morning observations", true),
            ("Record the remaining", "Inspect the original", true),
        ] {
            let segment = find(child);
            assert_eq!(
                segment.context.list_parent_label,
                Some(find(parent).node_id)
            );
            let narrow = prepare(segment, 360.);
            assert!(narrow.context.compact_outline);
            assert_eq!(has_header(&narrow), starts);
            assert!(!prepare(segment, 1600.).context.compact_outline);
            assert!(
                !segment.context.compact_outline,
                "canonical context stays unchanged"
            );
        }
    }

    #[test]
    fn tree_pressure_boundaries_keep_shallow_lists_in_normal_flow() {
        let source = "- One\n  - Two\n    - Three\n      - Four\n";
        let document = Document::from_markdown(source).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let last = projection.segments().last().unwrap();
        assert_eq!(last.context.outline_metrics, Some((4, 96)));
        assert!(!prepare(last, 416.).context.compact_outline);
        assert!(prepare(last, 415.).context.compact_outline);
        {
            let source = "- One\n  - Two\n    - Three\n";
            let document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            assert!(
                projection
                    .segments()
                    .iter()
                    .all(|s| !prepare(s, 240.).context.compact_outline)
            );
        }
    }

    #[test]
    fn parent_excerpts_are_bounded_and_do_not_split_combining_clusters() {
        assert_eq!(parent_excerpt("A complete parent."), "A complete parent.");
        assert_eq!(parent_excerpt("A\nparent"), "A parent");
        let source = "cafe\u{301} ".repeat(500);
        let excerpt = parent_excerpt(&source);
        assert!(excerpt.len() <= 1027);
        assert!(excerpt.ends_with('…'));
        assert!(
            source
                .grapheme_indices(true)
                .any(|(offset, _)| offset == excerpt.len() - '…'.len_utf8())
        );
        assert_eq!(
            parent_excerpt(&format!("a{}", "\u{301}".repeat(100_000))),
            "…"
        );
    }
}
