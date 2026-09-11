//! Presentation only: the toolkit/cache owns whether an image is loading or failed.
use super::*;

#[derive(Clone)]
pub(super) struct Presentation {
    node: NodeId,
    source: String,
    alt: String,
    width: f32,
    height: f32,
    zoom: f32,
    palette: MineralPalette,
    editor: gpui::WeakEntity<RichDocumentEditor>,
}

impl Presentation {
    pub(super) fn new(
        image: &document_core::ImageNode,
        width: f32,
        height: f32,
        zoom: f32,
        palette: MineralPalette,
        editor: gpui::WeakEntity<RichDocumentEditor>,
    ) -> Self {
        Self {
            node: image.id,
            source: image.source.clone(),
            alt: image.alt.as_string(),
            width,
            height,
            zoom,
            palette,
            editor,
        }
    }

    pub(super) fn render(&self, failed: bool) -> AnyElement {
        let title = if failed {
            "Image unavailable"
        } else {
            "Loading image…"
        };
        let destination = destination_label(&self.source);
        let description = if self.alt.is_empty() {
            destination.clone()
        } else {
            format!("{} — {destination}", self.alt)
        };
        // A table column can be narrow but still have enough vertical space
        // for status and description. Adapt its insets, not its content roles.
        let roomy = self.height >= 160. * self.zoom;
        let narrow = self.width < 240. * self.zoom;
        let inset = if !roomy {
            4.
        } else if narrow {
            8.
        } else {
            16.
        };
        // Reserve two title lines, one destination line, the recovery control
        // and three gaps before assigning the remaining height to alt text.
        let alt_lines = ((self.height / self.zoom - 2. * inset - 100.) / 18.)
            .floor()
            .clamp(1., 4.) as usize;
        let mut view = div()
            .id(("image-resource-state", self.node.get() as usize))
            .debug_selector(|| "image-resource-state".into())
            .role(Role::Group)
            .aria_label(title)
            .aria_description(description)
            .w(px(self.width))
            .h(px(self.height))
            .overflow_hidden()
            .flex()
            .flex_col()
            .justify_center()
            .gap(px(6. * self.zoom))
            .p(px(inset * self.zoom))
            .rounded(px(DocumentStyle::MEDIA_RADIUS * self.zoom))
            .bg(rgb(self.palette.surface_quiet))
            .text_color(rgb(self.palette.secondary))
            .text_size(px(13. * self.zoom))
            .line_height(px(18. * self.zoom))
            .cursor_default();
        if roomy {
            view = view
                .child(
                    div()
                        .debug_selector(|| "image-state-title".into())
                        .min_w_0()
                        .flex_none()
                        .line_clamp(2)
                        .text_color(rgb(self.palette.heading))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(title),
                )
                .when(!self.alt.is_empty(), |view| {
                    view.child(
                        div()
                            .debug_selector(|| "image-state-alt".into())
                            .min_w_0()
                            .flex_none()
                            .line_clamp(alt_lines)
                            .child(self.alt.clone()),
                    )
                })
                .child(div().min_w_0().flex_none().truncate().child(destination));
        }
        if failed {
            let state = self.clone();
            let keyboard_state = self.clone();
            view = view.child(
                Button::new(("retry-document-image", self.node.get() as usize))
                    .debug_selector(|| "retry-image-command".into())
                    .small()
                    .outline()
                    .self_start()
                    .flex_none()
                    .max_w_full()
                    .h(px(28. * self.zoom))
                    .px(px(8. * self.zoom))
                    .child(
                        div()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13. * self.zoom))
                            .line_height(px(18. * self.zoom))
                            .child("Retry image"),
                    )
                    .accessible_name("Retry image")
                    .tooltip("Try loading this image again")
                    .cursor_pointer()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    // Enter is bound by the ancestor editor. Consume that
                    // action here instead of inserting a document paragraph.
                    .on_action(move |_: &Enter, _, cx| keyboard_state.retry(cx))
                    .on_click(move |_, _, cx| {
                        state.retry(cx);
                    }),
            );
        } else if !roomy {
            view = view.child(div().truncate().child(title));
        }
        view.into_any_element()
    }

    fn retry(&self, cx: &mut App) {
        cx.stop_propagation();
        let _ = self.editor.update(cx, |editor, cx| {
            editor.retry_image_source(self.node, &self.source, cx);
        });
    }
}

/// Never display remote authority, credentials or query/fragment tokens. The
/// canonical destination remains in the image model/editor, not this preview.
fn destination_label(source: &str) -> String {
    if source
        .get(..5)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("data:"))
    {
        return "Embedded image".into();
    }
    let source = source.split(['?', '#']).next().unwrap_or_default();
    let remote = source
        .split_once("://")
        .map(|(_, rest)| rest)
        .or_else(|| source.strip_prefix("//"));
    let path = remote.map_or(source, |rest| {
        rest.split_once('/').map_or("", |(_, path)| path)
    });
    path.rsplit('/')
        .find(|part| !part.is_empty())
        .filter(|part| !part.trim().is_empty())
        .unwrap_or(if remote.is_some() {
            "Remote image"
        } else {
            "Image"
        })
        .to_owned()
}

impl RichDocumentEditor {
    pub(super) fn retry_image_source(
        &mut self,
        node: NodeId,
        source: &str,
        cx: &mut Context<Self>,
    ) {
        // A retained overlay must not retry a resource replaced/deleted by an edit.
        if !matches!(self.projection.block(node), Some(BlockNode::Image(image)) if image.source == source)
        {
            return;
        }
        cx.emit(EditorEvent::RetryImage {
            source: source.to_owned(),
            document_directory: self.document_directory.clone(),
        });
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn narrow_cell_keeps_visible_description_and_contained_recovery(cx: &mut gpui::TestAppContext) {
        const SOURCE: &str = "# Before\n\n<!-- mineral-table:v1 {\"border\":\"LogicalPixel\",\"widths\":[200,360]} -->\n<table><tr><td><p><img src=\"missing.png\" alt=\"A trail map with two observation points\"></p></td><td><p>Neighbor</p></td></tr></table>\n";
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::init_editor(cx);
        });
        for width in [160, 200, 263, 264, 265] {
            for quoted in [false, true] {
                for zoom in [1., 2.] {
                    for alternative in [
                        "A trail map with two observation points".to_owned(),
                        "The trail follows the river past the observation points and returns through the wooded valley. ".repeat(4),
                        "森の小道と二つの観測地点を示す地図 — خريطة مسار الوادي".to_owned(),
                    ] {
                        let mut source = SOURCE.replace("[200,360]", &format!("[{width},360]"));
                        source = source.replace("A trail map with two observation points", &alternative);
                        if quoted {
                            source = source
                                .replace("<td><p><img", "<td><blockquote><p><img")
                                .replace("</p></td><td>", "</p></blockquote></td><td>");
                        }
                        let (host, cx) = cx.add_window_view(|window, cx| ImageDocument {
                            editor: cx.new(|cx| {
                                let mut editor = RichDocumentEditor::new(
                                    Document::from_markdown(source.as_str()).unwrap(),
                                    window,
                                    cx,
                                );
                                editor.set_zoom_factor(zoom, cx);
                                editor
                            }),
                            cache: cx.new(|_| ControlledCache {
                                result: Some(Err(gpui::ImageCacheError::Asset("missing".into()))),
                            }),
                        });
                        let cx: &mut gpui::VisualTestContext = cx;
                        cx.run_until_parked();
                        cx.update(|window, cx| {
                            _ = window.draw(cx);
                        });
                        let state = cx
                            .debug_bounds("image-resource-state")
                            .expect("actual failed cell image");
                        assert_eq!(state.size.height, px(180. * zoom));
                        for selector in [
                            "image-state-title",
                            "image-state-alt",
                            "retry-image-command",
                        ] {
                            let child = cx.debug_bounds(selector)
                                .expect("narrow image must retain visible status and description, not only Retry");
                            assert!(
                                state.contains(&child.origin) && state.contains(&child.bottom_right()),
                                "{selector} must remain inside the cell image (width {width}, quoted {quoted}, zoom {zoom}): {child:?} vs {state:?}"
                            );
                        }
                        host.read_with(cx, |host, cx| {
                            assert_eq!(
                                host.editor.read(cx).document.snapshot().serialize().unwrap(),
                                source
                            )
                        });
                    }
                }
            }
        }
    }

    struct ControlledCache {
        result: Option<Result<Arc<gpui::RenderImage>, gpui::ImageCacheError>>,
    }

    impl gpui::ImageCache for ControlledCache {
        fn load(
            &mut self,
            _: &Resource,
            _: &mut Window,
            _: &mut App,
        ) -> Option<Result<Arc<gpui::RenderImage>, gpui::ImageCacheError>> {
            self.result.clone()
        }
    }

    struct ImageDocument {
        editor: Entity<RichDocumentEditor>,
        cache: Entity<ControlledCache>,
    }

    impl Render for ImageDocument {
        fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
            div()
                .size_full()
                .image_cache(self.cache.clone())
                .child(self.editor.clone())
        }
    }

    #[gpui::test]
    fn actual_callbacks_keep_geometry_and_retry_does_not_select_the_image(
        cx: &mut gpui::TestAppContext,
    ) {
        const SOURCE: &str = "# Before\n\n![A complete alternative description](missing.png)\n\nCaption: Source owned.\n";
        cx.update(|cx| {
            gpui_component::init(cx);
            crate::init_editor(cx);
        });
        let (host, cx) = cx.add_window_view(|window, cx| ImageDocument {
            editor: cx.new(|cx| {
                RichDocumentEditor::new(Document::from_markdown(SOURCE).unwrap(), window, cx)
            }),
            cache: cx.new(|_| ControlledCache { result: None }),
        });
        let cx: &mut gpui::VisualTestContext = cx;
        let (editor, cache) =
            host.read_with(cx, |host, _| (host.editor.clone(), host.cache.clone()));
        let retries = Arc::new(Mutex::new(Vec::new()));
        let recorded = retries.clone();
        editor
            .update(cx, |_, cx| {
                cx.subscribe(&editor, move |_, _, event, _| {
                    if let EditorEvent::RetryImage { source, .. } = event {
                        recorded.lock().unwrap().push(source.clone());
                    }
                })
            })
            .detach();
        cx.run_until_parked();
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(
            cx.debug_bounds("image-resource-state").is_none(),
            "no flash before loading delay"
        );
        // This toolkit pin uses wall-clock Instant::elapsed for Img's delay,
        // while its wake-up task uses the test executor's virtual clock.
        std::thread::sleep(Duration::from_millis(250));
        cx.executor().advance_clock(Duration::from_millis(250));
        editor.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        let loading = cx
            .debug_bounds("image-resource-state")
            .expect("actual delayed loading callback");
        assert!(
            cx.debug_bounds("retry-image-command").is_none(),
            "pending is not failed"
        );

        cache.update(cx, |cache, _| {
            cache.result = Some(Err(gpui::ImageCacheError::Asset("unreadable".into())))
        });
        editor.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert_eq!(cx.debug_bounds("image-resource-state").unwrap(), loading);
        for selection in [0..0, 0..3] {
            editor.update_in(cx, |editor, window, cx| {
                editor.set_selection(selection.clone(), false, window, cx)
            });
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            let retry = cx.debug_bounds("retry-image-command").unwrap();
            assert!(loading.contains(&retry.origin) && loading.contains(&retry.bottom_right()));
            cx.simulate_click(retry.center(), gpui::Modifiers::default());
            editor.read_with(cx, |editor, _| {
                assert_eq!(editor.selected_byte_range().0, selection);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
            });
        }
        assert_eq!(*retries.lock().unwrap(), ["missing.png", "missing.png"]);
        editor.update_in(cx, |editor, window, cx| {
            editor.set_selection(0..0, false, window, cx)
        });
        cx.update(|window, cx| {
            _ = window.draw(cx);
            editor.read(cx).focus_handle.clone().focus(window, cx);
            window.focus_next(cx);
            _ = window.draw(cx);
        });
        for key in ["enter", "space"] {
            let keystroke = gpui::Keystroke::parse(key).unwrap();
            cx.simulate_event(gpui::KeyDownEvent {
                keystroke: keystroke.clone(),
                is_held: false,
                prefer_character_input: false,
            });
            cx.simulate_event(gpui::KeyUpEvent { keystroke });
        }
        assert_eq!(
            retries.lock().unwrap().len(),
            4,
            "keyboard Enter/Space activate Retry once each"
        );
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.selected_byte_range().0, 0..0);
            assert_eq!(editor.document.snapshot().serialize().unwrap(), SOURCE);
        });

        // Retry resets the existing cache to pending; the failed overlay must
        // disappear immediately and the delay starts again for this same id.
        cache.update(cx, |cache, _| cache.result = None);
        editor.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("image-resource-state").is_none());
        std::thread::sleep(Duration::from_millis(250));
        cx.executor().advance_clock(Duration::from_millis(250));
        editor.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert_eq!(cx.debug_bounds("image-resource-state").unwrap(), loading);
        assert!(cx.debug_bounds("retry-image-command").is_none());

        // The toolkit accepts an empty-frame ready image (covered upstream).
        // Native fixture 103 separately verifies actual decoded SVG pixels.
        cache.update(cx, |cache, _| {
            cache.result = Some(Ok(Arc::new(gpui::RenderImage::new(smallvec::smallvec![]))))
        });
        editor.update(cx, |_, cx| cx.notify());
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        assert!(cx.debug_bounds("image-resource-state").is_none());
        assert!(cx.debug_bounds("retry-image-command").is_none());

        let node = editor.read_with(cx, |editor, _| {
            editor
                .projection
                .segments()
                .iter()
                .find(|s| s.context.image_source.is_some())
                .unwrap()
                .node_id
        });
        editor.update(cx, |editor, cx| {
            editor.retry_image_source(node, "obsolete.png", cx)
        });
        assert_eq!(
            retries.lock().unwrap().len(),
            4,
            "stale source cannot emit a retry"
        );
    }

    #[test]
    fn destination_previews_omit_remote_secrets() {
        for (source, expected) in [
            ("figures/route map.png", "route map.png"),
            (
                "https://user:password@example.test/route.png?token=secret#private",
                "route.png",
            ),
            (
                "https://user:password@example.test?token=secret",
                "Remote image",
            ),
            ("data:image/png;base64,secret", "Embedded image"),
            ("DATA:image/png;base64,secret", "Embedded image"),
            (
                "https://user:password@example.test?token=secret/private.png",
                "Remote image",
            ),
            (
                "//user:password@example.test#private/image.png",
                "Remote image",
            ),
            ("", "Image"),
        ] {
            assert_eq!(destination_label(source), expected);
        }
    }
}
