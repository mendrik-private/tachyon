//! One immutable, published geometry version, shared with an in-flight replan.
//! This is not a document-history cache: the view and its single worker own it.
//! Content mutations release the view's reuse handle before copy-on-write edits.
use super::*;

pub(super) struct PublishedGeometry {
    /// One source-order baseline for this environment, never a history chain.
    pub stack: Option<Arc<PublishedGeometry>>,
    projection: crate::projection::ProjectionGeometryKey,
    plan: crate::adaptive::PlanGeometryKey,
    measurement: Arc<()>,
    images: NodeImageDimensions,
    width: f32,
    zoom: f32,
    extensions: geometry_cache::RetainedExtensions,
    pub lines: Arc<Vec<VisualLineSpec>>,
    pub paint_order: Arc<Vec<usize>>,
    pub components: Arc<ComponentIndex>,
    pub height: f32,
}

#[derive(Clone, Copy)]
pub(super) struct GeometryInputs<'a> {
    pub projection: &'a TextProjection,
    pub plan: &'a AdaptivePlan,
    pub measurement: &'a FontMeasurement,
    pub images: &'a NodeImageDimensions,
    pub width: f32,
    pub zoom: f32,
}

impl PublishedGeometry {
    pub fn matches(&self, input: &GeometryInputs<'_>) -> bool {
        self.width.to_bits() == input.width.to_bits()
            && self.zoom.to_bits() == input.zoom.to_bits()
            && Arc::ptr_eq(&self.measurement, &input.measurement.identity)
            && self.images == *input.images
            && self.projection.matches(input.projection)
            && self.plan.matches(input.plan)
    }

    #[cfg(test)]
    fn build(input: GeometryInputs<'_>) -> Self {
        Self::build_with_previous(input, None)
    }

    pub fn build_with_previous(input: GeometryInputs<'_>, previous: Option<&Self>) -> Self {
        let previous =
            previous.filter(|p| Arc::ptr_eq(&p.measurement, &input.measurement.identity));
        let mut extensions = geometry_cache::ExtensionReuse::new(previous.map(|p| &p.extensions));
        let mut lines = arrangement::build_visual_lines_with_extensions(
            input.projection,
            input.images,
            input.width / input.zoom,
            input.plan,
            Some(input.measurement),
            0..input.projection.segments().len(),
            Some(&mut extensions),
        );
        scale_visual_lines(&mut lines, input.zoom);
        let paint_order = visual_line_paint_order(&lines);
        let height = visual_document_height(&lines);
        let components = component_geometry(
            input.projection,
            &lines,
            input.width,
            input.zoom,
            &paint_order,
        );
        Self {
            stack: None,
            projection: input.projection.geometry_key(),
            plan: input.plan.geometry_key(),
            measurement: input.measurement.identity.clone(),
            images: input.images.clone(),
            width: input.width,
            zoom: input.zoom,
            extensions: extensions.next,
            lines: Arc::new(lines),
            paint_order: Arc::new(paint_order),
            components: Arc::new(components),
            height,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn retained_extensions_rebase_source_ranges_after_preceding_text_edit(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = "Before.\n\n<details open><summary>More</summary><p>café 東京</p></details>\n\n```math\n\\frac{x}{2}\n```\n";
            let mut document = Document::from_markdown(source).unwrap();
            let projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = AdaptivePlan::default();
            let dimensions = HashMap::new();
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let baseline = PublishedGeometry::build(GeometryInputs { projection: &projection, plan: &plan,
                measurement: &measurement, images: &dimensions, width: 760., zoom: 1. });
            document.apply(EditCommand::ReplaceText { node_id: projection.segments()[0].node_id,
                range: 0..0, text: "東京 😀 added before extensions ".into(), selection_after: None, typing: false }).unwrap();
            let changed = TextProjection::from_snapshot(&document.snapshot());
            let input = GeometryInputs { projection: &changed, plan: &plan,
                measurement: &measurement, images: &dimensions, width: 760., zoom: 1. };
            let counts = diagnostics::MeasurementScope::new();
            let reused = PublishedGeometry::build_with_previous(input, Some(&baseline));
            assert_eq!(counts.take_stage().retained_extension_hits, 2);
            let fresh = PublishedGeometry::build(input);
            arrangement::tests::assert_same_geometry(&reused.lines, &fresh.lines);
            let html = |geometry: &PublishedGeometry| geometry.lines.iter().find(|l| l.html_preview.is_some()).unwrap().projected_range();
            assert!(html(&reused).start > html(&baseline).start);
            assert_eq!(reused.paint_order, fresh.paint_order);
            assert_eq!(reused.height, fresh.height);
            document.undo().unwrap();
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn retained_extensions_follow_loaded_image_identity_and_removal(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = "<div><img src='local.png' alt='Chart' width='40' height='40'></div>";
            let document = Document::from_markdown(source).unwrap();
            let mut projection = TextProjection::from_snapshot(&document.snapshot());
            let plan = AdaptivePlan::default();
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let dimensions = HashMap::new();
            let resource = hash(&resolved_image_resource("local.png", None));
            let red = blitz_dom::node::RasterImageData::new(1, 1, Arc::new(vec![255, 0, 0, 255]));
            let blue = blitz_dom::node::RasterImageData::new(1, 1, Arc::new(vec![0, 0, 255, 255]));
            let mut previous: Option<PublishedGeometry> = None;
            let counts = diagnostics::MeasurementScope::new();
            for (index, image) in [None, Some(red.clone()), Some(red), Some(blue), None]
                .into_iter()
                .enumerate()
            {
                let loaded = Arc::new(image.into_iter().map(|image| (resource, image)).collect());
                html_images::bind(&mut projection, &loaded, None);
                let input = GeometryInputs {
                    projection: &projection,
                    plan: &plan,
                    measurement: &measurement,
                    images: &dimensions,
                    width: 900.,
                    zoom: 1.,
                };
                counts.take_stage();
                let next = PublishedGeometry::build_with_previous(input, previous.as_ref());
                assert_eq!(
                    counts.take_stage().retained_extension_hits,
                    u64::from(index == 2),
                    "step {index}"
                );
                let fresh = PublishedGeometry::build(input);
                arrangement::tests::assert_same_geometry(&next.lines, &fresh.lines);
                let preview = next
                    .lines
                    .iter()
                    .find_map(|line| line.html_preview.as_ref());
                assert_eq!(preview.is_some(), matches!(index, 1..=3));
                if let Some(preview) = preview {
                    assert_eq!(
                        preview.image.bytes,
                        fresh.lines[0].html_preview.as_ref().unwrap().image.bytes
                    );
                    if index == 3 {
                        assert_ne!(
                            preview.image.bytes,
                            previous.as_ref().unwrap().lines[0]
                                .html_preview
                                .as_ref()
                                .unwrap()
                                .image
                                .bytes,
                            "same-sized replacement pixels must invalidate the retained preview"
                        );
                    }
                }
                previous = Some(next);
            }
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn window_transition_retains_unchanged_extensions(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = (0..3).map(|n| format!(
                "## Chapter {n}\n\n<details open><summary>More {n}</summary><p>Source text {n}.</p></details>\n\n$$\nx^{n} + y\n$$\n\n"
            )).collect::<String>();
            let document = Document::from_markdown(source.as_str()).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let windows = initial.adaptive.windows.clone();
            assert_eq!(windows.len(), 3);
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let prepare = |scope, previous: &PreparedDocumentView| {
                PreparedDocumentView::prepare_snapshot_with_images(
                    &document.snapshot(), &HashMap::new(), None,
                    ReflowViewport {
                        published_geometry: previous.published_geometry.clone(),
                        width: 900., height: 600., zoom: 1., preview_edit_node: None, expanded_code_tail: None,
                        editing_node: None, table_layout_lock: None,
                        html_disclosures: Arc::default(), html_loaded_images: Arc::default(),
                        trace_mode: LayoutTraceMode::Off, visible_roots: scope, resource_generation: 0,
                    }, &previous.adaptive, &measurement,
                ).0
            };
            let first = prepare(Some(windows[0].clone()), &initial);
            let counts = diagnostics::MeasurementScope::new();
            let second = prepare(Some(windows[1].clone()), &first);
            let measured = counts.take_stage();
            assert_eq!(measured.published_geometry_reuses, 0);
            assert_eq!(measured.retained_extension_hits, 5,
                "all HTML and unchanged math must be retained; only newly measured math changes source-line measurement mode");
            let input = GeometryInputs {
                projection: &second.projection, plan: &second.adaptive, measurement: &measurement,
                images: &HashMap::new(), width: 900., zoom: 1.,
            };
            let fresh = PublishedGeometry::build(input);
            arrangement::tests::assert_same_geometry(&second.visual_lines, &fresh.lines);
            for (before, after) in first.visual_lines.iter().filter_map(|l| l.html_preview.as_ref())
                .zip(second.visual_lines.iter().filter_map(|l| l.html_preview.as_ref())) {
                assert!(Arc::ptr_eq(before, after), "HTML raster, hit targets, disclosures and accessible text stay in the same immutable preview");
            }
            assert_eq!(second.paint_order, fresh.paint_order);
            assert_eq!(second.document_height, fresh.height);
            assert_eq!(second.visual_lines.iter().filter(|l| l.html_preview.is_some()).count(), 3);
            assert_eq!(second.visual_lines.iter().filter(|l| l.display_math.is_some()).count(), 3);
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn visiting_plain_window_replaces_fallback_geometry_without_changing_slots(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let source = format!(
                "## First\n\n{}\n\n## Second\n\n{}\n",
                "Wide WWW and narrow iii text stay in source order. ".repeat(20),
                "Different wraps must replace estimates when this chapter is visited. ".repeat(25)
            );
            let document = Document::from_markdown(source.as_str()).unwrap();
            let initial = PreparedDocumentView::prepare(&document);
            let windows = initial.adaptive.windows.clone();
            assert_eq!(windows.len(), 2);
            let measurement =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let prepare = |scope, previous: &PreparedDocumentView| {
                PreparedDocumentView::prepare_snapshot_with_images(
                    &document.snapshot(),
                    &HashMap::new(),
                    None,
                    ReflowViewport {
                        published_geometry: previous.published_geometry.clone(),
                        width: 640.,
                        height: 600.,
                        zoom: 1.,
                        preview_edit_node: None,
                        expanded_code_tail: None,
                        editing_node: None,
                        table_layout_lock: None,
                        html_disclosures: Arc::default(),
                        html_loaded_images: Arc::default(),
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: scope,
                        resource_generation: 0,
                    },
                    &previous.adaptive,
                    &measurement,
                )
                .0
            };
            let first = prepare(Some(windows[0].clone()), &initial);
            assert!(first.adaptive.slots.is_empty());
            let counts = diagnostics::MeasurementScope::new();
            let second = prepare(Some(windows[1].clone()), &first);
            let measured = counts.take_stage();
            assert_eq!(measured.published_geometry_reuses, 0);
            assert!(measured.shaping_calls > 0);
            assert!(second.adaptive.slots.is_empty());
            assert!(!Arc::ptr_eq(&first.visual_lines, &second.visual_lines));
            let full = prepare(None, &initial);
            arrangement::tests::assert_same_geometry(&second.visual_lines, &full.visual_lines);
            assert_eq!(second.paint_order, full.paint_order);
            assert_eq!(second.document_height, full.document_height);
            counts.take_stage();
            let warm = prepare(Some(windows[1].clone()), &second);
            assert_eq!(counts.take_stage().published_geometry_reuses, 1);
            assert!(Arc::ptr_eq(&warm.visual_lines, &second.visual_lines));
            assert_eq!(document.snapshot().serialize().unwrap(), source);
        });
    }

    #[gpui::test]
    fn published_geometry_checks_every_renderer_input(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let source = "# Title\n\nA short introduction.\n\nText with $x^2$.\n\n- North\n- South\n- East\n- West\n- Above\n- Below\n\n| Key | Value |\n| --- | --- |\n| Name | café |\n\n![Figure](figure.png)\n\n<details><summary>More</summary><p>Body</p></details>\n\n$$\nx^2 + y^2\n$$\n";
            let document = Document::from_markdown(source).unwrap();
            let snapshot = document.snapshot();
            let measurement = FontMeasurement::new(cx.text_system().clone(), "Spline Sans Mineral".into(), 1.);
            let other_font = FontMeasurement::new(cx.text_system().clone(), "monospace".into(), 1.);
            let mut projection = TextProjection::from_snapshot(&snapshot);
            projection.preview_edit_node = None;
            measurement.measure_tables(&mut projection);
            let images = projection.image_segments().map(|segment| (segment.node_id, (segment.context.image_source.clone().unwrap(), (640, 400)))).collect::<NodeImageDimensions>();
            let plan = build_measured_adaptive_plan(&projection, 1280., 1000., None, false, &measurement);
            assert!(!plan.slots.is_empty());
            let baseline = PublishedGeometry::build(GeometryInputs {
                projection: &projection, plan: &plan, measurement: &measurement,
                images: &images, width: 1280., zoom: 1.,
            });
            for case in 0..14 {
                let mut changed_projection = projection.clone();
                let mut changed_plan = plan.clone();
                let mut changed_images = images.clone();
                let mut width = 1280.;
                let mut zoom = 1.;
                let mut font = &measurement;
                match case {
                    0 => {},
                    1 => changed_projection.preview_edit_node = projection.segments().iter().find(|s| inline_math::has_math(&projection, s.node_id)).map(|s| s.node_id),
                    2 => {
                        let cell = projection.segments().iter().find(|s| s.context.table_cell.is_some()).unwrap();
                        changed_projection.lock_table_for_node(Some(cell.node_id));
                    },
                    3 => {
                        let table = projection.roots().find(|b| matches!(b, BlockNode::Table(_))).unwrap().id();
                        let mut measured = projection.table_measurements(table).unwrap().clone();
                        measured.preferred[0] += 100.;
                        changed_projection.install_table_measurements(table, measured);
                    },
                    4 => {
                        let html = projection.roots().find(|b| matches!(b, BlockNode::PreservedSource { .. })).unwrap();
                        let BlockNode::PreservedSource { source, .. } = html else { unreachable!() };
                        changed_projection.html_disclosures = Arc::new([(html.id(), crate::html::DisclosureState { source: source.clone(), overrides: [(0, true)].into() })].into_iter().collect());
                    },
                    5 => width += 0.125,
                    6 => zoom = 2.,
                    7 => font = &other_font,
                    8 => changed_images.values_mut().next().unwrap().1 = (640, 800),
                    9 => {
                        let slot = changed_plan.slots.values_mut().next().unwrap();
                        // Three tracks is now a valid baseline (four columns).
                        // Always mutate geometry, regardless of HashMap order.
                        slot.span = if slot.span == 3 { 2 } else { 3 };
                    },
                    10 => changed_plan.lists.values_mut().next().unwrap().layout = ListLayout::Outline,
                    11 => changed_plan.lead = None,
                    12 => {
                        let changed = Document::from_markdown(source.replace("café", "café edited")).unwrap();
                        changed_projection = TextProjection::from_snapshot(&changed.snapshot());
                        changed_projection.preview_edit_node = None;
                        measurement.measure_tables(&mut changed_projection);
                    },
                    // Diagnostics/planner bookkeeping isn't a renderer input.
                    13 => { changed_plan.measured_rows.viewport = 200.; changed_plan.resource_generation += 1; },
                    _ => unreachable!(),
                }
                let input = GeometryInputs {
                    projection: &changed_projection, plan: &changed_plan, measurement: font,
                    images: &changed_images, width, zoom,
                };
                assert_eq!(baseline.matches(&input), matches!(case, 0 | 13), "case {case}");
                let reused = PublishedGeometry::build_with_previous(input, Some(&baseline));
                let fresh = PublishedGeometry::build(input);
                arrangement::tests::assert_same_geometry(&reused.lines, &fresh.lines);
                assert_eq!(reused.paint_order, fresh.paint_order, "case {case}");
                assert_eq!(reused.height, fresh.height, "case {case}");
                for (a, b) in reused.lines.iter().zip(fresh.lines.iter()) {
                    if let Some((a, b)) = a.html_preview.as_ref().zip(b.html_preview.as_ref()) {
                        assert_eq!(a.image.bytes, b.image.bytes, "case {case}");
                        assert_eq!(a.accessible_text, b.accessible_text, "case {case}");
                        assert_eq!(a.text_ranges, b.text_ranges, "case {case}");
                        assert_eq!(format!("{:?}", a.disclosures), format!("{:?}", b.disclosures), "case {case}");
                        assert_eq!(format!("{:?}", a.anchors), format!("{:?}", b.anchors), "case {case}");
                        assert_eq!(format!("{:?}", a.text_hits), format!("{:?}", b.text_hits), "case {case}");
                    }
                }
                if baseline.matches(&input) {
                    arrangement::tests::assert_same_geometry(&baseline.lines, &fresh.lines);
                    assert_eq!(baseline.paint_order, fresh.paint_order);
                    assert_eq!(baseline.height, fresh.height);
                }
            }
            assert_eq!(snapshot.serialize().unwrap(), source);
        });
    }
}
