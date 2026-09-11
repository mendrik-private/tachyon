//! Rendering-independent document geometry and viewport policy.
//!
//! GPUI painting is an adapter over these data structures. Keeping geometry and
//! invalidation here makes hit testing, minimap projection, and performance
//! behavior independently testable.

mod adaptive;
mod bibliography;
mod controls;
mod diagram;
mod editor;
mod figures;
mod footnotes;
mod geometry;
mod height_tree;
mod html;
mod layout;
mod math;
mod metrics;
mod minimap;
mod outline;
mod projection;
mod quotes;
mod responsive;
mod schema;
mod session;
mod signals;
mod theme;
mod viewport;

pub use controls::ButtonAccessibilityExt;
pub use editor::{
    EditorEvent, EditorScrollAnchor, EditorViewState, LayoutDiagnosticsReport, LayoutTraceMode,
    PreparedDocumentView, RichDocumentEditor, SharedImageDimensions, init as init_editor,
};
pub use figures::FigureTextRole;
pub use geometry::{Point, Rect, Size};
pub use height_tree::{HeightTree, HeightTreeError};
pub use layout::{
    FragmentId, FragmentKind, LayoutBuildStatus, LayoutFragment, LayoutIndex, LayoutKey,
    LayoutSummary, PositionMapping,
};
pub use minimap::{
    MINIMAP_WIDTH, Minimap, MinimapAlertTone, MinimapCodeTone, MinimapPrimitive,
    MinimapPrimitiveKind,
};
pub use outline::{OutlineEntry, project_outline};
pub use projection::{ProjectionContext, ProjectionSegment, TextProjection};
pub use responsive::{ResponsiveLayout, WindowClass};
pub use session::{DocumentSessionId, SharedDocumentSession};
pub use theme::TachyonPalette;
pub use viewport::{ScrollAnchor, Viewport, VisibleRange};
