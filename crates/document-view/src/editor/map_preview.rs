//! An explicit authored map role, using the existing canonical image and link.
use super::*;
use gpui_component::Disableable;

const FOOTER_HEIGHT: f32 = 48.;

pub(super) fn label(image: &document_core::ImageNode) -> Option<String> {
    image
        .link
        .as_ref()
        .filter(|link| !link.target.0.trim().is_empty())?;
    let alt = image.alt.as_string();
    let alt = alt.trim();
    if !alt
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("map:"))
    {
        return None;
    }
    let label = alt[4..].trim();
    (!label.is_empty()).then(|| label.to_owned())
}

pub(super) fn footer_height(image: &document_core::ImageNode) -> f32 {
    if label(image).is_some() {
        FOOTER_HEIGHT
    } else {
        0.
    }
}

pub(super) fn render(
    image: &document_core::ImageNode,
    zoom: f32,
    width: f32,
    enabled: bool,
    palette: TachyonPalette,
    editor: gpui::WeakEntity<RichDocumentEditor>,
) -> Option<AnyElement> {
    let label = label(image)?;
    let node = image.id;
    let keyboard_editor = editor.clone();
    Some(
        div()
            .w(px(width))
            .h(px(FOOTER_HEIGHT * zoom))
            .bg(rgb(palette.page))
            .flex()
            .items_center()
            .child(
                Button::new(("open-map", node.get() as usize))
                    .debug_selector(|| "open-map-command".into())
                    .small()
                    .outline()
                    .max_w_full()
                    .h(px(32. * zoom))
                    .px(px(8. * zoom))
                    .text_color(rgb(palette.accent))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8. * zoom))
                            .child(Icon::new(IconName::ExternalLink).size(px(16. * zoom)))
                            .when(width >= 120. * zoom, |row| {
                                row.child(
                                    div()
                                        .text_size(px(13. * zoom))
                                        .line_height(px(18. * zoom))
                                        .child("Open map"),
                                )
                            }),
                    )
                    .accessible_name(format!("Open map: {label}"))
                    .tooltip(label)
                    .disabled(!enabled)
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_action(move |_: &Enter, window, cx| {
                        activate(&keyboard_editor, node, window, cx)
                    })
                    .on_click(move |_, window, cx| activate(&editor, node, window, cx)),
            )
            .into_any_element(),
    )
}

fn activate(
    editor: &gpui::WeakEntity<RichDocumentEditor>,
    node: NodeId,
    window: &mut Window,
    cx: &mut App,
) {
    cx.stop_propagation();
    let _ = editor.update(cx, |editor, cx| {
        // A retained control cannot turn a replaced ordinary image into a map.
        if matches!(editor.document.snapshot().node(node), Some(BlockNode::Image(image))
            if label(image).is_some() && image.link.as_ref().is_some_and(|link| editor.can_open_link(&link.target.0))) {
            editor.open_semantic_image_link(node, window, cx);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "# Map test\n\n[![Map: Courtyard and reading room](missing.png)](https://example.test/map)\n\nCaption: The complete source-owned description.\n";

    #[test]
    fn only_an_explicit_linked_map_has_a_destination_strip() {
        for (source, expected) in [
            ("[![Map: A place](preview.svg)](#place)\n", Some("A place")),
            (
                "[![mAP: 森の小道](preview.svg)](#place)\n",
                Some("森の小道"),
            ),
            ("![Map: A place](preview.svg)\n", None),
            ("[![A map of the park](map.svg)](#place)\n", None),
            ("[![Map:   ](preview.svg)](#place)\n", None),
            ("[![Mapping: A place](map.svg)](#place)\n", None),
        ] {
            let document = Document::from_markdown(source).unwrap();
            let snapshot = document.snapshot();
            let BlockNode::Image(image) = snapshot.blocks().iter().next().unwrap().as_ref() else {
                panic!("image")
            };
            assert_eq!(label(image).as_deref(), expected);
            assert_eq!(
                footer_height(image),
                if expected.is_some() { 48. } else { 0. }
            );
            assert_eq!(snapshot.serialize().unwrap(), source);
        }
    }

    #[test]
    fn measured_map_keeps_full_aspect_ratio_and_reserves_the_control_strip() {
        for width in [180., 496., 1000., 1600.] {
            let mut heights = Vec::new();
            for map in [false, true] {
                let source = format!(
                    "[![{}Courtyard](map.svg)](#place)\n\nCaption: Complete diagram.\n",
                    if map { "Map: " } else { "" }
                );
                let document = Document::from_markdown(source.as_str()).unwrap();
                let projection = TextProjection::from_snapshot(&document.snapshot());
                let image = projection.image_segments().next().unwrap();
                let dimensions =
                    HashMap::from([(image.node_id, ("map.svg".to_owned(), (1000, 400)))]);
                let lines = build_visual_lines_with_images(&projection, &dimensions, width);
                let line = lines
                    .iter()
                    .find(|line| line.projected_start() == image.projection_start())
                    .unwrap();
                heights.push(line.style.line_height);
                assert!(
                    (line.style.line_height
                        - (width.min(1000.) * 0.4 + if map { 48. } else { 0. }))
                    .abs()
                        < 0.01
                );
                assert!(
                    lines
                        .iter()
                        .filter(|next| next.projected_start() > image.projection_start())
                        .all(|next| next.y >= line.y + line.style.line_height)
                );
                assert_eq!(document.snapshot().serialize().unwrap(), source);
            }
            assert!((heights[1] - heights[0] - 48.).abs() < 0.01);
        }
    }

    #[gpui::test]
    fn unsafe_map_destination_cannot_activate(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::init_editor(cx);
        });
        let source = SOURCE.replace("https://example.test/map", "javascript:alert(1)");
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(
                Document::from_markdown(source.as_str()).unwrap(),
                window,
                cx,
            )
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let bounds = cx
            .debug_bounds("open-map-command")
            .expect("disabled, not disguised as a valid destination");
        cx.simulate_click(bounds.center(), gpui::Modifiers::default());
        let node = editor.read_with(cx, |editor, _| {
            editor.projection.image_segments().next().unwrap().node_id
        });
        cx.update(|window, cx| activate(&editor.downgrade(), node, window, cx));
        assert!(cx.opened_url().is_none());
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source)
        });
    }

    #[gpui::test]
    fn map_control_preserves_selection_and_uses_live_canonical_links(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::init_editor(cx);
        });
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
        });
        let cx: &mut gpui::VisualTestContext = cx;
        cx.run_until_parked();
        editor.update_in(cx, |editor, window, cx| {
            editor.set_selection(0..3, false, window, cx)
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let button = cx
            .debug_bounds("open-map-command")
            .expect("visible map action");
        cx.simulate_click(button.center(), gpui::Modifiers::default());
        assert_eq!(cx.opened_url().as_deref(), Some("https://example.test/map"));
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.selected_byte_range().0, 0..3);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
        });
        editor.update_in(cx, |editor, window, cx| {
            editor.set_selection(0..0, false, window, cx);
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let has_retry = cx.debug_bounds("retry-image-command").is_some();
        cx.update(|window, cx| {
            editor.read(cx).focus_handle.clone().focus(window, cx);
            window.focus_next(cx);
            // A failed preview contributes Retry before its map destination.
            if has_retry {
                window.focus_next(cx);
            }
            _ = window.draw(cx);
        });
        for key in ["enter", "space"] {
            cx.update(|_, cx| cx.open_url("https://example.test/sentinel"));
            let keystroke = gpui::Keystroke::parse(key).unwrap();
            cx.simulate_event(gpui::KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            cx.simulate_event(gpui::KeyUpEvent { keystroke });
            assert_eq!(
                cx.opened_url().as_deref(),
                Some("https://example.test/map"),
                "{key} activates the map"
            );
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE)
            });
        }
        let node = editor.read_with(cx, |editor, _| {
            editor.projection.image_segments().next().unwrap().node_id
        });
        // Replace the enclosing link through the real editor command while
        // retaining the same node/old control identity.
        editor.update_in(cx, |editor, window, cx| {
            let range = editor
                .projection
                .segment_for_node(node)
                .unwrap()
                .projection_range();
            editor.set_selection(range, false, window, cx);
            let result = editor
                .apply_command(EditCommand::SetLinkSelection {
                    target: Some("https://example.test/updated".into()),
                })
                .unwrap();
            editor.refresh_after_transaction(&result);
        });
        cx.update(|window, cx| activate(&editor.downgrade(), node, window, cx));
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.test/updated")
        );
        editor.update_in(cx, |editor, window, cx| {
            editor.undo(&Undo, window, cx);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
            let result = editor
                .apply_command(EditCommand::SetImageAttributes {
                    image_id: node,
                    source: "missing.png".into(),
                    alt: "An ordinary image".into(),
                })
                .unwrap();
            editor.refresh_after_transaction(&result);
        });
        cx.update(|window, cx| activate(&editor.downgrade(), node, window, cx));
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.test/updated"),
            "stale map role cannot open old link"
        );
        editor.update_in(cx, |editor, window, cx| {
            editor.undo(&Undo, window, cx);
            let result = editor
                .apply_command(EditCommand::DeleteBlock { node_id: node })
                .unwrap();
            editor.refresh_after_transaction(&result);
        });
        cx.update(|window, cx| activate(&editor.downgrade(), node, window, cx));
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.test/updated")
        );
    }
}
