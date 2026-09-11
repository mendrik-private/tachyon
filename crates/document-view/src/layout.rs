use std::{
    collections::{HashMap, HashSet},
    ops::Range,
    sync::{Arc, Mutex},
};

use document_core::{BlockNode, DocumentPosition, DocumentSnapshot, NodeId, Revision};

use crate::{HeightTree, HeightTreeError, Rect};

const LONG_BLOCK_FRAGMENT_BYTES: usize = 8 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LayoutBuildStatus {
    Complete,
    Cancelled,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FragmentId {
    pub node_id: NodeId,
    pub ordinal: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FragmentKind {
    Text,
    Heading(u8),
    ListItem,
    TableRow { row: usize, header: bool },
    Code,
    Image,
    BlockQuote,
    Alert,
    Footnote,
    ThematicBreak,
    PreservedSource,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct LayoutKey {
    pub node_id: NodeId,
    pub node_revision: Revision,
    pub font_instance: u64,
    pub width_bits: u32,
    pub scale_bits: u32,
}

impl LayoutKey {
    #[must_use]
    pub fn new(
        node_id: NodeId,
        node_revision: Revision,
        font_instance: u64,
        width: f32,
        scale: f32,
    ) -> Self {
        Self {
            node_id,
            node_revision,
            font_instance,
            width_bits: width.to_bits(),
            scale_bits: scale.to_bits(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct LayoutFragment {
    pub id: FragmentId,
    pub kind: FragmentKind,
    pub rect: Rect,
    pub text_range: Option<Range<usize>>,
    pub estimated: bool,
}

#[derive(Clone, Debug)]
pub struct PositionMapping {
    pub position: DocumentPosition,
    pub fragment: FragmentId,
    pub x: f32,
    pub y: f32,
}

#[derive(Clone, Debug)]
pub struct LayoutSummary {
    pub node_id: NodeId,
    pub revision: Revision,
    pub kind: FragmentKind,
    pub height: f32,
    pub line_count: usize,
    pub measured: bool,
}

#[derive(Clone, Debug)]
struct CachedNodeLayout {
    fragments: Arc<[CachedFragment]>,
}

#[derive(Clone, Debug)]
struct CachedFragment {
    kind: FragmentKind,
    text_range: Option<Range<usize>>,
    height: f32,
    estimated: bool,
}

#[derive(Clone, Debug)]
pub struct LayoutIndex {
    fragments: Vec<LayoutFragment>,
    heights: HeightTree,
    fragment_lookup: HashMap<FragmentId, usize>,
    cache: Arc<Mutex<HashMap<LayoutKey, CachedNodeLayout>>>,
    content_width: f32,
    overlay_revision: u64,
}

impl Default for LayoutIndex {
    fn default() -> Self {
        Self {
            fragments: Vec::new(),
            heights: HeightTree::default(),
            fragment_lookup: HashMap::new(),
            cache: Arc::new(Mutex::new(HashMap::new())),
            content_width: 0.0,
            overlay_revision: 0,
        }
    }
}

impl LayoutIndex {
    pub fn rebuild(
        &mut self,
        snapshot: &DocumentSnapshot,
        content_width: f32,
        font_instance: u64,
        scale: f32,
    ) -> Result<(), HeightTreeError> {
        let status =
            self.rebuild_cancellable(snapshot, content_width, font_instance, scale, || false)?;
        debug_assert_eq!(status, LayoutBuildStatus::Complete);
        Ok(())
    }

    /// Builds an index without publishing partial geometry when a newer
    /// revision supersedes this work. The cancellation callback is sampled at
    /// block and long-fragment boundaries.
    pub fn rebuild_cancellable(
        &mut self,
        snapshot: &DocumentSnapshot,
        content_width: f32,
        font_instance: u64,
        scale: f32,
        mut cancelled: impl FnMut() -> bool,
    ) -> Result<LayoutBuildStatus, HeightTreeError> {
        let width = content_width.max(1.0);
        let scale = scale.max(0.25);
        let mut cached = Vec::new();
        let mut active_keys = HashSet::new();
        let mut cache = self.cache.lock().unwrap_or_else(|error| error.into_inner());
        {
            let mut builder = LayoutBuilder {
                snapshot,
                font_instance,
                scale,
                cache: &mut cache,
                active_keys: &mut active_keys,
                cancelled: &mut cancelled,
            };
            if !builder.flatten(snapshot.blocks(), width, &mut cached) {
                return Ok(LayoutBuildStatus::Cancelled);
            }
        }
        cache.retain(|key, _| active_keys.contains(key));
        drop(cache);
        if cancelled() {
            return Ok(LayoutBuildStatus::Cancelled);
        }

        let mut y = 0.0;
        let mut fragments = Vec::with_capacity(cached.len());
        let mut fragment_lookup = HashMap::with_capacity(cached.len());
        for (position, (id, fragment)) in cached.into_iter().enumerate() {
            if position % 256 == 0 && cancelled() {
                return Ok(LayoutBuildStatus::Cancelled);
            }
            let rect = Rect::new(0.0, y, width, fragment.height);
            let index = fragments.len();
            fragment_lookup.insert(id, index);
            fragments.push(LayoutFragment {
                id,
                kind: fragment.kind,
                rect,
                text_range: fragment.text_range,
                estimated: fragment.estimated,
            });
            y += fragment.height;
        }
        let heights = HeightTree::new(fragments.iter().map(|fragment| fragment.rect.size.height))?;
        if cancelled() {
            return Ok(LayoutBuildStatus::Cancelled);
        }
        self.fragments = fragments;
        self.fragment_lookup = fragment_lookup;
        self.heights = heights;
        self.content_width = width;
        Ok(LayoutBuildStatus::Complete)
    }

    /// Creates an empty index that reuses this index's bounded current-layout
    /// cache. Cloning the cache handle is constant-time on the UI thread.
    #[must_use]
    pub fn fresh_with_shared_cache(&self) -> Self {
        Self {
            cache: self.cache.clone(),
            ..Self::default()
        }
    }

    #[must_use]
    pub fn fragments(&self) -> impl ExactSizeIterator<Item = LayoutFragment> + '_ {
        self.fragments
            .iter()
            .enumerate()
            .map(|(index, fragment)| self.fragment_at(index, fragment))
    }

    #[must_use]
    pub fn total_height(&self) -> f32 {
        self.heights.total_height()
    }

    #[must_use]
    pub fn content_width(&self) -> f32 {
        self.content_width
    }

    #[must_use]
    pub fn index_at_y(&self, y: f32) -> Option<usize> {
        self.heights.index_at_offset(y)
    }

    #[must_use]
    pub fn fragment(&self, id: FragmentId) -> Option<LayoutFragment> {
        self.fragment_lookup.get(&id).and_then(|index| {
            self.fragments
                .get(*index)
                .map(|fragment| self.fragment_at(*index, fragment))
        })
    }

    pub fn update_fragment_height(
        &mut self,
        id: FragmentId,
        new_height: f32,
    ) -> Result<f32, HeightTreeError> {
        let Some(index) = self.fragment_lookup.get(&id).copied() else {
            return Err(HeightTreeError::OutOfBounds {
                index: self.fragments.len(),
                len: self.fragments.len(),
            });
        };
        let previous = self.fragments[index].rect.size.height;
        self.heights.update(index, new_height)?;
        self.fragments[index].rect.size.height = new_height;
        self.fragments[index].estimated = false;
        let delta = new_height - previous;
        Ok(delta)
    }

    fn fragment_at(&self, index: usize, fragment: &LayoutFragment) -> LayoutFragment {
        let mut fragment = fragment.clone();
        fragment.rect.origin.y = self.heights.prefix_sum(index);
        fragment
    }

    pub fn invalidate_nodes(&mut self, nodes: impl IntoIterator<Item = NodeId>) {
        let nodes: std::collections::HashSet<_> = nodes.into_iter().collect();
        self.cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .retain(|key, _| !nodes.contains(&key.node_id));
    }

    pub fn invalidate_overlays(&mut self) {
        self.overlay_revision = self.overlay_revision.wrapping_add(1);
    }

    #[must_use]
    pub fn overlay_revision(&self) -> u64 {
        self.overlay_revision
    }

    #[must_use]
    pub fn summaries(&self, revision: Revision) -> Vec<LayoutSummary> {
        self.fragments
            .iter()
            .map(|fragment| LayoutSummary {
                node_id: fragment.id.node_id,
                revision,
                kind: fragment.kind,
                height: fragment.rect.size.height,
                line_count: fragment
                    .text_range
                    .as_ref()
                    .map_or(1, |range| (range.len() / 80).max(1)),
                measured: !fragment.estimated,
            })
            .collect()
    }
}

struct LayoutBuilder<'a> {
    snapshot: &'a DocumentSnapshot,
    font_instance: u64,
    scale: f32,
    cache: &'a mut HashMap<LayoutKey, CachedNodeLayout>,
    active_keys: &'a mut HashSet<LayoutKey>,
    cancelled: &'a mut dyn FnMut() -> bool,
}

impl LayoutBuilder<'_> {
    fn flatten(
        &mut self,
        blocks: &document_core::BlockSequence,
        width: f32,
        output: &mut Vec<(FragmentId, CachedFragment)>,
    ) -> bool {
        for block in blocks {
            if (self.cancelled)() {
                return false;
            }
            let key = LayoutKey::new(
                block.id(),
                self.snapshot.node_revision(block.id()).unwrap_or_default(),
                self.font_instance,
                width,
                self.scale,
            );
            self.active_keys.insert(key);
            let layout = if let Some(layout) = self.cache.get(&key) {
                layout.clone()
            } else {
                let Some(layout) = layout_block(block, width, self.scale, self.cancelled) else {
                    return false;
                };
                self.cache.insert(key, layout.clone());
                layout
            };
            for (ordinal, fragment) in layout.fragments.iter().enumerate() {
                if ordinal % 256 == 0 && (self.cancelled)() {
                    return false;
                }
                output.push((
                    FragmentId {
                        node_id: block.id(),
                        ordinal: ordinal as u32,
                    },
                    fragment.clone(),
                ));
            }
            match block.as_ref() {
                BlockNode::List(list) => {
                    for item in list.items.iter() {
                        if !self.flatten(&item.blocks, (width - 24.0).max(1.0), output) {
                            return false;
                        }
                    }
                }
                BlockNode::BlockQuote { blocks, .. }
                | BlockNode::Alert { blocks, .. }
                | BlockNode::FootnoteDefinition { blocks, .. }
                    if !self.flatten(blocks, (width - 24.0).max(1.0), output) =>
                {
                    return false;
                }
                BlockNode::Definition { blocks, .. } if !self.flatten(blocks, width, output) => {
                    return false;
                }
                _ => {}
            }
        }
        true
    }
}

fn layout_block(
    block: &BlockNode,
    width: f32,
    scale: f32,
    cancelled: &mut dyn FnMut() -> bool,
) -> Option<CachedNodeLayout> {
    let line_height = 28.8 * scale;
    let average_glyph = 9.0 * scale;
    let chars_per_line = (width / average_glyph).max(8.0) as usize;
    let mut fragments = Vec::new();
    match block {
        BlockNode::Paragraph(paragraph) => fragments.extend(text_fragments(
            &paragraph.content.as_string(),
            FragmentKind::Text,
            chars_per_line,
            line_height,
            cancelled,
        )?),
        BlockNode::Heading(heading) => {
            let heading_height = match heading.level {
                1 => 42.84,
                2 => 35.84,
                3 => 31.2,
                4 => 26.4,
                5 => 22.8,
                _ => 20.4,
            } * scale;
            fragments.extend(text_fragments(
                &heading.content.as_string(),
                FragmentKind::Heading(heading.level),
                chars_per_line,
                heading_height,
                cancelled,
            )?);
        }
        BlockNode::CodeBlock(code) => fragments.extend(text_fragments(
            &code.content.as_string(),
            FragmentKind::Code,
            chars_per_line,
            22.5 * scale,
            cancelled,
        )?),
        BlockNode::Table(table) => {
            for (row, _) in table.rows.iter().enumerate() {
                if row % 256 == 0 && cancelled() {
                    return None;
                }
                fragments.push(CachedFragment {
                    kind: FragmentKind::TableRow {
                        row,
                        header: row < table.header_rows,
                    },
                    text_range: None,
                    height: 44.0 * scale,
                    estimated: true,
                });
            }
        }
        BlockNode::Image(image) => fragments.push(CachedFragment {
            kind: FragmentKind::Image,
            text_range: None,
            height: image
                .intrinsic_size
                .map_or(180.0 * scale, |(image_width, image_height)| {
                    width.min(image_width as f32 * scale) * image_height as f32
                        / image_width.max(1) as f32
                }),
            estimated: image.intrinsic_size.is_none(),
        }),
        BlockNode::List(_) => fragments.push(chrome(FragmentKind::ListItem, 4.0 * scale)),
        // Definition containers contribute no painted chrome; their editable
        // term and description descendants supply the complete geometry.
        BlockNode::Definition { .. } => {}
        BlockNode::BlockQuote { .. } => {
            fragments.push(chrome(FragmentKind::BlockQuote, 4.0 * scale));
        }
        BlockNode::Alert { .. } => fragments.push(chrome(FragmentKind::Alert, 8.0 * scale)),
        BlockNode::FootnoteDefinition { .. } => {
            fragments.push(chrome(FragmentKind::Footnote, 4.0 * scale));
        }
        BlockNode::ThematicBreak { .. } => {
            fragments.push(chrome(FragmentKind::ThematicBreak, 17.0 * scale));
        }
        BlockNode::PreservedSource { .. } => {
            fragments.push(chrome(FragmentKind::PreservedSource, 36.0 * scale));
        }
    }
    Some(CachedNodeLayout {
        fragments: fragments.into(),
    })
}

fn text_fragments(
    text: &str,
    kind: FragmentKind,
    chars_per_line: usize,
    line_height: f32,
    cancelled: &mut dyn FnMut() -> bool,
) -> Option<Vec<CachedFragment>> {
    if text.is_empty() {
        return Some(vec![CachedFragment {
            kind,
            text_range: Some(0..0),
            height: line_height,
            estimated: true,
        }]);
    }
    let mut fragments = Vec::new();
    let mut start = 0;
    while start < text.len() {
        if cancelled() {
            return None;
        }
        let mut end = (start + LONG_BLOCK_FRAGMENT_BYTES).min(text.len());
        while end < text.len() && !text.is_char_boundary(end) {
            end -= 1;
        }
        let slice = &text[start..end];
        let logical_lines = slice
            .lines()
            .map(|line| line.chars().count().div_ceil(chars_per_line).max(1))
            .sum::<usize>()
            .max(1);
        fragments.push(CachedFragment {
            kind,
            text_range: Some(start..end),
            height: logical_lines as f32 * line_height,
            estimated: true,
        });
        start = end;
    }
    Some(fragments)
}

fn chrome(kind: FragmentKind, height: f32) -> CachedFragment {
    CachedFragment {
        kind,
        text_range: None,
        height,
        estimated: false,
    }
}

#[cfg(test)]
mod tests {
    use document_core::Document;

    use super::*;

    #[test]
    fn giant_paragraph_is_subdivided() {
        let document = Document::from_markdown("a".repeat(20_000)).expect("document");
        let mut index = LayoutIndex::default();
        index
            .rebuild(&document.snapshot(), 600.0, 1, 1.0)
            .expect("layout");
        assert!(index.fragments().len() >= 3);
    }

    #[test]
    fn local_height_update_derives_following_origins_from_the_tree() {
        let document = Document::from_markdown("first\n\nsecond").expect("document");
        let mut index = LayoutIndex::default();
        index
            .rebuild(&document.snapshot(), 600.0, 1, 1.0)
            .expect("layout");
        let fragments = index.fragments().collect::<Vec<_>>();
        let first = fragments[0].id;
        let original_second_y = fragments[1].rect.origin.y;
        let new_height = fragments[0].rect.size.height + 20.;

        index
            .update_fragment_height(first, new_height)
            .expect("height update");
        let updated = index.fragments().collect::<Vec<_>>();
        assert_eq!(updated[1].rect.origin.y, original_second_y + 20.);
    }

    #[test]
    fn fresh_index_reuses_the_same_node_layout_cache() {
        let document = Document::from_markdown("first\n\nsecond").expect("document");
        let mut first = LayoutIndex::default();
        first
            .rebuild(&document.snapshot(), 600.0, 1, 1.0)
            .expect("layout");
        let entries = first
            .cache
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .len();
        let mut next = first.fresh_with_shared_cache();

        assert!(Arc::ptr_eq(&first.cache, &next.cache));
        next.rebuild(&document.snapshot(), 600.0, 1, 1.0)
            .expect("cached layout");
        assert_eq!(
            next.cache
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .len(),
            entries
        );
    }

    #[test]
    fn cancelled_rebuild_does_not_publish_partial_geometry() {
        let initial = Document::from_markdown("stable\n\ngeometry").expect("document");
        let replacement =
            Document::from_markdown("a".repeat(11 * 1024 * 1024)).expect("oversized document");
        let mut index = LayoutIndex::default();
        index
            .rebuild(&initial.snapshot(), 600.0, 1, 1.0)
            .expect("initial layout");
        let original = index
            .fragments()
            .map(|fragment| fragment.id)
            .collect::<Vec<_>>();
        let original_height = index.total_height();
        let mut samples = 0;

        let status = index
            .rebuild_cancellable(&replacement.snapshot(), 600.0, 1, 1.0, || {
                samples += 1;
                samples > 3
            })
            .expect("cancellable layout");

        assert_eq!(status, LayoutBuildStatus::Cancelled);
        assert_eq!(
            index
                .fragments()
                .map(|fragment| fragment.id)
                .collect::<Vec<_>>(),
            original
        );
        assert_eq!(index.total_height(), original_height);
    }
}
