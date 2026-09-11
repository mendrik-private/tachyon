//! Visible resource requests go through the app image owner. Only already
//! decoded pixels cross to the bounded background conversion/layout path.
use super::*;
use blitz_dom::node::RasterImageData;
use std::collections::{HashSet, VecDeque};

pub(super) type LoadedImages = Arc<HashMap<u64, RasterImageData>>;
const MAX_IMAGE_BYTES: usize = 32 * 1024 * 1024;
const MAX_CACHE_BYTES: usize = 64 * 1024 * 1024;
const MAX_ENTRIES: usize = 128;

#[derive(Default)]
pub(super) struct HtmlImageCache {
    pub loaded: LoadedImages,
    active: HashSet<u64>,
    rejected: HashSet<u64>,
    recency: VecDeque<u64>,
    task: Option<Task<()>>,
}

impl HtmlImageCache {
    fn insert(&mut self, key: u64, image: RasterImageData) -> bool {
        let bytes = image.data.data().len();
        let active_bytes: usize = self
            .loaded
            .iter()
            .filter(|(id, _)| self.active.contains(id))
            .map(|(_, image)| image.data.data().len())
            .sum();
        if active_bytes.saturating_add(bytes) > MAX_CACHE_BYTES {
            return false;
        }
        let mut total: usize = self
            .loaded
            .values()
            .map(|image| image.data.data().len())
            .sum();
        while total + bytes > MAX_CACHE_BYTES || self.loaded.len() >= MAX_ENTRIES {
            let Some(index) = self
                .recency
                .iter()
                .rposition(|id| !self.active.contains(id))
            else {
                return false;
            };
            let old = self.recency.remove(index).unwrap();
            if let Some(image) = Arc::make_mut(&mut self.loaded).remove(&old) {
                total -= image.data.data().len();
            }
        }
        Arc::make_mut(&mut self.loaded).insert(key, image);
        self.recency.retain(|id| *id != key);
        self.recency.push_front(key);
        true
    }
}

/// Exact packed straight-alpha BGRA8 -> RGBA8 shuffle. No color transform,
/// resampling, alpha approximation or second image decode is performed.
fn rgba(width: u32, height: u32, bytes: &[u8]) -> Option<RasterImageData> {
    let length = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    if width == 0
        || height == 0
        || width > 16384
        || height > 16384
        || length > MAX_IMAGE_BYTES
        || length != bytes.len()
    {
        return None;
    }
    let mut rgba = bytes.to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Some(RasterImageData::new(width, height, Arc::new(rgba)))
}

pub(super) fn bind(
    projection: &mut TextProjection,
    loaded: &LoadedImages,
    directory: Option<&std::path::Path>,
) {
    let mut bindings = rustc_hash::FxHashMap::default();
    for (&node, references) in &projection.html_image_references {
        let Some(BlockNode::PreservedSource { source, .. }) = projection.block(node) else {
            continue;
        };
        let resources = references
            .iter()
            .filter_map(|reference| {
                let key = hash(&resolved_image_resource(&reference.source, directory));
                Some((reference.source.clone(), loaded.get(&key)?.clone()))
            })
            .collect::<crate::html::images::ImageResources>();
        if resources.is_empty() {
            continue;
        }
        let key = references
            .iter()
            .filter_map(|reference| {
                let image = resources.get(&reference.source)?;
                Some((
                    reference.source.clone(),
                    image.width,
                    image.height,
                    image.data.id(),
                ))
            })
            .collect();
        bindings.insert(
            node,
            crate::html::images::BoundImages {
                source: source.clone(),
                resources,
                key,
            },
        );
    }
    projection.html_images = Arc::new(bindings);
}

impl RichDocumentEditor {
    pub fn set_image_cache(&mut self, cache: gpui::AnyImageCache) {
        self.html_image_loader = Some(cache);
        self.shared_image_dimensions
            .get_or_insert_with(|| Arc::new(std::sync::Mutex::new((0, HashMap::new()))));
    }

    /// Returns the unique authored image resources for bounded background
    /// decoding. Visible painting remains responsible for GPU atlas uploads.
    #[must_use]
    pub fn document_image_resources(&self) -> Vec<Resource> {
        const MAX_PREFETCH_RESOURCES: usize = 32;
        let mut seen = rustc_hash::FxHashSet::default();
        self.projection
            .image_segments()
            .filter_map(|segment| segment.context.image_source.as_deref())
            .chain(
                self.projection
                    .html_image_references
                    .values()
                    .flatten()
                    .map(|reference| reference.source.as_str()),
            )
            .map(|source| resolved_image_resource(source, self.document_directory.as_deref()))
            // Avoid speculative network traffic and bound retained decoded
            // pixels for image-heavy documents.
            .filter(|resource| matches!(resource, Resource::Path(_)))
            .filter(|resource| seen.insert(hash(resource)))
            .take(MAX_PREFETCH_RESOURCES)
            .collect()
    }

    pub(super) fn request_html_images(
        &mut self,
        visible: &[usize],
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.html_image_loader.is_none() || self.projection.html_image_references.is_empty() {
            return;
        }
        let mut requests = Vec::new();
        let mut keys = HashSet::new();
        for &index in visible {
            let Some(segment) = segment_for_line(
                &self.projection,
                &self.visual_lines[index].projected_range(),
            ) else {
                continue;
            };
            let Some(references) = self.projection.html_image_references.get(&segment.node_id)
            else {
                continue;
            };
            for reference in references {
                if requests.len() >= MAX_ENTRIES {
                    break;
                }
                let resource =
                    resolved_image_resource(&reference.source, self.document_directory.as_deref());
                let key = hash(&resource);
                if keys.insert(key) {
                    requests.push((key, resource));
                }
            }
        }
        self.html_image_cache.active = keys;
        self.html_image_cache
            .rejected
            .retain(|key| self.html_image_cache.active.contains(key));
        if self.html_image_cache.task.is_some() {
            return;
        }
        for (key, resource) in requests {
            if self.html_image_cache.loaded.contains_key(&key)
                || self.html_image_cache.rejected.contains(&key)
            {
                continue;
            }
            let Some(result) = self
                .html_image_loader
                .as_ref()
                .unwrap()
                .load(&resource, window, cx)
            else {
                continue;
            };
            let Ok(image) = result else {
                self.html_image_cache.rejected.insert(key);
                continue;
            };
            // Static Blitz previews cannot silently replace animated media by
            // one frame. Such media keeps complete source/edit conversion.
            if image.frame_count() != 1
                || image
                    .as_bytes(0)
                    .is_none_or(|bytes| bytes.len() > MAX_IMAGE_BYTES)
            {
                self.html_image_cache.rejected.insert(key);
                continue;
            }
            let worker = cx
                .background_executor()
                .spawn_dedicated(move |_| async move {
                    let size = image.size(0);
                    rgba(
                        u32::try_from(size.width.0).ok()?,
                        u32::try_from(size.height.0).ok()?,
                        image.as_bytes(0)?,
                    )
                });
            self.html_image_cache.task = Some(cx.spawn(async move |this, cx| {
                let raster = worker.await;
                _ = this.update(cx, |this, cx| {
                    this.html_image_cache.task = None;
                    if raster.is_some_and(|raster| this.html_image_cache.insert(key, raster)) {
                        if let Some(shared) = &this.shared_image_dimensions {
                            let mut state = shared.lock().unwrap_or_else(|p| p.into_inner());
                            state.0 = state.0.wrapping_add(1);
                        }
                    } else {
                        this.html_image_cache.rejected.insert(key);
                    }
                    cx.notify();
                });
            }));
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    fn html_conversion_rebinds_ready_dimensions_to_new_figure_ids(cx: &mut gpui::TestAppContext) {
        cx.update(crate::init_editor);
        let source = "<div><img src=\"first.png\" alt=\"First\"><img src=\"second.png\" alt=\"Second\"></div>\n";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                let directory = std::path::PathBuf::from("/synthetic/figures");
                let ready = [("first.png", (960, 360)), ("second.png", (200, 400))]
                    .into_iter()
                    .map(|(source, size)| (hash(&resolved_image_resource(source, Some(&directory))), size))
                    .collect();
                editor.document_directory = Some(directory);
                let dimensions = Arc::new(Mutex::new((7, ready)));
                editor.set_image_dimensions(dimensions.clone(), cx);
                let node_id = editor.projection.roots().next().unwrap().id();
                editor.apply_structural_command(EditCommand::ConvertHtmlToMarkdown { node_id }, window, cx);
                let figures = editor.projection.image_segments().map(|s| s.node_id).collect::<Vec<_>>();
                assert_eq!(figures.len(), 2);
                for (id, expected) in figures.iter().zip([(960, 360), (200, 400)]) {
                    assert_eq!(editor.image_layout_dimensions.get(id).map(|(_, size)| *size), Some(expected),
                        "converted figures must reuse cached dimensions without a new resource arrival");
                }
                assert_eq!(dimensions.lock().unwrap().0, 7, "rebinding is not a resource arrival");
                assert!(editor.adaptive.pending_images.is_empty(), "the planner must also see ready dimensions");
                editor.undo(&Undo, window, cx);
                assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
                assert!(editor.image_layout_dimensions.is_empty(), "undo must remove obsolete figure bindings");
                editor.redo(&Redo, window, cx);
                assert_eq!(editor.image_layout_dimensions.len(), 2);
                assert!(editor.adaptive.pending_images.is_empty());
                // A document-relative name must not borrow another folder's
                // decoded dimensions, even when its spelling is identical.
                editor.document_directory = Some(std::path::PathBuf::from("/synthetic/elsewhere"));
                editor.refresh_projection();
                assert!(editor.image_layout_dimensions.is_empty());
                assert_eq!(editor.adaptive.pending_images.len(), 2);
            });
        });
    }

    struct DeferredCache {
        ready: bool,
        requests: Vec<gpui::Resource>,
        image: Arc<gpui::RenderImage>,
    }

    impl gpui::ImageCache for DeferredCache {
        fn load(
            &mut self,
            resource: &gpui::Resource,
            _window: &mut Window,
            _cx: &mut gpui::App,
        ) -> Option<Result<Arc<gpui::RenderImage>, gpui::ImageCacheError>> {
            self.requests.push(resource.clone());
            if !self.ready {
                return None;
            }
            Some(Ok(self.image.clone()))
        }
    }

    #[gpui::test]
    fn deferred_host_loader_publishes_exact_pixels_without_dirtying_the_document(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init_editor);
        let source = "<div><p>Before.</p><img src='local.png' alt='Local'></div>\n\n<div><img src='https://example.test/denied.png'></div>";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let mut bytes = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut bytes, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&[9, 21, 200, 128])
                .unwrap();
        }
        let loader = cx.new(|cx| DeferredCache {
            ready: false,
            requests: Vec::new(),
            image: gpui::Image::from_bytes(gpui::ImageFormat::Png, bytes)
                .to_image_data(cx.svg_renderer())
                .unwrap(),
        });
        editor.update(cx, |editor, _| {
            editor.set_image_cache(loader.clone().into())
        });
        let request = |cx: &mut gpui::VisualTestContext| {
            cx.update(|window, cx| {
                editor.update(cx, |editor, cx| {
                    let lines = (0..editor.visual_lines.len()).collect::<Vec<_>>();
                    editor.request_html_images(&lines, window, cx);
                })
            })
        };
        request(cx);
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(editor.html_image_cache.loaded.is_empty());
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
        });
        loader.update(cx, |loader, _| loader.ready = true);
        for _ in 0..4 {
            request(cx);
            cx.run_until_parked();
        }
        loader.read_with(cx, |loader, _| {
            assert!(!loader.requests.is_empty());
            assert!(
                loader
                    .requests
                    .iter()
                    .all(|resource| *resource == Resource::Path(PathBuf::from("local.png").into())),
                "HTML remote references must never reach the host loader"
            );
        });
        editor.read_with(cx, |editor, _| {
            assert_eq!(editor.html_image_cache.loaded.len(), 1);
            assert_eq!(
                editor
                    .html_image_cache
                    .loaded
                    .values()
                    .next()
                    .unwrap()
                    .data
                    .data(),
                &[9, 21, 200, 128]
            );
            assert!(editor.html_image_cache.task.is_none());
            assert_eq!(editor.document.snapshot().serialize().unwrap(), source);
            assert!(matches!(
                editor.document.undo(),
                Err(DocumentError::NothingToUndo)
            ));
        });
    }

    #[gpui::test]
    fn image_arrival_rebuilds_real_geometry_and_keeps_full_missing_fallback(
        cx: &mut gpui::TestAppContext,
    ) {
        cx.update(crate::init_editor);
        let source = "<div><p>Before.</p><img src='chart.png' width='160' height='80' alt='Chart'><p>After.</p></div>\n\n# Later\n\nUnchanged.";
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        editor.update(cx, |editor, _| {
            let snapshot = editor.document.snapshot();
            let prepare = |loaded, previous: &PreparedDocumentView| {
                PreparedDocumentView::prepare_snapshot_with_images(
                    &snapshot,
                    &HashMap::new(),
                    None,
                    ReflowViewport {
                        published_geometry: previous.published_geometry.clone(),
                        width: 760.,
                        height: 1000.,
                        zoom: 1.,
                        preview_edit_node: None,
                        expanded_code_tail: None,
                        editing_node: None,
                        table_layout_lock: None,
                        html_disclosures: Arc::default(),
                        html_loaded_images: loaded,
                        trace_mode: LayoutTraceMode::Off,
                        visible_roots: None,
                        resource_generation: 0,
                    },
                    &previous.adaptive,
                    &editor.measurement,
                )
                .0
            };
            let initial = PreparedDocumentView::prepare(&Document::from_markdown(source).unwrap());
            let missing = prepare(Arc::default(), &initial);
            assert!(
                missing
                    .visual_lines
                    .iter()
                    .all(|line| line.html_preview.is_none())
            );
            let loaded = Arc::new(HashMap::from([(
                hash(&resolved_image_resource("chart.png", None)),
                rgba(2, 1, &[0, 0, 255, 255, 0, 0, 255, 255]).unwrap(),
            )]));
            let ready = prepare(loaded.clone(), &missing);
            let preview = ready
                .visual_lines
                .iter()
                .find_map(|line| line.html_preview.as_ref())
                .unwrap();
            assert!(preview.height >= 80.);
            assert!(preview.accessible_text.contains("Chart"));
            assert!(!Arc::ptr_eq(&ready.visual_lines, &missing.visual_lines));
            let repeated = prepare(loaded, &ready);
            assert!(Arc::ptr_eq(&ready.visual_lines, &repeated.visual_lines));
            assert_eq!(snapshot.serialize().unwrap(), source);
        });
    }

    #[test]
    fn bgra_handoff_preserves_every_channel_and_straight_alpha() {
        let input = [0, 0, 255, 255, 250, 17, 8, 128, 201, 91, 2, 0];
        let image = rgba(3, 1, &input).unwrap();
        assert_eq!(
            image.data.data(),
            &[255, 0, 0, 255, 8, 17, 250, 128, 2, 91, 201, 0]
        );
        assert_eq!(input[0], 0, "shared decoder pixels remain untouched");
        assert!(rgba(3, 1, &input[..11]).is_none());
        assert!(rgba(0, 1, &[]).is_none());
        assert!(rgba(u32::MAX, u32::MAX, &[]).is_none());
    }

    #[test]
    fn loaded_html_resources_are_directory_bound_and_invalidate_geometry() {
        let doc = Document::from_markdown(
            "<div><img src='chart.png' alt='Chart' width='80' height='40'></div>",
        )
        .unwrap();
        let snapshot = doc.snapshot();
        let mut projection = TextProjection::from_snapshot(&snapshot);
        let node = snapshot.blocks().iter().next().unwrap().id();
        let before = projection.geometry_key();
        let directory = std::path::Path::new("/synthetic/one");
        let loaded = Arc::new(HashMap::from([(
            hash(&resolved_image_resource("chart.png", Some(directory))),
            rgba(1, 1, &[0, 0, 255, 255]).unwrap(),
        )]));
        bind(&mut projection, &loaded, Some(directory));
        assert!(!before.matches(&projection));
        assert!(projection.html_images(node).is_some());
        let ready = projection.geometry_key();
        bind(&mut projection, &loaded, Some(directory));
        assert!(ready.matches(&projection));
        bind(
            &mut projection,
            &loaded,
            Some(std::path::Path::new("/synthetic/two")),
        );
        assert!(projection.html_images(node).is_none());
        assert!(!ready.matches(&projection));
        assert_eq!(
            snapshot.serialize().unwrap(),
            doc.snapshot().serialize().unwrap()
        );
    }

    #[test]
    fn html_raster_cache_is_bounded_without_evicting_visible_images() {
        let mut cache = HtmlImageCache::default();
        let pixels = Arc::new(vec![0; 2048 * 2048 * 4]);
        for key in 0..4 {
            assert!(cache.insert(key, RasterImageData::new(2048, 2048, pixels.clone())));
        }
        cache.active = (0..4).collect();
        assert!(!cache.insert(4, RasterImageData::new(1, 1, Arc::new(vec![0; 4]))));
        assert_eq!(cache.loaded.len(), 4);
        cache.active.remove(&0);
        assert!(cache.insert(4, RasterImageData::new(1, 1, Arc::new(vec![0; 4]))));
        assert!(!cache.loaded.contains_key(&0));
        assert!(cache.loaded.contains_key(&1));
        assert!(
            cache
                .loaded
                .values()
                .map(|image| image.data.data().len())
                .sum::<usize>()
                <= MAX_CACHE_BYTES
        );
    }
}
