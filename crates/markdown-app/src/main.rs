use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
        mpsc::{self, Receiver, TryRecvError},
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use document_core::{Document, Revision, SourceIdentity};
use document_view::{
    EditorEvent, EditorScrollAnchor, EditorViewState, MineralPalette, OutlineEntry,
    PreparedDocumentView, RichDocumentEditor, SharedDocumentSession, init_editor, project_outline,
};
use futures::{
    StreamExt as _,
    channel::oneshot,
    future::{Either, select},
};
use gpui::{
    AnyWindowHandle, App, AppContext as _, Bounds, Entity, EntityId, Focusable as _, FontFallbacks,
    InteractiveElement as _, KeyBinding, KeyDownEvent, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, ParentElement as _, PathPromptOptions, PromptLevel, QuitMode, Render, Resource,
    Role, StatefulInteractiveElement as _, Styled as _, WeakEntity, WindowBounds,
    WindowDecorations, WindowOptions, div, font, image_cache as image_cache_element, point,
    prelude::FluentBuilder as _, px, relative, rgb, size, uniform_list,
};
use gpui_component::{
    ActiveTheme as _, Icon, IconName, Root, Sizable as _, Theme, ThemeMode, TitleBar,
    button::{Button, ButtonVariants as _},
    menu::{DropdownMenu as _, PopupMenuItem},
    scroll::{Scrollbar, ScrollbarMode},
};
use notify::Watcher as _;
use persistence::{
    ExternalState, PersistenceError, RecoverableSaveError, RecoveryEntry, RecoveryJournal,
    WorkspaceState, WorkspaceStateStore, atomic_save, atomic_write_new, detect_external_state,
    read_source_with_identity, save_with_recovery, source_identity,
};

mod assets;
mod file_dialog;
mod fonts;
mod image_cache;
mod instance;
mod performance;
mod persistence;

#[global_allocator]
static GLOBAL_ALLOCATOR: mimalloc3::MiMalloc = mimalloc3::MiMalloc;

const WINDOW_KEY_CONTEXT: &str = "MineralWindow";

gpui::actions!(
    mineral_window,
    [
        NewDocumentAction,
        OpenFileAction,
        OpenFolderAction,
        SaveDocumentAction,
        SaveAsAction,
        SaveCopyAction,
        UndoDocumentAction,
        RedoDocumentAction,
        ToggleNavigationAction,
        FindDocumentAction,
        FindNextAction,
        FindPreviousAction,
        ZoomInAction,
        ZoomOutAction,
        ResetZoomAction,
        CloseDocumentWindowAction,
    ]
);

fn main() {
    let startup_started_at = SystemTime::now();
    let startup_trace_started_at = Instant::now();
    startup_trace(startup_trace_started_at, "main");
    let initial_paths = std::env::args_os()
        .skip(1)
        .map(PathBuf::from)
        .collect::<Vec<_>>();
    let performance_config = match performance::PerformanceConfig::from_env() {
        Ok(config) => config,
        Err(error) => {
            eprintln!("invalid performance configuration: {error}");
            return;
        }
    };
    let startup_config = performance::StartupConfig::from_env();
    let launch_mode = match instance::LaunchMode::from_env() {
        Ok(mode) => mode,
        Err(error) => {
            eprintln!("invalid instance configuration: {error}");
            return;
        }
    };
    if performance_config.is_some() && startup_config.is_some() {
        eprintln!("startup and sustained performance modes cannot run together");
        return;
    }
    if performance_config.is_some() && initial_paths.len() > 1 {
        eprintln!("performance mode accepts at most one document path");
        return;
    }
    if startup_config.is_some() && initial_paths.len() != 1 {
        eprintln!("startup measurement requires exactly one document path");
        return;
    }
    if let instance::LaunchMode::Client(socket) = &launch_mode {
        let request = if std::env::var_os("MINERAL_INSTANCE_SHUTDOWN").is_some() {
            instance::Request::Shutdown
        } else {
            let Some(path) = initial_paths.first() else {
                eprintln!("instance client requires exactly one document path");
                return;
            };
            let startup = startup_config.as_ref().and_then(|config| {
                unix_nanos(startup_started_at).map(|started_unix_nanos| instance::StartupRequest {
                    output: config.output.clone(),
                    label: config.label.clone(),
                    cache_state: config.cache_state.clone(),
                    started_unix_nanos,
                })
            });
            if startup_config.is_some() && startup.is_none() {
                eprintln!("system clock is earlier than the Unix epoch");
                return;
            }
            instance::Request::Open {
                path: path.clone(),
                startup,
            }
        };
        if let Err(error) = instance::forward(socket, &request) {
            eprintln!("instance request failed: {error}");
        }
        return;
    }
    let server = match launch_mode {
        instance::LaunchMode::Server(socket) => match instance::Server::bind(socket) {
            Ok(server) => Some(server),
            Err(error) => {
                eprintln!("instance server failed: {error}");
                return;
            }
        },
        instance::LaunchMode::Standalone => None,
        instance::LaunchMode::Client(_) => unreachable!("client returned above"),
    };
    let resident_server = server.is_some();
    let mut initial_preloads = initial_paths
        .iter()
        .map(|path| start_document_preload(path, startup_trace_started_at))
        .collect::<Vec<_>>();
    let http_client = reqwest_client::ReqwestClient::user_agent("Mineral/0.1")
        .expect("HTTP client initialization must succeed");
    let mut application = gpui_platform::application()
        .with_http_client(Arc::new(http_client))
        .with_assets(assets::Assets);
    if resident_server {
        application = application.with_quit_mode(QuitMode::Explicit);
    }
    startup_trace(startup_trace_started_at, "application-created");
    application.run(move |cx| {
        startup_trace(startup_trace_started_at, "application-run");
        cx.set_app_identity("dev.mineral.Markdown", "Mineral");
        cx.set_reduce_motion(prefers_reduced_motion());
        fonts::register(cx);
        gpui_component::init(cx);
        sync_mineral_component_theme(None, cx);
        init_editor(cx);
        cx.bind_keys([
            KeyBinding::new("ctrl-n", NewDocumentAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("ctrl-o", OpenFileAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("ctrl-shift-o", OpenFolderAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("ctrl-s", SaveDocumentAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("ctrl-shift-s", SaveAsAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("ctrl-alt-shift-s", SaveCopyAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("ctrl-z", UndoDocumentAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("ctrl-f", FindDocumentAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("f3", FindNextAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("shift-f3", FindPreviousAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("ctrl-shift-z", RedoDocumentAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new(
                "ctrl-alt-n",
                ToggleNavigationAction,
                Some(WINDOW_KEY_CONTEXT),
            ),
            KeyBinding::new(
                "ctrl-w",
                CloseDocumentWindowAction,
                Some(WINDOW_KEY_CONTEXT),
            ),
            KeyBinding::new("ctrl-=", ZoomInAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("ctrl--", ZoomOutAction, Some(WINDOW_KEY_CONTEXT)),
            KeyBinding::new("ctrl-0", ResetZoomAction, Some(WINDOW_KEY_CONTEXT)),
        ]);

        let initial_items = if initial_paths.is_empty() {
            vec![(None, None)]
        } else {
            initial_paths
                .iter()
                .cloned()
                .map(Some)
                .zip(initial_preloads.drain(..))
                .collect()
        };
        let sessions = Rc::new(RefCell::new(SessionRegistry::default()));
        for (initial_path, initial_preload) in initial_items {
            let completion = startup_config.as_ref().map(|_| {
                if resident_server {
                    StartupCompletion::CloseWindow(None)
                } else {
                    StartupCompletion::QuitApplication
                }
            });
            open_markdown_window(
                cx,
                initial_path,
                sessions.clone(),
                LaunchInstrumentation {
                    performance: performance_config.clone(),
                    startup: startup_config.clone(),
                    started_at: startup_started_at,
                    trace_started_at: startup_trace_started_at,
                    initial_preload,
                    completion,
                },
            )
            .expect("Wayland window creation must succeed");
        }
        if let Some(server) = server {
            spawn_instance_listener(server, sessions, cx);
        }
    });
}

fn unix_nanos(time: SystemTime) -> Option<u64> {
    time.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|elapsed| u64::try_from(elapsed.as_nanos()).ok())
}

fn system_time_from_unix_nanos(nanos: u64) -> SystemTime {
    UNIX_EPOCH + Duration::from_nanos(nanos)
}

fn new_untitled_recovery_key() -> PathBuf {
    let created = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    PathBuf::from(format!(
        "mineral-untitled-v1/{}-{created}",
        std::process::id()
    ))
}

fn start_document_preload(
    path: &std::path::Path,
    trace_started_at: Instant,
) -> Option<InitialPreload> {
    if path.is_dir() {
        return None;
    }
    let (sender, receiver) = mpsc::sync_channel(1);
    let path = path.to_path_buf();
    let recovery = RecoveryJournal::for_current_user();
    std::thread::Builder::new()
        .name("mineral-initial-load".into())
        .spawn(move || {
            let _ = sender.send(load_document(path, recovery, trace_started_at));
        })
        .expect("initial document loader thread must start");
    Some(receiver)
}

fn window_options() -> WindowOptions {
    WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(Bounds::new(
            point(px(80.0), px(80.0)),
            size(px(1100.0), px(720.0)),
        ))),
        window_min_size: Some(size(px(480.0), px(360.0))),
        titlebar: Some(TitleBar::title_bar_options()),
        app_id: Some("dev.mineral.Markdown".into()),
        window_decorations: Some(WindowDecorations::Client),
        app_owns_titlebar_drag: true,
        ..WindowOptions::default()
    }
}

fn open_markdown_window(
    cx: &mut App,
    path: Option<PathBuf>,
    sessions: SharedSessionRegistry,
    instrumentation: LaunchInstrumentation,
) -> Result<AnyWindowHandle, String> {
    startup_trace(instrumentation.trace_started_at, "before-open-window");
    cx.open_window(window_options(), move |window, cx| {
        startup_trace(instrumentation.trace_started_at, "window-opened");
        let view = cx.new(|cx| MarkdownWindow::new(path, sessions, instrumentation, window, cx));
        cx.new(|cx| Root::new(view, window, cx))
    })
    .map(Into::into)
    .map_err(|error| error.to_string())
}

fn spawn_instance_listener(
    server: instance::Server,
    sessions: SharedSessionRegistry,
    cx: &mut App,
) {
    let (mut receiver, guard) = server.into_parts();
    cx.spawn(async move |cx| {
        let _guard = guard;
        while let Some(inbound) = receiver.next().await {
            let (request, completion) = inbound.into_parts();
            match request {
                instance::Request::Shutdown => {
                    completion.finish(Ok(()));
                    cx.update(|cx| cx.quit());
                    break;
                }
                instance::Request::Open { path, startup } => {
                    let trace_started_at = Instant::now();
                    let initial_preload = start_document_preload(&path, trace_started_at);
                    let (startup, started_at, startup_completion) = match startup {
                        Some(startup) => (
                            Some(performance::StartupConfig {
                                output: startup.output,
                                label: startup.label,
                                cache_state: startup.cache_state,
                                reuses_render_process: true,
                            }),
                            system_time_from_unix_nanos(startup.started_unix_nanos),
                            Some(StartupCompletion::CloseWindow(Some(completion))),
                        ),
                        None => {
                            let result = cx.update(|cx| {
                                open_markdown_window(
                                    cx,
                                    Some(path),
                                    sessions.clone(),
                                    LaunchInstrumentation {
                                        performance: None,
                                        startup: None,
                                        started_at: SystemTime::now(),
                                        trace_started_at,
                                        initial_preload,
                                        completion: None,
                                    },
                                )
                            });
                            let result = result.map(|_| ());
                            completion.finish(result);
                            continue;
                        }
                    };
                    let _ = cx.update(|cx| {
                        open_markdown_window(
                            cx,
                            Some(path),
                            sessions.clone(),
                            LaunchInstrumentation {
                                performance: None,
                                startup,
                                started_at,
                                trace_started_at,
                                initial_preload,
                                completion: startup_completion,
                            },
                        )
                    });
                }
            }
        }
    })
    .detach();
}

fn startup_trace(started_at: Instant, label: &str) {
    if std::env::var_os("MINERAL_STARTUP_TRACE").is_some() {
        eprintln!(
            "startup {label}: {:.3} ms",
            started_at.elapsed().as_secs_f64() * 1_000.
        );
    }
}

type SharedSessionRegistry = Rc<RefCell<SessionRegistry>>;

type InitialPreload = Receiver<Result<LoadedDocument, String>>;

struct LoadedDocument {
    canonical: PathBuf,
    document: Document,
    prepared: PreparedDocumentView,
    identity: SourceIdentity,
    recovery_entry: Option<RecoveryEntry>,
    recovery_warning: Option<String>,
    source_bytes: usize,
}

#[derive(Clone, Debug)]
struct DocumentCompletionTicket {
    request: u64,
    epoch: u64,
    revision: Revision,
    source_path: Option<PathBuf>,
}

struct ReloadCandidate {
    document: Document,
    prepared: PreparedDocumentView,
    identity: SourceIdentity,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SaveTargetMode {
    Adopt,
    Copy,
}

#[derive(Debug)]
enum SaveJobError {
    Document(document_core::DocumentError),
    Persistence(RecoverableSaveError),
}

impl SaveJobError {
    fn is_external_change(&self) -> bool {
        let Self::Persistence(error) = self else {
            return false;
        };
        matches!(
            error,
            RecoverableSaveError::Recovered {
                save: PersistenceError::ExternalChange(_),
            } | RecoverableSaveError::Unrecovered {
                save: PersistenceError::ExternalChange(_),
                ..
            }
        )
    }
}

#[derive(Debug, thiserror::Error)]
enum SaveTargetError {
    #[error(transparent)]
    Document(#[from] document_core::DocumentError),
    #[error(transparent)]
    Persistence(#[from] PersistenceError),
}

impl std::fmt::Display for SaveJobError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Document(error) => error.fmt(formatter),
            Self::Persistence(error) => error.fmt(formatter),
        }
    }
}

struct LaunchInstrumentation {
    performance: Option<performance::PerformanceConfig>,
    startup: Option<performance::StartupConfig>,
    started_at: SystemTime,
    trace_started_at: Instant,
    initial_preload: Option<InitialPreload>,
    completion: Option<StartupCompletion>,
}

enum StartupCompletion {
    QuitApplication,
    CloseWindow(Option<instance::Completion>),
}

fn load_document(
    path: PathBuf,
    recovery: RecoveryJournal,
    trace_started_at: Instant,
) -> Result<LoadedDocument, String> {
    let canonical = std::fs::canonicalize(&path).map_err(|error| error.to_string())?;
    let (source, identity) =
        read_source_with_identity(&canonical).map_err(|error| error.to_string())?;
    let source_bytes = source.len();
    let document = Document::from_markdown(source).map_err(|error| error.to_string())?;
    let prepared = PreparedDocumentView::prepare(&document);
    let (recovery_entry, recovery_warning) = match recovery.load(&canonical) {
        Ok(entry) => (entry, None),
        Err(error) => (
            None,
            Some(format!(
                "The document opened, but its recovery record could not be read: {error}"
            )),
        ),
    };
    startup_trace(trace_started_at, "open-file-worker-ready");
    Ok(LoadedDocument {
        canonical,
        document,
        prepared,
        identity,
        recovery_entry,
        recovery_warning,
        source_bytes,
    })
}

#[derive(Default)]
struct SessionRegistry {
    sessions: HashMap<PathBuf, FileSession>,
}

struct FileSession {
    document: SharedDocumentSession,
    source_identity: SourceIdentity,
    saved_revision: Revision,
    dirty: bool,
    save_in_flight: bool,
    listeners: Vec<SessionListener>,
}

#[derive(Clone)]
struct SessionListener {
    id: EntityId,
    window: WeakEntity<MarkdownWindow>,
}

struct SessionAttachment {
    document: SharedDocumentSession,
    source_identity: SourceIdentity,
    saved_revision: Revision,
    dirty: bool,
    reused: bool,
}

impl SessionRegistry {
    fn prune(&mut self) {
        for session in self.sessions.values_mut() {
            session
                .listeners
                .retain(|listener| listener.window.upgrade().is_some());
        }
        self.sessions.retain(|_, session| {
            session.dirty
                || session.save_in_flight
                || !session.listeners.is_empty()
                || session.document.strong_count() > 1
        });
    }

    fn attach(
        &mut self,
        path: PathBuf,
        loaded: Document,
        source_identity: SourceIdentity,
    ) -> SessionAttachment {
        self.prune();
        let reused = self.sessions.contains_key(&path);
        let entry = self.sessions.entry(path).or_insert_with(|| FileSession {
            document: SharedDocumentSession::new(loaded),
            source_identity,
            saved_revision: Revision::default(),
            dirty: false,
            save_in_flight: false,
            listeners: Vec::new(),
        });
        SessionAttachment {
            document: entry.document.clone(),
            source_identity: entry.source_identity.clone(),
            saved_revision: entry.saved_revision,
            dirty: entry.dirty,
            reused,
        }
    }

    fn subscribe(&mut self, path: &std::path::Path, listener: SessionListener) {
        let Some(session) = self.sessions.get_mut(path) else {
            return;
        };
        session
            .listeners
            .retain(|current| current.id != listener.id);
        session.listeners.push(listener);
    }

    fn detach(&mut self, path: &std::path::Path, listener_id: EntityId) {
        let remove = self.sessions.get_mut(path).is_some_and(|session| {
            session
                .listeners
                .retain(|listener| listener.id != listener_id);
            session.listeners.is_empty() && !session.dirty && !session.save_in_flight
        });
        if remove {
            self.sessions.remove(path);
        }
        self.prune();
    }

    fn contains_other(&self, path: &std::path::Path, document: &SharedDocumentSession) -> bool {
        self.sessions
            .get(path)
            .is_some_and(|session| !session.document.ptr_eq(document))
    }

    fn adopt(
        &mut self,
        path: PathBuf,
        document: SharedDocumentSession,
        source_identity: SourceIdentity,
        saved_revision: Revision,
    ) -> bool {
        if self.contains_other(&path, &document) {
            return false;
        }
        let dirty = document.snapshot().revision() != saved_revision;
        if let Some(session) = self.sessions.get_mut(&path) {
            session.source_identity = source_identity;
            session.saved_revision = saved_revision;
            session.dirty = dirty;
            session.save_in_flight = false;
        } else {
            self.sessions.insert(
                path,
                FileSession {
                    document,
                    source_identity,
                    saved_revision,
                    dirty,
                    save_in_flight: false,
                    listeners: Vec::new(),
                },
            );
        }
        true
    }

    fn has_other_listener(&mut self, path: &std::path::Path, listener_id: EntityId) -> bool {
        self.prune();
        self.sessions.get(path).is_some_and(|session| {
            session
                .listeners
                .iter()
                .any(|listener| listener.id != listener_id)
        })
    }

    fn forget_if_same(&mut self, path: &std::path::Path, document: &SharedDocumentSession) {
        if self
            .sessions
            .get(path)
            .is_some_and(|session| session.document.ptr_eq(document))
        {
            self.sessions.remove(path);
        }
    }

    fn listeners(&mut self, path: &std::path::Path) -> Vec<SessionListener> {
        self.prune();
        self.sessions
            .get(path)
            .map_or_else(Vec::new, |session| session.listeners.clone())
    }

    fn mark_dirty(&mut self, path: &std::path::Path) {
        if let Some(session) = self.sessions.get_mut(path) {
            session.dirty = true;
        }
    }

    fn mark_saved(&mut self, path: &std::path::Path, revision: Revision, identity: SourceIdentity) {
        if let Some(session) = self.sessions.get_mut(path) {
            session.saved_revision = revision;
            session.source_identity = identity;
            session.dirty = session.document.snapshot().revision() != revision;
            session.save_in_flight = false;
        }
    }

    fn mark_reloaded(&mut self, path: &std::path::Path, identity: SourceIdentity) {
        if let Some(session) = self.sessions.get_mut(path) {
            session.source_identity = identity;
            session.saved_revision = session.document.snapshot().revision();
            session.dirty = false;
        }
    }

    fn claim_save(&mut self, path: &std::path::Path) -> bool {
        let Some(session) = self.sessions.get_mut(path) else {
            return false;
        };
        if session.save_in_flight {
            return false;
        }
        session.save_in_flight = true;
        true
    }

    fn release_save(&mut self, path: &std::path::Path) {
        if let Some(session) = self.sessions.get_mut(path) {
            session.save_in_flight = false;
        }
    }

    fn status(&self, path: &std::path::Path) -> Option<(SourceIdentity, Revision, bool)> {
        self.sessions.get(path).map(|session| {
            (
                session.source_identity.clone(),
                session.saved_revision,
                session.dirty,
            )
        })
    }
}

fn prefers_reduced_motion() -> bool {
    let explicit = std::env::var("MINERAL_REDUCED_MOTION").ok();
    let gtk_animations = std::env::var("GTK_ENABLE_ANIMATIONS").ok();
    reduced_motion_from_values(explicit.as_deref(), gtk_animations.as_deref())
}

fn reduced_motion_from_values(explicit: Option<&str>, gtk_animations: Option<&str>) -> bool {
    explicit
        .map(|value| {
            matches!(
                value.trim().to_ascii_lowercase().as_str(),
                "1" | "true" | "yes" | "on"
            )
        })
        .unwrap_or_else(|| {
            gtk_animations.is_some_and(|value| {
                matches!(
                    value.trim().to_ascii_lowercase().as_str(),
                    "0" | "false" | "no" | "off"
                )
            })
        })
}

fn sync_mineral_component_theme(window: Option<&mut gpui::Window>, cx: &mut App) {
    Theme::change(ThemeMode::Light, window, cx);
    let palette = MineralPalette::LIGHT;
    {
        let theme = Theme::global_mut(cx);
        let colors = &mut theme.colors;
        colors.background = rgb(palette.page).into();
        colors.foreground = rgb(palette.text).into();
        colors.border = rgb(palette.border).into();
        colors.popover = rgb(palette.floating).into();
        colors.popover_foreground = rgb(palette.text).into();
        colors.accent = rgb(palette.hover).into();
        colors.accent_foreground = rgb(palette.text).into();
        colors.muted = rgb(palette.surface_quiet).into();
        colors.muted_foreground = rgb(palette.secondary).into();
        colors.primary = rgb(palette.accent).into();
        colors.primary_foreground = rgb(palette.panel).into();
        colors.primary_hover = rgb(palette.accent).into();
        colors.info = rgb(palette.info).into();
        colors.info_foreground = rgb(palette.panel).into();
        colors.success = rgb(palette.success).into();
        colors.success_foreground = rgb(palette.panel).into();
        colors.warning = rgb(palette.warning).into();
        colors.warning_foreground = rgb(palette.panel).into();
        colors.danger = rgb(palette.error).into();
        colors.danger_foreground = rgb(palette.panel).into();
        colors.ring = rgb(palette.accent).into();
        colors.selection = rgb(palette.selection).into();
        colors.link = rgb(palette.accent).into();
        colors.input = rgb(palette.border).into();
        colors.button = rgb(palette.surface).into();
        colors.button_foreground = rgb(palette.text).into();
        colors.button_hover = rgb(palette.hover).into();
        colors.button_active = rgb(palette.accent_muted).into();
        colors.sidebar = rgb(palette.panel).into();
        colors.sidebar_foreground = rgb(palette.text).into();
        colors.sidebar_border = rgb(palette.border).into();
        colors.sidebar_accent = rgb(palette.hover).into();
        colors.sidebar_accent_foreground = rgb(palette.text).into();
        colors.title_bar = rgb(palette.panel).into();
        colors.title_bar_border = rgb(palette.border).into();
        colors.overlay = gpui::rgba(MineralPalette::with_alpha(palette.page, 0x99)).into();
        theme.tokens = (&theme.colors).into();
        theme.font_family = "Spline Sans Mineral".into();
        theme.mono_font_family = "Spline Sans Mono Mineral".into();
        theme.radius = px(8.);
        theme.radius_lg = px(8.);
    }
    Theme::sync_base(cx);
}

struct MarkdownWindow {
    editor: Entity<RichDocumentEditor>,
    image_cache: Entity<image_cache::BoundedImageCache>,
    session_registry: SharedSessionRegistry,
    filename: String,
    source_path: Option<PathBuf>,
    source_identity: Option<SourceIdentity>,
    recovery: RecoveryJournal,
    recovery_key: PathBuf,
    recovery_generation: u64,
    recovery_in_flight: bool,
    recovery_dirty: bool,
    workspace_state_store: WorkspaceStateStore,
    last_open_directory: Option<PathBuf>,
    file_chooser_open: bool,
    workspace_state_dirty: bool,
    workspace_state_in_flight: bool,
    workspace_state_generation: u64,
    pending_view_state: Option<EditorViewState>,
    unsaved: bool,
    saved_revision: Revision,
    autosave_generation: u64,
    save_in_flight: bool,
    reload_in_flight: bool,
    document_epoch: u64,
    open_request: u64,
    reload_request: u64,
    pending_reload: Option<ReloadCandidate>,
    close_prompt_in_flight: bool,
    close_after_save: bool,
    force_close: bool,
    external_watch_cancel: Option<oneshot::Sender<()>>,
    external_change_pending: bool,
    conflict: bool,
    recovery_entry: Option<RecoveryEntry>,
    navigation_root: Option<PathBuf>,
    navigation_root_explicit: bool,
    navigation_nodes: Vec<NavigationNode>,
    expanded_folders: HashSet<PathBuf>,
    navigation_request: u64,
    outline: Arc<Vec<OutlineEntry>>,
    navigation_overlay: bool,
    navigation_collapsed: bool,
    navigation_width: f32,
    navigation_dragging: bool,
    navigation_split: f32,
    navigation_split_dragging: bool,
    outline_in_flight: bool,
    outline_requested: Option<Revision>,
    outline_request: u64,
    outline_cancel_epoch: Arc<AtomicU64>,
    startup_error: Option<String>,
    startup_config: Option<performance::StartupConfig>,
    startup_started_at: SystemTime,
    startup_trace_started_at: Instant,
    startup_completion: Option<StartupCompletion>,
    window_handle: AnyWindowHandle,
    startup_source_bytes: usize,
    startup_ready: Option<StartupReady>,
}

struct StartupReady {
    config: performance::StartupConfig,
    viewport_width: f32,
    viewport_height: f32,
    scale_factor: f32,
    window: AnyWindowHandle,
    completion: StartupCompletion,
}

fn schedule_startup_completion(
    completion: StartupCompletion,
    window: AnyWindowHandle,
    result: Result<(), String>,
    cx: &mut gpui::Context<MarkdownWindow>,
) {
    // Readiness is emitted from the editor's paint callback. Hop through the
    // foreground executor so the callback and frame can unwind before closing
    // its window or asking the Linux event loop to stop.
    cx.spawn(async move |_this, cx| {
        cx.background_executor()
            .timer(Duration::from_millis(1))
            .await;
        match completion {
            StartupCompletion::QuitApplication => {
                cx.update(|cx| cx.quit());
            }
            StartupCompletion::CloseWindow(response) => {
                let close_result = window
                    .update(cx, |_, window, _| window.remove_window())
                    .map_err(|error| error.to_string());
                if let Some(response) = response {
                    response.finish(result.and(close_result));
                }
            }
        }
    })
    .detach();
}

#[derive(Clone)]
struct NavigationNode {
    path: PathBuf,
    label: String,
    kind: NavigationNodeKind,
}

#[derive(Clone)]
enum NavigationNodeKind {
    Directory {
        expanded: bool,
        children: Option<Vec<NavigationNode>>,
    },
    File,
}

#[derive(Clone)]
struct NavigationRow {
    path: PathBuf,
    label: String,
    depth: usize,
    directory_expanded: Option<bool>,
}

impl MarkdownWindow {
    fn new(
        path: Option<PathBuf>,
        session_registry: SharedSessionRegistry,
        instrumentation: LaunchInstrumentation,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Self {
        let LaunchInstrumentation {
            performance,
            startup,
            started_at: startup_started_at,
            trace_started_at: startup_trace_started_at,
            initial_preload,
            completion: startup_completion,
        } = instrumentation;
        startup_trace(startup_trace_started_at, "view-new-start");
        sync_mineral_component_theme(Some(window), cx);
        cx.observe_window_appearance(window, |_, window, cx| {
            sync_mineral_component_theme(Some(window), cx);
            cx.notify();
        })
        .detach();
        let navigation_root = path.as_deref().and_then(|path| {
            if path.is_dir() {
                Some(path.to_path_buf())
            } else {
                path.parent().map(PathBuf::from)
            }
        });
        let initial_file = path.as_ref().filter(|path| !path.is_dir()).cloned();
        let initial_directory = path.as_ref().filter(|path| path.is_dir()).cloned();
        let navigation_root_explicit = initial_directory.is_some();
        let filename = initial_file
            .as_deref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled.md")
            .to_owned();
        let editor = cx.new(|cx| RichDocumentEditor::new(Document::empty(), window, cx));
        editor.focus_handle(cx).focus(window, cx);
        let image_cache = cx.new(|cx| image_cache::BoundedImageCache::new(256 * 1024 * 1024, cx));
        let image_dimensions = image_cache.read(cx).dimensions();
        editor.update(cx, |editor, cx| {
            editor.set_layout_trace_mode(performance::layout_trace_mode());
            editor.set_image_dimensions(image_dimensions, cx);
            editor.set_image_cache(image_cache.clone().into());
        });
        let recovery = RecoveryJournal::for_current_user();
        let recovery_key = initial_file
            .clone()
            .unwrap_or_else(new_untitled_recovery_key);
        let workspace_state_store = WorkspaceStateStore::for_current_user();
        cx.subscribe(&editor, |this, _editor, event, cx| {
            match event {
                EditorEvent::Ready => this.finish_startup_measurement(cx),
                EditorEvent::Changed => {
                    this.unsaved = true;
                    if let Some(path) = this.source_path.clone() {
                        this.session_registry.borrow_mut().mark_dirty(&path);
                        this.broadcast_session_state(path, false, cx);
                    }
                    this.schedule_autosave(cx);
                    this.schedule_recovery(cx);
                    this.schedule_workspace_state(cx);
                    this.refresh_outline(cx);
                }
                EditorEvent::ViewChanged => {
                    this.schedule_workspace_state(cx);
                }
                EditorEvent::SaveRequested => this.queue_save(true, cx),
                EditorEvent::OpenLocalDocument { path, fragment } => {
                    this.open_file_at(path.clone(), fragment.clone(), cx);
                }
                EditorEvent::LinkFailed(error) => this.startup_error = Some(error.clone()),
                EditorEvent::LayoutDiagnostics(report) => {
                    performance::emit_layout_diagnostics(report);
                    return;
                }
                EditorEvent::RetryImage {
                    source,
                    document_directory,
                } => {
                    let resource = image_resource(source, document_directory.as_deref());
                    this.image_cache.update(cx, |cache, _| {
                        cache.retry_failed(&resource);
                    });
                }
            }
            cx.notify();
        })
        .detach();
        let outline = Arc::new(project_outline(
            editor.read(cx).document().snapshot().blocks(),
        ));
        let mut this = Self {
            editor,
            image_cache,
            session_registry,
            filename,
            source_path: None,
            source_identity: None,
            recovery,
            recovery_key,
            recovery_generation: 0,
            recovery_in_flight: false,
            recovery_dirty: false,
            workspace_state_store,
            workspace_state_dirty: false,
            last_open_directory: None,
            file_chooser_open: false,
            workspace_state_in_flight: false,
            workspace_state_generation: 0,
            pending_view_state: None,
            unsaved: false,
            saved_revision: Revision::default(),
            autosave_generation: 0,
            save_in_flight: false,
            reload_in_flight: false,
            document_epoch: 0,
            open_request: 0,
            reload_request: 0,
            pending_reload: None,
            close_prompt_in_flight: false,
            close_after_save: false,
            force_close: false,
            external_watch_cancel: None,
            external_change_pending: false,
            conflict: false,
            recovery_entry: None,
            navigation_nodes: Vec::new(),
            expanded_folders: HashSet::new(),
            navigation_request: 0,
            navigation_root,
            navigation_root_explicit,
            outline,
            navigation_overlay: false,
            navigation_collapsed: false,
            navigation_width: 224.,
            navigation_dragging: false,
            navigation_split: 0.6,
            navigation_split_dragging: false,
            outline_in_flight: false,
            outline_requested: None,
            outline_request: 0,
            outline_cancel_epoch: Arc::new(AtomicU64::new(0)),
            startup_error: initial_file.as_ref().map(|_| "Loading document…".into()),
            startup_config: startup,
            startup_started_at,
            startup_trace_started_at,
            startup_completion,
            window_handle: window.window_handle(),
            startup_source_bytes: 0,
            startup_ready: None,
        };
        let window_entity = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            window_entity
                .update(cx, |this, cx| this.handle_window_should_close(window, cx))
                .unwrap_or(true)
        });
        this.load_workspace_state(path.is_none(), cx);
        if let Some(preload) = initial_preload {
            this.accept_initial_preload(preload, cx);
        } else if let Some(path) = initial_file {
            let entity = cx.entity();
            cx.defer(move |cx| {
                entity.update(cx, |this, cx| this.open_file(path, cx));
            });
        } else if let Some(directory) = initial_directory {
            let entity = cx.entity();
            cx.defer(move |cx| {
                entity.update(cx, |this, cx| {
                    this.load_navigation(directory, true, cx);
                });
            });
        }
        if let Some(config) = performance {
            this.spawn_performance_run(config, window, cx);
        }
        startup_trace(startup_trace_started_at, "view-new-end");
        this
    }

    fn accept_initial_preload(&mut self, preload: InitialPreload, cx: &mut gpui::Context<Self>) {
        let ticket = self.begin_open_ticket(cx);
        match preload.try_recv() {
            Ok(result) => self.finish_document_load(result, ticket, None, cx),
            Err(TryRecvError::Disconnected) => self.finish_document_load(
                Err("initial document loader stopped unexpectedly".into()),
                ticket,
                None,
                cx,
            ),
            Err(TryRecvError::Empty) => {
                self.reload_in_flight = true;
                let wait = cx
                    .background_executor()
                    .spawn_dedicated(move |_| async move {
                        preload.recv().unwrap_or_else(|_| {
                            Err("initial document loader stopped unexpectedly".into())
                        })
                    });
                cx.spawn(async move |this, cx| {
                    let result = wait.await;
                    let _ = this.update(cx, |this, cx| {
                        this.finish_document_load(result, ticket, None, cx);
                    });
                })
                .detach();
            }
        }
    }

    fn begin_open_ticket(&mut self, cx: &gpui::Context<Self>) -> DocumentCompletionTicket {
        self.open_request = self.open_request.wrapping_add(1);
        DocumentCompletionTicket {
            request: self.open_request,
            epoch: self.document_epoch,
            revision: self.editor.read(cx).document().snapshot().revision(),
            source_path: self.source_path.clone(),
        }
    }

    fn document_matches_ticket(
        &self,
        ticket: &DocumentCompletionTicket,
        cx: &gpui::Context<Self>,
    ) -> bool {
        self.document_epoch == ticket.epoch
            && self.source_path == ticket.source_path
            && self.editor.read(cx).document().snapshot().revision() == ticket.revision
    }

    fn current_document_ticket(&self, cx: &gpui::Context<Self>) -> DocumentCompletionTicket {
        DocumentCompletionTicket {
            request: self.open_request,
            epoch: self.document_epoch,
            revision: self.editor.read(cx).document().snapshot().revision(),
            source_path: self.source_path.clone(),
        }
    }

    fn handle_window_should_close(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if self.force_close {
            self.detach_active_session(cx);
            return true;
        }
        if self.unsaved || self.save_in_flight {
            self.request_close(window, cx);
            return false;
        }
        true
    }

    fn request_close(&mut self, window: &mut gpui::Window, cx: &mut gpui::Context<Self>) {
        if !self.unsaved && !self.save_in_flight {
            self.force_close = true;
            self.detach_active_session(cx);
            window.remove_window();
            return;
        }
        if self.close_prompt_in_flight {
            return;
        }
        self.close_prompt_in_flight = true;
        let answer = window.prompt(
            PromptLevel::Warning,
            "Save changes before closing?",
            Some("Unsaved changes will be lost if this window is closed."),
            &["Save", "Discard", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, window| {
            let answer = answer.await.ok();
            let _ = this.update_in(window, |this, window, cx| {
                this.close_prompt_in_flight = false;
                match answer {
                    Some(0) => {
                        this.close_after_save = true;
                        this.queue_save(true, cx);
                    }
                    Some(1) => {
                        this.close_after_save = false;
                        this.force_close = true;
                        this.detach_active_session(cx);
                        window.remove_window();
                    }
                    _ => {
                        this.close_after_save = false;
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn finish_pending_close(&mut self, cx: &mut gpui::Context<Self>) {
        if !self.close_after_save || self.unsaved || self.save_in_flight {
            return;
        }
        self.close_after_save = false;
        self.force_close = true;
        self.detach_active_session(cx);
        let _ = self
            .window_handle
            .update(cx, |_, window, _| window.remove_window());
    }

    fn detach_active_session(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(path) = self.source_path.as_deref() {
            self.session_registry
                .borrow_mut()
                .detach(path, cx.entity_id());
        }
    }

    fn new_document(&mut self, cx: &mut gpui::Context<Self>) {
        if self.unsaved || self.save_in_flight {
            self.startup_error =
                Some("Save or discard the current changes before creating a new document.".into());
            cx.notify();
            return;
        }
        self.detach_active_session(cx);
        self.open_request = self.open_request.wrapping_add(1);
        self.reload_request = self.reload_request.wrapping_add(1);
        self.document_epoch = self.document_epoch.wrapping_add(1);
        self.autosave_generation = self.autosave_generation.wrapping_add(1);
        self.outline_cancel_epoch.fetch_add(1, Ordering::AcqRel);
        if let Some(cancel) = self.external_watch_cancel.take() {
            let _ = cancel.send(());
        }
        self.editor.update(cx, |editor, cx| {
            editor.set_document_directory(None, cx);
            editor.replace_document(Document::empty(), cx);
        });
        self.filename = "Untitled.md".into();
        self.source_path = None;
        self.source_identity = None;
        self.recovery_key = new_untitled_recovery_key();
        self.recovery_generation = self.recovery_generation.wrapping_add(1);
        self.recovery_dirty = false;
        self.pending_view_state = None;
        self.pending_reload = None;
        self.recovery_entry = None;
        self.reload_in_flight = false;
        self.conflict = false;
        self.startup_error = None;
        self.saved_revision = self.editor.read(cx).document().snapshot().revision();
        self.unsaved = false;
        self.refresh_outline(cx);
        self.queue_workspace_state(cx);
        cx.notify();
    }

    fn prompt_open_file(&mut self, cx: &mut gpui::Context<Self>) {
        if self.file_chooser_open {
            return;
        }
        self.file_chooser_open = true;
        let ticket = self.current_document_ticket(cx);
        let candidates = self
            .last_open_directory
            .iter()
            .cloned()
            .chain(
                self.source_path
                    .as_deref()
                    .and_then(|path| path.parent())
                    .map(PathBuf::from),
            )
            .chain(self.navigation_root.iter().cloned())
            .collect::<Vec<_>>();
        let directory = cx
            .background_executor()
            .spawn_dedicated(move |_| async move { file_dialog::existing_directory(&candidates) });
        cx.spawn(async move |this, cx| {
            let directory = directory.await;
            if this.update(cx, |_, _| ()).is_err() {
                return;
            }
            let result = file_dialog::open_file(directory.as_deref()).await;
            let _ = this.update(cx, |this, cx| {
                this.file_chooser_open = false;
                match result {
                Ok(Some(path)) => {
                    if !this.document_matches_ticket(&ticket, cx) {
                        this.startup_error = Some(
                            "The document changed while the file chooser was open; no file was opened."
                                .into(),
                        );
                    } else {
                        this.open_file(path, cx);
                    }
                    cx.notify();
                }
                Ok(None) => {}
                Err(error) => {
                    this.startup_error = Some(format!("Could not open the file chooser: {error}"));
                    cx.notify();
                }
                }
            });
        })
        .detach();
    }

    fn prompt_open_folder(&mut self, cx: &mut gpui::Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Open a folder".into()),
        });
        cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.load_navigation(path, true, cx);
                        this.navigation_overlay = false;
                    }
                    cx.notify();
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    this.startup_error =
                        Some(format!("Could not open the folder chooser: {error}"));
                    cx.notify();
                }
                Err(_) => {}
            });
        })
        .detach();
    }

    fn prompt_save_target(&mut self, mode: SaveTargetMode, cx: &mut gpui::Context<Self>) {
        if self.save_in_flight {
            return;
        }
        let ticket = self.current_document_ticket(cx);
        let directory = self
            .source_path
            .as_deref()
            .and_then(std::path::Path::parent)
            .map(PathBuf::from)
            .or_else(|| self.navigation_root.clone())
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let suggested_name = if mode == SaveTargetMode::Copy {
            let stem = self
                .source_path
                .as_deref()
                .and_then(std::path::Path::file_stem)
                .and_then(|stem| stem.to_str())
                .unwrap_or("document");
            format!("{stem}-copy.md")
        } else {
            self.filename.clone()
        };
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested_name));
        cx.spawn(async move |this, cx| {
            let result = receiver.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(Ok(Some(path))) => {
                    if !this.document_matches_ticket(&ticket, cx) {
                        this.close_after_save = false;
                        this.startup_error = Some(
                            "The document changed while the save chooser was open; nothing was written."
                                .into(),
                        );
                        cx.notify();
                    } else {
                        this.save_to_target(path, mode, cx);
                    }
                }
                Ok(Ok(None)) => {
                    this.close_after_save = false;
                }
                Ok(Err(error)) => {
                    this.close_after_save = false;
                    this.startup_error = Some(format!("Could not open the save chooser: {error}"));
                    cx.notify();
                }
                Err(_) => {
                    this.close_after_save = false;
                }
            });
        })
        .detach();
    }

    fn save_to_target(
        &mut self,
        target: PathBuf,
        mode: SaveTargetMode,
        cx: &mut gpui::Context<Self>,
    ) {
        let document = self.editor.read(cx).shared_session();
        if mode == SaveTargetMode::Adopt
            && self
                .source_path
                .as_ref()
                .is_some_and(|path| path != &target)
            && self.source_path.as_deref().is_some_and(|path| {
                self.session_registry
                    .borrow_mut()
                    .has_other_listener(path, cx.entity_id())
            })
        {
            self.close_after_save = false;
            self.startup_error = Some(
                "This document is open in another window. Close the other view before changing its file path."
                    .into(),
            );
            cx.notify();
            return;
        }
        if mode == SaveTargetMode::Adopt
            && self
                .session_registry
                .borrow()
                .contains_other(&target, &document)
        {
            self.close_after_save = false;
            self.startup_error = Some(
                "That file is already open in another window; choose a different path.".into(),
            );
            cx.notify();
            return;
        }
        let snapshot = document.snapshot();
        let revision = snapshot.revision();
        let epoch = self.document_epoch;
        let source_path = self.source_path.clone();
        let recovery_key = self.recovery_key.clone();
        let recovery = self.recovery.clone();
        let base_identity = self.source_identity.clone();
        self.save_in_flight = true;
        let save = cx
            .background_executor()
            .spawn_dedicated(move |_| async move {
                let markdown = snapshot.serialize()?;
                let entry = RecoveryEntry::new(
                    recovery_key.clone(),
                    revision,
                    markdown.clone(),
                    base_identity,
                );
                let mut recovery_warning = recovery.write(&entry).err();
                let saved = match source_identity(&target) {
                    Ok(identity) => {
                        let save_snapshot = document_core::SaveSnapshot {
                            revision,
                            bytes: markdown.as_bytes().into(),
                            expected_identity: Some(identity),
                        };
                        atomic_save(save_snapshot).map(|identity| (identity.path.clone(), identity))
                    }
                    Err(PersistenceError::Io { source, .. })
                        if source.kind() == std::io::ErrorKind::NotFound =>
                    {
                        atomic_write_new(&target, markdown.as_bytes())
                            .map(|identity| (identity.path.clone(), identity))
                    }
                    Err(error) => Err(error),
                }
                .map_err(SaveTargetError::from)?;
                if mode == SaveTargetMode::Adopt
                    && let Err(error) = recovery.clear_revision(&recovery_key, revision)
                {
                    recovery_warning = Some(error);
                }
                Ok::<_, SaveTargetError>((saved.0, saved.1, recovery_warning))
            });
        cx.spawn(async move |this, cx| {
            let result = save.await;
            let _ = this.update(cx, |this, cx| {
                if this.document_epoch != epoch || this.source_path != source_path {
                    return;
                }
                this.save_in_flight = false;
                match result {
                    Ok((path, _identity, recovery_warning)) if mode == SaveTargetMode::Copy => {
                        this.startup_error = Some(match recovery_warning {
                            Some(error) => format!(
                                "Saved a copy to {}, but recovery storage failed: {error}",
                                path.display()
                            ),
                            None => format!("Saved a copy to {}", path.display()),
                        });
                    }
                    Ok((path, identity, recovery_warning)) => {
                        let document = this.editor.read(cx).shared_session();
                        let previous_path = this.source_path.clone();
                        {
                            let mut registry = this.session_registry.borrow_mut();
                            if !registry.adopt(
                                path.clone(),
                                document.clone(),
                                identity.clone(),
                                revision,
                            ) {
                                this.close_after_save = false;
                                this.startup_error = Some(
                                    "The file was written, but another window opened that path before it could be attached."
                                        .into(),
                                );
                                cx.notify();
                                return;
                            }
                            if let Some(previous_path) = previous_path.as_deref()
                                && previous_path != path
                            {
                                registry.forget_if_same(previous_path, &document);
                            }
                            registry.subscribe(
                                &path,
                                SessionListener {
                                    id: cx.entity_id(),
                                    window: cx.entity().downgrade(),
                                },
                            );
                        }
                        this.document_epoch = this.document_epoch.wrapping_add(1);
                        this.source_path = Some(path.clone());
                        this.recovery_key = path.clone();
                        this.recovery_generation = this.recovery_generation.wrapping_add(1);
                        this.recovery_dirty = false;
                        this.source_identity = Some(identity);
                        this.saved_revision = revision;
                        this.filename = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .unwrap_or("document.md")
                            .to_owned();
                        this.editor.update(cx, |editor, cx| {
                            editor.set_document_directory(path.parent().map(PathBuf::from), cx);
                        });
                        this.unsaved = this.editor.read(cx).document().snapshot().revision()
                            != revision;
                        this.conflict = false;
                        this.recovery_entry = None;
                        this.startup_error = recovery_warning.map(|error| {
                            format!("Saved, but stale recovery data could not be cleared: {error}")
                        });
                        if !this.navigation_root_explicit
                            && let Some(parent) = path.parent().map(PathBuf::from)
                        {
                            this.load_navigation(parent, false, cx);
                        }
                        this.reconcile_external_watch(cx);
                        this.queue_workspace_state(cx);
                        if this.unsaved {
                            this.close_after_save = false;
                            this.schedule_autosave(cx);
                            this.startup_error = Some(
                                "Saved, but newer edits remain in the document.".into(),
                            );
                        } else {
                            this.finish_pending_close(cx);
                        }
                    }
                    Err(error) => {
                        this.close_after_save = false;
                        this.startup_error = Some(error.to_string());
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn finish_startup_measurement(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(ready) = self.startup_ready.take() else {
            return;
        };
        startup_trace(self.startup_trace_started_at, "first-editable-frame");
        let report = performance::StartupReport::capture(
            &ready.config,
            SystemTime::now()
                .duration_since(self.startup_started_at)
                .unwrap_or_default(),
            self.startup_source_bytes,
            ready.viewport_width,
            ready.viewport_height,
            ready.scale_factor,
        );
        let report_result = performance::write_startup_report(&ready.config.output, &report);
        match &report_result {
            Ok(()) => eprintln!(
                "startup report written to {}",
                ready.config.output.display()
            ),
            Err(error) => eprintln!("startup report failed: {error}"),
        }
        schedule_startup_completion(ready.completion, ready.window, report_result, cx);
    }

    fn spawn_performance_run(
        &mut self,
        config: performance::PerformanceConfig,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn_in(window, async move |_this, cx| {
            let sustain_until = Instant::now() + config.warmup + config.duration;
            if let Err(error) = cx.update(|window, _| {
                performance::sustain_frames(window, sustain_until);
            }) {
                eprintln!("performance warmup could not start: {error}");
                return;
            }
            cx.background_executor().timer(config.warmup).await;
            let started_at = Instant::now();
            let baseline =
                match cx.update(|window, _| performance::PerformanceCapture::start(window)) {
                    Ok(baseline) => baseline,
                    Err(error) => {
                        eprintln!("performance run could not start: {error}");
                        return;
                    }
                };

            if config.exercise_resize {
                let interval = config.duration / 3;
                cx.background_executor().timer(interval).await;
                let _ = cx.update(|window, _| window.resize(size(px(960.), px(680.))));
                cx.background_executor().timer(interval).await;
                let _ = cx.update(|window, _| window.resize(size(px(1200.), px(800.))));
                cx.background_executor()
                    .timer(config.duration.saturating_sub(interval.saturating_mul(2)))
                    .await;
            } else {
                cx.background_executor().timer(config.duration).await;
            }
            // Leave one presentation interval for the final requested frame to drain.
            cx.background_executor()
                .timer(config.refresh_period().saturating_mul(2))
                .await;
            let result = cx.update(|window, _| {
                performance::PerformanceCapture::finish(
                    baseline,
                    window,
                    &config,
                    started_at.elapsed(),
                )
            });
            match result {
                Ok(Ok(report)) => {
                    let output = config.output.clone();
                    let write_result = cx
                        .background_executor()
                        .spawn(async move { performance::write_report(&output, &report) })
                        .await;
                    match write_result {
                        Ok(()) => {
                            eprintln!("performance report written to {}", config.output.display())
                        }
                        Err(error) => eprintln!("performance report failed: {error}"),
                    }
                }
                Ok(Err(error)) => eprintln!("performance run failed: {error}"),
                Err(error) => eprintln!("performance window closed early: {error}"),
            }
            let _ = cx.update(|_, cx| cx.quit());
        })
        .detach();
    }

    fn broadcast_session_state(
        &mut self,
        path: PathBuf,
        clear_conflict: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let document = self.editor.read(cx).shared_session();
        let (listeners, status) = {
            let mut registry = self.session_registry.borrow_mut();
            (registry.listeners(&path), registry.status(&path))
        };
        let Some((identity, saved_revision, dirty)) = status else {
            return;
        };
        let own_id = cx.entity_id();
        for listener in listeners {
            if listener.id == own_id {
                continue;
            }
            let path = path.clone();
            let document = document.clone();
            let identity = identity.clone();
            let _ = listener.window.update(cx, move |window, cx| {
                if window.source_path.as_deref() != Some(path.as_path())
                    || !window.editor.read(cx).shared_session().ptr_eq(&document)
                {
                    return;
                }
                let projection_changed = window
                    .editor
                    .update(cx, |editor, cx| editor.sync_shared_session(cx));
                window.source_identity = Some(identity);
                window.saved_revision = saved_revision;
                window.unsaved = dirty;
                if clear_conflict {
                    window.conflict = false;
                    window.startup_error = None;
                }
                if projection_changed {
                    window.refresh_outline(cx);
                }
                window.schedule_workspace_state(cx);
                cx.notify();
            });
        }
    }

    fn load_workspace_state(&mut self, restore_active: bool, cx: &mut gpui::Context<Self>) {
        let store = self.workspace_state_store.clone();
        let load = cx
            .background_executor()
            .spawn_dedicated(move |_| async move { store.load() });
        cx.spawn(async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| match result {
                Ok(state) => {
                    if this.last_open_directory.is_none() {
                        this.last_open_directory = state.last_open_directory.or_else(|| {
                            state
                                .active_path
                                .as_deref()
                                .and_then(|path| path.parent())
                                .map(PathBuf::from)
                        });
                    }
                    this.navigation_width = clamp_navigation_width(state.navigation_width);
                    this.navigation_split = clamp_navigation_split(state.navigation_split);
                    this.expanded_folders = state.expanded_folders.into_iter().collect();
                    if !this.navigation_root_explicit
                        && let Some(root) = state.navigation_root
                    {
                        this.navigation_root = Some(root);
                        this.navigation_root_explicit = true;
                    }
                    if restore_active && let Some(path) = state.active_path {
                        this.pending_view_state = Some(EditorViewState {
                            selection: state.selection_start..state.selection_end,
                            reversed: state.selection_reversed,
                            scroll_y: state.scroll_y,
                            scroll_anchor: state.scroll_anchor_node.map(|node_id| {
                                EditorScrollAnchor {
                                    node_id,
                                    node_text_hint: state.scroll_anchor_text_hint,
                                    node_text_offset: state.scroll_anchor_text_offset,
                                    projection_offset: state.scroll_anchor_projection_offset,
                                    intra_line_offset: state.scroll_anchor_intra_line_offset,
                                }
                            }),
                        });
                        this.open_file(path, cx);
                    } else {
                        if restore_active && let Some(key) = state.draft_recovery_key {
                            this.recovery_key = key.clone();
                            this.load_untitled_recovery(key, cx);
                        }
                        if let Some(root) = this.navigation_root.clone() {
                            let explicit = this.navigation_root_explicit;
                            this.load_navigation(root, explicit, cx);
                        }
                        cx.notify();
                    }
                }
                Err(error) => {
                    this.startup_error = Some(error.to_string());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn queue_workspace_state(&mut self, cx: &mut gpui::Context<Self>) {
        self.workspace_state_dirty = true;
        if self.workspace_state_in_flight {
            return;
        }
        self.workspace_state_dirty = false;
        self.workspace_state_in_flight = true;
        let store = self.workspace_state_store.clone();
        let view_state = self.editor.read(cx).view_state();
        let scroll_anchor = view_state.scroll_anchor.as_ref();
        let mut expanded_folders = self.expanded_folders.iter().cloned().collect::<Vec<_>>();
        expanded_folders.sort();
        let state = WorkspaceState {
            last_open_directory: self.last_open_directory.clone(),
            active_path: self.source_path.clone(),
            draft_recovery_key: self
                .source_path
                .is_none()
                .then(|| self.recovery_key.clone()),
            navigation_root: self.navigation_root.clone(),
            navigation_width: self.navigation_width,
            navigation_split: self.navigation_split,
            expanded_folders,
            selection_start: view_state.selection.start,
            selection_end: view_state.selection.end,
            selection_reversed: view_state.reversed,
            scroll_y: view_state.scroll_y,
            scroll_anchor_node: scroll_anchor.map(|anchor| anchor.node_id),
            scroll_anchor_text_hint: scroll_anchor
                .map(|anchor| anchor.node_text_hint.clone())
                .unwrap_or_default(),
            scroll_anchor_text_offset: scroll_anchor.map_or(0, |anchor| anchor.node_text_offset),
            scroll_anchor_projection_offset: scroll_anchor
                .map_or(0, |anchor| anchor.projection_offset),
            scroll_anchor_intra_line_offset: scroll_anchor
                .map_or(0., |anchor| anchor.intra_line_offset),
        };
        let save = cx
            .background_executor()
            .spawn_dedicated(move |_| async move { store.write(&state) });
        cx.spawn(async move |this, cx| {
            let result = save.await;
            let _ = this.update(cx, |this, cx| {
                this.workspace_state_in_flight = false;
                if let Err(error) = result {
                    this.startup_error = Some(error.to_string());
                }
                if this.workspace_state_dirty {
                    this.schedule_workspace_state(cx);
                } else {
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn schedule_workspace_state(&mut self, cx: &mut gpui::Context<Self>) {
        self.workspace_state_dirty = true;
        self.workspace_state_generation = self.workspace_state_generation.wrapping_add(1);
        let generation = self.workspace_state_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(500))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.workspace_state_generation == generation
                    && this.workspace_state_dirty
                    && !this.workspace_state_in_flight
                {
                    this.queue_workspace_state(cx);
                }
            });
        })
        .detach();
    }

    fn schedule_autosave(&mut self, cx: &mut gpui::Context<Self>) {
        self.autosave_generation = self.autosave_generation.wrapping_add(1);
        let generation = self.autosave_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(750))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.autosave_generation == generation && this.unsaved && !this.conflict {
                    this.queue_save(false, cx);
                }
            });
        })
        .detach();
    }

    fn schedule_recovery(&mut self, cx: &mut gpui::Context<Self>) {
        self.recovery_dirty = true;
        self.recovery_generation = self.recovery_generation.wrapping_add(1);
        let generation = self.recovery_generation;
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(250))
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.recovery_generation == generation
                    && this.recovery_dirty
                    && !this.recovery_in_flight
                {
                    this.queue_recovery(cx);
                }
            });
        })
        .detach();
    }

    fn queue_recovery(&mut self, cx: &mut gpui::Context<Self>) {
        if self.recovery_in_flight || !self.recovery_dirty {
            return;
        }
        if self.editor.read(cx).composition_active() {
            self.schedule_recovery(cx);
            return;
        }
        let snapshot = self.editor.read(cx).document().snapshot();
        let revision = snapshot.revision();
        let epoch = self.document_epoch;
        let key = self.recovery_key.clone();
        let completion_key = key.clone();
        let base_identity = self.source_identity.clone();
        let recovery = self.recovery.clone();
        self.recovery_dirty = false;
        self.recovery_in_flight = true;
        let write = cx
            .background_executor()
            .spawn_dedicated(move |_| async move {
                let markdown = snapshot.serialize().map_err(|error| error.to_string())?;
                let entry = RecoveryEntry::new(key, revision, markdown, base_identity);
                recovery.write(&entry).map_err(|error| error.to_string())
            });
        cx.spawn(async move |this, cx| {
            let result = write.await;
            let _ = this.update(cx, |this, cx| {
                this.recovery_in_flight = false;
                if this.document_epoch == epoch {
                    if let Err(error) = result {
                        this.startup_error = Some(format!(
                            "Changes are still in memory, but recovery storage failed: {error}"
                        ));
                    }
                    let current = this.editor.read(cx).document().snapshot().revision();
                    if current != revision {
                        this.recovery_dirty = true;
                    } else if !this.unsaved {
                        this.clear_recovery_revision(completion_key, revision, cx);
                    }
                }
                if this.recovery_dirty {
                    this.schedule_recovery(cx);
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn clear_recovery_revision(
        &self,
        key: PathBuf,
        revision: Revision,
        cx: &mut gpui::Context<Self>,
    ) {
        let recovery = self.recovery.clone();
        cx.background_executor()
            .spawn_dedicated(move |_| async move {
                if let Err(error) = recovery.clear_revision(&key, revision) {
                    eprintln!("recovery cleanup failed for {}: {error}", key.display());
                }
            })
            .detach();
    }

    fn arm_external_watch(&mut self, cx: &mut gpui::Context<Self>) {
        if let Some(cancel) = self.external_watch_cancel.take() {
            let _ = cancel.send(());
        }
        let Some(path) = self.source_path.clone() else {
            return;
        };
        let Some(parent) = path.parent().map(PathBuf::from) else {
            return;
        };
        let (event_sender, event_receiver) = oneshot::channel();
        let mut event_sender = Some(event_sender);
        let watched_path = path.clone();
        let watcher =
            notify::recommended_watcher(move |event: Result<notify::Event, notify::Error>| {
                let relevant = event
                    .as_ref()
                    .map_or(true, |event| event_targets_path(event, &watched_path));
                if relevant && let Some(sender) = event_sender.take() {
                    let _ = sender.send(event);
                }
            });
        let Ok(mut watcher) = watcher else {
            self.startup_error = Some("Could not start the native file watcher".into());
            cx.notify();
            return;
        };
        if let Err(error) = watcher.watch(&parent, notify::RecursiveMode::NonRecursive) {
            self.startup_error = Some(format!("Could not watch {}: {error}", parent.display()));
            cx.notify();
            return;
        }
        let (cancel_sender, cancel_receiver) = oneshot::channel();
        self.external_watch_cancel = Some(cancel_sender);
        cx.spawn(async move |this, cx| {
            let _watcher = watcher;
            match select(event_receiver, cancel_receiver).await {
                Either::Left((event, _)) => {
                    let _ = this.update(cx, |this, cx| {
                        if this.source_path.as_ref() != Some(&path) {
                            return;
                        }
                        match event {
                            Ok(Ok(_)) => this.inspect_external_change(cx),
                            Ok(Err(error)) => {
                                this.startup_error = Some(format!("File watcher failed: {error}"));
                                this.arm_external_watch(cx);
                            }
                            Err(_) => this.arm_external_watch(cx),
                        }
                    });
                }
                Either::Right(_) => {}
            }
        })
        .detach();
    }

    fn inspect_external_change(&mut self, cx: &mut gpui::Context<Self>) {
        if self.save_in_flight || self.reload_in_flight {
            self.external_change_pending = true;
            self.arm_external_watch(cx);
            return;
        }
        let (Some(path), Some(expected)) = (self.source_path.clone(), self.source_identity.clone())
        else {
            return;
        };
        let path_for_check = path.clone();
        let expected_for_check = expected.clone();
        let check = cx
            .background_executor()
            .spawn_dedicated(move |_| async move {
                detect_external_state(&path_for_check, &expected_for_check)
            });
        cx.spawn(async move |this, cx| {
            let state = check.await;
            let _ = this.update(cx, |this, cx| {
                if this.source_path.as_ref() != Some(&path)
                    || this.source_identity.as_ref() != Some(&expected)
                {
                    this.arm_external_watch(cx);
                    return;
                }
                match state {
                    Ok(ExternalState::Unchanged) => this.arm_external_watch(cx),
                    Ok(ExternalState::Modified(_)) if this.unsaved => {
                        this.conflict = true;
                        this.startup_error = Some(
                            "File changed outside Mineral. Choose Reload, Save copy, or Overwrite."
                                .into(),
                        );
                        this.arm_external_watch(cx);
                        cx.notify();
                    }
                    Ok(ExternalState::Modified(_)) => this.queue_reload(false, cx),
                    Ok(ExternalState::Deleted) => {
                        this.conflict = true;
                        this.startup_error = Some(
                            "The source file was renamed or deleted. Save a copy to preserve this document."
                                .into(),
                        );
                        this.arm_external_watch(cx);
                        cx.notify();
                    }
                    Err(error) => {
                        this.startup_error = Some(error.to_string());
                        this.arm_external_watch(cx);
                        cx.notify();
                    }
                }
            });
        })
        .detach();
    }

    fn reconcile_external_watch(&mut self, cx: &mut gpui::Context<Self>) {
        if self.external_change_pending {
            self.external_change_pending = false;
            self.inspect_external_change(cx);
        } else {
            self.arm_external_watch(cx);
        }
    }

    fn queue_reload(&mut self, force: bool, cx: &mut gpui::Context<Self>) {
        if self.reload_in_flight {
            return;
        }
        let Some(path) = self.source_path.clone() else {
            return;
        };
        let view_state = self.editor.read(cx).view_state();
        let revision = self.editor.read(cx).document().snapshot().revision();
        let epoch = self.document_epoch;
        self.reload_request = self.reload_request.wrapping_add(1);
        let request = self.reload_request;
        self.reload_in_flight = true;
        let load_path = path.clone();
        let load = cx
            .background_executor()
            .spawn_dedicated(move |_| async move {
                let (source, identity) =
                    read_source_with_identity(&load_path).map_err(|error| error.to_string())?;
                let document =
                    Document::from_markdown(source).map_err(|error| error.to_string())?;
                let prepared = PreparedDocumentView::prepare(&document);
                Ok::<_, String>((document, prepared, identity))
            });
        cx.spawn(async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                if this.reload_request != request {
                    return;
                }
                this.reload_in_flight = false;
                match result {
                    Ok((document, prepared, identity))
                        if this.source_path.as_ref() == Some(&path)
                            && this.document_epoch == epoch
                            && this.editor.read(cx).document().snapshot().revision() == revision
                            && (force || !this.unsaved) =>
                    {
                        this.editor.update(cx, |editor, cx| {
                            editor.replace_document_prepared(document, prepared, cx);
                            editor.restore_view_state(&view_state, cx);
                        });
                        this.document_epoch = this.document_epoch.wrapping_add(1);
                        this.pending_reload = None;
                        this.session_registry
                            .borrow_mut()
                            .mark_reloaded(&path, identity.clone());
                        this.source_identity = Some(identity);
                        this.saved_revision = this.editor.read(cx).document().snapshot().revision();
                        this.refresh_outline(cx);
                        this.unsaved = false;
                        this.conflict = false;
                        this.autosave_generation = this.autosave_generation.wrapping_add(1);
                        this.startup_error = None;
                        this.broadcast_session_state(path.clone(), true, cx);
                    }
                    Ok((document, prepared, identity))
                        if this.source_path.as_ref() == Some(&path)
                            && this.document_epoch == epoch =>
                    {
                        this.pending_reload = Some(ReloadCandidate {
                            document,
                            prepared,
                            identity,
                        });
                        this.conflict = true;
                        this.startup_error = Some(
                            "The document changed while reload was running. Current edits were kept; choose Reload again to use the disk version."
                                .into(),
                        );
                    }
                    Ok(_) => {}
                    Err(error) => this.startup_error = Some(error),
                }
                this.reconcile_external_watch(cx);
                cx.notify();
            });
        })
        .detach();
    }

    fn reload_external(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(candidate) = self.pending_reload.take() else {
            self.queue_reload(true, cx);
            return;
        };
        let Some(path) = self.source_path.clone() else {
            return;
        };
        let view_state = self.editor.read(cx).view_state();
        self.editor.update(cx, |editor, cx| {
            editor.replace_document_prepared(candidate.document, candidate.prepared, cx);
            editor.restore_view_state(&view_state, cx);
        });
        self.document_epoch = self.document_epoch.wrapping_add(1);
        self.session_registry
            .borrow_mut()
            .mark_reloaded(&path, candidate.identity.clone());
        self.source_identity = Some(candidate.identity);
        self.saved_revision = self.editor.read(cx).document().snapshot().revision();
        self.unsaved = false;
        self.conflict = false;
        self.autosave_generation = self.autosave_generation.wrapping_add(1);
        self.startup_error = None;
        self.refresh_outline(cx);
        self.broadcast_session_state(path, true, cx);
        self.reconcile_external_watch(cx);
        cx.notify();
    }

    fn restore_recovery(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(entry) = self.recovery_entry.take() else {
            return;
        };
        if self.reload_in_flight {
            self.recovery_entry = Some(entry);
            return;
        }
        let epoch = self.document_epoch;
        let revision = self.editor.read(cx).document().snapshot().revision();
        self.reload_request = self.reload_request.wrapping_add(1);
        let request = self.reload_request;
        self.reload_in_flight = true;
        let parse_entry = entry.clone();
        let parse = cx.background_executor().spawn(async move {
            let document =
                Document::from_markdown(parse_entry.markdown).map_err(|error| error.to_string())?;
            let prepared = PreparedDocumentView::prepare(&document);
            Ok::<_, String>((document, prepared))
        });
        cx.spawn(async move |this, cx| {
            let result = parse.await;
            let _ = this.update(cx, |this, cx| {
                if this.reload_request != request
                    || this.document_epoch != epoch
                    || this.editor.read(cx).document().snapshot().revision() != revision
                {
                    this.recovery_entry = Some(entry);
                    this.reload_in_flight = false;
                    this.startup_error = Some(
                        "The document changed while recovery was loading; current edits were kept."
                            .into(),
                    );
                    cx.notify();
                    return;
                }
                this.reload_in_flight = false;
                match result {
                    Ok((document, prepared)) => {
                        this.editor.update(cx, |editor, cx| {
                            editor.replace_document_prepared(document, prepared, cx);
                        });
                        this.document_epoch = this.document_epoch.wrapping_add(1);
                        this.unsaved = true;
                        if let Some(path) = this.source_path.clone() {
                            this.session_registry.borrow_mut().mark_dirty(&path);
                            this.broadcast_session_state(path, false, cx);
                        }
                        this.refresh_outline(cx);
                        this.schedule_workspace_state(cx);
                        this.conflict = this.source_path.is_some();
                        this.startup_error = Some(if this.conflict {
                            "Recovered draft loaded. Review it, then choose Overwrite, Save copy, or Reload."
                                .into()
                        } else {
                            "Recovered untitled draft loaded. Review it, then choose Save As."
                                .into()
                        });
                    }
                    Err(error) => this.startup_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_untitled_recovery(&mut self, key: PathBuf, cx: &mut gpui::Context<Self>) {
        let recovery = self.recovery.clone();
        let expected_key = key.clone();
        let load = cx
            .background_executor()
            .spawn_dedicated(move |_| async move { recovery.load(&key) });
        cx.spawn(async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                if this.source_path.is_some() || this.recovery_key != expected_key {
                    return;
                }
                match result {
                    Ok(Some(entry)) => {
                        this.recovery_entry = Some(entry);
                        this.startup_error = Some(
                            "An unsaved draft from the previous session is available.".into(),
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        this.startup_error = Some(format!(
                            "The empty document opened, but its recovery record could not be read: {error}"
                        ));
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn open_file(&mut self, path: PathBuf, cx: &mut gpui::Context<Self>) {
        self.open_file_at(path, None, cx);
    }

    fn reveal_heading(&mut self, fragment: &str, cx: &mut gpui::Context<Self>) {
        let editor = self.editor.clone();
        let found = self
            .window_handle
            .update(cx, |_, window, cx| {
                editor.update(cx, |editor, cx| {
                    editor.navigate_to_heading(fragment, window, cx)
                })
            })
            .unwrap_or(false);
        if !found {
            self.startup_error = Some(format!("Heading not found: #{fragment}"));
        }
        cx.notify();
    }

    fn open_file_at(
        &mut self,
        path: PathBuf,
        fragment: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        startup_trace(self.startup_trace_started_at, "open-file-start");
        // An explicit link back into this file is navigation, not a reload;
        // it remains usable with unsaved edits and preserves content history.
        if self.source_path.as_ref() == Some(&path) {
            if let Some(fragment) = fragment.as_deref() {
                self.reveal_heading(fragment, cx);
            }
            self.navigation_overlay = false;
            return;
        }
        if self.unsaved {
            self.startup_error =
                Some("Save or resolve the current document before opening another file.".into());
            cx.notify();
            return;
        }
        if self.reload_in_flight {
            self.navigation_overlay = false;
            return;
        }
        let ticket = self.begin_open_ticket(cx);
        self.reload_in_flight = true;
        let load_path = path.clone();
        let recovery = self.recovery.clone();
        let trace_started_at = self.startup_trace_started_at;
        let load = cx
            .background_executor()
            .spawn_dedicated(move |_| async move {
                load_document(load_path, recovery, trace_started_at)
            });
        cx.spawn(async move |this, cx| {
            let result = load.await;
            let _ = this.update(cx, |this, cx| {
                this.finish_document_load(result, ticket, fragment, cx);
            });
        })
        .detach();
    }

    fn finish_document_load(
        &mut self,
        result: Result<LoadedDocument, String>,
        ticket: DocumentCompletionTicket,
        fragment: Option<String>,
        cx: &mut gpui::Context<Self>,
    ) {
        let current_revision = self.editor.read(cx).document().snapshot().revision();
        if self.open_request != ticket.request
            || self.document_epoch != ticket.epoch
            || self.source_path != ticket.source_path
            || current_revision != ticket.revision
        {
            if self.open_request == ticket.request {
                self.reload_in_flight = false;
                self.startup_error = Some(
                    "The document changed while another file was opening; the current edits were kept."
                        .into(),
                );
                if self.startup_config.take().is_some()
                    && let Some(completion) = self.startup_completion.take()
                {
                    schedule_startup_completion(
                        completion,
                        self.window_handle,
                        Err("document startup was superseded by a newer edit".into()),
                        cx,
                    );
                }
                cx.notify();
            }
            return;
        }
        self.reload_in_flight = false;
        match result {
            Ok(loaded) => {
                startup_trace(self.startup_trace_started_at, "open-file-install");
                self.detach_active_session(cx);
                let attachment = self.session_registry.borrow_mut().attach(
                    loaded.canonical.clone(),
                    loaded.document,
                    loaded.identity,
                );
                self.session_registry.borrow_mut().subscribe(
                    &loaded.canonical,
                    SessionListener {
                        id: cx.entity_id(),
                        window: cx.entity().downgrade(),
                    },
                );
                let document_directory = loaded.canonical.parent().map(PathBuf::from);
                self.last_open_directory.clone_from(&document_directory);
                let view_state = self.pending_view_state.take();
                let reused_session = attachment.reused;
                self.editor.update(cx, |editor, cx| {
                    editor.set_document_directory(document_directory, cx);
                    if reused_session {
                        editor.attach_shared_session(attachment.document, cx);
                    } else {
                        editor.attach_shared_session_prepared(
                            attachment.document,
                            loaded.prepared,
                            cx,
                        );
                    }
                    if let Some(view_state) = view_state.as_ref() {
                        editor.restore_view_state(view_state, cx);
                    }
                });
                self.filename = loaded
                    .canonical
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("document.md")
                    .to_owned();
                if !self.navigation_root_explicit
                    && let Some(root) = loaded.canonical.parent().map(PathBuf::from)
                {
                    self.load_navigation(root, false, cx);
                }
                self.source_path = Some(loaded.canonical);
                self.recovery_key = self
                    .source_path
                    .clone()
                    .expect("loaded document has a source path");
                self.document_epoch = self.document_epoch.wrapping_add(1);
                self.source_identity = Some(attachment.source_identity);
                self.saved_revision = attachment.saved_revision;
                self.startup_source_bytes = loaded.source_bytes;
                self.recovery_entry = loaded.recovery_entry;
                self.refresh_outline(cx);
                self.unsaved = attachment.dirty;
                self.conflict = false;
                self.autosave_generation = self.autosave_generation.wrapping_add(1);
                self.navigation_overlay = false;
                self.startup_error = loaded.recovery_warning.or_else(|| {
                    self.recovery_entry.as_ref().map(|entry| {
                        format!(
                            "Recovery available for revision {} from a previous session",
                            entry.revision
                        )
                    })
                });
                self.reconcile_external_watch(cx);
                if let Some(fragment) = fragment.as_deref() {
                    self.reveal_heading(fragment, cx);
                }
                self.queue_workspace_state(cx);
            }
            Err(error) => {
                self.pending_view_state = None;
                self.startup_error = Some(error.clone());
                if self.startup_config.take().is_some()
                    && let Some(completion) = self.startup_completion.take()
                {
                    schedule_startup_completion(
                        completion,
                        self.window_handle,
                        Err(format!("document startup failed: {error}")),
                        cx,
                    );
                }
            }
        }
        cx.notify();
    }

    fn load_navigation(
        &mut self,
        directory: PathBuf,
        explicit: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        self.navigation_root = Some(directory.clone());
        self.navigation_root_explicit = explicit;
        self.navigation_request = self.navigation_request.wrapping_add(1);
        let request = self.navigation_request;
        let requested_directory = directory.clone();
        let expanded_folders = self.expanded_folders.clone();
        let load = cx
            .background_executor()
            .spawn_dedicated(
                move |_| async move { navigation_children(&directory, &expanded_folders) },
            );
        cx.spawn(async move |this, cx| {
            let navigation_nodes = load.await;
            let _ = this.update(cx, |this, cx| {
                if this.navigation_request == request
                    && this.navigation_root.as_ref() == Some(&requested_directory)
                {
                    match navigation_nodes {
                        Ok(nodes) => {
                            this.navigation_nodes = nodes;
                            this.startup_error = None;
                        }
                        Err(error) => this.startup_error = Some(error),
                    }
                    this.queue_workspace_state(cx);
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn toggle_navigation_directory(&mut self, path: PathBuf, cx: &mut gpui::Context<Self>) {
        let Some(node) = find_navigation_node_mut(&mut self.navigation_nodes, &path) else {
            return;
        };
        let NavigationNodeKind::Directory { expanded, children } = &mut node.kind else {
            return;
        };
        *expanded = !*expanded;
        let is_expanded = *expanded;
        let needs_load = is_expanded && children.is_none();
        if is_expanded {
            self.expanded_folders.insert(path.clone());
        } else {
            self.expanded_folders.remove(&path);
        }
        self.queue_workspace_state(cx);
        cx.notify();
        if !needs_load {
            return;
        }

        let requested_path = path.clone();
        let expanded_folders = self.expanded_folders.clone();
        let load = cx
            .background_executor()
            .spawn_dedicated(move |_| async move {
                navigation_children(&requested_path, &expanded_folders)
            });
        cx.spawn(async move |this, cx| {
            let children = load.await;
            let _ = this.update(cx, |this, cx| {
                match children {
                    Ok(children) => {
                        if let Some(node) =
                            find_navigation_node_mut(&mut this.navigation_nodes, &path)
                            && let NavigationNodeKind::Directory {
                                children: current, ..
                            } = &mut node.kind
                        {
                            *current = Some(children);
                        }
                    }
                    Err(error) => this.startup_error = Some(error),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn overwrite_external(&mut self, cx: &mut gpui::Context<Self>) {
        if self.reload_in_flight {
            return;
        }
        let Some(path) = self.source_path.clone() else {
            return;
        };
        self.reload_in_flight = true;
        let inspect = cx
            .background_executor()
            .spawn_dedicated(move |_| async move { source_identity(&path) });
        cx.spawn(async move |this, cx| {
            let result = inspect.await;
            let _ = this.update(cx, |this, cx| {
                this.reload_in_flight = false;
                match result {
                    Ok(identity) => {
                        this.source_identity = Some(identity);
                        this.conflict = false;
                        this.queue_save(true, cx);
                    }
                    Err(error) => this.startup_error = Some(error.to_string()),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn refresh_outline(&mut self, cx: &mut gpui::Context<Self>) {
        let snapshot = self.editor.read(cx).document().snapshot();
        let revision = snapshot.revision();
        self.outline_requested = Some(revision);
        self.outline_request = self.outline_request.wrapping_add(1);
        let request = self.outline_request;
        let document_epoch = self.document_epoch;
        let epoch = self.outline_cancel_epoch.fetch_add(1, Ordering::AcqRel) + 1;
        if self.outline_in_flight {
            return;
        }
        self.outline_in_flight = true;
        let cancellation = self.outline_cancel_epoch.clone();
        let rebuild = cx
            .background_executor()
            .spawn_dedicated(move |_| async move {
                if cancellation.load(Ordering::Acquire) != epoch {
                    return None;
                }
                let outline = Arc::new(project_outline(snapshot.blocks()));
                (cancellation.load(Ordering::Acquire) == epoch).then_some((revision, outline))
            });
        cx.spawn(async move |this, cx| {
            let result = rebuild.await;
            let _ = this.update(cx, |this, cx| {
                this.outline_in_flight = false;
                if let Some((completed_revision, outline)) = result {
                    let current_revision = this.editor.read(cx).document().snapshot().revision();
                    if outline_completion_is_current(
                        this.outline_request,
                        request,
                        this.document_epoch,
                        document_epoch,
                        current_revision,
                        completed_revision,
                    ) {
                        this.outline = outline;
                        this.outline_requested = None;
                        cx.notify();
                    }
                }
                if this.outline_requested.is_some() {
                    this.refresh_outline(cx);
                }
            });
        })
        .detach();
    }

    fn navigation_resize_start(
        &mut self,
        _: &MouseDownEvent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.navigation_dragging = true;
        window.prevent_default();
        cx.stop_propagation();
    }

    fn navigation_resize_move(
        &mut self,
        event: &MouseMoveEvent,
        _: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.navigation_dragging {
            self.navigation_width = clamp_navigation_width(event.position.x.into());
            cx.notify();
        }
    }

    fn navigation_resize_end(
        &mut self,
        _: &MouseUpEvent,
        _: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.navigation_dragging {
            self.navigation_dragging = false;
            self.queue_workspace_state(cx);
            cx.notify();
        }
    }

    fn navigation_split_start(
        &mut self,
        _: &MouseDownEvent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.navigation_split_dragging = true;
        window.prevent_default();
        cx.stop_propagation();
    }

    fn navigation_split_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.navigation_split_dragging {
            return;
        }
        let window_height: f32 = window.bounds().size.height.into();
        let content_height = (window_height - 68.).max(1.);
        let pointer_y: f32 = event.position.y.into();
        self.navigation_split = clamp_navigation_split((pointer_y - 52.) / content_height);
        cx.notify();
    }

    fn navigation_split_end(
        &mut self,
        _: &MouseUpEvent,
        _: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.navigation_split_dragging {
            self.navigation_split_dragging = false;
            self.queue_workspace_state(cx);
            cx.notify();
        }
    }

    fn adjust_navigation_split(&mut self, delta: f32, cx: &mut gpui::Context<Self>) {
        self.navigation_split = clamp_navigation_split(self.navigation_split + delta);
        self.queue_workspace_state(cx);
        cx.notify();
    }

    fn queue_save(&mut self, explicit: bool, cx: &mut gpui::Context<Self>) {
        if self.save_in_flight || (self.conflict && !explicit) {
            return;
        }
        if self.editor.read(cx).composition_active() {
            if !explicit {
                self.schedule_autosave(cx);
                return;
            }
            let error = self
                .editor
                .update(cx, |editor, cx| editor.commit_pending_composition(cx).err());
            if let Some(error) = error {
                self.startup_error = Some(format!("Could not finish text composition: {error}"));
                cx.notify();
                return;
            }
        }
        let (Some(path), Some(identity)) = (self.source_path.clone(), self.source_identity.clone())
        else {
            if explicit {
                self.prompt_save_target(SaveTargetMode::Adopt, cx);
            }
            return;
        };
        if !self.session_registry.borrow_mut().claim_save(&path) {
            if explicit {
                self.startup_error = Some("This shared document is already being saved".into());
                cx.notify();
            }
            return;
        }
        let snapshot = self.editor.read(cx).document().snapshot();
        let revision = snapshot.revision();
        let epoch = self.document_epoch;
        let recovery = self.recovery.clone();
        let session_registry = self.session_registry.clone();
        self.save_in_flight = true;
        let save = cx
            .background_executor()
            .spawn_dedicated(move |_| async move {
                let save_snapshot = snapshot
                    .save_snapshot(Some(identity))
                    .map_err(SaveJobError::Document)?;
                save_with_recovery(save_snapshot, &recovery)
                    .map(|outcome| (revision, outcome.identity, outcome.cleanup_warning))
                    .map_err(SaveJobError::Persistence)
            });
        cx.spawn(async move |this, cx| {
            let result = save.await;
            match &result {
                Ok((revision, identity, _)) => {
                    session_registry
                        .borrow_mut()
                        .mark_saved(&path, *revision, identity.clone())
                }
                Err(_) => session_registry.borrow_mut().release_save(&path),
            }
            let _ = this.update(cx, |this, cx| {
                if this.source_path.as_ref() != Some(&path) || this.document_epoch != epoch {
                    return;
                }
                this.save_in_flight = false;
                match result {
                    Ok((revision, identity, cleanup_warning)) => {
                        this.source_identity = Some(identity);
                        this.saved_revision = revision;
                        this.recovery_generation = this.recovery_generation.wrapping_add(1);
                        this.recovery_dirty = false;
                        let current = this.editor.read(cx).document().snapshot().revision();
                        this.unsaved = current != revision;
                        this.conflict = false;
                        this.recovery_entry = None;
                        this.startup_error = cleanup_warning.map(|warning| {
                            format!(
                                "Saved, but stale recovery data could not be cleared: {warning}"
                            )
                        });
                        if this.unsaved {
                            this.close_after_save = false;
                            this.schedule_autosave(cx);
                        } else {
                            this.finish_pending_close(cx);
                        }
                        this.broadcast_session_state(path.clone(), true, cx);
                        this.reconcile_external_watch(cx);
                    }
                    Err(error) => {
                        this.close_after_save = false;
                        this.conflict = error.is_external_change();
                        this.startup_error = Some(error.to_string());
                        this.reconcile_external_watch(cx);
                    }
                }
                cx.notify();
            });
        })
        .detach();
    }
}

fn event_targets_path(event: &notify::Event, target: &std::path::Path) -> bool {
    event.paths.iter().any(|path| path == target)
}

fn image_resource(source: &str, directory: Option<&std::path::Path>) -> Resource {
    if source.starts_with("http://") || source.starts_with("https://") {
        Resource::Uri(source.to_owned().into())
    } else {
        let path = PathBuf::from(source);
        Resource::Path(
            if path.is_absolute() {
                path
            } else {
                directory.map_or(path.clone(), |directory| directory.join(path))
            }
            .into(),
        )
    }
}

fn navigation_children(
    directory: &std::path::Path,
    expanded_folders: &HashSet<PathBuf>,
) -> Result<Vec<NavigationNode>, String> {
    let entries = std::fs::read_dir(directory)
        .map_err(|error| format!("Could not read {}: {error}", directory.display()))?;
    let mut nodes = entries
        .filter_map(Result::ok)
        .filter_map(|entry| {
            let path = entry.path();
            let file_type = entry.file_type().ok()?;
            let label = path
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("Untitled")
                .to_owned();
            if label.starts_with('.') || label == "target" {
                return None;
            }
            if file_type.is_dir() {
                let expanded = expanded_folders.contains(&path);
                return Some(NavigationNode {
                    path: path.clone(),
                    label,
                    kind: NavigationNodeKind::Directory {
                        expanded,
                        children: expanded
                            .then(|| navigation_children(&path, expanded_folders).ok())
                            .flatten(),
                    },
                });
            }
            let is_markdown = path
                .extension()
                .and_then(|extension| extension.to_str())
                .is_some_and(|extension| {
                    extension.eq_ignore_ascii_case("md")
                        || extension.eq_ignore_ascii_case("markdown")
                });
            (file_type.is_file() && is_markdown).then_some(NavigationNode {
                path,
                label,
                kind: NavigationNodeKind::File,
            })
        })
        .collect::<Vec<_>>();
    nodes.sort_by_cached_key(|node| {
        (
            usize::from(matches!(node.kind, NavigationNodeKind::File)),
            node.label.to_lowercase(),
        )
    });
    Ok(nodes)
}

fn find_navigation_node_mut<'a>(
    nodes: &'a mut [NavigationNode],
    path: &std::path::Path,
) -> Option<&'a mut NavigationNode> {
    for node in nodes {
        if node.path == path {
            return Some(node);
        }
        if let NavigationNodeKind::Directory {
            children: Some(children),
            ..
        } = &mut node.kind
            && let Some(found) = find_navigation_node_mut(children, path)
        {
            return Some(found);
        }
    }
    None
}

fn navigation_rows(nodes: &[NavigationNode]) -> Vec<NavigationRow> {
    fn visit(nodes: &[NavigationNode], depth: usize, rows: &mut Vec<NavigationRow>) {
        for node in nodes {
            let directory_expanded = match &node.kind {
                NavigationNodeKind::Directory { expanded, .. } => Some(*expanded),
                NavigationNodeKind::File => None,
            };
            rows.push(NavigationRow {
                path: node.path.clone(),
                label: node.label.clone(),
                depth,
                directory_expanded,
            });
            if let NavigationNodeKind::Directory {
                expanded: true,
                children: Some(children),
            } = &node.kind
            {
                visit(children, depth + 1, rows);
            }
        }
    }

    let mut rows = Vec::new();
    visit(nodes, 0, &mut rows);
    rows
}

fn clamp_navigation_width(width: f32) -> f32 {
    width.clamp(180., 320.)
}

fn clamp_navigation_split(split: f32) -> f32 {
    split.clamp(0.25, 0.75)
}

fn outline_completion_is_current(
    current_request: u64,
    request: u64,
    current_document_epoch: u64,
    document_epoch: u64,
    current_revision: Revision,
    completed_revision: Revision,
) -> bool {
    current_request == request
        && current_document_epoch == document_epoch
        && current_revision == completed_revision
}

fn navigation_is_visible(wide: bool, collapsed: bool, overlay: bool) -> bool {
    if wide { !collapsed } else { overlay }
}

fn toggle_navigation(wide: bool, collapsed: &mut bool, overlay: &mut bool) {
    if wide {
        *collapsed = !*collapsed;
        *overlay = false;
    } else {
        *overlay = !*overlay;
    }
}

impl Render for MarkdownWindow {
    fn render(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl gpui::IntoElement {
        startup_trace(self.startup_trace_started_at, "root-render-start");
        if self.source_path.is_some()
            && !self.reload_in_flight
            && let Some(config) = self.startup_config.take()
        {
            let viewport = window.viewport_size();
            let completion = self
                .startup_completion
                .take()
                .unwrap_or(StartupCompletion::QuitApplication);
            self.startup_ready = Some(StartupReady {
                config,
                viewport_width: f32::from(viewport.width),
                viewport_height: f32::from(viewport.height),
                scale_factor: window.scale_factor(),
                window: self.window_handle,
                completion,
            });
        }
        let palette = MineralPalette::for_dark(cx.theme().is_dark());
        let width: f32 = window.bounds().size.width.into();
        let wide_navigation = width >= 800.;
        let navigation_visible = navigation_is_visible(
            wide_navigation,
            self.navigation_collapsed,
            self.navigation_overlay,
        );
        let runtime_error = self.editor.read(cx).last_error().map(ToOwned::to_owned);
        let status = runtime_error.or_else(|| self.startup_error.clone());
        let active_path = self.source_path.clone();
        let active_heading = self.editor.read(cx).active_heading_node();
        let files_empty = self.navigation_nodes.is_empty();
        let file_rows = navigation_rows(&self.navigation_nodes)
            .into_iter()
            .enumerate()
            .map(|(index, row)| {
                let path = row.path.clone();
                let is_directory = row.directory_expanded.is_some();
                let active = !is_directory && active_path.as_ref() == Some(&row.path);
                let icon = match row.directory_expanded {
                    Some(true) => IconName::FolderOpen,
                    Some(false) => IconName::FolderClosed,
                    None => IconName::File,
                };
                let accessible_label = if is_directory {
                    format!("Folder {}", row.label)
                } else {
                    format!("Open {}", row.label)
                };
                let keyboard_path = path.clone();
                div()
                    .id(("navigation-file", index))
                    .role(Role::TreeItem)
                    .aria_label(accessible_label)
                    .aria_level(row.depth + 1)
                    .aria_selected(active)
                    .when_some(row.directory_expanded, |item, expanded| {
                        item.aria_expanded(expanded)
                    })
                    .tab_stop(true)
                    .flex()
                    .items_center()
                    .flex_shrink_0()
                    .w_full()
                    .h(px(28.))
                    .pl(px(8. + row.depth as f32 * 16.))
                    .pr(px(8.))
                    .gap(px(7.))
                    .rounded(px(4.))
                    .when(active, |row| {
                        row.bg(rgb(palette.accent_muted))
                            .text_color(rgb(palette.text))
                    })
                    .hover(|row| row.bg(rgb(palette.hover)))
                    .on_key_down(cx.listener(move |this, event: &KeyDownEvent, _, cx| {
                        if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                            if is_directory {
                                this.toggle_navigation_directory(keyboard_path.clone(), cx);
                            } else {
                                this.open_file(keyboard_path.clone(), cx);
                            }
                            cx.stop_propagation();
                        }
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if is_directory {
                            this.toggle_navigation_directory(path.clone(), cx);
                        } else {
                            this.open_file(path.clone(), cx);
                        }
                    }))
                    .child(Icon::new(icon).size_4().flex_shrink_0())
                    .child(
                        div()
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .child(row.label),
                    )
            })
            .collect::<Vec<_>>();
        let outline = self.outline.clone();
        let outline_count = outline.len();
        let outline_entity = cx.entity();
        let outline_rows = uniform_list(
            "outline-scroll",
            outline_count,
            move |range, _window, _cx| {
                range
                    .filter_map(|index| outline.get(index).cloned())
                    .map(|entry| {
                        let node_id = entry.node_id;
                        let keyboard_entity = outline_entity.clone();
                        let click_entity = outline_entity.clone();
                        div()
                            .id(("outline-heading", node_id.get() as usize))
                            .role(Role::TreeItem)
                            .aria_label(entry.title.clone())
                            .aria_level(entry.level as usize)
                            .aria_selected(active_heading == Some(node_id))
                            .tab_stop(true)
                            .flex()
                            .items_center()
                            .flex_shrink_0()
                            .w_full()
                            .h(px(28.))
                            .pl(px(f32::from(entry.level.saturating_sub(1)) * 8.))
                            .pr(px(8.))
                            .rounded(px(4.))
                            .overflow_hidden()
                            .hover(move |row| row.bg(rgb(palette.hover)))
                            .on_key_down(move |event: &KeyDownEvent, window, cx| {
                                if matches!(event.keystroke.key.as_str(), "enter" | "space") {
                                    keyboard_entity.update(cx, |this, cx| {
                                        this.editor.update(cx, |editor, cx| {
                                            editor.jump_to_node(node_id, window, cx);
                                        });
                                        this.navigation_overlay = false;
                                    });
                                    cx.stop_propagation();
                                }
                            })
                            .on_click(move |_, window, cx| {
                                click_entity.update(cx, |this, cx| {
                                    this.editor.update(cx, |editor, cx| {
                                        editor.jump_to_node(node_id, window, cx);
                                    });
                                    this.navigation_overlay = false;
                                });
                            })
                            .child(
                                div()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(if entry.title.is_empty() {
                                        "Untitled heading".to_owned()
                                    } else {
                                        entry.title
                                    }),
                            )
                    })
                    .collect::<Vec<_>>()
            },
        )
        .min_h_0()
        .h_full();
        let outline_rows = div()
            .id("outline-tree")
            .role(Role::Tree)
            .aria_label("Document outline")
            .min_h_0()
            .h_full()
            .when(outline_count == 0, |tree| {
                tree.child(
                    div()
                        .px(px(8.))
                        .py(px(6.))
                        .text_color(rgb(palette.secondary))
                        .child("No headings in this document"),
                )
            })
            .when(outline_count > 0, |tree| tree.child(outline_rows));
        let root_label = self
            .navigation_root
            .as_deref()
            .and_then(|root| root.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Files")
            .to_owned();
        let navigation = div()
            .id("document-navigation")
            .role(Role::Navigation)
            .aria_label("Files and document outline")
            .flex()
            .flex_col()
            .w(px(self.navigation_width))
            .min_w(px(180.))
            .max_w(px(320.))
            .h_full()
            .px(px(12.))
            .py(px(16.))
            .border_r_1()
            .border_color(rgb(palette.border))
            .bg(rgb(palette.panel))
            .text_size(px(13.))
            .text_color(rgb(palette.secondary))
            .when(!wide_navigation, |navigation| {
                navigation
                    .absolute()
                    .top(px(0.))
                    .bottom(px(0.))
                    .left(px(0.))
            })
            .child(
                div()
                    .flex()
                    .flex_col()
                    .h(relative(self.navigation_split))
                    .min_h_0()
                    .gap(px(6.))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(palette.text))
                            .child(root_label),
                    )
                    .child(
                        div()
                            .id("navigation-files-scroll")
                            .role(Role::Tree)
                            .aria_label("Markdown folders and files")
                            .flex()
                            .flex_col()
                            .min_h_0()
                            .overflow_y_scroll()
                            .children(file_rows)
                            .when(files_empty, |tree| {
                                tree.child(
                                    div()
                                        .px(px(8.))
                                        .py(px(6.))
                                        .child("No Markdown files in this folder"),
                                )
                            }),
                    ),
            )
            .child(
                div()
                    .id("navigation-section-split")
                    .role(Role::Slider)
                    .aria_label("Resize files and outline sections")
                    .aria_min_numeric_value(25.)
                    .aria_max_numeric_value(75.)
                    .aria_numeric_value(f64::from(self.navigation_split * 100.))
                    .aria_numeric_value_step(5.)
                    .tab_stop(true)
                    .flex_shrink_0()
                    .h(px(12.))
                    .w_full()
                    .cursor(gpui::CursorStyle::ResizeUpDown)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::navigation_split_start))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::navigation_split_end))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::navigation_split_end))
                    .on_key_down(cx.listener(|this, event: &KeyDownEvent, _, cx| {
                        match event.keystroke.key.as_str() {
                            "up" | "left" => this.adjust_navigation_split(-0.05, cx),
                            "down" | "right" => this.adjust_navigation_split(0.05, cx),
                            _ => return,
                        }
                        cx.stop_propagation();
                    })),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .gap(px(6.))
                    .child(
                        div()
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .text_color(rgb(palette.text))
                            .child("Outline"),
                    )
                    .child(outline_rows),
            );
        let mut body_font = font("Spline Sans Mineral");
        body_font.fallbacks = Some(FontFallbacks::from_fonts(vec![
            "Noto Sans Mineral".into(),
            "Noto Sans".into(),
            "DejaVu Sans".into(),
        ]));
        let zoom_percent = self.editor.read(cx).zoom_percent();
        let zoom_out_editor = self.editor.clone();
        let reset_zoom_editor = self.editor.clone();
        let zoom_in_editor = self.editor.clone();
        let application_menu_focus = self.editor.focus_handle(cx);
        let zoom_controls = div()
            .ml_auto()
            .flex()
            .items_center()
            .gap(px(2.))
            .child(
                Button::new("zoom-out")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Minus)
                    .tooltip("Zoom out — Ctrl+−")
                    .on_click(move |_, _, cx| {
                        zoom_out_editor.update(cx, |editor, cx| editor.zoom_out(cx));
                    }),
            )
            .child(
                Button::new("zoom-reset")
                    .ghost()
                    .xsmall()
                    .label(format!("{zoom_percent}%"))
                    .tooltip("Reset zoom — Ctrl+0")
                    .on_click(move |_, _, cx| {
                        reset_zoom_editor.update(cx, |editor, cx| editor.reset_zoom(cx));
                    }),
            )
            .child(
                Button::new("zoom-in")
                    .ghost()
                    .xsmall()
                    .icon(IconName::Plus)
                    .tooltip("Zoom in — Ctrl++")
                    .on_click(move |_, _, cx| {
                        zoom_in_editor.update(cx, |editor, cx| editor.zoom_in(cx));
                    }),
            );
        let application_menu = Button::new("application-menu")
            .ghost()
            .xsmall()
            .icon(IconName::Menu)
            .tooltip("Application menu")
            .dropdown_menu(move |menu, _window, _cx| {
                menu.action_context(application_menu_focus.clone())
                    .min_w(px(320.))
                    .max_w(px(320.))
                    .item(PopupMenuItem::new("New document").action(Box::new(NewDocumentAction)))
                    .item(PopupMenuItem::new("Open file…").action(Box::new(OpenFileAction)))
                    .item(PopupMenuItem::new("Open folder…").action(Box::new(OpenFolderAction)))
                    .item(PopupMenuItem::separator())
                    .item(PopupMenuItem::new("Save").action(Box::new(SaveDocumentAction)))
                    .item(PopupMenuItem::new("Save as…").action(Box::new(SaveAsAction)))
                    .item(PopupMenuItem::new("Save a copy…").action(Box::new(SaveCopyAction)))
                    .item(PopupMenuItem::separator())
                    .item(PopupMenuItem::new("Undo").action(Box::new(UndoDocumentAction)))
                    .item(PopupMenuItem::new("Redo").action(Box::new(RedoDocumentAction)))
                    .item(
                        PopupMenuItem::new("Find in document…")
                            .icon(IconName::Search)
                            .action(Box::new(FindDocumentAction)),
                    )
                    .item(PopupMenuItem::separator())
                    .item(
                        PopupMenuItem::new("Toggle navigation")
                            .action(Box::new(ToggleNavigationAction)),
                    )
                    .item(PopupMenuItem::separator())
                    .item(PopupMenuItem::new("Zoom in").action(Box::new(ZoomInAction)))
                    .item(PopupMenuItem::new("Zoom out").action(Box::new(ZoomOutAction)))
                    .item(PopupMenuItem::new("Actual size").action(Box::new(ResetZoomAction)))
                    .item(
                        PopupMenuItem::new("Close window")
                            .action(Box::new(CloseDocumentWindowAction)),
                    )
            });

        let scene = div()
            .flex()
            .flex_col()
            .size_full()
            .key_context(WINDOW_KEY_CONTEXT)
            .on_action(cx.listener(|this, _: &NewDocumentAction, _, cx| {
                this.new_document(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenFileAction, _, cx| {
                this.prompt_open_file(cx);
            }))
            .on_action(cx.listener(|this, _: &OpenFolderAction, _, cx| {
                this.prompt_open_folder(cx);
            }))
            .on_action(cx.listener(|this, _: &SaveDocumentAction, _, cx| {
                this.queue_save(true, cx);
            }))
            .on_action(cx.listener(|this, _: &SaveAsAction, _, cx| {
                this.prompt_save_target(SaveTargetMode::Adopt, cx);
            }))
            .on_action(cx.listener(|this, _: &SaveCopyAction, _, cx| {
                this.prompt_save_target(SaveTargetMode::Copy, cx);
            }))
            .on_action(cx.listener(|this, _: &UndoDocumentAction, window, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.perform_undo(window, cx));
            }))
            .on_action(cx.listener(|this, _: &RedoDocumentAction, window, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.perform_redo(window, cx));
            }))
            .on_action(cx.listener(|this, _: &ToggleNavigationAction, window, cx| {
                let width: f32 = window.bounds().size.width.into();
                toggle_navigation(
                    width >= 800.,
                    &mut this.navigation_collapsed,
                    &mut this.navigation_overlay,
                );
                cx.notify();
            }))
            .on_action(cx.listener(|this, _: &FindDocumentAction, window, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.open_find(window, cx));
            }))
            .on_action(cx.listener(|this, _: &FindNextAction, _, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.find_next(false, cx));
            }))
            .on_action(cx.listener(|this, _: &FindPreviousAction, _, cx| {
                this.editor
                    .update(cx, |editor, cx| editor.find_next(true, cx));
            }))
            .on_action(cx.listener(|this, _: &ZoomInAction, _, cx| {
                this.editor.update(cx, |editor, cx| editor.zoom_in(cx));
            }))
            .on_action(cx.listener(|this, _: &ZoomOutAction, _, cx| {
                this.editor.update(cx, |editor, cx| editor.zoom_out(cx));
            }))
            .on_action(cx.listener(|this, _: &ResetZoomAction, _, cx| {
                this.editor.update(cx, |editor, cx| editor.reset_zoom(cx));
            }))
            .on_action(
                cx.listener(|this, _: &CloseDocumentWindowAction, window, cx| {
                    this.request_close(window, cx);
                }),
            )
            .bg(rgb(palette.page))
            .text_color(rgb(palette.text))
            .font(body_font)
            .child(
                TitleBar::new()
                    .bg(rgb(palette.panel))
                    .border_color(rgb(palette.border))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .h_full()
                            .flex_1()
                            .min_w_0()
                            .pr(px(8.))
                            .gap(px(8.))
                            .text_size(px(13.))
                            .child(application_menu)
                            .child(
                                div()
                                    .id("document-window-title")
                                    .aria_label(self.filename.clone())
                                    .tooltip({
                                        let filename = self.filename.clone();
                                        move |window, cx| {
                                            gpui_component::tooltip::Tooltip::new(filename.clone())
                                                .build(window, cx)
                                        }
                                    })
                                    // Do not let a filename's intrinsic width
                                    // push native window controls offscreen.
                                    .flex_1()
                                    .w_0()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .child(self.filename.clone()),
                            )
                            .when(self.unsaved, |header| {
                                header.child(div().text_color(rgb(palette.accent)).child("●"))
                            })
                            .child(zoom_controls)
                            .when_some(status, |header, message| {
                                header.child(
                                    div()
                                        .max_w(px(420.))
                                        .text_color(rgb(palette.error))
                                        .overflow_hidden()
                                        .child(message),
                                )
                            })
                            .when(self.recovery_entry.is_some(), |header| {
                                header.child(
                                    div()
                                        .id("restore-recovery")
                                        .role(Role::Button)
                                        .px(px(8.))
                                        .py(px(4.))
                                        .rounded(px(4.))
                                        .bg(rgb(palette.surface))
                                        .on_click(cx.listener(|this, _, _, cx| {
                                            this.restore_recovery(cx);
                                        }))
                                        .child("Restore"),
                                )
                            })
                            .when(self.conflict, |header| {
                                header
                                    .child(
                                        div()
                                            .id("reload-external")
                                            .role(Role::Button)
                                            .px(px(8.))
                                            .py(px(4.))
                                            .rounded(px(4.))
                                            .bg(rgb(palette.surface))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.reload_external(cx);
                                            }))
                                            .child("Reload"),
                                    )
                                    .child(
                                        div()
                                            .id("save-conflict-copy")
                                            .role(Role::Button)
                                            .px(px(8.))
                                            .py(px(4.))
                                            .rounded(px(4.))
                                            .bg(rgb(palette.surface))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.prompt_save_target(SaveTargetMode::Copy, cx);
                                            }))
                                            .child("Save copy"),
                                    )
                                    .child(
                                        div()
                                            .id("overwrite-external")
                                            .role(Role::Button)
                                            .px(px(8.))
                                            .py(px(4.))
                                            .rounded(px(4.))
                                            .bg(rgb(palette.accent))
                                            .text_color(rgb(palette.panel))
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.overwrite_external(cx);
                                            }))
                                            .child("Overwrite"),
                                    )
                            }),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .relative()
                    .on_mouse_move(cx.listener(Self::navigation_resize_move))
                    .on_mouse_move(cx.listener(Self::navigation_split_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::navigation_resize_end))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::navigation_split_end))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::navigation_resize_end))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::navigation_split_end))
                    .when(navigation_visible, |workspace| workspace.child(navigation))
                    .when(wide_navigation && navigation_visible, |workspace| {
                        workspace.child(
                            div()
                                .id("navigation-resize-handle")
                                .w(px(8.))
                                .h_full()
                                .cursor(gpui::CursorStyle::ResizeLeftRight)
                                .on_mouse_down(
                                    MouseButton::Left,
                                    cx.listener(Self::navigation_resize_start),
                                ),
                        )
                    })
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .min_w_0()
                            .justify_center()
                            .relative()
                            .px(px(28.))
                            .capture_any_mouse_down(cx.listener(|this, _, _, cx| {
                                this.editor.update(cx, |editor, cx| {
                                    editor.stop_momentum();
                                    cx.notify();
                                });
                            }))
                            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                                if event.pressed_button == Some(MouseButton::Left) {
                                    this.editor.update(cx, |_, cx| {
                                        cx.emit(EditorEvent::ViewChanged);
                                        cx.notify();
                                    });
                                }
                            }))
                            .child(
                                div()
                                    .w_full()
                                    .max_w(px(1280.))
                                    .h_full()
                                    .py(px(20.))
                                    .text_size(px(18.))
                                    .line_height(px(28.8))
                                    .child(
                                        image_cache_element(self.image_cache.clone())
                                            .size_full()
                                            .child(self.editor.clone()),
                                    ),
                            )
                            .child(
                                div()
                                    .absolute()
                                    .top(px(20.))
                                    .bottom(px(20.))
                                    .left_0()
                                    .right_0()
                                    .child(
                                        Scrollbar::vertical(&self.editor.read(cx).scroll_handle())
                                            .viewport_from_layout()
                                            .id("document-scrollbar")
                                            .mode(ScrollbarMode::Scrolling),
                                    ),
                            ),
                    ),
            );
        startup_trace(self.startup_trace_started_at, "root-render-end");
        scene
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_identity(path: PathBuf) -> SourceIdentity {
        SourceIdentity {
            path,
            length: 3,
            modified: None,
            content_hash: [0; 32],
            #[cfg(unix)]
            device: 1,
            #[cfg(unix)]
            inode: 2,
        }
    }

    #[test]
    fn duplicate_paths_reuse_one_document_and_save_claim() {
        let path = PathBuf::from("/tmp/mineral-shared.md");
        let identity = test_identity(path.clone());
        let mut registry = SessionRegistry::default();
        let first = registry.attach(
            path.clone(),
            Document::from_markdown("one").expect("document"),
            identity.clone(),
        );
        first
            .document
            .apply(document_core::EditCommand::ReplaceSelection {
                text: "shared ".into(),
                typing: false,
            })
            .expect("edit");
        registry.mark_dirty(&path);
        let second = registry.attach(
            path.clone(),
            Document::from_markdown("stale disk read").expect("document"),
            identity,
        );

        assert!(first.document.ptr_eq(&second.document));
        assert!(second.dirty);
        assert_eq!(
            second.document.snapshot().serialize().expect("serialize"),
            "shared one"
        );
        assert!(registry.claim_save(&path));
        assert!(!registry.claim_save(&path));
        registry.release_save(&path);
        assert!(registry.claim_save(&path));
    }

    #[test]
    fn registry_prunes_clean_sessions_after_the_last_view_releases_them() {
        let first_path = PathBuf::from("/tmp/mineral-pruned-first.md");
        let second_path = PathBuf::from("/tmp/mineral-pruned-second.md");
        let mut registry = SessionRegistry::default();
        let first = registry.attach(
            first_path.clone(),
            Document::from_markdown("first").expect("first document"),
            test_identity(first_path),
        );
        assert_eq!(first.document.strong_count(), 2);
        drop(first);

        let second = registry.attach(
            second_path.clone(),
            Document::from_markdown("second").expect("second document"),
            test_identity(second_path.clone()),
        );
        assert_eq!(registry.sessions.len(), 1);
        assert!(registry.sessions.contains_key(&second_path));
        drop(second);
    }

    #[test]
    fn explicit_detach_releases_idle_session_before_view_clone_drops() {
        let path = PathBuf::from("/tmp/mineral-detached.md");
        let mut registry = SessionRegistry::default();
        let attachment = registry.attach(
            path.clone(),
            Document::from_markdown("clean").expect("document"),
            test_identity(path.clone()),
        );
        assert_eq!(attachment.document.strong_count(), 2);

        registry.detach(&path, EntityId::from(1));

        assert!(!registry.sessions.contains_key(&path));
        assert_eq!(attachment.document.strong_count(), 1);
    }

    #[test]
    fn adopting_a_saved_revision_preserves_newer_dirty_content() {
        let path = PathBuf::from("/tmp/mineral-adopted.md");
        let mut registry = SessionRegistry::default();
        let attachment = registry.attach(
            path.clone(),
            Document::from_markdown("base").expect("document"),
            test_identity(path.clone()),
        );
        let saved_revision = attachment.document.snapshot().revision();
        attachment
            .document
            .apply(document_core::EditCommand::ReplaceSelection {
                text: "new ".into(),
                typing: false,
            })
            .expect("newer edit");

        assert!(registry.adopt(
            path.clone(),
            attachment.document.clone(),
            test_identity(path.clone()),
            saved_revision,
        ));

        let (_, revision, dirty) = registry.status(&path).expect("adopted session");
        assert_eq!(revision, saved_revision);
        assert!(dirty, "an edit newer than the adopted save stays dirty");
    }

    #[test]
    fn native_watch_events_are_filtered_to_the_active_file() {
        let target = PathBuf::from("/tmp/docs/active.md");
        let unrelated = notify::Event::new(notify::EventKind::Any)
            .add_path(PathBuf::from("/tmp/docs/other.md"));
        let atomic_rename = notify::Event::new(notify::EventKind::Any)
            .add_path(PathBuf::from("/tmp/docs/.active.tmp"))
            .add_path(target.clone());
        assert!(!event_targets_path(&unrelated, &target));
        assert!(event_targets_path(&atomic_rename, &target));
    }

    #[test]
    fn outline_completion_rejects_same_revision_from_a_new_document() {
        let revision = Revision(7);
        assert!(outline_completion_is_current(
            3, 3, 9, 9, revision, revision
        ));
        assert!(!outline_completion_is_current(
            3, 3, 10, 9, revision, revision
        ));
        assert!(!outline_completion_is_current(
            4, 3, 9, 9, revision, revision
        ));
        assert!(!outline_completion_is_current(
            3,
            3,
            9,
            9,
            revision,
            Revision(8)
        ));
    }

    #[test]
    fn navigation_width_respects_contract_bounds() {
        assert_eq!(clamp_navigation_width(120.), 180.);
        assert_eq!(clamp_navigation_width(224.), 224.);
        assert_eq!(clamp_navigation_width(480.), 320.);
    }

    #[test]
    fn navigation_split_keeps_both_scroll_regions_usable() {
        assert_eq!(clamp_navigation_split(0.1), 0.25);
        assert_eq!(clamp_navigation_split(0.6), 0.6);
        assert_eq!(clamp_navigation_split(0.95), 0.75);
    }

    #[test]
    fn navigation_toggle_changes_visibility_at_both_breakpoints() {
        let (mut collapsed, mut overlay) = (false, false);
        assert!(navigation_is_visible(true, collapsed, overlay));
        toggle_navigation(true, &mut collapsed, &mut overlay);
        assert!(!navigation_is_visible(true, collapsed, overlay));
        toggle_navigation(true, &mut collapsed, &mut overlay);
        assert!(navigation_is_visible(true, collapsed, overlay));

        assert!(!navigation_is_visible(false, collapsed, overlay));
        toggle_navigation(false, &mut collapsed, &mut overlay);
        assert!(navigation_is_visible(false, collapsed, overlay));
        toggle_navigation(false, &mut collapsed, &mut overlay);
        assert!(!navigation_is_visible(false, collapsed, overlay));
    }

    #[test]
    fn reduced_motion_prefers_explicit_setting_then_gtk_animation_policy() {
        assert!(reduced_motion_from_values(Some("true"), Some("1")));
        assert!(!reduced_motion_from_values(Some("false"), Some("0")));
        assert!(reduced_motion_from_values(None, Some("0")));
        assert!(!reduced_motion_from_values(None, Some("1")));
    }

    #[test]
    fn navigation_only_descends_into_expanded_folders() {
        let root = std::env::temp_dir().join(format!(
            "mineral-navigation-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let notes = root.join("notes");
        std::fs::create_dir_all(&notes).expect("fixture directory");
        std::fs::write(root.join("readme.md"), "# Root").expect("fixture markdown");
        std::fs::write(root.join("ignored.txt"), "not markdown").expect("fixture text");
        std::fs::write(notes.join("nested.markdown"), "# Nested").expect("nested markdown");

        let collapsed = navigation_children(&root, &HashSet::new()).expect("navigation");
        assert_eq!(navigation_rows(&collapsed).len(), 2);
        assert!(matches!(
            collapsed[0].kind,
            NavigationNodeKind::Directory {
                expanded: false,
                children: None
            }
        ));

        let expanded = navigation_children(&root, &HashSet::from([notes])).expect("navigation");
        let rows = navigation_rows(&expanded);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[1].depth, 1);
        assert_eq!(rows[1].label, "nested.markdown");
        std::fs::remove_dir_all(root).expect("cleanup isolated navigation fixture");
    }
}
