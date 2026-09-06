use document_core::{Affinity, DocumentPosition, NodeId};

use crate::{FragmentId, LayoutIndex};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScrollAnchor {
    pub node_id: NodeId,
    pub fragment_ordinal: u32,
    pub intra_fragment_offset: f32,
}

impl ScrollAnchor {
    #[must_use]
    pub const fn fragment_id(self) -> FragmentId {
        FragmentId {
            node_id: self.node_id,
            ordinal: self.fragment_ordinal,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VisibleRange {
    pub start: usize,
    pub end: usize,
}

impl VisibleRange {
    #[must_use]
    pub fn as_range(self) -> std::ops::Range<usize> {
        self.start..self.end
    }
}

#[derive(Clone, Debug)]
pub struct Viewport {
    scroll_y: f32,
    height: f32,
    anchor: Option<ScrollAnchor>,
}

impl Viewport {
    #[must_use]
    pub fn new(height: f32) -> Self {
        Self {
            scroll_y: 0.0,
            height: height.max(0.0),
            anchor: None,
        }
    }

    #[must_use]
    pub fn scroll_y(&self) -> f32 {
        self.scroll_y
    }

    #[must_use]
    pub fn height(&self) -> f32 {
        self.height
    }

    pub fn resize(&mut self, height: f32, layout: &LayoutIndex) {
        self.height = height.max(0.0);
        self.clamp(layout);
    }

    /// Precise pointer/trackpad delta. No easing is applied.
    pub fn apply_scroll_delta(&mut self, delta_y: f32, layout: &LayoutIndex) {
        self.scroll_y = (self.scroll_y + delta_y).max(0.0);
        self.clamp(layout);
        self.capture_anchor(layout);
    }

    #[must_use]
    pub fn visible_with_overscan(&self, layout: &LayoutIndex) -> VisibleRange {
        let start_y = (self.scroll_y - self.height).max(0.0);
        let end_y = (self.scroll_y + self.height * 2.0).min(layout.total_height());
        let start = layout.index_at_y(start_y).unwrap_or(0);
        let end = layout
            .index_at_y(end_y)
            .map_or(layout.fragments().len(), |index| index + 1);
        VisibleRange { start, end }
    }

    pub fn capture_anchor(&mut self, layout: &LayoutIndex) {
        self.anchor = layout.index_at_y(self.scroll_y).and_then(|index| {
            let fragment = layout.fragments().nth(index)?;
            Some(ScrollAnchor {
                node_id: fragment.id.node_id,
                fragment_ordinal: fragment.id.ordinal,
                intra_fragment_offset: self.scroll_y - fragment.rect.origin.y,
            })
        });
    }

    /// Restores the stable fragment/intra-fragment anchor after reflow above it.
    pub fn restore_anchor(&mut self, layout: &LayoutIndex) {
        if let Some(anchor) = self.anchor
            && let Some(fragment) = layout.fragment(anchor.fragment_id())
        {
            self.scroll_y = fragment.rect.origin.y + anchor.intra_fragment_offset;
            self.clamp(layout);
        }
    }

    #[must_use]
    pub fn approximate_position(&self, layout: &LayoutIndex) -> Option<DocumentPosition> {
        let fragment = layout
            .index_at_y(self.scroll_y)
            .and_then(|index| layout.fragments().nth(index))?;
        Some(DocumentPosition::new(
            fragment.id.node_id,
            fragment.text_range.as_ref().map_or(0, |range| range.start),
            Affinity::Downstream,
        ))
    }

    fn clamp(&mut self, layout: &LayoutIndex) {
        self.scroll_y = self
            .scroll_y
            .min((layout.total_height() - self.height).max(0.0));
    }
}
