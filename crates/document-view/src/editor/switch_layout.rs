//! Document switches: the provisional layout a newly installed document
//! starts with is held back until its first measured reflow, and that reflow's
//! result is kept for recently viewed documents so switching back installs it
//! at once. Later reflows (edits, resize, zoom, scroll, focus) neither hold nor
//! touch this cache.
use super::*;
use crate::projection::ProjectionOffset;
use std::collections::VecDeque;

/// How long a switch may hide its provisional 760 px layout while the first
/// measured reflow prepares. A slower document shows the placeholder instead,
/// so a very large file never looks stuck behind a blank page.
pub(super) const PRESENTATION_HOLD_BUDGET: Duration = Duration::from_millis(250);
const CAPACITY: usize = 8;

#[derive(Clone, Copy)]
pub(super) struct Pending {
    pub content: u64,
    pub generation: u64,
}

/// Every input of a document's first measured reflow. The previous plan is
/// the provisional one, itself a function of `content`; resource generation
/// is global, so a load elsewhere conservatively misses.
pub(super) struct Key {
    content: u64,
    directory: Option<PathBuf>,
    measurement: Arc<FontMeasurement>,
    width: u32,
    height: u32,
    zoom: u32,
    resources: u64,
    visible_roots: Option<Range<usize>>,
    preview_edit_node: Option<NodeId>,
    expanded_code_tail: Option<NodeId>,
    html_disclosures: Arc<rustc_hash::FxHashMap<NodeId, crate::html::DisclosureState>>,
}

impl PartialEq for Key {
    fn eq(&self, other: &Self) -> bool {
        // The entry holds its measurement, so its address cannot be reused.
        Arc::ptr_eq(&self.measurement, &other.measurement)
            && self.content == other.content
            && self.directory == other.directory
            && self.width == other.width
            && self.height == other.height
            && self.zoom == other.zoom
            && self.resources == other.resources
            && self.visible_roots == other.visible_roots
            && self.preview_edit_node == other.preview_edit_node
            && self.expanded_code_tail == other.expanded_code_tail
            && self.html_disclosures == other.html_disclosures
    }
}

struct Entry {
    key: Key,
    layout: PreparedDocumentView,
    images: NodeImageDimensions,
}

/// Newest first, bounded by entry count. Entries share no mutable projection
/// coordinates with any editor; each hit installs its own detached copy.
#[derive(Default)]
pub(super) struct Cache(VecDeque<Entry>);

impl Cache {
    pub fn take(&mut self, key: &Key) -> Option<ReflowOutput> {
        let index = self.0.iter().position(|entry| entry.key == *key)?;
        let entry = self.0.remove(index)?;
        let layout = detach_view(&entry.layout)?;
        let images = entry.images.clone();
        self.0.push_front(entry);
        Some((layout, images, None))
    }

    pub fn insert(&mut self, key: Key, layout: PreparedDocumentView, images: NodeImageDimensions) {
        self.0.retain(|entry| entry.key != key);
        self.0.push_front(Entry {
            key,
            layout,
            images,
        });
        self.0.truncate(CAPACITY);
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// Serialized content plus canonical node IDs and revisions: a fresh parse of
/// the same file reproduces all three, an edited session does not.
pub(super) fn content_fingerprint(
    snapshot: &DocumentSnapshot,
    projection: &TextProjection,
) -> Option<u64> {
    let mut hasher = DefaultHasher::new();
    snapshot.serialize().ok()?.hash(&mut hasher);
    for root in projection.roots() {
        root.id().hash(&mut hasher);
        projection.node_revision(root.id()).hash(&mut hasher);
    }
    for segment in projection.segments() {
        segment.node_id.hash(&mut hasher);
        segment.top_level_node_id.hash(&mut hasher);
        projection.node_revision(segment.node_id).hash(&mut hasher);
    }
    Some(hasher.finish())
}

/// A copy that shares no offset chunks with `view`. Edits rebase a
/// projection's chunks in place, so lines are rebound to the copy's own.
pub(super) fn detach_view(view: &PreparedDocumentView) -> Option<PreparedDocumentView> {
    let projection = view.projection.clone();
    let mut chunks = HashMap::<*const ProjectionOffset, Arc<ProjectionOffset>>::new();
    for (old, new) in view.projection.segments().iter().zip(projection.segments()) {
        if old.projection_local_start() != new.projection_local_start() {
            return None;
        }
        let rebound = chunks
            .entry(Arc::as_ptr(&old.projection_start))
            .or_insert_with(|| new.projection_start.clone());
        if !Arc::ptr_eq(rebound, &new.projection_start) {
            return None;
        }
    }
    let visual_lines = view
        .visual_lines
        .iter()
        .map(|line| {
            let mut line = line.clone();
            line.source.projection_start = chunks
                .get(&Arc::as_ptr(&line.source.projection_start))?
                .clone();
            Some(line)
        })
        .collect::<Option<Vec<_>>>()?;
    Some(PreparedDocumentView {
        projection,
        visual_lines: Arc::new(visual_lines),
        paint_order: view.paint_order.clone(),
        document_height: view.document_height,
        adaptive: view.adaptive.clone(),
        components: view.components.clone(),
        // Only a reuse hint for the next worker, bound to the original lines.
        published_geometry: None,
        recovery: None,
        content: None,
    })
}

impl RichDocumentEditor {
    /// The cache key for a reflow that would be this document's first
    /// measured one, or `None` when it is not or depends on transient state.
    pub(super) fn switch_layout_key(
        &self,
        width: f32,
        height: f32,
        resources: u64,
        visible_roots: Option<Range<usize>>,
    ) -> Option<Key> {
        let pending = self.switch_layout?;
        (pending.generation == self.document.generation()
            && self.layout_focus.is_none()
            && self.projection.table_layout_lock.is_none()
            && self.published_geometry.is_none()
            && self.html_image_cache.loaded.is_empty())
        .then(|| Key {
            content: pending.content,
            directory: self.document_directory.clone(),
            measurement: self.measurement.clone(),
            width: width.to_bits(),
            height: height.to_bits(),
            zoom: self.zoom_factor.to_bits(),
            resources,
            visible_roots,
            preview_edit_node: self.projection.preview_edit_node,
            expanded_code_tail: self.projection.expanded_code_tail,
            html_disclosures: self.projection.html_disclosures.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{cell::Cell, rc::Rc};

    const FIRST: &str = "# Field notes\n\nAn opening paragraph long enough to wrap at a measured width, with a second sentence for good measure.\n\n## Records\n\n- **Owner:** Ada\n- **State:** Draft\n\n| Name | Value |\n| --- | --- |\n| Alpha | 1 |\n| Beta | 2 |\n";
    const SECOND: &str = "# Another document\n\nShort and different.\n";

    fn draw(cx: &mut gpui::VisualTestContext) {
        cx.update(|window, cx| {
            _ = window.draw(cx);
        });
        cx.run_until_parked();
    }

    fn open<'a>(
        cx: &'a mut gpui::TestAppContext,
        source: &str,
    ) -> (
        Entity<RichDocumentEditor>,
        &'a mut gpui::VisualTestContext,
        Rc<Cell<usize>>,
    ) {
        cx.update(crate::init_editor);
        let (editor, cx) = cx.add_window_view(|window, cx| {
            RichDocumentEditor::new(Document::from_markdown(source).unwrap(), window, cx)
        });
        let ready = Rc::new(Cell::new(0));
        cx.update(|_, cx| {
            let ready = ready.clone();
            cx.subscribe(&editor, move |_, event: &EditorEvent, _| {
                if matches!(event, EditorEvent::Ready) {
                    ready.set(ready.get() + 1);
                }
            })
            .detach();
        });
        for _ in 0..3 {
            draw(cx);
        }
        editor.read_with(cx, |editor, _| {
            assert!(editor.measured_layout && !editor.presentation_held());
            assert!(!editor.painted_lines.is_empty());
        });
        // The window's first frame may precede this subscription.
        ready.set(0);
        (editor, cx, ready)
    }

    fn switch_to(
        editor: &Entity<RichDocumentEditor>,
        cx: &mut gpui::VisualTestContext,
        source: &str,
    ) {
        cx.update(|_, cx| {
            editor.update(cx, |editor, cx| {
                editor.replace_document(Document::from_markdown(source).unwrap(), cx);
                assert!(editor.presentation_held(), "a switch starts held");
            })
        });
    }

    #[gpui::test]
    fn switch_paints_nothing_until_the_measured_layout_commits(cx: &mut gpui::TestAppContext) {
        let (editor, cx, ready) = open(cx, SECOND);
        let (release, hold) = futures::channel::oneshot::channel();
        cx.update(|_, cx| editor.update(cx, |editor, _| editor.reflow.hold_next = Some(hold)));
        switch_to(&editor, cx, FIRST);
        for _ in 0..2 {
            draw(cx);
        }
        editor.read_with(cx, |editor, _| {
            assert!(editor.reflow.is_active(), "the measured reflow is running");
            assert!(editor.presentation_held());
            assert!(!editor.measured_layout);
            assert!(
                editor.painted_lines.is_empty(),
                "placeholder text must not paint"
            );
        });
        assert_eq!(ready.get(), 0, "Ready waits for a frame with content");
        release.send(()).unwrap();
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(editor.measured_layout && !editor.presentation_held());
        });
        draw(cx);
        editor.read_with(cx, |editor, _| assert!(!editor.painted_lines.is_empty()));
        assert_eq!(ready.get(), 1);
    }

    #[gpui::test]
    fn switch_shows_the_placeholder_once_the_hold_budget_expires(cx: &mut gpui::TestAppContext) {
        let (editor, cx, ready) = open(cx, SECOND);
        let (release, hold) = futures::channel::oneshot::channel();
        cx.update(|_, cx| editor.update(cx, |editor, _| editor.reflow.hold_next = Some(hold)));
        switch_to(&editor, cx, FIRST);
        draw(cx);
        cx.background_executor
            .advance_clock(PRESENTATION_HOLD_BUDGET - Duration::from_millis(1));
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| assert!(editor.presentation_held()));
        cx.background_executor
            .advance_clock(Duration::from_millis(1));
        cx.run_until_parked();
        draw(cx);
        editor.read_with(cx, |editor, _| {
            assert!(!editor.presentation_held());
            assert!(!editor.measured_layout, "the placeholder is what is shown");
            assert!(editor.reflow.is_active());
            assert!(!editor.painted_lines.is_empty());
        });
        assert_eq!(ready.get(), 1);
        release.send(()).unwrap();
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| assert!(editor.measured_layout));
    }

    #[gpui::test]
    fn failed_or_expired_reflow_ends_the_hold(cx: &mut gpui::TestAppContext) {
        let (editor, cx, _) = open(cx, SECOND);
        cx.update(|_, cx| editor.update(cx, |editor, _| editor.reflow.fail_next = true));
        switch_to(&editor, cx, FIRST);
        draw(cx);
        editor.read_with(cx, |editor, _| {
            assert!(
                editor
                    .last_error()
                    .is_some_and(|e| e.starts_with("Layout update failed"))
            );
            assert!(
                !editor.presentation_held(),
                "a failure shows what is installed"
            );
            assert!(!editor.measured_layout);
        });
        draw(cx);
        editor.read_with(cx, |editor, _| assert!(!editor.painted_lines.is_empty()));

        let (release, hold) = futures::channel::oneshot::channel();
        cx.update(|_, cx| editor.update(cx, |editor, _| editor.reflow.hold_next = Some(hold)));
        switch_to(&editor, cx, SECOND);
        draw(cx);
        // The budget alone would end this hold; re-arm it to observe expiry.
        cx.background_executor
            .advance_clock(reflow::TIMEOUT - Duration::from_millis(1));
        cx.run_until_parked();
        cx.update(|_, cx| {
            editor.update(cx, |editor, _| {
                assert!(editor.reflow.is_active());
                editor.presentation_hold = Some(Task::ready(()));
            })
        });
        cx.background_executor
            .advance_clock(Duration::from_millis(2));
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(editor.last_error().is_some_and(|e| e.contains("timed out")));
            assert!(!editor.presentation_held());
        });
        release.send(()).unwrap();
        cx.run_until_parked();
    }

    #[gpui::test]
    fn edits_resizes_and_zoom_never_hold(cx: &mut gpui::TestAppContext) {
        let (editor, cx, _) = open(cx, SECOND);
        switch_to(&editor, cx, FIRST);
        draw(cx);
        draw(cx);
        let shown = |cx: &mut gpui::VisualTestContext| {
            editor.read_with(cx, |editor, _| {
                assert!(!editor.presentation_held());
                assert!(!editor.painted_lines.is_empty());
            });
        };
        shown(cx);
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.replace_text_in_range(None, "x", window, cx);
                assert!(!editor.presentation_held());
            })
        });
        draw(cx);
        shown(cx);
        cx.simulate_resize(size(px(420.), px(700.)));
        draw(cx);
        shown(cx);
        cx.update(|_, cx| editor.update(cx, |editor, cx| editor.zoom_in(cx)));
        draw(cx);
        shown(cx);
        cx.background_executor.advance_clock(Duration::from_secs(1));
        draw(cx);
        draw(cx);
        shown(cx);
    }

    #[gpui::test]
    fn switching_back_installs_the_cached_measured_layout(cx: &mut gpui::TestAppContext) {
        let (editor, cx, ready) = open(cx, SECOND);
        switch_to(&editor, cx, FIRST);
        draw(cx);
        let (fresh_lines, fresh_plan, fresh_height) = editor.read_with(cx, |editor, _| {
            assert!(editor.measured_layout && !editor.presentation_held());
            assert_eq!(editor.measured_layouts.len(), 1);
            (
                editor.visual_lines.clone(),
                editor.adaptive.clone(),
                editor.document_height,
            )
        });
        let back = |cx: &mut gpui::VisualTestContext| {
            switch_to(&editor, cx, SECOND);
            draw(cx);
            switch_to(&editor, cx, FIRST);
            // One frame: the cache commits during render, before paint.
            cx.update(|window, cx| {
                _ = window.draw(cx);
            });
            editor.read_with(cx, |editor, _| {
                assert!(!editor.presentation_held(), "a cache hit never holds");
                assert!(editor.measured_layout);
                assert!(!editor.reflow.is_active(), "no worker was needed");
                assert!(!editor.painted_lines.is_empty());
                assert!(!Arc::ptr_eq(&editor.visual_lines, &fresh_lines));
                arrangement::tests::assert_same_geometry(&editor.visual_lines, &fresh_lines);
                assert!(fresh_plan.geometry_key().matches(&editor.adaptive));
                assert_eq!(editor.adaptive.windows, fresh_plan.windows);
                assert_eq!(
                    editor.adaptive.measurement_ranges,
                    fresh_plan.measurement_ranges
                );
                assert_eq!(
                    editor.adaptive.resource_generation,
                    fresh_plan.resource_generation
                );
                assert_eq!(editor.document_height, fresh_height);
                // Installed lines read the editor's own projection chunks.
                for line in editor.visual_lines.iter() {
                    assert!(
                        editor
                            .projection
                            .segments()
                            .iter()
                            .any(|segment| Arc::ptr_eq(
                                &segment.projection_start,
                                &line.source.projection_start
                            ))
                    );
                }
            });
        };
        back(cx);
        let ready_after_hit = ready.get();
        // Edit the installed copy; the cached entry must stay pristine.
        cx.update(|window, cx| {
            editor.update(cx, |editor, cx| {
                editor.replace_text_in_range(Some(0..0), "Edited ", window, cx);
            })
        });
        cx.background_executor.advance_clock(Duration::from_secs(1));
        draw(cx);
        back(cx);
        assert!(ready.get() > ready_after_hit);

        // Changed content misses.
        let (release, hold) = futures::channel::oneshot::channel();
        cx.update(|_, cx| editor.update(cx, |editor, _| editor.reflow.hold_next = Some(hold)));
        switch_to(&editor, cx, &FIRST.replace("Alpha", "Gamma"));
        draw(cx);
        editor.read_with(cx, |editor, _| {
            assert!(editor.presentation_held() && editor.reflow.is_active());
        });
        release.send(()).unwrap();
        cx.run_until_parked();
        // A changed width misses too.
        switch_to(&editor, cx, SECOND);
        draw(cx);
        cx.simulate_resize(size(px(500.), px(700.)));
        draw(cx);
        let (release, hold) = futures::channel::oneshot::channel();
        cx.update(|_, cx| editor.update(cx, |editor, _| editor.reflow.hold_next = Some(hold)));
        switch_to(&editor, cx, FIRST);
        draw(cx);
        editor.read_with(cx, |editor, _| {
            assert!(editor.presentation_held() && editor.reflow.is_active());
        });
        release.send(()).unwrap();
        cx.run_until_parked();
        editor.read_with(cx, |editor, _| {
            assert!(editor.measured_layout && !editor.presentation_held());
            assert!(editor.measured_layouts.len() <= CAPACITY);
        });
    }

    #[gpui::test]
    fn cache_is_bounded_and_evicts_the_least_recently_used(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            let measurement = Arc::new(FontMeasurement::new(
                cx.text_system().clone(),
                "Public Sans Tachyon".into(),
                1.,
            ));
            let key = |content| Key {
                content,
                directory: None,
                measurement: measurement.clone(),
                width: 900_f32.to_bits(),
                height: 700_f32.to_bits(),
                zoom: 1_f32.to_bits(),
                resources: 0,
                visible_roots: None,
                preview_edit_node: None,
                expanded_code_tail: None,
                html_disclosures: Arc::default(),
            };
            let layout =
                || PreparedDocumentView::prepare(&Document::from_markdown(SECOND).unwrap());
            let mut cache = Cache::default();
            for content in 0..CAPACITY as u64 {
                cache.insert(key(content), layout(), HashMap::new());
            }
            assert!(cache.take(&key(0)).is_some(), "a hit refreshes recency");
            cache.insert(key(100), layout(), HashMap::new());
            assert_eq!(cache.len(), CAPACITY);
            assert!(
                cache.take(&key(1)).is_none(),
                "the oldest entry was evicted"
            );
            assert!(cache.take(&key(0)).is_some());
            assert!(cache.take(&key(100)).is_some());
            let other = Arc::new(FontMeasurement::new(
                cx.text_system().clone(),
                "Public Sans Tachyon".into(),
                1.,
            ));
            assert!(
                cache
                    .take(&Key {
                        measurement: other,
                        ..key(0)
                    })
                    .is_none(),
                "another measurement environment misses"
            );
        });
    }
}
