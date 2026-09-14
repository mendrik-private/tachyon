//! One source-preserving geometry constructor for named editorial objects.
use super::*;

pub(super) fn build_segment(
    projection: &TextProjection,
    segment: &crate::ProjectionSegment,
    width: f32,
    first: bool,
    last: bool,
    fonts: Option<&FontMeasurement>,
) -> Vec<VisualLineSpec> {
    let block = projection.block(segment.node_id);
    let code = matches!(block, Some(BlockNode::CodeBlock(_)));
    let heading = matches!(block, Some(BlockNode::Heading(_)));
    let leading = if segment.context.color_role.is_some() {
        color_leading(width)
    } else if segment.context.metric.is_some() {
        metrics::leading(width)
    } else {
        0.
    };
    let mut lines = build_visual_lines_for_segment(
        projection,
        segment,
        &HashMap::new(),
        (width - 2. * CARD_PADDING - leading + 8.).max(1.),
        &[],
        fonts,
        None,
    );
    let count = lines.len();
    for (index, line) in lines.iter_mut().enumerate() {
        line.inset += CARD_PADDING + leading;
        line.style.space_above = if index == 0 {
            (if first { CARD_PADDING } else { 0. })
                + if code {
                    CODE_HEADER_HEIGHT + CODE_BLOCK_PADDING
                } else {
                    0.
                }
        } else {
            0.
        };
        line.style.space_below = if index + 1 == count {
            (if last {
                CARD_PADDING
            } else if heading
                || segment.context.metric.is_some()
                || segment.context.color_role.is_some()
            {
                8.
            } else if segment.context.list_depth > 0 {
                12.
            } else {
                16.
            }) + if code { CODE_BLOCK_PADDING } else { 0. }
        } else {
            0.
        };
    }
    lines
}

/// A literal swatch stays visible even in a compact card. Unlike optional
/// metric iconography this is the object being described, not decoration.
pub(super) fn color_leading(width: f32) -> f32 {
    if width >= 360. { 72. } else { 48. }
}

pub(super) fn measure(
    projection: &TextProjection,
    roots: &[&BlockNode],
    segments: &HashMap<NodeId, Vec<usize>>,
    width: f32,
    plan: &AdaptivePlan,
    fonts: &FontMeasurement,
) -> Option<crate::adaptive::rows::GroupMeasurement> {
    let member = plan.editorials.get(&roots.first()?.id())?;
    if roots.len() > 4
        || roots
            .iter()
            .any(|root| plan.editorials.get(&root.id()) != Some(member))
    {
        return None;
    }
    let indexes = roots
        .iter()
        .map(|root| segments.get(&root.id()).map(Vec::as_slice))
        .collect::<Option<Vec<_>>>()?;
    let indexes = indexes.into_iter().flatten().copied().collect::<Vec<_>>();
    if indexes.len() > 20
        || indexes
            .iter()
            .any(|&i| projection.segments()[i].projection_len() > 4096)
    {
        return None;
    }
    let width = if member.wide {
        width
    } else {
        width.min(plan.prose_measures.reference + 2. * CARD_PADDING)
    };
    let mut result = crate::adaptive::rows::GroupMeasurement::default();
    for (i, &index) in indexes.iter().enumerate() {
        let segment = &projection.segments()[index];
        let width = if matches!(
            projection.block(segment.node_id),
            Some(BlockNode::CodeBlock(_))
        ) {
            width
        } else {
            width.min(plan.prose_measures.reference + 2. * CARD_PADDING)
        };
        let lines = build_segment(
            projection,
            segment,
            width,
            i == 0,
            i + 1 == indexes.len(),
            Some(fonts),
        );
        for line in &lines {
            let measured =
                fonts.line_width(projection, line.projected_range(), line.style.font_size)?;
            let code_padding = if matches!(
                projection.block(segment.node_id),
                Some(BlockNode::CodeBlock(_))
            ) {
                CODE_BLOCK_PADDING
            } else {
                0.
            };
            result.overflow |= measured
                > (width - line.inset - line.code_gutter() - CARD_PADDING - code_padding).max(1.)
                    + 0.5;
            result.height +=
                line.style.space_above + line.style.line_height + line.style.space_below;
        }
        if segment.context.metric == Some(crate::metrics::TextRole::Value)
            || segment.context.color_role == Some(crate::signals::ColorRole::Literal)
        {
            // A quantity and its unit are one value. A peer template that
            // wraps it is rejected; it must never split a decimal into rows
            // just to fit a more decorative arrangement.
            result.overflow |= lines.len() != 1;
        }
        // Intrinsic preferred measure (not the resulting wrapped line) is used
        // for the comfort score; exact rendered glyph widths above decide fit.
        let text = &projection.text()[segment.projection_range()];
        let end = text
            .grapheme_indices(true)
            .nth(55)
            .map_or(text.len(), |(i, _)| i);
        let end = text[..end].find(['\n', '\r']).unwrap_or(end);
        let style = lines.first()?.style;
        let preferred = fonts.line_width(
            projection,
            segment.projection_start()..segment.projection_start() + end,
            style.font_size,
        )?;
        result.preferred_width = result
            .preferred_width
            .max(preferred + lines[0].inset + CARD_PADDING);
    }
    Some(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adaptive::{CardAccent, rows::RowKind};
    const SOURCE: &str =
        include_str!("../../../../performance/layout-fixtures/59-editorial-objects.md");
    const EXCHANGES: &str =
        include_str!("../../../../performance/layout-fixtures/60-request-response.md");

    #[gpui::test]
    fn embedded_code_scroll_extent_matches_its_painted_viewport(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let source = format!("## Request\n\n```http\n{}\n```\n", "literal_".repeat(100));
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        editor.update(cx, |editor, cx| editor.set_zoom_factor(2., cx));
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.update(cx, |editor, cx| {
                let line = editor
                    .painted_lines
                    .iter()
                    .filter(|l| l.horizontal_owner.is_some())
                    .max_by(|a, b| a.layout.width().partial_cmp(&b.layout.width()).unwrap())
                    .unwrap();
                let owner = line.horizontal_owner.unwrap();
                let mask = line.content_mask.unwrap().bounds;
                let trailing = line.bounds.left() + line.layout.width();
                let (viewport, content) = editor.horizontal_metrics[&owner];
                assert_eq!(
                    viewport,
                    f32::from(mask.size.width),
                    "the scroll viewport must exclude the card inset exactly as painting does"
                );
                assert!(content > viewport);
                editor.on_scroll_wheel(
                    &ScrollWheelEvent {
                        position: mask.center(),
                        delta: gpui::ScrollDelta::Pixels(point(px(-100000.), px(0.))),
                        ..Default::default()
                    },
                    window,
                    cx,
                );
                let offset = editor.horizontal_scrolls[&owner];
                assert_eq!(offset, content - viewport);
                let right_inset = f32::from(mask.right() - trailing) + offset;
                assert!(
                    (right_inset - CODE_BLOCK_PADDING * 2.).abs() < 0.5,
                    "right inset {right_inset}"
                );
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            });
        });
    }

    #[gpui::test]
    fn exchange_pairing_respects_heading_levels_direction_and_section_barriers(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let fonts = FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            for (first, second, boundary, paired) in [
                ("## Request", "## Response", "", true),
                ("### Request", "### Response", "", true),
                ("## Request", "### Response", "", false),
                ("## Request", "## Response", "## Another topic\n\nUnrelated explanation.\n\n", false),
                ("### Response", "### Request", "", false),
                ("### Request", "### Request", "", false),
            ] {
                let source = format!("# Exchange\n\n{first}\n\n```http\nFirst literal\n```\n\n{boundary}{second}\n\n```http\nSecond literal\n```\n");
                let doc = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&doc.snapshot());
                let plan = arrangement::build_measured_adaptive_plan(&projection, 1200., 1000., None, false, &fonts);
                let codes = projection.roots().filter(|r| matches!(r, BlockNode::CodeBlock(_))).map(|r| r.id()).collect::<Vec<_>>();
                let actual = plan.slots.get(&codes[0]).zip(plan.slots.get(&codes[1]))
                    .is_some_and(|(a, b)| a.columns == 2 && b.columns == 2 && a.group == b.group);
                assert_eq!(actual, paired, "{first} / {boundary} / {second}");
            }
        });
    }

    #[gpui::test]
    fn technical_exchanges_measure_pair_and_stack_in_source_order(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let doc = Document::from_markdown(EXCHANGES).unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            let heading_id =
                |name: &str| {
                    projection.roots().find(|root| {
                    matches!(root, BlockNode::Heading(h) if h.content.as_cow() == name)
                }).unwrap().id()
                };
            for width in [1200., 760., 480., 230.] {
                let plan = arrangement::build_measured_adaptive_plan(
                    &projection,
                    width,
                    1000.,
                    None,
                    false,
                    &fonts,
                );
                let lines = arrangement::build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                assert_eq!(
                    plan.editorials
                        .iter()
                        .filter(|(id, m)| **id == m.owner)
                        .count(),
                    7
                );
                for (request, response) in [
                    ("Request: Create", "Response: Created"),
                    ("Request: Inspect", "Response: Found"),
                    ("Request: Missing title", "Response: Rejected"),
                ] {
                    let left = plan.slots[&heading_id(request)];
                    let right = plan.slots[&heading_id(response)];
                    if width == 1200. {
                        assert_eq!(
                            left.columns,
                            2,
                            "{request}: {:?}; candidates {:?}",
                            plan.measured_rows.chosen,
                            plan.measured_rows
                                .candidates
                                .iter()
                                .filter(|r| r.ids.contains(&heading_id(request)))
                                .collect::<Vec<_>>()
                        );
                        assert_eq!(left.group, right.group);
                        assert_eq!(left.span, 6);
                        assert_eq!(right.span, 6);
                        assert_eq!(left.item, 0);
                        assert_eq!(right.item, 1);
                        assert!(
                            (right.left(width) - left.left(width) - left.width(width) - LAYOUT_GAP)
                                .abs()
                                < 0.01
                        );
                    } else if width < 560. {
                        assert_eq!(left.columns, 1);
                        assert_eq!(right.columns, 1);
                    }
                }
                assert_eq!(plan.slots[&heading_id("Request: Delete")].columns, 1);
                assert!(!plan.editorials.contains_key(&heading_id("Request")));
                for row in plan
                    .measured_rows
                    .chosen
                    .iter()
                    .filter(|r| r.kind == RowKind::Peer)
                {
                    let tallest = row.heights.iter().copied().fold(0_f32, f32::max);
                    for part in &row.parts {
                        let root = projection.roots().nth(part.start).unwrap().id();
                        let slot = plan.slots[&root];
                        let first = lines.iter().find(|l| l.slot == Some(slot)).unwrap();
                        assert!((first.table_row_height - tallest).abs() < 0.01);
                    }
                }
                for segment in projection.segments() {
                    let content = lines
                        .iter()
                        .filter(|l| segment.projection_range().contains(&l.projected_start()))
                        .collect::<Vec<_>>();
                    assert_eq!(
                        content
                            .iter()
                            .map(|l| &projection.text()[l.projected_range()])
                            .collect::<String>()
                            .replace(['\n', '\r'], ""),
                        projection.text()[segment.projection_range()].replace(['\n', '\r'], ""),
                    );
                    if plan.editorials.contains_key(&segment.top_level_node_id)
                        && !matches!(
                            projection.block(segment.node_id),
                            Some(BlockNode::CodeBlock(_))
                        )
                    {
                        for line in content {
                            let slot = line.slot.unwrap();
                            assert!(line.inset >= 24.);
                            assert!(
                                fonts
                                    .line_width(
                                        &projection,
                                        line.projected_range(),
                                        line.style.font_size
                                    )
                                    .unwrap()
                                    <= slot.width(width) - line.inset - 24. + 0.5
                            );
                        }
                    }
                }
            }
            assert_eq!(doc.snapshot().serialize().unwrap(), EXCHANGES);
        });
    }

    #[gpui::test]
    fn editing_an_exchange_label_preserves_the_payload_track(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let mut doc = Document::from_markdown(EXCHANGES).unwrap();
            let before = TextProjection::from_snapshot(&doc.snapshot());
            let plan = arrangement::build_measured_adaptive_plan(
                &before, 1200., 1000., None, false, &fonts,
            );
            let owner = before
                .roots()
                .find(|root| {
                    matches!(root,
                        BlockNode::Heading(h) if h.content.as_cow() == "Response: Created"
                    )
                })
                .unwrap()
                .id();
            let payload = before
                .segments()
                .iter()
                .find(|s| {
                    plan.editorials
                        .get(&s.top_level_node_id)
                        .is_some_and(|m| m.owner == owner)
                        && matches!(before.block(s.node_id), Some(BlockNode::CodeBlock(_)))
                })
                .unwrap()
                .node_id;
            let slot = plan.slots[&payload];
            assert_eq!(slot.columns, 2);
            doc.apply(EditCommand::ReplaceText {
                node_id: owner,
                range: 0..8,
                text: "Result".into(),
                typing: true,
                selection_after: None,
            })
            .unwrap();
            let after = TextProjection::from_snapshot(&doc.snapshot());
            let locked = arrangement::build_edit_locked_adaptive_plan(
                &after,
                1280.,
                1000.,
                Some(&plan),
                false,
                &fonts,
                Some(owner),
            );
            assert_eq!(locked.slots[&payload].left(1280.), slot.left(1200.));
            assert_eq!(locked.slots[&payload].width(1280.), slot.width(1200.));
            let released = arrangement::build_edit_locked_adaptive_plan(
                &after,
                1280.,
                1000.,
                Some(&locked),
                false,
                &fonts,
                None,
            );
            assert!(!released.editorials.contains_key(&owner));
            doc.undo().unwrap();
            assert_eq!(doc.snapshot().serialize().unwrap(), EXCHANGES);
        });
    }

    #[gpui::test]
    fn editorial_objects_use_measured_natural_geometry_and_keep_source(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let doc = Document::from_markdown(SOURCE).unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            for width in [1200., 760., 480., 230.] {
                let plan = arrangement::build_measured_adaptive_plan(
                    &projection,
                    width,
                    1000.,
                    None,
                    false,
                    &fonts,
                );
                let lines = arrangement::build_measured_visual_lines(
                    &projection,
                    &HashMap::new(),
                    width,
                    &plan,
                    Some(&fonts),
                );
                let owners = plan
                    .editorials
                    .iter()
                    .filter(|(id, member)| **id == member.owner)
                    .map(|(&id, _)| id)
                    .collect::<Vec<_>>();
                assert_eq!(owners.len(), 8);
                if width == 1200. {
                    assert!(
                        plan.measured_rows
                            .chosen
                            .iter()
                            .filter(|row| row.kind == RowKind::Peer)
                            .count()
                            >= 3,
                        "{:?}",
                        plan.measured_rows.chosen
                    );
                }
                if width < 560. {
                    assert!(owners.iter().all(|id| plan.slots[id].columns == 1));
                }
                for owner in owners {
                    let slot = plan.slots[&owner];
                    let content = lines
                        .iter()
                        .filter(|line| line.slot == Some(slot))
                        .collect::<Vec<_>>();
                    assert!(!content.is_empty());
                    assert_eq!(content[0].y - content[0].table_row_y, 24.);
                    let last = content.last().unwrap();
                    let last_block = projection
                        .segment_for_range(&last.projected_range())
                        .and_then(|s| projection.block(s.node_id))
                        .unwrap();
                    let code_pad = if matches!(last_block, BlockNode::CodeBlock(_)) {
                        CODE_BLOCK_PADDING
                    } else {
                        0.
                    };
                    let extra = last.table_row_y + last.table_row_height
                        - last.y
                        - last.style.line_height
                        - 24.
                        - code_pad;
                    assert!(extra >= -0.01);
                    if slot.columns == 1 {
                        assert!(extra.abs() < 0.01);
                    }
                    for line in &content {
                        let segment = projection
                            .segment_for_range(&line.projected_range())
                            .unwrap();
                        let block = projection.block(segment.node_id).unwrap();
                        if !matches!(block, BlockNode::CodeBlock(_)) {
                            assert!(
                                fonts
                                    .line_width(
                                        &projection,
                                        line.projected_range(),
                                        line.style.font_size
                                    )
                                    .unwrap()
                                    <= slot.width(width) - line.inset - CARD_PADDING + 0.5,
                                "width {width}, text {:?}",
                                &projection.text()[line.projected_range()]
                            );
                        }
                    }
                }
                for row in plan
                    .measured_rows
                    .chosen
                    .iter()
                    .filter(|row| row.kind == RowKind::Peer)
                {
                    let expected = row.heights.iter().copied().fold(0_f32, f32::max);
                    for part in &row.parts {
                        let root = projection.roots().nth(part.start).unwrap().id();
                        if !plan.editorials.contains_key(&root) {
                            continue;
                        }
                        let first = lines
                            .iter()
                            .find(|l| {
                                projection
                                    .segment_for_range(&l.projected_range())
                                    .is_some_and(|s| s.node_id == root)
                            })
                            .unwrap();
                        assert!(
                            (first.table_row_height - expected).abs() < 0.01,
                            "measure {expected}, paint {}, root {root:?}",
                            first.table_row_height
                        );
                    }
                }
                for segment in projection.segments() {
                    let fragments = lines
                        .iter()
                        .filter(|l| segment.projection_range().contains(&l.projected_start()))
                        .map(|l| &projection.text()[l.projected_range()])
                        .collect::<String>();
                    assert_eq!(
                        fragments.replace(['\n', '\r'], ""),
                        projection.text()[segment.projection_range()].replace(['\n', '\r'], "")
                    );
                }
                let ordinary = projection
                    .segments()
                    .iter()
                    .find(|s| {
                        projection.text()[s.projection_range()]
                            .starts_with("Decision making takes context")
                    })
                    .unwrap();
                assert!(
                    !plan
                        .slots
                        .get(&ordinary.node_id)
                        .is_some_and(|s| matches!(s.card_accent, CardAccent::Editorial(_)))
                );
                let mut scaled = lines.clone();
                scale_visual_lines(&mut scaled, 2.);
                for (a, b) in lines.iter().zip(&scaled) {
                    assert!((a.inset * 2. - b.inset).abs() < 0.01);
                    assert!((a.table_row_height * 2. - b.table_row_height).abs() < 0.01);
                }
            }
            assert_eq!(doc.snapshot().serialize().unwrap(), SOURCE);
        });
    }

    #[gpui::test]
    fn editorial_typing_locks_complete_object_and_exact_undo(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Public Sans Tachyon".into(), 1.);
            let mut doc = Document::from_markdown(SOURCE).unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            let plan = arrangement::build_measured_adaptive_plan(
                &projection,
                1200.,
                1000.,
                None,
                false,
                &fonts,
            );
            let segment = projection
                .segments()
                .iter()
                .find(|s| {
                    projection.text()[s.projection_range()]
                        .starts_with("Keep documents in the workspace")
                })
                .unwrap();
            let id = segment.node_id;
            let slot = plan.slots[&id];
            assert_eq!(slot.columns, 2);
            doc.apply(EditCommand::ReplaceText {
                node_id: id,
                range: 0..0,
                text: "More context stays editable. ".repeat(90),
                typing: true,
                selection_after: None,
            })
            .unwrap();
            let projection = TextProjection::from_snapshot(&doc.snapshot());
            let locked = arrangement::build_edit_locked_adaptive_plan(
                &projection,
                1280.,
                1000.,
                Some(&plan),
                false,
                &fonts,
                Some(id),
            );
            assert_eq!(locked.slots[&id].columns, 1);
            assert!(locked.slots[&id].width(1280.) > slot.width(1200.));
            let lines = arrangement::build_measured_visual_lines(
                &projection,
                &HashMap::new(),
                1280.,
                &locked,
                Some(&fonts),
            );
            let segment = projection.segment_for_node(id).unwrap();
            assert_eq!(
                lines
                    .iter()
                    .filter(|l| segment.projection_range().contains(&l.projected_start()))
                    .map(|l| &projection.text()[l.projected_range()])
                    .collect::<String>(),
                &projection.text()[segment.projection_range()]
            );
            let released = arrangement::build_edit_locked_adaptive_plan(
                &projection,
                1280.,
                1000.,
                Some(&locked),
                false,
                &fonts,
                None,
            );
            assert!(!released.editorials.contains_key(&id));
            doc.undo().unwrap();
            assert_eq!(doc.snapshot().serialize().unwrap(), SOURCE);
        });
    }
}
