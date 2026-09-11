# Native Markdown viewer and rich editor

Layout follow-up (September 8, 2026): [Editorial layout grammar plan](performance/LAYOUT-GRAMMAR-PLAN.md) specifies the next, not-yet-implemented composition milestone. Its grammar, spacing, bounded prose columns, and component text wrapping supersede conflicting layout prescriptions below; the editing and source-preservation contract remains unchanged.

## 1. Product contract

Build a native Rust/GPUI application for **Linux Wayland**, with one continuously rendered, always-editable Markdown surface. Clicking places the caret; selecting text reveals formatting tools. Raw Markdown is never exposed.

The first release includes local file navigation, an outline, adaptive document layouts, rich tables, lists, links, images, code examples, undo, and autosave. The September 6 redesign targets **scrolling above 60 fps through 10 MB**, with 120 Hz as a stretch target. Startup preparation may take longer. See [the adaptive layout plan and validation](performance/ADAPTIVE-LAYOUTS.md).

Confirmed decisions\:&#32;

- Full rich content inside table cells.
- Code blocks remain rendered and editable document content.
- Autosave preserves untouched Markdown regions byte-for-byte.
- Remove the minimap from the application for now; retain Files and Outline.
- Use all eight images in `designs/` as the visual reference: warm paper, dark serif headings, green accents, fine borders and restrained panels. No heading shadows or decorative heading badges.
- Start with the light theme, independent of the system appearance.
- Choose layout automatically without user-facing preferences; keep editing and source order stable.
- No tabs, source pane, permanent formatting toolbar, accounts, synchronization, or plugin system in v1.

## 2. Visual and component design system

### Typography and palette

The [reference site](https://www.whichai.dev/with-design-skill/opus-5/4) specifies **Fraunces** headings, **Spline Sans** body text, and **Spline Sans Mono** technical text. Its heading settings include weight 600, negative tracking, and Fraunces's `SOFT`, `WONK`, and optical-size axes.

Use those families and heading characteristics at document-appropriate sizes:

| Role | Typography |
| --- | --- |
| Body / lead / table cells | Spline Sans 18/28.8 px; lead 20/31 px; table 15.5/25 px |
| H1 | Fraunces 44/50 px, weight 600; `SOFT=30`, `WONK=1`, `opsz=120` |
| H2 | Fraunces 28/34 px, weight 600; `SOFT=40`, `WONK=1`, `opsz=72` |
| H3-H6 | Fraunces 26/22/19/17 px, weight 600, line height 1.2; `SOFT=40`, `WONK=1`, `opsz=20` |
| Navigation and controls | Spline Sans 13 px/1.4 |
| Code | Spline Sans Mono 15 px/1.5 |

Bundle fonts locally with their licenses. Generate static Fraunces instances during asset preparation: the inspected GPUI font interface does not expose arbitrary variation axes. Include deterministic italic/oblique faces and fallback coverage for unsupported scripts.

The following light palette is an adaptation for this application:

<!-- tachyon-table:v1 {"border":"Dotted","widths":[380.078125,230.203125]} -->
| Token | Value |
| --- | --- |
| Page \/ navigation background | `#FCFBF8` \/ `#F3F2ED` |
| Primary text | `#1B2430` |
| Secondary text | `#59636F` |
| Hover surface | `#E7ECDF` |
| Floating surfaces | `#FFFFFF` |
| Links and active controls | `#256F50` |
| Selection | `#DCEBE1` |
| Table rules | `#DEDFD7` |
| Errors | `#9E4B3F` |

Use a 4 px spacing scale. Paragraph spacing is 16 px; ordinary headings have 20 px above and 7 px below. H1/H2 use a subtle gray text shadow, and chapter numbers have distinct green sans typography. Quotes have inset padding; cards have quiet backgrounds and fine outlines. Avoid textures, gradients, and page shadows. Floating controls use an 8 px corner radius and a restrained shadow. Keyboard focus remains visibly outlined.

### Window layout

- **Header:** 36 px high, with the document filename, a subtle unsaved/error indicator, and one hamburger menu beside compositor-appropriate window controls.
- **Left navigation:** 224 px initially, resizable from 180-320 px. The upper 60% contains folders and sibling Markdown files; the lower 40% contains the outline. Both scroll independently. A draggable gap separates them without a visible rule.
- **Document:** horizontally centered within the remaining workspace, with a wider reading measure and 28 px horizontal padding. Reflow source soft breaks; preserve explicit hard breaks. Place the document scrollbar on the outer window edge. Wheel scrolling eases to rest; precise trackpad input and reduced-motion preferences retain direct behavior.
- **Responsive behavior:** below 800 px, collapse navigation into a temporary overlay. Preserve access through the menu and shortcuts. Minimum window size: 480 x 360 px. The canvas grows to 1280 px while ordinary prose stays within 960 px.
- **Automatic composition:** no layout selectors or saved overrides. Match short independent lists, explicit arrow sequences, adjacent figures, and repeated bounded sibling sections to appropriate arrangements. Keep source order and typing stability; retain vertical fallbacks for complex content. Fit automatically sized table columns above readable minimums and align near-width tables to the reading edge. Preserve explicitly saved column widths exactly, including when the table is close to the prose width, and contain any resulting overflow.
- Tables and images may use the full central workspace width\. Prose retains its reading measure\. Wide tables scroll horizontally within their own area\.

<!-- tachyon-table:v1 {"border":"LogicalPixel","widths":[293.83984375,347.54296875]} -->
| Hallo | This is a test |
| --- | --- |
| abc | def |

### Rendered components and editing

<!-- tachyon-table:v1 {"border":"LogicalPixel","widths":[241.33203125,401.553955078125]} -->
| Component | Appearance and behavior |
| --- | --- |
| Paragraphs and headings | One shared text engine\. Formatting changes preserve the selection and scroll anchor\. Heading changes immediately update the outline\. |
| Bold\, italic\, strikethrough\, inline code | Available through selection toolbar\, context menu\, and shortcuts\. Mixed selections show mixed formatting state\. |
| Lists | Hanging markers and 24 px indentation per level\. Enter splits items\; Enter on an empty item exits or outdents\. Tab\/Shift\+Tab indent\/outdent\. Backspace at an item\'s start outdents before merging\. |
| Task lists | Small native\-style checkboxes aligned to the first text line\. Toggling is one undoable edit\. |
| Links | Underlined green text\. Ordinary click positions the caret\; Ctrl\+click follows the link\. A local popover edits label and destination\. |
| Blockquotes | Indented text with a faint background tint and a narrow content\-level rule\. No surrounding box\. |
| Code blocks | Quiet tinted background\, monospace text\, preserved whitespace\, and local horizontal scrolling\. No line numbers or editor chrome\. |
| Images | Preserve aspect ratio\, reserve space\, and expose source\/alt\-text controls on selection\. Loading and failure states occupy the same reserved area\. |
| Tables | Body typography\, semibold header\, 12 px horizontal and 8 px vertical cell padding\. Border modes\: none\, dotted\, or one physical pixel\; default dotted\. No zebra striping\. |

### Floating tools

- Show a 36 px selection toolbar after pointer selection completes, or after keyboard selection settles for 100 ms.
- Include block style, bold, italic, strike, inline code, link, and list controls. Table selection adds row, column, alignment, and border commands.
- Anchor to the selection and clamp to the viewport. Toolbar interaction preserves the document selection.
- Context click exposes insertion at the clicked location: paragraph, heading, list, task list, quote, code block, image, table, and thematic break.
- Escape closes transient UI and returns focus to the document. Hide transient tools during scrolling.
- All commands share one command registry and remain reachable by keyboard.

### Table interaction contract

- New tables contain two columns, a header, and two body rows.
- Cells use the same text, selection, formatting, image, and block-editing engine as the document.
- Enter inserts a paragraph or splits a list item inside the cell; Shift+Enter inserts a hard break.
- Tab/Shift+Tab move between cells. Tab from the final cell adds a row. Ctrl+Enter moves to a paragraph after the table.
- Cell text selection and rectangular cell selection are separate states. Dragging text selects text; edge handles select rows/columns.
- Support insert, delete, and move row/column commands; column alignment; draggable column widths; and rectangular clipboard paste.
- TSV paste expands the table as necessary and remains one undoable transaction. Plain prose paste edits the active cell.
- Column widths remain stable while typing. Recompute wrapping only for affected columns.
- Merged cells and nested tables are excluded from v1; paragraphs, headings, lists, quotes, code, links, and images are supported inside cells.

## 3. Architecture and persistence

### Dependencies and ownership

Use three crates:

- **Document core:** structured content, selections, transactions, undo, Markdown/HTML import, and source-preserving serialization. No GPUI dependency.
- **Document view:** shaping, adaptive arrangements, hit testing, selection painting, tables, viewport virtualization, and outline projection.
- **Application:** GPUI window, commands, navigation, filesystem services, image cache, and recovery.

Start with GPUI Component commit `ff3eb1128ac1058f1bb88e777744ce1237aa3b79` and its recorded Zed dependency commit `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`. Pin dependencies and the working toolchain in the repository. Use installed Rust 1.98.0 as the initial build baseline.

Reuse GPUI Component menus, popovers, tooltips, and small property inputs. Its [TextView](https://github.com/longbridge/gpui-component/blob/main/website/docs/components/text-view.md) is a rendering component; the document editing surface must be purpose-built.

Study Zed's selection, movement, display mapping, scrolling, and minimap implementations. Use its editor as behavioral reference; do not make the application depend on Zed's editor/workspace stack.

Target Wayland explicitly. Disable optional X11 support wherever the dependency graph permits; any unavoidable transitive X11 code does not become a supported runtime target.

### Document model and interfaces

Use stable node IDs, typed block nodes, rich inline runs, and rope-backed editable text. Table cells contain block sequences.

The central interfaces are:

- `DocumentSnapshot`: immutable document revision with shared unchanged storage.
- `DocumentPosition`: node ID, text offset, and affinity.
- `Selection`: text range or rectangular table range.
- `EditCommand -> TransactionResult`: document changes, transformed selection, inverse operations, and dirty node IDs.
- `LayoutIndex`: node/fragment geometry, cumulative heights, and document-position mapping.
- `SaveSnapshot`: serialized revision plus its expected on-disk identity.

The UI thread owns mutable editing state. Workers receive immutable snapshots and return revision-tagged results. Stale results are discarded. Ordinary typing updates the model directly; it does not serialize and reparse Markdown.

Use one document-wide selection and undo system, including table cells. Group continuous typing until a 500 ms pause, caret move, or structural command. Treat IME composition as provisional text and commit it as one undoable operation.

Implement GPUI's input-handler contract, including UTF-16 conversion, marked text, candidate-window bounds, and point-to-character mapping. Movement operates on grapheme clusters and shaped visual lines.

### Markdown and HTML

Use [Comrak](https://github.com/kivikakk/comrak) for CommonMark/GFM import, with tables, task lists, strikethrough, autolinks, and tag filtering enabled. Include GitHub-style alerts and footnotes as explicit extensions.

Preserve an original-source spine containing block ranges, separators, reference definitions, comments, and front matter. Unchanged regions serialize verbatim. Edits regenerate only the smallest enclosing serialization unit that preserves correct syntax—potentially an entire list or table.

- Ordinary tables remain GFM pipe tables.
- Tables requiring block content serialize as HTML tables with semantic HTML inside cells. This follows the limitation that [GFM pipe-table cells contain inline content](https://github.github.com/gfm/#tables-extension-).
- Persist table borders and column widths in a versioned hidden HTML comment immediately preceding the table. Create metadata only when needed; move and copy it with its table.
- Never hide the actual rich-cell content exclusively in metadata.
- Unknown metadata is preserved. Malformed application metadata is ignored without altering content.
- Supported HTML imports into the same document model. Unsupported constructs remain preserved source objects with a compact rendered placeholder; scripts and external HTML execution are never enabled.

Clipboard output includes plain text, HTML, and an application-rich representation. Prefer the rich representation for internal paste, then supported HTML, then plain text. Provide an explicit "Paste as Markdown" command.

### Files, autosave, and images

- One active document per window; maintain shared sessions when the same file is opened twice.
- Opening a file shows its parent directory and siblings. Opening a folder establishes the navigation root. Load expanded directories lazily.
- Remember expanded folders, pane widths, active document, selection, and scroll anchor.
- Autosave after 750 ms of inactivity; Ctrl+S flushes immediately. Serialize on a worker and write through a same-directory temporary file followed by atomic replacement, preserving file permissions.
- Store a local recovery journal in XDG state storage. Preserve edits after save failures and offer recovery after an interrupted session.
- Watch external changes. Reload clean documents while preserving the nearest stable anchor. If unsaved edits exist, pause autosave and offer reload, save a copy, or explicit overwrite; never silently choose a version.
- Fetch referenced HTTP(S) images automatically and lazily. Resolve local images relative to the document.
- Use four concurrent downloads, HTTP freshness validators, a 512 MiB disk LRU cache, and separate bounded decoded-image/GPU caches.
- Decode and resize images off the UI thread. Reserve dimensions from image headers and preserve the viewport anchor when dimensions change. Cached images remain available offline.
- Bound downloads and decoded dimensions; failed or oversized images retain alt text and a retry action.

## 4. Rendering and performance contract

**Scrolling above 60 fps is the current acceptance target to measure.** The older 120 Hz/startup qualification protocol below remains a separate, stricter benchmark, not the September 6 acceptance gate.

### Rendering strategy

- Maintain a height-augmented tree for `O(log n)` viewport lookup and local height updates.
- Render visible content plus one viewport of overscan in either direction.
- Virtualize table rows and subdivide very long text/code blocks into layout fragments. Never require shaping an entire giant paragraph to display one screen.
- Cache shaped runs and wrapped lines by node revision, font instance, width, and scale factor.
- Invalidate only affected layout fragments. Selection and caret changes repaint overlays without rebuilding document content.
- Preserve scrolling with a stable node/fragment anchor and intra-fragment offset when images, tables, or wrapping change above the viewport.
- Consume precise Wayland scroll deltas and frame callbacks. Preserve supplied scroll phases; do not add easing to direct trackpad movement.
- Outline jumps use a cancellable 120 ms transition. Reduced motion makes programmatic jumps immediate.
- Cache component bounds, heading positions and interval maxima when publishing geometry. Scroll frames perform bounded visible-range queries, with no minimap or full-document component scans.
- Limit speculative layout work to short, interruptible idle slices. Disk I/O, parsing, serialization, image decoding, and directory traversal stay off the UI thread.
- Keep the application event-driven while idle.

### Acceptance budgets

Measure optimized builds on the available Ryzen AI Max+ PRO 395/Radeon 8060S machine, using an actual 120 Hz Wayland display. Record compositor, driver, resolution, scale, power mode, and compiler.

| Measurement | Target |
| --- | --- |
| Warm launch to editable first viewport, 100 KB document | p95 <=100 ms |
| Cold launch to editable first viewport, 100 KB document | p95 <=250 ms |
| Open and prepare editable 10 MB document | p95 <=1 second |
| Application frame work during scrolling/editing | p99 <=6 ms |
| Input event to presented caret/text update | p95 <=16.7 ms |
| Missed presentation deadlines during sustained interaction | <0.1%; no application-caused stall >=25 ms |

Measure startup both with warm caches and with cold filesystem/application caches; report first-ever GPU initialization separately. Network image completion is excluded from document-ready timing, but loading images must not cause missed interaction budgets.

## 5. Delivery sequence and validation

1. **Native foundation and performance harness\:** pinned build\, Wayland window\, bundled typography\, editable paragraph\, IME\, selection\, and measured scrolling\. Establish presentation timing before building the remaining shell\.
2. **Document core\:** source\-preserving import\/export\, transactions\, cross\-block editing\, clipboard\, undo\, autosave\, recovery\, and external\-change handling\.
3. **Components\:** lists\, links\, images\, code\, alerts\, footnotes\, and rich tables using the shared editing engine\.
4. **Navigation\:** filesystem tree\, outline\, selection toolbar\, context insertion\, and responsive layout\.
5. **Release qualification\:** performance\, visual fidelity\, accessibility\, packaging\, and clean\-machine startup\.
6. Hallo\: friends

Required validation\:

- GFM fixtures and semantic *round* trips\; byte\-identical unchanged regions\; CRLF\, Unicode\, references\, front matter\, comments\, and unknown HTML preservation\.
- Editing properties: undo restores document and selection; arbitrary operations preserve tree invariants; stale worker results cannot replace newer content.
- Pointer and keyboard workflows across paragraphs, lists, images, and cells, including dragging beyond the viewport.
- Combining marks, emoji, RTL/CJK text, dead keys, and Fcitx5/IBus composition on Wayland.
- Table insertion, row/column operations, multiline rich cells, TSV paste, border persistence, column resizing, and HTML-table interoperability.
- Disk-full, permission failure, external modification, rename/delete, crash recovery, network failure, and cached-image reopening.
- AccessKit/AT-SPI semantics for headings, links, lists, editable text, trees, and table cells. Verify keyboard-only operation, Orca, reduced motion, and fractional scaling.
- Visual fixtures for the complete component system at 100%, 125%, 150%, and 200% scale.
- Thirty startup samples per scenario and five 60-second interaction runs for 100 KB, 1 MB, and 10 MB fixtures. Include image-heavy documents, large tables, deep lists, resize, selection drag, and simultaneous autosave.
- Stress-test oversized paragraphs and files above 10 MB for cancellation, bounded foreground work, and data preservation; they do not inherit the 10 MB timing guarantee.

Crusty consultation found no existing repository constraints. Implementation must run Crusty change preparation before edits and validation afterward. No application code or performance results exist yet; the figures above are release gates.
