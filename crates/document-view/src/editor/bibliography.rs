use super::*;

const SOURCE: &str = include_str!("../../../../performance/layout-fixtures/71-bibliography.md");

#[gpui::test]
fn bibliography_has_measured_hanging_indents_spacing_and_source_order(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        let document = Document::from_markdown(SOURCE).unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        for (width, zoom) in [(1280., 1.), (420., 1.), (640., 2.)] {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), zoom);
            let plan = build_measured_adaptive_plan(&projection, width, 1000., None, false, &fonts);
            let lines = build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                width,
                &plan,
                Some(&fonts),
            );
            assert!(plan.prose_flows.is_empty());
            assert!(plan.resources.is_empty());
            let mut previous_citation: Option<(&VisualLineSpec, NodeId)> = None;
            let mut entries = 0;
            for segment in projection.segments() {
                let own = lines
                    .iter()
                    .filter(|line| {
                        projection
                            .segment_for_range(&line.projected_range())
                            .unwrap()
                            .node_id
                            == segment.node_id
                    })
                    .collect::<Vec<_>>();
                assert_eq!(
                    own.iter()
                        .map(|line| &projection.text()[line.projected_range()])
                        .collect::<String>(),
                    projection.text()[segment.projection_range()]
                );
                let Some(owner) = segment.context.bibliography else {
                    previous_citation = None;
                    continue;
                };
                entries += 1;
                assert!(own.len() >= 2, "specimen must exercise wrapped citations");
                if let Some((last, previous_owner)) = previous_citation
                    && owner == previous_owner
                {
                    assert!(
                        (own[0].y - last.y - last.style.line_height - 12.).abs() < 0.01,
                        "citation gap {}",
                        own[0].y - last.y - last.style.line_height
                    );
                }
                for (i, line) in own.iter().enumerate() {
                    assert!(line.slot.is_none_or(|s| !s.cards && s.columns == 1));
                    assert_eq!(
                        (line.style.font_size, line.style.line_height),
                        (DocumentStyle::READING_SIZE, DocumentStyle::READING_LEADING)
                    );
                    let expected = if segment.context.list_depth == 0 {
                        if i == 0 { 0. } else { 24. }
                    } else {
                        container_inset(segment)
                    };
                    assert!(
                        (line.inset - expected).abs() < 0.01,
                        "citation inset {} != {expected}",
                        line.inset
                    );
                    let available = line.width_fraction * width - line.inset - 8.;
                    let shaped = fonts
                        .line_width(&projection, line.projected_range(), line.style.font_size)
                        .unwrap();
                    assert!(
                        shaped <= available + 0.5,
                        "citation overflows: {shaped} > {available}"
                    );
                    let runs = styled_projection_runs(
                        &projection,
                        &line.projected_range(),
                        line.projected_range().len(),
                        &gpui::TextStyle {
                            font_family: "Public Sans Tachyon".into(),
                            ..Default::default()
                        },
                        false,
                        TachyonPalette::LIGHT,
                    );
                    assert!(
                        runs.iter()
                            .all(|r| r.font.family.as_ref() == DocumentStyle::BODY_FONT_FAMILY),
                        "citation fonts: {:?}",
                        runs.iter().map(|r| &r.font.family).collect::<Vec<_>>()
                    );
                }
                previous_citation = Some((own.last().unwrap(), owner));
            }
            assert_eq!(entries, 8);
            // Native focus must not switch citation measure, spacing or topology.
            for segment in projection.segments() {
                let held = build_edit_locked_adaptive_plan(
                    &projection,
                    width,
                    1000.,
                    Some(&plan),
                    false,
                    &fonts,
                    Some(segment.node_id),
                );
                let focused = build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &held,
                    Some(&fonts),
                );
                assert_eq!(focused.len(), lines.len());
                for (before, after) in lines.iter().zip(&focused) {
                    assert_eq!(before.projected_range(), after.projected_range());
                    for (a, b) in [
                        (before.inset, after.inset),
                        (before.y, after.y),
                        (before.x_fraction, after.x_fraction),
                        (before.width_fraction, after.width_fraction),
                    ] {
                        assert!(
                            (a - b).abs() < 0.01,
                            "focus changed citation geometry: {a} -> {b}"
                        );
                    }
                }
            }
        }
        assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
    });
}

#[gpui::test]
fn bibliography_reflows_grown_citations_without_restyling_and_undo_is_exact(
    cx: &mut gpui::TestAppContext,
) {
    cx.update(|cx| {
        let mut document = Document::from_markdown(SOURCE).unwrap();
        let before = TextProjection::from_snapshot(&document.snapshot());
        let fonts =
            FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
        let old = build_measured_adaptive_plan(&before, 1280., 1000., None, false, &fonts);
        let target = before
            .segments()
            .iter()
            .find(|s| s.context.bibliography.is_some())
            .unwrap()
            .node_id;
        document
            .apply(EditCommand::ReplaceText {
                node_id: target,
                range: 0..0,
                text: "An additional author and an extended introduction. ".repeat(12),
                typing: true,
                selection_after: None,
            })
            .unwrap();
        let projection = TextProjection::from_snapshot(&document.snapshot());
        let plan = build_edit_locked_adaptive_plan(
            &projection,
            1280.,
            1000.,
            Some(&old),
            true,
            &fonts,
            Some(target),
        );
        let lines =
            build_measured_visual_lines(&projection, &HashMap::new(), 1280., &plan, Some(&fonts));
        let own = lines
            .iter()
            .filter(|line| {
                projection
                    .segment_for_range(&line.projected_range())
                    .unwrap()
                    .node_id
                    == target
            })
            .collect::<Vec<_>>();
        assert!(own.len() > 10);
        assert!(
            own.iter()
                .all(|line| line.style.font_size == DocumentStyle::READING_SIZE
                    && line.slot.is_none_or(|s| !s.cards))
        );
        assert!(own.iter().skip(1).all(|line| line.inset == 24.));
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
        let length = before.block(target).unwrap().text().unwrap().len();
        document
            .apply(EditCommand::ReplaceText {
                node_id: target,
                range: length - 1..length,
                text: ":".into(),
                typing: true,
                selection_after: None,
            })
            .unwrap();
        let mut typing = TextProjection::from_snapshot(&document.snapshot());
        assert!(
            typing
                .segment_for_node(target)
                .unwrap()
                .context
                .bibliography
                .is_none()
        );
        typing.retain_bibliography(&old.bibliography, Some(target));
        assert!(
            typing
                .segment_for_node(target)
                .unwrap()
                .context
                .bibliography
                .is_some()
        );
        let plan = build_edit_locked_adaptive_plan(
            &typing,
            1280.,
            1000.,
            Some(&old),
            true,
            &fonts,
            Some(target),
        );
        assert!(plan.bibliography.contains_key(&target));
        document.undo().unwrap();
        assert_eq!(document.snapshot().serialize().unwrap(), SOURCE);
    });
}
