# Mineral Markdown

Mineral is a native, Wayland-only Markdown editor written in Rust with GPUI. It
keeps Markdown rendered while it is edited: there is no source pane, preview
mode, account, or cloud service.

The current implementation includes a native title bar, file and folder
choosers, a resizable file/outline navigator, rendered-document minimap, rich
tables and lists, links and images, undo/redo, atomic autosave, external-change
handling, and crash recovery. Light and dark palettes follow the desktop
appearance. Bundled Fraunces, Spline Sans, Spline Sans Mono, and Noto Sans
fonts make rendering independent of the host font set.

## Build and run

Mineral requires Linux with a Wayland compositor, a Vulkan-capable graphics
stack, Rust 1.98, and the native development libraries used by GPUI. On
Ubuntu, the relevant packages are:

```sh
sudo apt-get install build-essential clang cmake git libfontconfig-dev \
  libglib2.0-dev libssl-dev libvulkan1 libwayland-dev libx11-xcb-dev \
  libxkbcommon-x11-dev libzstd-dev pkg-config
```

Build and open a document:

```sh
cargo build --release --locked --bin mineral-markdown
target/release/mineral-markdown path/to/document.md
```

With no path, Mineral restores the last workspace or opens a recoverable
untitled draft.

Run the repository checks with:

```sh
scripts/check.sh
```

The script requires the checked-in lockfile, verifies the two Git source
commits, checks formatting and Clippy with warnings denied, and runs all target
and documentation tests.

## Keyboard commands

| Command | Shortcut |
| --- | --- |
| New document | `Ctrl+N` |
| Open file | `Ctrl+O` |
| Open folder | `Ctrl+Shift+O` |
| Save | `Ctrl+S` |
| Save as | `Ctrl+Shift+S` |
| Save a copy | `Ctrl+Alt+Shift+S` |
| Undo / redo | `Ctrl+Z` / `Ctrl+Shift+Z` |
| Toggle navigation | `Ctrl+Alt+N` |
| Close window | `Ctrl+W` |
| Bold / italic / strike / code | `Ctrl+B` / `Ctrl+I` / `Ctrl+Shift+X` / `Ctrl+E` |
| Link | `Ctrl+K` |
| Paste as Markdown | `Ctrl+Shift+V` |

The divider between Files and Outline is keyboard adjustable after focusing
it. Both navigation sections and the document scroll independently.

## Storage and recovery

Mineral writes documents with a same-directory temporary file and atomic
replacement, retaining file permissions. A 750 ms idle autosave is used for
named files. Recovery state is journaled independently under the XDG state
directory, including untitled drafts. If the source changed outside Mineral,
autosave pauses and the application requires an explicit Reload, Overwrite, or
Save Copy decision.

Application state follows the XDG base-directory convention. Override
`XDG_STATE_HOME` and `XDG_CACHE_HOME` to isolate a run.

## Clipboard interoperability

Copy produces plain text plus Mineral's versioned rich Markdown metadata. A
second Mineral window prefers that rich representation, preserving structure
and formatting; malformed or foreign metadata falls back safely to plain text.
The pinned GPUI Wayland backend currently advertises only text, image, and file
clipboard entries, so HTML and Mineral-specific MIME data are not exported to
other applications as native Wayland MIME types.

## Architecture

- `document-core` owns the rich document model, commands, undo, Markdown/HTML
  import and source-preserving serialization.
- `document-view` owns layout, virtualized rendering, input, hit testing,
  outline, minimap, and accessibility semantics.
- `markdown-app` owns native windows, command routing, navigation, persistence,
  recovery, file watching, image loading, and the release harness.

The product contract is in [plan.md](plan.md). Qualification procedures and
recorded results are under [performance](performance/), while desktop
integration is described in [packaging/README.md](packaging/README.md).

## Reproducible dependency policy

`Cargo.lock` is authoritative and all build/check commands use `--locked`.
GPUI Component is pinned in `Cargo.toml` to
`ff3eb1128ac1058f1bb88e777744ce1237aa3b79`. GPUI, `gpui_platform`, and
`reqwest_client` resolve through both Mineral and GPUI Component's matching Git
source to Zed commit `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`.

When updating either project, update all related entries in one Cargo
operation, inspect `cargo tree -d --locked` for a duplicated GPUI universe,
update `[workspace.metadata.source-pins]`, and run `scripts/check.sh`. Do not
add a `rev` query to only one side of this shared source: Cargo treats distinct
Git source URLs as distinct crates even when they resolve to the same commit.

## License

Mineral is dual-licensed under [MIT](LICENSE-MIT) or
[Apache-2.0](LICENSE-APACHE), at your option. The bundled fonts retain the
licenses recorded in `assets/fonts/`.
