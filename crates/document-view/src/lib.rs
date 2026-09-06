//! Rendering-independent document geometry and viewport policy.
//!
//! GPUI painting is an adapter over these data structures. Keeping geometry and
//! invalidation here makes hit testing, minimap projection, and performance
//! behavior independently testable.

mod editor;
mod geometry;
mod height_tree;
mod layout;
mod minimap;
mod outline;
mod projection;
mod responsive;
mod session;
mod theme;
mod viewport;

pub use editor::{
    EditorEvent, EditorScrollAnchor, EditorViewState, PreparedDocumentView, RichDocumentEditor,
    SharedImageDimensions, init as init_editor,
};
pub use geometry::{Point, Rect, Size};
pub use height_tree::{HeightTree, HeightTreeError};
pub use layout::{
    FragmentId, FragmentKind, LayoutBuildStatus, LayoutFragment, LayoutIndex, LayoutKey,
    LayoutSummary, PositionMapping,
};
pub use minimap::{Minimap, MinimapPrimitive, MinimapPrimitiveKind};
pub use outline::{OutlineEntry, project_outline};
pub use projection::{ProjectionContext, ProjectionSegment, TextProjection};
pub use responsive::{ResponsiveLayout, WindowClass};
pub use session::SharedDocumentSession;
pub use theme::MineralPalette;
pub use viewport::{ScrollAnchor, Viewport, VisibleRange};
