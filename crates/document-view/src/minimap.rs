use document_core::NodeId;

use crate::{FragmentKind, LayoutIndex, Rect};

const MINIMAP_WIDTH: f32 = 40.0;
const MINIMAP_INSET: f32 = 2.0;
// One mark per two logical pixels keeps the miniature legible at fractional
// scale without constructing hundreds of overlapping GPUI elements per frame.
const MINIMAP_RESOLUTION: f32 = 0.5;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinimapPrimitiveKind {
    TextLine,
    Heading,
    TableGrid,
    Image,
    Placeholder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinimapPrimitive {
    pub node_id: NodeId,
    pub kind: MinimapPrimitiveKind,
    pub rect: Rect,
    pub refined: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Minimap {
    primitives: Vec<MinimapPrimitive>,
    document_height: f32,
}

impl Minimap {
    pub fn rebuild(&mut self, layout: &LayoutIndex, minimap_height: f32) {
        self.document_height = layout.total_height();
        let minimap_height = minimap_height.max(1.0);
        let scale = if self.document_height > 0.0 {
            minimap_height / self.document_height
        } else {
            1.0
        };
        // At most one mark per two logical minimap pixels. This keeps a
        // multi-MB document cheap to paint while retaining headings, tables,
        // images, indentation, and varied text-line silhouettes.
        let mut slots: Vec<Option<MinimapPrimitive>> =
            vec![None; (minimap_height * MINIMAP_RESOLUTION).ceil() as usize + 1];
        for fragment in layout.fragments() {
            let (kind, inset, nominal_line_height, max_lines) = match fragment.kind {
                FragmentKind::Heading(level) => (
                    MinimapPrimitiveKind::Heading,
                    MINIMAP_INSET + f32::from(level.saturating_sub(1)),
                    fragment.rect.size.height.max(1.),
                    1,
                ),
                FragmentKind::TableRow { .. } => (
                    MinimapPrimitiveKind::TableGrid,
                    MINIMAP_INSET,
                    fragment.rect.size.height.max(1.),
                    1,
                ),
                FragmentKind::Image => (
                    MinimapPrimitiveKind::Image,
                    MINIMAP_INSET,
                    fragment.rect.size.height.max(1.),
                    1,
                ),
                FragmentKind::ListItem => (MinimapPrimitiveKind::TextLine, 6., 28.8, 12),
                FragmentKind::BlockQuote | FragmentKind::Alert | FragmentKind::Footnote => {
                    (MinimapPrimitiveKind::TextLine, 5., 28.8, 12)
                }
                FragmentKind::Text | FragmentKind::Code => {
                    (MinimapPrimitiveKind::TextLine, MINIMAP_INSET, 28.8, 12)
                }
                _ => (
                    MinimapPrimitiveKind::Placeholder,
                    8.,
                    fragment.rect.size.height.max(1.),
                    1,
                ),
            };
            let line_count = ((fragment.rect.size.height / nominal_line_height).ceil() as usize)
                .clamp(1, max_lines);
            let scaled_height = fragment.rect.size.height * scale;
            for line in 0..line_count {
                let y = fragment.rect.origin.y * scale
                    + scaled_height * line as f32 / line_count as f32;
                let slot = (y * MINIMAP_RESOLUTION).floor() as usize;
                let Some(destination) = slots.get_mut(slot) else {
                    continue;
                };
                let width = silhouette_width(&fragment, kind, line, line_count, inset);
                let height = match kind {
                    MinimapPrimitiveKind::Heading => scaled_height.clamp(2., 5.),
                    MinimapPrimitiveKind::Image => scaled_height.clamp(4., 14.),
                    MinimapPrimitiveKind::TableGrid => scaled_height.clamp(1., 3.),
                    _ => 1.,
                };
                let candidate = MinimapPrimitive {
                    node_id: fragment.id.node_id,
                    kind,
                    rect: Rect::new(inset, y, width, height),
                    refined: !fragment.estimated,
                };
                if destination.as_ref().is_none_or(|current| {
                    primitive_priority(candidate.kind) >= primitive_priority(current.kind)
                }) {
                    *destination = Some(candidate);
                }
            }
        }
        self.primitives = slots.into_iter().flatten().collect();
    }

    #[must_use]
    pub fn primitives(&self) -> &[MinimapPrimitive] {
        &self.primitives
    }

    #[must_use]
    pub fn viewport_indicator(
        &self,
        scroll_y: f32,
        viewport_height: f32,
        minimap_height: f32,
    ) -> Rect {
        let scale = minimap_height.max(1.0) / self.document_height.max(1.0);
        Rect::new(
            0.0,
            scroll_y.max(0.0) * scale,
            MINIMAP_WIDTH,
            (viewport_height * scale).max(16.0),
        )
    }

    #[must_use]
    pub fn document_offset_for_pointer(&self, pointer_y: f32, minimap_height: f32) -> f32 {
        (pointer_y.max(0.0) / minimap_height.max(1.0) * self.document_height)
            .min(self.document_height)
    }
}

fn silhouette_width(
    fragment: &crate::LayoutFragment,
    kind: MinimapPrimitiveKind,
    line: usize,
    line_count: usize,
    inset: f32,
) -> f32 {
    let available = (MINIMAP_WIDTH - inset - MINIMAP_INSET).max(6.);
    match kind {
        MinimapPrimitiveKind::Heading => {
            let variation = (fragment.id.node_id.get() % 5) as f32;
            (available - variation).max(18.)
        }
        MinimapPrimitiveKind::TableGrid | MinimapPrimitiveKind::Image => available,
        MinimapPrimitiveKind::TextLine => {
            let seed = fragment
                .id
                .node_id
                .get()
                .wrapping_add(u64::from(fragment.id.ordinal))
                .wrapping_add(line as u64 * 11);
            let mut fraction = 0.72 + (seed % 25) as f32 / 100.;
            if line + 1 == line_count
                && let Some(range) = &fragment.text_range
            {
                let remainder = range.len() % 72;
                if remainder != 0 {
                    fraction = (remainder as f32 / 72.).clamp(0.32, 0.92);
                }
            }
            (available * fraction).max(8.)
        }
        MinimapPrimitiveKind::Placeholder => available * 0.58,
    }
}

fn primitive_priority(kind: MinimapPrimitiveKind) -> u8 {
    match kind {
        MinimapPrimitiveKind::Placeholder => 0,
        MinimapPrimitiveKind::TextLine => 1,
        MinimapPrimitiveKind::TableGrid => 2,
        MinimapPrimitiveKind::Image => 3,
        MinimapPrimitiveKind::Heading => 4,
    }
}

#[cfg(test)]
mod tests {
    use document_core::Document;

    use super::*;

    #[test]
    fn rendered_minimap_is_bounded_and_keeps_a_document_silhouette() {
        let document = Document::from_markdown(concat!(
            "# Long document heading\n\n",
            "A short paragraph.\n\n",
            "A substantially longer paragraph that wraps across several visual lines in the document and therefore should leave several varied marks in its miniature.\n\n",
            "- nested list item\n\n",
            "| Mineral | Depth |\n| --- | --- |\n| Quartz | 40 cm |\n\n",
            "![core sample](sample.png)\n"
        ))
        .expect("minimap fixture");
        let mut layout = LayoutIndex::default();
        layout
            .rebuild(&document.snapshot(), 760., 1, 1.)
            .expect("layout");
        let mut minimap = Minimap::default();
        minimap.rebuild(&layout, 240.);

        assert!(minimap.primitives().len() <= 121);
        assert!(
            minimap
                .primitives()
                .iter()
                .all(|primitive| primitive.rect.right() <= MINIMAP_WIDTH)
        );
        assert!(
            minimap
                .primitives()
                .iter()
                .any(|primitive| primitive.kind == MinimapPrimitiveKind::Heading)
        );
        assert!(
            minimap
                .primitives()
                .iter()
                .any(|primitive| primitive.kind == MinimapPrimitiveKind::TableGrid)
        );
        assert!(
            minimap
                .primitives()
                .iter()
                .any(|primitive| primitive.kind == MinimapPrimitiveKind::Image)
        );
        let distinct_widths = minimap
            .primitives()
            .iter()
            .map(|primitive| primitive.rect.size.width.to_bits())
            .collect::<std::collections::HashSet<_>>();
        assert!(distinct_widths.len() >= 4);
        assert_eq!(minimap.viewport_indicator(0., 100., 240.).size.width, 40.);
    }
}
