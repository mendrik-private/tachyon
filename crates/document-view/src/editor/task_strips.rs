//! Task lists retain canonical vertical order at every width.
#[cfg(test)]
mod tests {
    use super::super::*;

    #[gpui::test]
    fn checklist_items_keep_distinct_rows_and_exact_undo_at_every_scale(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            for count in 2..=12 {
                let source = (0..count)
                    .map(|i| format!("- [ ] Task {i}\n"))
                    .collect::<String>();
                let mut document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let BlockNode::List(list) = projection.roots().next().unwrap() else {
                    panic!("task list")
                };
                for zoom in [1., 1.5, 2.] {
                    let fonts = FontMeasurement::new(
                        cx.text_system().clone(),
                        "Spline Sans Tachyon".into(),
                        zoom,
                    );
                    for width in [360., 900., 1800.] {
                        let plan = build_measured_adaptive_plan(
                            &projection,
                            width,
                            1000.,
                            None,
                            false,
                            &fonts,
                        );
                        assert_eq!(plan.lists[&list.id].layout, ListLayout::Checklist);
                        assert!(plan.slots.is_empty());
                        let lines = build_measured_visual_lines(
                            &projection,
                            &HashMap::new(),
                            width,
                            &plan,
                            Some(&fonts),
                        );
                        assert_eq!(lines.len(), count);
                        assert!(
                            lines
                                .windows(2)
                                .all(|pair| pair[1].y >= pair[0].y + pair[0].style.line_height)
                        );
                        assert!((lines[0].y - LAYOUT_HEADER).abs() < 0.01);
                        assert!(lines.iter().all(|line| line.inset == 32.));
                        let focused = build_edit_locked_adaptive_plan(
                            &projection,
                            width,
                            1000.,
                            Some(&plan),
                            false,
                            &fonts,
                            Some(projection.segments()[0].node_id),
                        );
                        assert!(focused.slots.is_empty());
                    }
                }
                document
                    .apply(EditCommand::ToggleTask {
                        item_id: list.items[0].id,
                    })
                    .unwrap();
                assert!(
                    document
                        .snapshot()
                        .serialize()
                        .unwrap()
                        .starts_with("- [x] Task 0")
                );
                document.undo().unwrap();
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }

    #[gpui::test]
    fn rich_nested_mixed_and_long_tasks_stay_vertical(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let fonts =
                FontMeasurement::new(cx.text_system().clone(), "Spline Sans Tachyon".into(), 1.);
            for source in [
                "- [ ] One\n  - [ ] Child\n- [ ] Two\n",
                "- [ ] One\n- Two\n- Three\n",
                "- [ ] One\n\n  Supporting paragraph.\n- [ ] Two\n",
                "- [ ] One\n- [ ] ![Image](photo.png)\n",
                "- [ ] שלום עולם\n- [ ] בדיקה\n",
                &format!(
                    "- [ ] {}\n- [ ] {}\n",
                    "Long explanation ".repeat(60),
                    "Further explanation ".repeat(60)
                ),
                &"- [ ] One\n".repeat(13),
            ] {
                let document = Document::from_markdown(source).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let plan =
                    build_measured_adaptive_plan(&projection, 1280., 1000., None, false, &fonts);
                assert!(plan.slots.is_empty(), "{source}");
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
        });
    }
}
