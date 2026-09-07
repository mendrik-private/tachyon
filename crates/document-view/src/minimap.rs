use std::collections::HashMap;

use document_core::NodeId;

use crate::Rect;

pub const MINIMAP_WIDTH: f32 = 112.0;
const MINIMAP_INSET: f32 = 5.0;
const MINIMAP_TRAILING_GUTTER: f32 = 5.0;
// One representative mark per logical minimap pixel keeps rendering bounded
// while frames and markers preserve the document's component hierarchy.
const MINIMAP_RESOLUTION: f32 = 1.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MinimapAlertTone {
    Info,
    Success,
    Warning,
    Error,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinimapCodeTone {
    Plain,
    Keyword,
    String,
    Comment,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MinimapPrimitiveKind {
    TextLine,
    ListLine,
    Heading,
    HeadingBadge,
    ListMarker,
    TaskMarker { checked: bool },
    CodeLine(MinimapCodeTone),
    TableLine,
    Image,
    CodeFrame,
    TableFrame,
    TableRule,
    TaskFrame,
    QuoteRail,
    AlertFrame(MinimapAlertTone),
    Placeholder,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MinimapPrimitive {
    pub node_id: NodeId,
    pub kind: MinimapPrimitiveKind,
    pub rect: Rect,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MinimapSourceKind {
    Text,
    List,
    Task {
        checked: bool,
    },
    Heading {
        level: u8,
    },
    Code(MinimapCodeTone),
    Table {
        table_id: NodeId,
        row: usize,
        column: usize,
    },
    Image,
    Placeholder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MinimapFrameKind {
    Code,
    Table,
    Task,
    Quote,
    Alert(MinimapAlertTone),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct MinimapSourceLine {
    pub node_id: NodeId,
    pub top_level_node_id: NodeId,
    pub kind: MinimapSourceKind,
    pub y: f32,
    pub height: f32,
    pub inset: f32,
    pub x_fraction: f32,
    pub width_fraction: f32,
    pub fill_fraction: f32,
    pub first_visual_line: bool,
    pub alert: Option<(NodeId, MinimapAlertTone)>,
    pub quote_depth: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct FrameKey {
    node_id: NodeId,
    kind: MinimapFrameKind,
}

#[derive(Clone, Copy, Debug)]
struct FrameExtent {
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
}

impl FrameExtent {
    fn include(&mut self, left: f32, top: f32, right: f32, bottom: f32) {
        self.left = self.left.min(left);
        self.top = self.top.min(top);
        self.right = self.right.max(right);
        self.bottom = self.bottom.max(bottom);
    }
}

#[derive(Clone, Copy, Debug)]
struct LineSlot {
    line: MinimapPrimitive,
    accessory: Option<MinimapPrimitive>,
}

#[derive(Clone, Debug, Default)]
pub struct Minimap {
    primitives: Vec<MinimapPrimitive>,
    document_height: f32,
}

impl Minimap {
    pub(crate) fn rebuild_rendered(
        &mut self,
        lines: impl IntoIterator<Item = MinimapSourceLine>,
        document_height: f32,
        content_width: f32,
        minimap_height: f32,
    ) {
        self.document_height = document_height.max(1.0);
        let minimap_height = minimap_height.max(1.0);
        let scale = minimap_height / self.document_height;
        let slot_count = (minimap_height * MINIMAP_RESOLUTION).ceil() as usize + 1;
        let mut slots: Vec<Option<LineSlot>> = vec![None; slot_count];
        let frame_limit = ((minimap_height / 4.).ceil() as usize).clamp(8, 96);
        let mut frames = HashMap::<FrameKey, FrameExtent>::with_capacity(frame_limit);
        let mut table_columns = HashMap::<(NodeId, usize), f32>::with_capacity(frame_limit);
        let content_width = content_width.max(1.);
        let usable_width = (MINIMAP_WIDTH - MINIMAP_INSET * 2. - MINIMAP_TRAILING_GUTTER).max(16.);

        for source in lines {
            if !source.y.is_finite() || !source.height.is_finite() {
                continue;
            }
            let y = (source.y.max(0.) * scale).min((minimap_height - 1.).max(0.));
            let slot = (y * MINIMAP_RESOLUTION).floor() as usize;
            let Some(destination) = slots.get_mut(slot) else {
                continue;
            };
            let x_fraction = source.x_fraction.max(0.);
            let width_fraction = source.width_fraction.max(0.05);
            let inset_fraction = source.inset.max(0.) / content_width;
            let frame_left = (MINIMAP_INSET + usable_width * x_fraction)
                .clamp(MINIMAP_INSET, MINIMAP_WIDTH - MINIMAP_TRAILING_GUTTER - 2.);
            let frame_right = (MINIMAP_INSET + usable_width * (x_fraction + width_fraction))
                .clamp(frame_left + 2., MINIMAP_WIDTH - MINIMAP_TRAILING_GUTTER);
            let mut line_left =
                (frame_left + usable_width * inset_fraction).clamp(frame_left, frame_right - 2.);
            let mut line_width = ((frame_right - line_left) * source.fill_fraction.clamp(0.12, 1.))
                .clamp(2., frame_right - line_left);
            let scaled_height = (source.height.max(1.) * scale).max(1.);
            let remaining_height = (minimap_height - y).max(1.);

            let (kind, priority, height, accessory) = match source.kind {
                MinimapSourceKind::Heading { level } => {
                    let badge_width = if level <= 1 { 5. } else { 4. };
                    let accessory = source.first_visual_line.then_some(MinimapPrimitive {
                        node_id: source.node_id,
                        kind: MinimapPrimitiveKind::HeadingBadge,
                        rect: Rect::new(
                            line_left,
                            y,
                            badge_width,
                            badge_width.min(4.).min(remaining_height),
                        ),
                    });
                    if accessory.is_some() {
                        line_left += badge_width + 3.;
                        line_width = (line_width - badge_width - 3.).max(4.);
                    }
                    (
                        MinimapPrimitiveKind::Heading,
                        8,
                        scaled_height
                            .clamp(2., if level <= 1 { 5. } else { 4. })
                            .min(remaining_height),
                        accessory,
                    )
                }
                MinimapSourceKind::Task { checked } => {
                    let accessory = source.first_visual_line.then_some(MinimapPrimitive {
                        node_id: source.node_id,
                        kind: MinimapPrimitiveKind::TaskMarker { checked },
                        rect: Rect::new(line_left, y, 3., 3_f32.min(remaining_height)),
                    });
                    if accessory.is_some() {
                        line_left += 6.;
                        line_width = (line_width - 6.).max(3.);
                    }
                    (MinimapPrimitiveKind::ListLine, 5, 1., accessory)
                }
                MinimapSourceKind::List => {
                    let accessory = source.first_visual_line.then_some(MinimapPrimitive {
                        node_id: source.node_id,
                        kind: MinimapPrimitiveKind::ListMarker,
                        rect: Rect::new(line_left, y, 2., 2_f32.min(remaining_height)),
                    });
                    if accessory.is_some() {
                        line_left += 5.;
                        line_width = (line_width - 5.).max(3.);
                    }
                    (MinimapPrimitiveKind::ListLine, 4, 1., accessory)
                }
                MinimapSourceKind::Code(tone) => {
                    (MinimapPrimitiveKind::CodeLine(tone), 6, 1., None)
                }
                MinimapSourceKind::Table {
                    table_id, column, ..
                } => {
                    if table_columns.len() < frame_limit * 3 {
                        table_columns
                            .entry((table_id, column))
                            .or_insert(frame_left);
                    }
                    (MinimapPrimitiveKind::TableLine, 7, 1., None)
                }
                MinimapSourceKind::Image => (
                    MinimapPrimitiveKind::Image,
                    9,
                    scaled_height.clamp(5., 18.).min(remaining_height),
                    None,
                ),
                MinimapSourceKind::Text => (MinimapPrimitiveKind::TextLine, 3, 1., None),
                MinimapSourceKind::Placeholder => (MinimapPrimitiveKind::Placeholder, 1, 1., None),
            };
            let candidate = LineSlot {
                line: MinimapPrimitive {
                    node_id: source.node_id,
                    kind,
                    rect: Rect::new(line_left, y, line_width, height),
                },
                accessory,
            };
            if destination
                .as_ref()
                .is_none_or(|current| priority >= primitive_priority(current.line.kind))
            {
                *destination = Some(candidate);
            }

            let frame_top = (y - 2.).max(0.);
            let frame_bottom = (y + scaled_height + 2.).min(minimap_height);
            match source.kind {
                MinimapSourceKind::Code(_) => include_frame(
                    &mut frames,
                    frame_limit,
                    FrameKey {
                        node_id: source.node_id,
                        kind: MinimapFrameKind::Code,
                    },
                    frame_left,
                    frame_top,
                    frame_right,
                    frame_bottom,
                ),
                MinimapSourceKind::Table { table_id, .. } => include_frame(
                    &mut frames,
                    frame_limit,
                    FrameKey {
                        node_id: table_id,
                        kind: MinimapFrameKind::Table,
                    },
                    frame_left,
                    frame_top,
                    frame_right,
                    frame_bottom,
                ),
                MinimapSourceKind::Task { .. } => include_frame(
                    &mut frames,
                    frame_limit,
                    FrameKey {
                        node_id: source.top_level_node_id,
                        kind: MinimapFrameKind::Task,
                    },
                    frame_left,
                    frame_top,
                    frame_right,
                    frame_bottom,
                ),
                _ => {}
            }
            if source.quote_depth > 0 {
                include_frame(
                    &mut frames,
                    frame_limit,
                    FrameKey {
                        node_id: source.top_level_node_id,
                        kind: MinimapFrameKind::Quote,
                    },
                    (frame_left - 2.).max(MINIMAP_INSET),
                    frame_top,
                    frame_left,
                    frame_bottom,
                );
            }
            if let Some((alert_id, tone)) = source.alert {
                include_frame(
                    &mut frames,
                    frame_limit,
                    FrameKey {
                        node_id: alert_id,
                        kind: MinimapFrameKind::Alert(tone),
                    },
                    frame_left,
                    frame_top,
                    frame_right,
                    frame_bottom,
                );
            }
        }

        let mut frame_primitives = frames
            .iter()
            .map(|(key, extent)| MinimapPrimitive {
                node_id: key.node_id,
                kind: match key.kind {
                    MinimapFrameKind::Code => MinimapPrimitiveKind::CodeFrame,
                    MinimapFrameKind::Table => MinimapPrimitiveKind::TableFrame,
                    MinimapFrameKind::Task => MinimapPrimitiveKind::TaskFrame,
                    MinimapFrameKind::Quote => MinimapPrimitiveKind::QuoteRail,
                    MinimapFrameKind::Alert(tone) => MinimapPrimitiveKind::AlertFrame(tone),
                },
                rect: if key.kind == MinimapFrameKind::Quote {
                    Rect::new(
                        extent.left,
                        extent.top,
                        1.,
                        (extent.bottom - extent.top).max(1.),
                    )
                } else {
                    Rect::new(
                        extent.left,
                        extent.top,
                        (extent.right - extent.left).max(2.),
                        (extent.bottom - extent.top).max(2.),
                    )
                },
            })
            .collect::<Vec<_>>();
        frame_primitives.sort_by(|left, right| {
            left.rect
                .origin
                .y
                .total_cmp(&right.rect.origin.y)
                .then_with(|| left.rect.origin.x.total_cmp(&right.rect.origin.x))
        });
        for ((table_id, column), x) in table_columns {
            if column == 0 {
                continue;
            }
            let Some(extent) = frames.get(&FrameKey {
                node_id: table_id,
                kind: MinimapFrameKind::Table,
            }) else {
                continue;
            };
            frame_primitives.push(MinimapPrimitive {
                node_id: table_id,
                kind: MinimapPrimitiveKind::TableRule,
                rect: Rect::new(x, extent.top, 1., (extent.bottom - extent.top).max(1.)),
            });
        }

        let line_count = slots.iter().filter(|slot| slot.is_some()).count();
        self.primitives.clear();
        self.primitives
            .reserve(frame_primitives.len() + line_count * 2);
        self.primitives.extend(frame_primitives);
        for slot in slots.into_iter().flatten() {
            self.primitives.push(slot.line);
            if let Some(accessory) = slot.accessory {
                self.primitives.push(accessory);
            }
        }
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
        let track_height = minimap_height.max(1.0);
        let document_height = self.document_height.max(1.0);
        let indicator_height = (viewport_height.max(0.0) / document_height * track_height)
            .max(16.0)
            .min(track_height);
        let document_scroll_range = (document_height - viewport_height.max(0.0)).max(0.0);
        let track_scroll_range = (track_height - indicator_height).max(0.0);
        let y = if document_scroll_range > 0.0 {
            scroll_y.clamp(0.0, document_scroll_range) / document_scroll_range * track_scroll_range
        } else {
            0.0
        };
        Rect::new(0.0, y, MINIMAP_WIDTH, indicator_height)
    }

    #[must_use]
    pub fn scroll_thumb(&self, scroll_y: f32, viewport_height: f32, minimap_height: f32) -> Rect {
        let indicator = self.viewport_indicator(scroll_y, viewport_height, minimap_height);
        Rect::new(
            MINIMAP_WIDTH - 3.0,
            indicator.origin.y,
            3.0,
            indicator.size.height,
        )
    }

    #[must_use]
    pub fn document_offset_for_pointer(&self, pointer_y: f32, minimap_height: f32) -> f32 {
        (pointer_y.max(0.0) / minimap_height.max(1.0) * self.document_height)
            .min(self.document_height)
    }
}

fn include_frame(
    frames: &mut HashMap<FrameKey, FrameExtent>,
    frame_limit: usize,
    key: FrameKey,
    left: f32,
    top: f32,
    right: f32,
    bottom: f32,
) {
    if let Some(extent) = frames.get_mut(&key) {
        extent.include(left, top, right, bottom);
    } else if frames.len() < frame_limit {
        frames.insert(
            key,
            FrameExtent {
                left,
                top,
                right,
                bottom,
            },
        );
    }
}

fn primitive_priority(kind: MinimapPrimitiveKind) -> u8 {
    match kind {
        MinimapPrimitiveKind::Placeholder => 1,
        MinimapPrimitiveKind::TextLine => 3,
        MinimapPrimitiveKind::ListLine
        | MinimapPrimitiveKind::ListMarker
        | MinimapPrimitiveKind::TaskMarker { .. } => 5,
        MinimapPrimitiveKind::CodeLine(_) => 6,
        MinimapPrimitiveKind::TableLine | MinimapPrimitiveKind::TableRule => 7,
        MinimapPrimitiveKind::Heading | MinimapPrimitiveKind::HeadingBadge => 8,
        MinimapPrimitiveKind::Image => 9,
        MinimapPrimitiveKind::CodeFrame
        | MinimapPrimitiveKind::TableFrame
        | MinimapPrimitiveKind::TaskFrame
        | MinimapPrimitiveKind::QuoteRail
        | MinimapPrimitiveKind::AlertFrame(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(value: u64) -> NodeId {
        NodeId::new_unchecked(value)
    }

    #[test]
    fn rendered_minimap_is_bounded_and_keeps_structured_components() {
        let heading = id(1);
        let code = id(2);
        let table = id(3);
        let task = id(4);
        let alert = id(5);
        let lines = [
            MinimapSourceLine {
                node_id: heading,
                top_level_node_id: heading,
                kind: MinimapSourceKind::Heading { level: 1 },
                y: 20.,
                height: 42.,
                inset: 20.,
                x_fraction: 0.,
                width_fraction: 1.,
                fill_fraction: 0.7,
                first_visual_line: true,
                alert: None,
                quote_depth: 0,
            },
            MinimapSourceLine {
                node_id: task,
                top_level_node_id: task,
                kind: MinimapSourceKind::Task { checked: true },
                y: 90.,
                height: 28.,
                inset: 24.,
                x_fraction: 0.,
                width_fraction: 1.,
                fill_fraction: 0.58,
                first_visual_line: true,
                alert: None,
                quote_depth: 0,
            },
            MinimapSourceLine {
                node_id: code,
                top_level_node_id: code,
                kind: MinimapSourceKind::Code(MinimapCodeTone::Keyword),
                y: 150.,
                height: 22.,
                inset: 16.,
                x_fraction: 0.,
                width_fraction: 1.,
                fill_fraction: 0.8,
                first_visual_line: true,
                alert: None,
                quote_depth: 0,
            },
            MinimapSourceLine {
                node_id: table,
                top_level_node_id: table,
                kind: MinimapSourceKind::Table {
                    table_id: table,
                    row: 0,
                    column: 1,
                },
                y: 210.,
                height: 28.,
                inset: 12.,
                x_fraction: 0.5,
                width_fraction: 0.5,
                fill_fraction: 0.72,
                first_visual_line: true,
                alert: None,
                quote_depth: 0,
            },
            MinimapSourceLine {
                node_id: alert,
                top_level_node_id: alert,
                kind: MinimapSourceKind::Text,
                y: 280.,
                height: 28.,
                inset: 48.,
                x_fraction: 0.,
                width_fraction: 1.,
                fill_fraction: 0.62,
                first_visual_line: true,
                alert: Some((alert, MinimapAlertTone::Info)),
                quote_depth: 0,
            },
        ];
        let mut minimap = Minimap::default();
        minimap.rebuild_rendered(lines, 400., 760., 240.);

        assert!(minimap.primitives().len() <= 2 * 241 + 96 * 4);
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
                .all(|primitive| primitive.rect.bottom() <= 240.)
        );
        for kind in [
            MinimapPrimitiveKind::HeadingBadge,
            MinimapPrimitiveKind::TaskFrame,
            MinimapPrimitiveKind::CodeFrame,
            MinimapPrimitiveKind::TableFrame,
            MinimapPrimitiveKind::AlertFrame(MinimapAlertTone::Info),
        ] {
            assert!(
                minimap
                    .primitives()
                    .iter()
                    .any(|primitive| primitive.kind == kind),
                "missing {kind:?}"
            );
        }
        assert_eq!(
            minimap.viewport_indicator(0., 100., 240.).size.width,
            MINIMAP_WIDTH
        );
        let indicator = minimap.viewport_indicator(10_000., 1., 240.);
        assert_eq!(indicator.bottom(), 240.);
        assert!(indicator.size.height >= 16.);
        let thumb = minimap.scroll_thumb(10_000., 1., 240.);
        assert_eq!(thumb.origin.x, MINIMAP_WIDTH - 3.);
        assert_eq!(thumb.origin.y, indicator.origin.y);
        assert_eq!(thumb.size.height, indicator.size.height);
    }

    #[test]
    fn pointer_mapping_uses_the_same_rendered_document_height() {
        let mut minimap = Minimap::default();
        minimap.rebuild_rendered([], 1_200., 760., 300.);
        assert_eq!(minimap.document_offset_for_pointer(150., 300.), 600.);
        assert_eq!(minimap.document_offset_for_pointer(400., 300.), 1_200.);
    }
}
