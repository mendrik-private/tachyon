//! Adaptive layout, hit testing, virtualization, and outline for rich Markdown
//! documents, rendered through GPUI by the editor view.

mod adaptive;
mod bibliography;
mod controls;
mod diagram;
mod editor;
mod figures;
pub mod fonts;
mod footnotes;
mod html;
mod lru;
mod math;
mod metrics;
mod outline;
mod projection;
mod quotes;
mod schema;
mod session;
mod signals;
mod theme;

pub use controls::ButtonAccessibilityExt;
pub use editor::{
    EditorEvent, EditorScrollAnchor, EditorViewState, LayoutDiagnosticsReport, LayoutTraceMode,
    PreparedDocumentView, RichDocumentEditor, init as init_editor,
};
pub use outline::{OutlineEntry, project_outline};
pub use projection::TextProjection;
pub use session::{DocumentSessionId, SharedDocumentSession};
pub use theme::TachyonPalette;

pub(crate) use figures::FigureTextRole;
pub(crate) use projection::{ProjectionContext, ProjectionSegment};
