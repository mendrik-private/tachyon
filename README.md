# Mineral

Mineral is a native, Wayland-only Markdown editor written in Rust with GPUI. It
keeps Markdown rendered while it is edited: there is no source pane, preview
mode, account, or cloud service.

The current implementation includes a native title bar, file and folder
choosers, a resizable file/outline navigator, adaptive document layouts, rich
tables and lists, links and images, undo/redo, atomic autosave, external-change
handling, and crash recovery. The current design uses a warm light theme, green
accents, and serif headings. Bundled Fraunces, Spline Sans, Spline Sans Mono, and Noto Sans
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
untitled draft. Open File remembers the folder of the last successfully opened
file across restarts; cancelling the chooser leaves that folder unchanged.

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
| Open link at caret | `Alt+Enter` |
| Find in document | `Ctrl+F` |
| Next / previous find result | `F3` / `Shift+F3` (or `Enter` / `Shift+Enter` in find) |
| Close find and return to document | `Escape` in find |
| Paste as Markdown | `Ctrl+Shift+V` |

The divider between Files and Outline is keyboard adjustable after focusing
it. Both navigation sections and the document scroll independently.

Find follows source order and includes text inside supported HTML disclosures.
Matches in wide tables, code and editable HTML scroll their local content into
view without horizontally shifting the page.
Finding text does not change Markdown or content undo history. HTML results
without a verified editable text target are copyable but read-only; click
editable document text before typing.

Document scrolling eases out after wheel or trackpad input ends. Trackpad
movement stays direct while input continues; its release velocity produces a
short coast. Clicking, editing, reversing direction, or reaching an edge stops
the old motion. Reduced-motion mode uses immediate scrolling without a coast.

## Adaptive layouts

Short independent lists can use two or three columns in row order. Once the
actual canvas is available, the editor measures candidate layouts using its
paint fonts: automatic grids require 3–9 flat items, at most five rendered
lines per item, balanced item heights, and no text overflow. Small changes retain
the previous legal choice. Instructions and explicit cross-step references
use steps; tasks, nested outlines and long uneven items retain a vertical flow.
Layout is always automatic—there are no per-document layout selectors or saved
overrides. Three or four short, structurally matching sibling sections can form
cards; explicit arrow sequences can stack within list cards. Adjacent standalone
images form measured two- or three-column gallery rows when space permits.
Galleries wait for all their image dimensions before choosing rows, so staggered
loading does not lock in a fragmented arrangement. Narrow windows and galleries
with unavailable dimensions retain source-order stacks. Images remain complete;
authored alt text and enclosing links survive editing, copying, and undo.

Prose uses a wider measure (up to 960 logical pixels); tables, code, figures and
grids can use a 1280-pixel canvas. Soft source line breaks reflow, while explicit
hard breaks remain. Tables fit their columns where possible, retaining readable
minimum widths and contained overflow. Layout stays fixed during typing and
reflows after resizing settles; selection, undo and saved Markdown stay independent.
Ordinary typing in a lead, list grid, table row, or compact paired row rebuilds that row's
geometry at its retained widths without reshaping the rest of the document.
Table columns retain their widths while focused, including background reflow;
blur, resizing and font/zoom changes allow fresh column sizing.
Structural edits and oversized rows still use the general
rebuild path; fully bounded background replanning remains in progress.

Short explanations can share a measured row with their immediately following
code, table, or image. Explanation/image rows wait for known dimensions and keep the whole
figure visible; missing resources, tall content, and narrow canvases stack.
Resource arrival does not rearrange an explanation while it is being edited.

Display formulas are retained with their source-bound geometry in both theme
colors. Scrolling does not reparse or relayout them when the shared formula
cache evicts older entries. Editing the source refreshes its preview; invalid
formulas keep the editable source fallback.

Compatible HTML fragments render through Blitz. Click their text to place a
temporary caret, or select and copy text without changing the HTML. The first
typed edit converts the fragment to Markdown and edits at that position in one
undoable transaction; bold, italic and links retain their Markdown equivalents.
This conversion replaces authored HTML layout, styling, and disclosure controls
with Markdown content; all authored disclosure text, including closed bodies,
is retained in source order. Open bodies with a complete verified text map can
be edited directly. Closed, missing-summary, or ambiguous previews offer explicit
**Edit text** conversion when the complete content is supported. Unsupported or
ambiguous structures retain the explicit conversion/source fallback. IME
composition can start directly within one verified HTML text leaf. Conversion
and preedit remain provisional until commit; cancellation restores the HTML,
and one undo reverses conversion plus the committed text. Selections spanning
multiple converted text blocks are not supported for composition; use **Edit
text** and choose a position within one block. Native dead-key composition is verified;
full CJK input-method/candidate-window validation remains open.

Links in source-verified HTML text show their destination on hover. Ctrl-click
opens the link; Alt+Enter opens the link at the HTML caret. Ordinary
clicks still select text for editing. The context menu offers Open link and Copy
link address without converting the fragment. HTML and Markdown links share
native routing: heading anchors (including duplicate suffixes), local `.md` /
`.markdown` files relative to the current file's folder, and HTTP/HTTPS/mailto.
Percent-encoded filenames and Unicode heading anchors are supported. Local
links use the normal file-open safeguards: save/resolve unsaved edits before
switching files. Missing headings/files show an error without rewriting source.
Other protocols and remote file hosts are not opened. Opaque fragments without a verified text map do not expose
link hit regions yet. No active DOM or network provider is added to measurement.

Local images in supported HTML fragments now use the same bounded image loader
as Markdown images. Only visible/lookahead fragments request resources; decoded
pixels are handed to Blitz in a bounded background task. Missing, oversized,
animated or remote HTML images retain the source fallback. CSS resource URLs
remain denied. Reading does not alter HTML or add visible alt-text captions.
Text beside standalone HTML figures supports the same verified first-edit
conversion; image-only rows become Markdown figures with their links, titles and
alt text retained. One undo restores the original HTML after a direct text edit.

Links can also target authored IDs inside supported HTML previews, including
wrapped inline text and nested disclosures. Navigation opens only the closed
ancestors needed for the target, without rewriting `open` attributes. Duplicate
HTML IDs and Markdown heading anchors resolve in source order. A verified HTML
text target receives a temporary caret; otherwise navigation preserves the
existing caret. Unrenderable targets report an error and retain source access.

HTML disclosure summaries are native, focusable controls: click or press
Enter/Space to expand or collapse, and Tab/Shift+Tab to move between controls.
Their reading state is local to the view; the original `open` attributes and
content undo history are unchanged. Reopening the file restores authored state.
When no summary was authored, the reader supplies a “Details” control; that UI
label never enters saved HTML or copied fragment text. Authored empty summaries
remain empty but retain an activation target. Direct text inside a closed
disclosure stays hidden until opened.
The context menu's **Restore authored disclosures** command resets reading
choices, including recovery from an expanded fragment that exceeds preview
limits. Hidden/transformed summaries without reliable bounds stay static.

The document scrollbar sits at the outer window edge. Formatting and table
commands have icons and tooltips.

Open the [synthetic documents](performance/layout-fixtures/) to explore the
design. The [implementation and validation notes](performance/ADAPTIVE-LAYOUTS.md)
describe candidate selection, conservative fallbacks and performance evidence.

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
  outline, adaptive presentation, and accessibility semantics.
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
