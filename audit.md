# Mineral Markdown implementation audit

> Historical baseline: this report records defects reproduced before the
> implementation work. It is intentionally preserved as the task-by-task
> rationale, not as current product status. Current commands and architecture
> are summarized in `README.md`; current measured release results are in
> `performance/RELEASE-QUALIFICATION.md`.

Audited 2026-09-06. This is an implementation handoff, not a claim that the application is ready for use on valuable documents.

## 1. Verdict and scope

The three-crate foundation is reasonable and should be retained. The immediate problem is incomplete integration between the document model, serialization, editing, geometry, and application lifecycle. Visual polish alone will not make this editor reliable.

**Fix save/reopen correctness and selection invariants first.** The audit reproduced paragraph boundaries disappearing after save, literal prose changing into Markdown syntax, table formatting being discarded, and a caret pointing into a deleted table row. The native UI also edits the wrong table column and paints text across cell boundaries. These are release blockers.

The code compiles and its existing tests pass. That does not contradict these findings: most tests exercise helpers or selected happy paths rather than complete input → transaction → save → reopen workflows.

### Constraints to preserve

- Follow `plan.md`: Rust/GPUI, Linux Wayland, one always-editable rendered surface, three crates, local files, source preservation, rich table cells, OS-selected light/dark appearance, and a rendered minimap.
- Retain Fraunces, Spline Sans, and Spline Sans Mono with the existing licensed font assets. Fix layout before changing type sizes to conceal overflow.
- Retain the explicit exclusions: no tabs, raw-source pane, permanent formatting toolbar, accounts, synchronization, or plugin framework.
- The 120 Hz/10 MB requirement is a measured release gate. It is not satisfied by a fast height lookup or a GPU-backed toolkit.
- This checkout has no commits; all application files were untracked at inspection. Preserve existing work. Establish a reviewed source baseline before implementing the audit; do not reset or discard the checkout.
- Crusty was consulted. It had no published guidance or indexed governing documents. Change preparation completed as `ctx_4dcca11c4afe`; the live audit report was `arch_b9b8bca11ad9`. Human instructions and current source remain authoritative. Consult and prepare again for each implementation task.

### Evidence labels

- **Reproduced:** observed by an executed probe, benchmark, or native interaction.
- **Source-confirmed:** the named code path establishes the gap; a full native reproduction was not performed.
- **Unverified:** a required qualification check remains outstanding. Do not convert these into claims of success or failure without running the check.

Line numbers below describe the inspected checkout. Use the named functions as anchors after refactoring.

## 2. Baseline actually verified

| Check | Result |
| --- | --- |
| `cargo test --workspace --all-targets --locked` | Passed: 28 core + 26 view + 22 app tests; performance binary has 0 tests |
| `cargo fmt --all -- --check` | Passed |
| `cargo clippy --workspace --all-targets --locked -- -D warnings` | Passed |
| `cargo build --release --locked --bin mineral-markdown --bin mineral-perf` | Passed |
| Additional temporary regression probes against the compiled core library | 7 deliberately failing checks; failures listed below |
| Native release app in isolated Weston | Launched and inspected at 1280×720 and 640×480; real Wayland pointer/key input exercised |
| AT-SPI inspection | Tree exists, but editor text was unavailable and semantic document nodes reported identical 1×1 bounds |
| Actual 120 Hz presentation, cold launch, Fcitx5/IBus, Orca, fractional scaling, real crash recovery | Unverified |

Native tests used a disposable Markdown file and isolated `XDG_STATE_HOME`/`XDG_CACHE_HOME`. No production document was edited. The app instances launched by the audit were stopped afterwards. Workspace screenshots `wayland-screenshot-2026-09-06_00-22-10.png` and `wayland-screenshot-2026-09-06_00-22-33.png` were also inspected and left unchanged; they are supplemental visual evidence, not the source of the disposable-file editing tests.

### Measured performance

Commands executed after the release build:

```sh
target/release/mineral-perf --bytes 102400 --runs 30
target/release/mineral-perf --bytes 10485760 --runs 10
```

Environment reported by the harness: Rust 1.98.0 (`88d9e12ae`, 2026-08-18), GNOME Wayland, balanced power profile. The isolated visual session used headless Weston with the GL renderer and kiosk shell. These are CPU-stage measurements, not display presentation measurements.

| Stage, p95 | 100 KiB / 30 runs | 10 MiB / 10 runs |
| --- | ---: | ---: |
| Parse | 4.65 ms | 851.07 ms |
| Standalone text projection | 1.01 ms | 251.31 ms |
| `PreparedDocumentView::prepare` | 1.10 ms | 415.37 ms |
| Model edit | 0.67 ms | 146.96 ms |
| View refresh for the edit | 0.03 ms | 26.22 ms |
| Combined localized edit | 0.72 ms | 172.87 ms |
| Estimated layout construction | 0.88 ms | 326.06 ms |
| Cached estimated reflow | 0.58 ms | 240.30 ms |
| Height-index lookup | 0.028 µs | 0.066 µs |

Interpretation:

- Ten large-file samples are exploratory; their p95 is effectively the maximum sample. Repeat the required qualification protocol before publishing performance claims.
- `PreparedDocumentView::prepare` already builds a projection. Do not add the standalone projection timing to that timing as though the app performs both sequentially.
- Do not sum stage percentiles and label the result an end-to-end percentile. The current harness does not measure open-to-editable or input-to-present.
- The approximately 173 ms combined edit does establish a serious problem: this work is synchronous in the editing path before GPUI painting and presentation. A one-screen height lookup cannot compensate for it.
- The harness edits a late top-level text node once per fresh document. It does not cover repeated typing, first/middle positions, nested cells, accumulated history, IME, or actual input-handler overhead.
- `performance/BASELINE.md` predates the added prepared-view/edit measurements. Update it with raw per-run data after the harness is corrected.

### Reproduced regression probes

These checks were compiled separately with `rustc --test`, using the existing `document-core` library; they did not modify the repository test suite. Exit status was 101, with 0 passed and 7 failed.

| Probe | Observed failure | Task |
| --- | --- | --- |
| `literal_prose_round_trips` | Replacing paragraph text with `# literal` reloads as a heading | A01 |
| `split_keeps_three_paragraphs` | Splitting `one` after `o` in `one\n\ntwo\n` produces 3 model blocks but only 2 after reopen | A02 |
| `structural_edit_keeps_unmodified_source` | Inserting a paragraph rewrites untouched `Title\n=====\n` to `# Title` | A02 |
| `table_border_does_not_erase_inline_formatting` | Changing only the border serializes `**bold**` as `bold` | A03 |
| `deleting_selected_row_keeps_valid_caret` | Row deletion succeeds while the selection references its removed paragraph | A04 |
| `typing_through_editor_command_sequence_groups_undo` | Two quick input replacements followed by undo leave `basea`, not `base` | A08 |
| `editing_last_node_preserves_unrelated_arc` | Editing the second paragraph clones the unrelated first paragraph node | A10 |

The typing probe reproduces the current editor's `SetSelection` → `ReplaceSelection` sequence. Its eventual acceptance test belongs at the input-handler boundary: explicit user caret movement must still break typing groups. Do not “fix” the core by making all `SetSelection` commands preserve grouping.

## 3. Architecture: what exists and what should own each responsibility

### Current dependency and execution paths

```text
markdown-app
  main.rs: window, sessions, file lifecycle, navigation, layout jobs, minimap, shell
  persistence.rs: identity, atomic saves, recovery, workspace-state file
  image_cache.rs: fetching, disk records, decoding and GPU cache integration
       ↓
document-view
  editor.rs: input, history orchestration, projection updates, visual lines,
             hit testing, painting, tables, images, popovers, accessibility
  layout.rs + height_tree.rs + minimap.rs: a SECOND estimated geometry pipeline
  projection.rs: flattened text and global offset maps
  session.rs: shared Document in Rc<RefCell<_>>
       ↓
document-core
  document.rs: snapshots, transactions, traversal/mutation, history, composition
  markdown.rs/html.rs: import and serialization
  model.rs/text.rs/table.rs/clipboard.rs: content and editing primitives
```

The issue with `editor.rs` (~4,752 lines), `document.rs` (~3,002), and `main.rs` (~2,059) is responsibility and invariant ownership, not line count alone. Each contains several independently changing state machines. Splitting them into arbitrary `helpers.rs` files would preserve the actual problems.

Keep these existing strengths:

- Core has no GPUI dependency and application code forbids unsafe Rust.
- Stable node IDs, rope-backed text, immutable snapshot handles, and transaction results are useful foundations.
- Atomic same-directory replacement, permission preservation, conflict checks, recovery attempts, and fault-injection tests already exist.
- Image download/dimension limits, four active loads, HTTP validators, and bounded cache mechanisms already exist. Improve their gaps instead of replacing them wholesale.
- GPUI input-handler integration, shaped-line caching, visible-line selection, and cancellation of estimated-layout jobs are real implementations, although incomplete.

### Intended module boundaries

Create modules incrementally while fixing the corresponding behavior; do not move every file in one change.

```text
document-core/src/
  document.rs                 snapshot facade and atomic transaction publication
  transaction/mod.rs          command dispatch, change set, selection transforms
  transaction/text.rs         text/range operations
  transaction/structure.rs    split/join/insert/delete and container rules
  transaction/table.rs        table edits and rectangular selection transforms
  tree.rs                     read-only lookup and mutation of the located path
  history.rs                  undo groups and provisional composition lifecycle
  markdown/import.rs          parser adapter and source associations
  markdown/serialize.rs       context-aware regeneration
  source.rs                   untouched slices, boundaries, source-unit ownership
  html.rs, clipboard.rs, model.rs, text.rs

document-view/src/
  editor/mod.rs               GPUI entity facade and view state
  editor/input.rs             GPUI input contract and command dispatch
  editor/movement.rs          visual/grapheme navigation and selection gestures
  editor/overlays.rs          toolbar, link/image UI and transient focus
  layout/mod.rs               ONE authoritative geometry index
  layout/text.rs              measured wrapping and bounded shaping fragments
  layout/table.rs             cell boxes, row heights, clips and column geometry
  paint.rs                    painting from published geometry
  accessibility.rs            semantics/actions/bounds from the same geometry
  projection.rs, minimap.rs, outline.rs, responsive.rs, session.rs

markdown-app/src/
  main.rs                     startup, registration and window creation only
  window.rs                   compose header/navigation/editor/status
  commands.rs                 application command catalog and menu/shortcut wiring
  sessions.rs                 file-session ownership and weak window attachments
  file_controller.rs          open/reload/save/conflict/close state transitions
  navigation.rs               root, lazy directory model and tree commands
  workspace_state.rs          restore/persist window and navigation state
  theme.rs                    OS appearance → shared semantic visual tokens
  persistence.rs              filesystem adapter with typed outcomes
  image_cache.rs              resource service; split disk/decoded ownership if needed
```

API rules:

1. Only core transactions mutate document content. A successful transaction must return a valid selection, an exact change set, and undo information. Core errors must leave document, selection, history and composition coherent.
2. Resolve a node's path read-only before taking mutable ownership. Mutation copies only the affected path; unrelated `Arc<BlockNode>` values remain shared.
3. `DocumentSnapshot` is immutable input to workers. Tag results with session identity, document epoch, revision, request sequence, and relevant layout configuration. Revision alone is not unique across files or reloads.
4. Keep one active file controller per shared file session. Views observe its status; a window is not the owner of a save operation's completion.
5. Keep geometry authoritative in `document-view`. Painting, hit tests, cursor movement, accessibility bounds, scroll anchors, outline jumps, and minimap consume the same published geometry.
6. App owns OS integrations and resources. Pass resolved resources/theme values into view code. Core must not learn about GPUI, paths used by the cache, HTTP, or window controls.
7. Module facades expose intentional operations and read-only accessors. Keep traversal helpers, raw source-unit mutation, cache keys and transaction internals private. Do not add a service trait for every struct. A testable filesystem/clock/completion boundary is useful; generic repositories and event buses are not required.
8. Eventually separate per-window selection/scroll/focus from shared content/history. Two windows should not move each other's caret merely because they share a file. Make this an explicit session/view API change in A12, with tests; do not silently change undo semantics during a file move.

## 4. Ordered implementation backlog

Priorities: **P0** = data preservation or a fundamental workflow blocker; **P1** = required editing, responsiveness, accessibility or appearance; **P2** = qualification and maintainability. “Depends on” is implementation ordering, not permission to omit a task.

### A01 — P0: preserve literal text and inline semantics when saving

**Evidence:** reproduced; `document-core/src/markdown.rs:911` (`serialize_inline`), `:982` (`open_style`), `:1004` (`escape_inline`); `html.rs:65` (`render_node`). Escaping handles only backslash, `*`, `_`, `[` and `]`. Literal `# literal`, `- literal`, `1. literal`, `> literal`, backticks and `&copy;` changed structure, formatting, or text after reopen. HTML text is also written into Markdown without escaping.

**Implement:**

1. Separate escaping contexts: paragraph/line start, ordinary inline text, link label, destination, image attributes, inline-code content and pipe-table cell. Preserve literal text as literal text; explicit rich commands and “Paste as Markdown” are the paths that create syntax.
2. Use a syntax-aware serializer or build a Comrak AST for dirty units and use its formatter where it preserves the required semantics. If keeping custom serialization, cover all contexts with semantic round-trip fixtures before enabling autosave on them.
3. Emit code-span fences longer than any embedded backtick run and handle leading/trailing spaces according to Markdown rules. Do not surround all inline code with one backtick unconditionally.
4. Preserve formatting across adjacent run boundaries. Avoid opening/closing every style independently when doing so changes emphasis nesting or creates invalid delimiters.
5. Escape/encode link and image destinations containing parentheses, spaces, angle brackets or quotes. Route HTML text and attributes through the appropriate context too.
6. Preserve intentional code-block blank lines; `serialize_block` currently trims all trailing CR/LF before appending one newline.

**Acceptance:** semantic document equality after edit → serialize → import for the literal examples above, adjacent bold/italic runs, code containing backticks, literal HTML/entity text, hard breaks, Unicode, and complex URLs. Compare block kinds, text, inline styles, destinations and breaks while ignoring regenerated IDs. Tests must include actual edits; unchanged-source pass-through does not exercise this serializer.

**Depends on:** none. **Likely files:** `markdown.rs`, `html.rs`, new serialization fixtures/tests.

### A02 — P0: repair structural serialization and source preservation

**Evidence:** reproduced; `markdown.rs:547` (`build_spine`) and `:592` (`serialize`); `document.rs:1568` (`split_selection`), `:2341` (`structure_changed`). Any structural change forces every original block to regenerate. New blocks receive only a single separating newline in some cases. Splitting `one\n\ntwo\n` at offset 1 serialized as `o\nne\n\n\ntwo\n`, so the new paragraphs merged on import. An unrelated insertion rewrote a setext heading.

**Implement:**

1. Replace the document-wide “regenerate all when structure changed” decision with explicit dirty serialization units and changed adjacency boundaries.
2. Model separators/reference definitions/comments/front matter as owned source-spine records rather than incidental prefixes that may disappear with a deleted neighbor. Decide separator ownership for insertion, deletion, movement and merging.
3. Preserve the exact source slices of unchanged units even when another unit is inserted or removed. Regenerate the smallest enclosing list/table when its internal structure changes.
4. When adjacent generated blocks require a blank line, insert the required separator; one newline is insufficient for separate paragraphs. Preserve CRLF for regenerated boundaries in CRLF documents.
5. Define empty-paragraph persistence: Markdown cannot preserve arbitrary runs of empty paragraph nodes. Normalize transient empty editing placeholders deliberately; do not confuse them with nonempty paragraph boundaries.
6. On successful saves, allow source-spine rebasing/dirty cleanup without clearing undo or making stale workers appear current. An older save completion must not mark newer edits clean.

**Acceptance:** split/join/insert/delete/move followed by reopen preserves nonempty block topology and inline meaning. Unrelated setext headings, list markers, fenced-code spelling, reference definitions, comments, front matter and mixed untouched line endings remain byte-identical. Deleting a block must not accidentally delete a reference definition needed elsewhere. Undo restores the original document and selection; redo then save remains correct.

**Depends on:** A01 for regenerated units.

### A03 — P0: table saves must retain formatting and metadata

**Evidence:** reproduced; `markdown.rs:791` (`serialize_pipe_table`) uses `plain_text()`, discarding inline runs. `table.rs:230` sends any one-paragraph cell through this lossy path. Metadata import at `markdown.rs:28` attaches metadata only when the next parser node is a GFM `Table`, while rich tables are emitted as HTML by `serialize_html_table` (`:827`).

**Implement:**

1. Serialize GFM cells using the rich inline serializer, with pipe-specific escaping. Bold, links, code, images and strike do not alone require HTML tables.
2. Keep HTML serialization for genuinely block-rich cells. Import the immediately preceding versioned table metadata for both GFM and supported HTML tables.
3. Preserve widths, border mode, column alignment, header-row semantics and unknown metadata fields through both formats. HTML export currently needs an explicit representation for column alignment.
4. Ensure metadata cannot terminate its own HTML comment through unescaped content. Malformed/unknown versions stay inert and source-preserved.
5. Handle hard breaks and empty cells deliberately; do not serialize a break as `<br>` and then expose literal markup after reopen.

**Acceptance:** changing only border/width/alignment retains bold and link destinations in every cell. Insert a second paragraph into a cell, save as HTML, reopen and assert all rich blocks plus metadata survive. Remove the extra block and verify the chosen serialization remains semantically equivalent. Test literal pipes, backticks, CRLF and unknown metadata.

**Depends on:** A01–A02.

### A04 — P0: enforce valid selections after every structural edit

**Evidence:** reproduced; `document.rs:694` (`DeleteBlock`), `:716` onwards (table operations), `:968` (`mutate_table`) and `:1954` (`validate_selection`). Transaction publication validates tree shape but does not consistently transform/validate the resulting selection. Deleting the row containing the caret leaves a dangling node ID. `paste_tsv` also replaces cell paragraphs and IDs.

**Implement:**

1. Add one selection-transform phase to transaction publication. For removed text, choose the nearest surviving position in the same cell/row/container, then the adjacent document position. Create an editable empty paragraph if the document would otherwise have no caret destination.
2. Transform rectangular row/column coordinates through insert/delete/move, then clamp only according to documented table semantics. Preserve selection direction and affinity where meaningful.
3. Validate supplied `ReplaceText.selection_after` too. Validate tree/text/selection invariants at public mutation boundaries without turning ordinary edits into repeated full-tree scans.
4. Make import of documents containing only thematic breaks, front matter or preserved HTML produce a usable editing position without changing untouched serialization.
5. Reject invalid operations atomically; never publish content with a selection that points to missing/non-text nodes.

**Acceptance:** after each successful command, both text endpoints resolve and lie on valid UTF-8 boundaries, or every selected table coordinate exists. Delete the active row/column/last block, paste TSV over the active cell, move the selected row, undo and redo, then type immediately without clicking elsewhere. Include a document containing only `---` and one containing only an HTML comment.

**Depends on:** none; integrate before broad interaction work.

### A05 — P0: make file completion handling safe against concurrent edits

**Evidence:** source-confirmed; `main.rs:722` (`queue_reload`), `:781` (`restore_recovery`), `:827` (`open_file`), `:1090` (`refresh_layout`), `:1199` (`queue_save`). Reload checks the path on completion but not the revision/generation at which it began. An edit made while the worker runs can be overwritten and marked clean. Open similarly checks unsaved state only before starting. Estimated-layout cache keys contain node ID/revision but no document identity; IDs and revision zero recur in different documents.

**Implement:**

1. Introduce a file-session identity and a document epoch incremented on replacement/reload. Capture `{session_id, epoch, revision, request_id}` for all file and preparation jobs; geometry jobs additionally capture width/font/scale generation.
2. On completion, match the full ticket before applying a result. A reload started on a clean document must become a conflict if edits occurred meanwhile. Preserve those edits and keep the candidate disk content separately until a decision.
3. Do not allow an open completion to overwrite a newly edited untitled/current buffer. Either defer switching until it is saved/recovered, or cancel the open and keep the edit. The loading UI must make the pending target and cancellation available.
4. Reset or namespace layout caches when attaching a different session or replacing a document. Check the cancellation epoch again on the UI thread; a worker check just before returning is insufficient.
5. Complete save ownership in the session controller even if its initiating window is gone. Validate the saved ticket before updating a window's identity/status.
6. Capture file bytes and their identity coherently: inspect before/after reading, retry or report conflict if they change. Preserve a content fingerprint when needed to distinguish same-size/same-timestamp replacements. The current separate `read_to_string` then `source_identity` can pair old bytes with a newer identity.
7. Preserve typed persistence errors across the worker boundary. Replace `error.contains("changed outside")` with matching on an error variant. Distinguish missing files from permission/stat failures.

**Acceptance:** deterministic barriers pause open/reload/save/layout immediately before completion. Edit, switch files, undo, close the originating window or start a newer request; releasing the barrier must not replace newer content, clear the wrong dirty flag or install geometry from another file. Repeat with two files having identical initial IDs/revisions but different content. Existing `Document::accept_worker_result` tests alone are insufficient: the app must use equivalent checks on its actual jobs.

**Depends on:** A04; ownership extraction in A12 can be done here in small steps.

### A06 — P0: finish open/save/close/recovery as real user workflows

**Evidence:** source-confirmed; `main.rs:52` accepts command-line paths, but there are no file/folder chooser actions or Save As implementation. `queue_save` tells an untitled document to choose a path without providing that action. No window-close/app-quit flush guard exists. Journaling occurs inside saving; conflict-paused/untitled edits have no independent recovery path. `RecoveryJournal::write` (`persistence.rs:315`) syncs the file but not the directory after its rename. Recovery restoration immediately schedules autosave despite saying “after review.”

**Implement:**

1. Implement New, Open File, Open Folder, Save, Save As, Save Copy and Close through the shared command system. Use native GPUI/platform dialogs supported by the pinned dependency. A first-launch empty window must be actionable without a command-line path.
2. Save As keeps the draft until a successful write, handles existing-target replacement explicitly, and updates session identity, image base directory and navigation atomically. Cancel leaves content/focus intact.
3. Make close/quit a state transition: flush pending edits or durably journal them; on failure keep the document open with Retry, Save Copy and an explicit discard path. Closing during the 750 ms debounce must not lose the edit.
4. Journal committed edits independently from autosave, including conflict-paused and untitled sessions. Give untitled drafts stable IDs rather than requiring a disk path. Bound the recovery-loss window explicitly and flush on close. Provisional IME text is not a committed edit.
5. Sync the recovery directory after publishing/removing journal entries if claiming crash durability. Store private recovery files with appropriate owner-only permissions. Use a versioned stable journal key rather than relying on `DefaultHasher` stability across versions.
6. A corrupt journal must not prevent opening the valid source file. Show a recoverable warning and retain/quarantine the corrupt record for inspection.
7. A recovery entry needs the base disk identity. Restore it as an unsaved draft; require an explicit save/overwrite decision if disk has changed. Do not overwrite disk 750 ms after “Restore” while implying there is time to review.
8. Track changes observed during save/reload rather than dropping watcher events when busy. Reconcile disk state after completion; re-arming a one-shot watcher does not prove no event was missed.

**Acceptance:** first launch → type → Save As → close → reopen works. Cancel every chooser without loss. Close/quit before debounce, during save, during conflict and after a failed write; committed content survives. Inject failure before/after temporary sync, rename, directory sync and journal cleanup. Do not claim a write never happened when replacement succeeded but final durability confirmation failed; expose an appropriate uncertain-durability outcome and reconcile identity.

**Depends on:** A01–A05 for safe data and state transitions.

### A07 — P1: replace approximate visual lines with authoritative measured layout

**Evidence:** reproduced and source-confirmed; `editor.rs:3720` (`build_visual_lines_for_segment`) wraps prose at 68 characters, headings at 28/38/48, independently of actual width. `prepaint` (`:2990`) then shapes one line without width-aware wrapping. At 640 px the paragraph clips on the right. `position_visual_lines` and the separately rebuilt `LayoutIndex` disagree about heights; `main.rs:1090` supplies fixed width 760, font instance 1 and scale 1.

**Implement:**

1. Use actual available logical width after pane sizing, padding, nesting and cell insets. Shape styled text with the pinned GPUI text API and obtain measured wrap boundaries. Support Unicode line-break opportunities and grapheme-safe emergency breaking.
2. Put measured and estimated fragments in one layout index. Replace `VisualLineSpec`'s independent document geometry with records from that index; estimators may populate unknown fragments but must never pretend to be measurements.
3. Store node-local byte ranges, stable fragment IDs, baseline/line bounds and caret mapping. Painting, hit testing and IME must use these exact records.
4. Keep authoritative content height and viewport bounds separate. `element_bounds` currently comes from the full document element; do not use that rectangle as the visible viewport for autoscroll or overlay clamping.
5. Preserve a stable fragment/position anchor plus intra-fragment offset on width/font/scale/image changes. `sync_image_dimensions` currently reduces an anchor to node ID and a line-local offset, then restores to that node's first line; this can jump within long paragraphs.
6. Have minimap and outline consume the same geometry. Map the indicator over the actual scroll range; clamp the effective 16 px indicator inside its track at the end. Validate mixed tall/short table fragments intersecting the viewport rather than looking back only one start position.
7. Do not run whole-document layout during resize. Prioritize the visible region and preserve a stable estimated extent until background refinement completes.

**Acceptance:** 480, 640, 799, 800, 999, 1000 and 1440 px windows; varied text widths, headings, nesting and 100/125/150/200% scale. Prose fits the reading region, no missing/overlapping glyphs, caret-to-point round trips match grapheme boundaries, and outline/minimap agree with the actual visible section. Image completion above the viewport and resize preserve the anchor within one physical pixel after layout settles.

**Depends on:** A04–A05. **Do not:** independently improve the minimap estimator while retaining two contradictory geometry systems.

### A08 — P1: fix input grouping, navigation, selection and IME behavior

**Evidence:** reproduced grouping failure; source-confirmed remaining gaps. `editor.rs:888` always sets selection before replacing; `document.rs:259` breaks the typing group on every selection command. Left/right (`:968`) use selection direction to choose the collapse edge incorrectly. `vertical_target` (`:998`) walks the currently painted line list. Ordinary typing/left/right/End do not consistently call `keep_offset_visible`. Mouse autoscroll (`:1963`) compares against content bounds and only advances on mouse-move events.

**Implement:**

1. Dispatch an atomic replace-range command carrying the intended selection, or skip redundant `SetSelection` when it is truly unchanged. Preserve a typing group only for contiguous edits with the same input mode and caret. Break it for real movement, selection, formatting, structural commands, paste and a pause over 500 ms.
2. Collapse a nonempty selection with Left to its lower visual/logical edge and Right to its upper edge, independent of drag direction; handle bidirectional visual movement explicitly.
3. Add conventional Ctrl+Left/Right, Ctrl+Home/End, selection variants, word deletion and PageUp/Down, using the command catalog. Double-click selects a word; triple-click/line selection requires one documented policy.
4. Resolve vertical movement in document geometry, not neighboring entries of a paint list; table columns sharing a Y position are not consecutive vertical lines. Maintain preferred X and correct wrap affinity.
5. Reveal the caret after every relevant edit/movement/composition update. Use frame-driven drag autoscroll while the pointer is outside viewport bounds, including a stationary held pointer. Cancel on release, focus loss or Escape.
6. Provide explicit composition commit/cancel paths and restore provisional text on cancel. Do not let autosave or another shared view commit provisional composition accidentally. Candidate bounds and point mapping use visible measured geometry.
7. Test bidi selections without assuming `x(start) <= x(end)`; selection painting may need multiple visual intervals per logical range.

**Acceptance:** GPUI input-handler tests type `a`, `b` within 500 ms and undo once; result removes both. Moving between them creates two groups. Test backwards selections, wrapped Home/End, movement past overscan, typing at the last visible line, stationary drag beyond each edge, combining accents, emoji ZWJ sequences, mixed Arabic/Latin and CJK. Real Fcitx5/IBus tests must cover provisional updates, commit, cancel, focus change and undo; helper UTF-16 tests alone do not qualify IME.

**Depends on:** A04 and A07 for geometry-dependent cases.

### A09 — P1: table layout and interaction must use actual cell boxes

**Evidence:** reproduced; `editor.rs:1733` (`index_for_mouse_position`) compares only vertical distance. Clicking “Bravo” in column 2 at 1280×720, pressing End and one Backspace changed `Alpha` to `Alph`; `Bravo` stayed unchanged. `prepaint` clips table content to the whole document width rather than each cell. The long first cell visibly paints over `Beta` and `Gamma`. `push_table_border` (`:3404`) paints a box per text line; `TextAlign::Left` is used for every line despite column alignment controls. Physical-pixel rules use a logical `px(1.)`.

**Implement:**

1. Layout rows from measured cell content widths/heights. Cell inner width equals resolved column width minus 12 px horizontal padding on each side. Row height is the maximum cell block-stack height plus 8 px top/bottom padding.
2. Hit-test row and column using both X and Y, subtract the table's local horizontal scroll, then map into the selected cell's block/line. Share this mapping with caret movement and accessibility.
3. Intersect painting/hit-test clips with the individual cell and table viewport. Paint shared table rules once at row/column boundaries, not around each line of text.
4. Apply column alignment to text origin/paint and caret mapping. Preserve the typography of headings/code/lists inside cells; `visual_line_style_for` currently returns generic 18 px/44 px style for all table text.
5. Keep column widths stable while typing. During drag show live width feedback without committing hundreds of undo entries; mouse-up commits one transaction. Do not silently scale a specified small width to fill the whole table.
6. Distinguish text drag from rectangular selection with real edge handles. Provide keyboard selection alternatives. Implement Cut/Delete of rectangular contents; current rectangular Cut only copies.
7. Preserve horizontal scroll per table/code block when it temporarily leaves the viewport. `paint` currently removes owners absent from the current horizontal metrics.
8. Compute one-physical-pixel rules as scale-aware snapped geometry. Add visible horizontal overflow affordances without permanent editor chrome.

**Acceptance:** a 3-column fixture selects/edits the clicked cell in all columns before and after horizontal scroll. Long content stays inside cells and grows row height. Alignment affects glyphs and caret consistently. Resize is immediate, stable and one undo step. Row/column insert/delete/move, Tab/Shift+Tab, final-cell Tab, Ctrl+Enter and rectangular paste all work with valid selections. Multi-block cell tests must pass at fractional scale.

**Depends on:** A03–A04 and A07–A08.

### A10 — P1: fix full-tree copying in ordinary edits

**Evidence:** reproduced lost sharing and approximately 147 ms model-edit p95 at 10 MiB. `document.rs:2074` (`mutate_node`) clones a sequence, then calls `Arc::make_mut` on every earlier sibling while searching descendants, even if it cannot contain the target. `mutate_node_optional` repeats this. `find_node` is recursive linear search. Selection validation/materialization and copy-on-write revision maps add work.

**Implement in two measurable steps:**

1. Locate the target and its ancestor path without mutation. Call `Arc::make_mut` only on that path. Consolidate `mutate_node`/`mutate_node_optional` around the same implementation; record touched/inserted/deleted nodes directly in the transaction instead of discovering all changes through pointer comparisons.
2. Measure again. If sibling-array copying, lookup, revision-map cloning or dirty-set cloning still exceeds the edit budget, introduce indexed node access and a persistent/chunked sequence behind `BlockSequence`. Keep its consumer facade; do not expose a new storage representation to every command. The target is bounded path updates, not a full `Vec` clone per keystroke.

Use rope byte/UTF-16 metrics for validation/conversion where possible; `RichText::validate_range` (`text.rs:111`) and `validates_position` materialize the entire node string for boundary checks. Keep `as_string` for serialization/debug or genuinely bounded outputs. Maintain revision/dirty data without retaining every deleted node indefinitely.

**Acceptance:** unchanged sibling nodes retain pointer sharing across a local edit and undo. All semantics from A01–A04 pass. Measure first/middle/last paragraph, deep list, giant paragraph and rich cell, with 1,000 sequential edits and undo/redo. Report time, allocations and retained history memory. Initial diagnostic target: model portion p95 ≤2 ms on the qualification machine; overall frame/input gates still govern release. Do not add unsafe indexing or SIMD to compensate for copying the wrong data.

**Depends on:** A04; preserve serializer behavior established in A01–A03.

### A11 — P1: remove global work from view refresh and rendering

**Evidence:** measured ~26 ms view-edit p95 even on the harness's late-node case. `projection.rs:96` replaces a range of one flat `String` and updates following segment/UTF-16 offsets; its comment claiming no copying/walking of the remainder is inaccurate. `editor.rs:3576` shifts all following visual-line ranges/heights and paint-order indices. Table edits fall back to a full projection/layout rebuild. `build_visual_lines_for_segment` leaves code lines unfragmented. `styled_runs` materializes node text; `sync_image_dimensions` scans every image segment. Shell rendering reconstructs all file/outline rows.

**Implement:**

1. Store text/node fragments independently and derive global byte/UTF-16 positions via prefix sums or an order-statistics index. Use node-local ranges in cache keys and geometry so an edit before a fragment does not rewrite that fragment's offsets.
2. Update only dirty fragments and affected table rows/columns. Height-tree updates and stable anchors replace shifting every downstream Y coordinate.
3. Fragment giant paragraphs and code lines into bounded shaping units. Preserve shaping/line-break context across boundaries; never split graphemes or lose bidi/ligature context just to meet a byte chunk limit.
4. Make image-dimension changes events keyed to affected nodes. Do not collect and compare every image's dimensions on each render.
5. Maintain outline updates independently of full estimated layout. Reuse `project_outline` instead of maintaining another `OutlineEntry`/recursive traversal in `main.rs:1411`; update only affected headings when possible.
6. Virtualize large file trees and outlines with stable path/node IDs; using the visible row index as identity makes insertion/reordering unstable. Avoid recreating every row after each editor notification.
7. Store cancellable timer/task handles for autosave, workspace-state debounce and toolbar delay. A generation check prevents stale application but does not stop spawning a timer for every pointer move/keystroke.
8. Bound cache work by bytes as well as entry count. Do not equate a 2,048-line cap with bounded memory if a line may contain megabytes.

**Acceptance:** instrument actual nodes visited, bytes copied, fragments shaped, invalidations and allocations. A single-character edit/selection movement must not traverse every untouched paragraph or rebuild the file tree. Table typing recomputes only coupled row/column geometry. Giant single-line code is cancellable and keeps foreground work bounded. Initial view-update diagnostic target p95 ≤1.5 ms; full frame p99 ≤6 ms remains the release gate.

**Depends on:** A07, A09–A10.

### A12 — P1: centralize file sessions and their resource lifetimes

**Evidence:** source-confirmed; `main.rs:115` (`SessionRegistry`) retains strong sessions indefinitely and only deduplicates subscriptions by the current window ID. There is no detach/prune path. Save/dirty/identity/conflict state is duplicated in registry, window and shared editor state. `load_workspace_state` writes one global file from multiple windows. `session.rs` shares the document's selection, while selection-only changes do not bump generation or broadcast like content changes.

**Implement:**

1. Move file lifecycle state into the shared session/controller. Use a typed status model with explicit dirty/saving/conflict/recovery information and a pending-save revision; keep orthogonal states separate where they truly are independent.
2. Window attach/detach owns one weak subscription. Prune dead listeners and release clean inactive sessions/caches. Keep dirty/recoverable sessions only under an explicit lifecycle rule.
3. Session-owned save tasks must release claims on all outcomes, including cancellation and originating-window destruction. Avoid a permanently stuck `save_in_flight` flag.
4. Give each view an independent selection/scroll/composition focus context. Commands take the initiating selection and return transformed selections; shared history records the origin and restores the relevant selection on undo. Content edits transform other live selections without arbitrarily moving their viewport.
5. Persist navigation root, expanded directories, split ratio, widths and per-document/view anchors under an explicit workspace/window key. Define which active window is restored; do not let arbitrary background save completion choose it.
6. Canonicalize file identity once and reuse it. Reopening an existing session should not parse the file and then throw the freshly prepared document away.

**Acceptance:** open the same file in two windows, edit/undo/save from either, close one during a save, and reopen. Content and dirty status agree; carets/scroll positions remain independent. Repeatedly open/close 100 documents and verify sessions, listeners, history and caches return to a bounded steady state. Simultaneous workspace-state writes do not overwrite unrelated windows' state.

**Depends on:** A05–A06; coordinate the view-selection API with A04/A08.

### A13 — P1: complete nested rich editing and clipboard interoperability

**Evidence:** source-confirmed; `document.rs:1151` and `:1290` restrict multi-block replacement/paste to top-level text nodes. `clipboard.rs:136` rejects cross-block copy with nested endpoints. Insertion after a selection (`document.rs:668`) resolves a top-level enclosing block, so insertion from a table cell can end up outside the table. `editor.rs:1139` publishes only plain text plus private metadata, while paste never reads HTML. The core `ClipboardPayload` precedence tests do not exercise the native clipboard adapter.

**Implement:**

1. Use document-order traversal with container paths for selections. Define edits within list items, quotes and a cell's block sequence; support the same split/join/format operations there as in ordinary prose.
2. Insert at the selected container position, not always after its top-level ancestor. Respect the v1 exclusion of nested tables: disable/reject insertion where unsupported while preserving the selection.
3. For text ranges spanning table cells, preserve table structure and operate on selected text portions; rectangular selection is the explicit matrix operation. Write these semantics down in core tests before adding UI shortcuts.
4. Publish plain text, semantic HTML and the app representation through actual Wayland MIME offers when the pinned GPUI clipboard API supports them. If it lacks HTML/custom offers, isolate the missing capability in one platform adapter/dependency patch and qualify it; private `ClipboardItem` metadata is not proof of inter-application MIME support.
5. Paste chooses supported app-rich content, then supported HTML, then literal plain text. Current TSV detection runs before rich paste; preserve explicit rich formatting unless the user is performing a matrix paste.
6. Preserve empty TSV rows/cells and CRLF consistently; `table.rs` currently filters empty lines. Bound clipboard parse/expansion work and make it one transaction.

**Acceptance:** copy from a rich cell to another cell, paste multiple paragraphs into a quote/list/cell, select across nested nodes and replace/cut/undo, and paste a rectangular TSV including an empty row. Test native clipboard exchange with at least one browser and one plain text editor, in both directions, with the source app exiting after copy. “Paste as Markdown” parses syntax; ordinary text paste does not.

**Depends on:** A01–A04, A08–A09.

### A14 — P1: one discoverable command model and reliable transient UI

**Evidence:** source-confirmed; actions/bindings in `editor.rs:133–204`, hand-built toolbar/context-menu `div`s at `:2244–2939`, and header in `main.rs:1720`. The header control only toggles navigation; it is not the planned application menu. There is no complete keyboard command inventory. Toolbar hardcodes H2, has no list conversion controls or mixed formatting state. Context menus/popovers are positioned without reliable viewport clamping; the toolbar assumes a 440 px width even when table tools extend it. Most controls have no keyboard focus/role/tooltip behavior.

**Implement:**

1. Make one typed command catalog describing ID, label, scope, enabled/checked/mixed state, shortcut and dispatch. Core `EditCommand` remains the content mutation model; the catalog maps user actions to it rather than duplicating mutations.
2. Add the application menu with A06 file actions, undo/redo, navigation/outline/minimap commands and keyboard help. Save must work when focus is in navigation or a popover, not only the editor key context.
3. Use GPUI Component buttons, menus, tooltips and popovers where available. Inspect the pinned API before implementing; preserve document selection when transient controls take focus.
4. Offer Paragraph/H1–H6, quote/code and list/task-list commands with actual mixed/active state. Use meaningful tooltips instead of relying on labels such as `+R`, `−C` and arrows alone.
5. Anchor overlays in viewport coordinates with measured dimensions; clamp all edges, flip below selection when necessary, and move excess table commands into overflow. Right-click near the bottom must not place commands offscreen.
6. Show selection tools after pointer release or keyboard settlement; the delayed callback must also check that pointer dragging/scrolling/composition has ended. Dismiss on scroll/Escape and restore editor focus.
7. Present loading, recovery, save errors and conflicts as distinct states. Keep details/actions in a readable status surface; do not truncate the sole recovery instructions inside a narrow titlebar or color “Loading” as an error.

**Acceptance:** complete the entire command inventory with keyboard only. Check focus order, visible focus, shortcuts, disabled states, mixed formatting, Escape and return focus. At 480×360 every critical action remains reachable; long filenames/errors and table selections do not push controls outside the window.

**Depends on:** A06, A08–A09, A13.

### A15 — P1: implement the agreed visual system consistently

**Evidence:** source-confirmed and visually inspected. `main.rs:68` forces `ThemeMode::Dark`; both app/editor define separate hard-coded dark constants. `main.rs` adds navigation/minimap borders despite the plan's quiet surface treatment. `editor.rs:3928` loses heading/code sizing inside cells; no heading tracking application was found. Alerts/footnote definitions are flattened without distinct context in `projection.rs:414`; inline HTML and inline images do not have equivalent rendered components.

**Implement:**

1. Introduce shared semantic tokens resolved from OS appearance; update existing windows on appearance changes. Do not add an app theme preference. Theme text, links, selection, caret, popovers, rules and loading/errors together; changing only `ThemeMode` cannot recolor hard-coded paint quads.
2. Use the exact typography/spacing roles below. Measure prose width from the bundled body font; 68 characters is a reading-measure target, not an instruction to break after the 68th scalar value.
3. Remove unnecessary sidebar/minimap panel rules; use spacing, tonal grouping, hover and active-row cues. Preserve visible focus and content-level quote/table rules.
4. Build a single component fixture for paragraphs/headings, emphasis/code/links, ordered/unordered/task lists, quotes, all alerts, footnotes, thematic breaks, images, preserved HTML, and rich tables. Every supported node needs a deliberate visual representation and editing/selection state.
5. Distinguish soft line breaks from semantic hard breaks in rendering. Never show raw `<em>`/`<br>`/image syntax as if it were ordinary rich content. Preserve unsupported objects inertly with compact labeled placeholders and a usable insertion position.
6. Render inline images as inline replaced content, not only standalone `Image` blocks. Render alert kind/title and footnote reference/target affordances. Keep actual rich content in the document, never only in metadata.
7. Connect OS reduced-motion preference updates. Current environment-variable reading is a startup override, not complete system preference integration. Keep the override useful for deterministic tests.

**Acceptance:** reviewed screenshots for every component/state in light and dark at all supported scales and widths. No overlap, clipping, raw syntax leakage, inconsistent fonts, or unexplained empty panels. Normal text contrast ≥4.5:1, large text ≥3:1; focus/controls remain distinguishable. Test actual rendered colors/geometry rather than checking token constants alone.

**Depends on:** A07, A09 and A14. Theme/token extraction can start earlier, but final visual approval follows layout correctness.

### A16 — P1: make accessibility functional rather than decorative

**Evidence:** reproduced AT-SPI limitations; `editor.rs:4079–4216` creates semantic nodes as absolute 1×1 elements at origin. A paragraph containing one link receives `Role::Link` for the whole paragraph. Native AT-SPI output had no exposed text for the editor, no link actions and identical bounds for headings/cells. Navigation items advertise click but were not reported as focusable in the inspected tree. Role existence tests do not prove screen-reader usability.

**Implement:**

1. Generate stable semantic text runs, link spans, table rows/cells and headings with their actual visible geometry. Use the pinned GPUI/AccessKit text and selection facilities; identify an upstream capability gap explicitly if the adapter cannot expose them.
2. Expose editable text, caret/selection, word/line boundaries, text retrieval and editing actions. Link spans need target/action semantics, not a link role applied to the entire paragraph.
3. Expose row/column indices, header relationships, task checkbox state, navigation expansion/selection and minimap value/range/action. Do not mark every footnote or static callout as a live alert.
4. Make keyboard focus explicit. Trees use a roving focus model and arrow-key navigation; Tab enters/leaves a group. The editor's Tab indentation/cell navigation must have an alternate way to reach other regions.
5. Expose accessible scroll/reveal actions for offscreen content while keeping tree updates bounded. Update meaningful changes without rebuilding/reannouncing the entire document after each character.

**Acceptance:** AT-SPI tests retrieve actual text and caret/selection and invoke links/checkboxes. Bounds track real content after scroll/resize. An Orca session can read headings, navigate cells, edit and undo, access all file actions and recover from a save conflict. Keyboard-only navigation can enter and leave every pane/popover. Record unsupported upstream behavior as an open blocker, not an `aria_label` workaround.

**Depends on:** A07–A09, A14–A15.

### A17 — P1: finish image resource limits, invalidation and failure UI

**Evidence:** source-confirmed; `image_cache.rs:177` delegates full decoding to `ImageAssetLoader`, then checks decoded byte weight at `:216`. Header dimension limits do not bound total decoded animation frames or all simultaneous decoding allocations. There is no app-controlled resize-before-upload step. The dimensions map is not pruned with entries. Each window has its own disk-cache lock despite sharing the directory. `write_atomic` (`:610`) uses PID + milliseconds, which can collide between same-process cache instances. `resolve` has no explicit per-request/body deadline and waits for network before using cached bytes.

**Implement:**

1. Share the resource service/cache ownership across windows as appropriate. Use collision-resistant temporary names and coordinate disk publication/eviction; publish data and metadata coherently. Do not hold a process mutex across network awaits.
2. Reserve decode memory before work, bound dimensions × frames × bytes per pixel with checked arithmetic, and cap simultaneous decode/upload memory. Reject unsupported oversized animations before a full allocation where the decoder permits it.
3. Decode/resize to an appropriate display resolution on workers and keep original bytes in disk cache. Cache by source identity and requested size/scale, with bounded variants.
4. Prune dimension metadata or bound it separately without destabilizing layout when decoded images are evicted. Local images need modification identity/invalidation; a path-only permanent cache entry becomes stale after the file changes.
5. Apply request/header/body deadlines and cancellation. Display validated cached content promptly while revalidating when policy allows, and retain offline fallback. A failed body read should have the same considered fallback policy as a connection/server failure.
6. Implement explicit loading/failure/oversized states with alt text, reserved aspect ratio and Retry. Resolve link/image resources centrally; consolidate duplicated app/editor path resolution and define local paths, percent encoding, fragment links and external URL handling.

**Acceptance:** four-load scheduling, slow response/body, redirects, offline cached reopen, corrupt cache, missing/replaced local image, huge PNG/SVG and many-frame animation. Measure peak resident/decoded/GPU memory during simultaneous loads and across multiple windows. Failure retains alt text and stable height; Retry recovers without reopening the document. Image arrival above the viewport preserves the A07 anchor.

**Depends on:** A05, A07, A12.

### A18 — P1: preserve navigation root and responsive state

**Evidence:** source-confirmed; `open_file` unconditionally calls `load_navigation(canonical.parent())`, replacing an explicitly opened folder root when a nested file is selected. `navigation_children` silently flattens directory read errors and includes generated/hidden folders. Width/breakpoint logic is duplicated between `responsive.rs` and `main.rs`; the standalone helper is not driving the shell. The navigation/outline 60/40 split is fixed rather than draggable.

**Implement:**

1. Track explicit folder-root mode separately from “show this file's parent.” Opening a descendant retains an explicit root; opening an unrelated file makes a deliberate root transition.
2. Use one responsive-layout policy in the actual shell. Restore pane width and split ratio, clamp at the current window size, and retain them when a pane temporarily collapses.
3. Add the draggable navigation/outline split with keyboard alternatives and independent scroll. Show active heading from the authoritative geometry/scroll position.
4. Make overlay ordering, outside-click dismissal, Escape and focus return explicit below 800 px. Minimap disappears below 1000 px while its navigation function remains reachable through commands.
5. Show directory-loading/error/empty states. Avoid presenting `target`, `.git`, and internal metadata folders as the primary document navigation by default; define a documented filter/reveal rule. Do not silently drop unreadable directories as though they were empty.

**Acceptance:** open a folder, expand two levels, open a descendant and retain the root/expansion. Resize across every breakpoint, reopen and verify widths/split/anchor. Test unreadable, removed and large directories, keyboard tree traversal, no pointer interception by a hidden overlay and no inaccessible focus behind a visible overlay.

**Depends on:** A12, A14; use A11 virtualization for large trees.

### A19 — P2: make validation repeatable and tied to actual workflows

**Evidence:** the repository has no checked-in CI configuration or integration fixture suite. Existing tests are inline and valuable, but seven targeted audit probes still fail. `mineral-perf` is a CPU helper benchmark, not the presentation/input harness required by `plan.md`.

**Implement:**

1. Add reviewed fixtures and integration tests in `crates/document-core/tests/`, GPUI workflow tests in the view crate, and deterministic file-controller tests in the app. Start with the audit regressions; do not aim for a test-count/coverage percentage.
2. Add a Linux/Wayland build workflow and `scripts/check.sh` containing the baseline commands. Record native build prerequisites and the supported toolchain in a root README. Keep fast model tests runnable without a compositor.
3. Add scripts for isolated state/cache and disposable documents, native screenshot capture, AT-SPI snapshots and interaction replay. Tests must assert visible behavior and saved content, not merely that a process starts.
4. Expand the benchmark to output raw stage/event records, independent end-to-end timings, fixture identity/hash and all environment details. Include actual input-handler, save and paint paths. Separate warmup from sampling.
5. Add a small qualification report template listing scenario, source revision/digest, command, environment, result, raw artifact and unresolved limitation. Performance/visual claims require artifacts from the same build being reviewed.

**Acceptance:** a clean environment can run the documented commands and reproduce core failures before their fixes and passes afterward. CI does not accept screenshot differences automatically or hide failed interaction tests behind retries. Section 6's matrix has explicit passing evidence or named open blockers.

**Depends on:** regression scaffolding starts immediately; final qualification follows A01–A18.

### A20 — P2: enforce reproducible dependencies and remove misleading/dead paths

**Evidence:** `Cargo.toml:44–46` declares Zed Git dependencies without `rev`; `workspace.metadata.source-pins` is descriptive and does not pin Cargo resolution. `Cargo.lock` currently does lock Zed to `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`. Crusty flagged the three floating declarations. It also flagged blocking operations in `open_file`/`queue_reload`; those particular calls are already inside GPUI `spawn_dedicated`, so the suggested async-blocking diagnosis is not established and must not be copied as a finding.

**Implement:**

1. Keep the lockfile and use `--locked` for CI/release. Document an intentional update procedure and enforce the expected Zed revision for the complete dependency graph.
2. If adding explicit Git revisions, first inspect GPUI Component's transitive source declarations. Ensure all GPUI/Zed consumers resolve to one compatible source identity; naive direct-only `rev` additions can create duplicate crate universes. Use one coherent source configuration and inspect `cargo tree -d`/metadata before accepting the change.
3. Audit active Wayland/X11 features against the stated platform support without claiming all transitive X11 code is removable. Do not migrate toolkit or upgrade the pinned UI dependencies as an incidental cleanup.
4. Consolidate the duplicate outline, responsive, resource-resolution and scroll-anchor implementations when their owning tasks land. Search all callers before deleting an exported helper. `document-view/src/lib.rs` currently describes painting as an adapter over geometry more strongly than the code supports; update documentation to match the resulting architecture.
5. Retain typed errors and purposeful facades; remove only proven dead dependencies/exports. Add root license files matching the declared license policy and keep bundled font notices/package metadata intact.

**Acceptance:** clean locked build/test, one intended GPUI/Zed dependency graph, no changed product scope, no old layout pipeline still driving a second consumer, and packaging checks in section 6 pass. Record any optional metadata warning rather than inventing a project homepage.

**Depends on:** A07/A11/A12 for consolidation; lock/build documentation can land early.

## 5. Concrete visual specification for the implementation

This section applies the accepted `plan.md` direction. It is not a new visual concept or permission for a toolkit rewrite. The [original visual reference](https://www.whichai.dev/with-design-skill/opus-5/4) was fetched; use its typography/mineral character together with the editor-specific dimensions already agreed in `plan.md`.

### Shared roles

| Role | Required value/behavior |
| --- | --- |
| Body/cell text | Spline Sans, 18 logical px, weight 400, line height 28.8 px |
| H1 | Fraunces static H1 face, 42/42.84 px, weight 600, tracking −0.022 em |
| H2 | Fraunces static H2 face, 32/35.84 px, weight 600, tracking −0.014 em |
| H3–H6 | Fraunces, 26/22/19/17 px, line height 1.2 |
| Code | Spline Sans Mono, 15/22.5 px; preserve whitespace; local horizontal overflow |
| Navigation/controls | Spline Sans, 13/18.2 px; stable 28 px navigation rows |
| Block spacing | 4 px scale; paragraph after 16 px; heading before 32 px/after 12 px; no extra per-wrapped-line paragraph gap |
| Table cells | 12 px horizontal and 8 px vertical padding; semibold header; dotted/default, none or one physical pixel rules; no zebra striping |
| Overlays | 36 px selection-toolbar height, 8 px radius, restrained shadow; real hover/pressed/disabled/mixed/focus states |
| Shell | 36 px header target; 224 px initial navigation clamped 180–320; 64 px minimap region; ≥32 px document side padding |

If the pinned text API cannot apply tracking directly, validate a coherent shaping/positioning implementation or explicitly document the limitation. Do not insert spaces between characters to imitate tracking: that breaks copying, selection and accessibility.

| Semantic color | Light | Dark starting point |
| --- | --- | --- |
| Page | `#FCFBF8` | `#18232F` |
| Navigation | `#FCFBF8` | `#141F2A` |
| Primary text | `#1B2430` | `#E8E3D8` |
| Secondary text | `#59636F` | `#91A3B5` |
| Hover | `#F1EFE9` | `#293947` |
| Floating surface | `#FFFFFF` | `#273643` |
| Link | `#256F50` | `#43A17A`, subject to contrast verification |
| Active/caret accent | `#256F50` | `#D59B3B` |
| Selection | `#DCEBE1` | `#514229` |
| Table rule | `#D8D5CE` | `#354554` |
| Error | `#9E4B3F` | `#C66B5B` |

Dark values are an implementation starting point extracted from the existing app, not a completed contrast/visual approval. Derive distinct active/inactive selection, focus and disabled roles and measure the composited result. Preserve document photography/illustrations rather than recoloring them globally.

### Window behavior

| Window | Required composition |
| --- | --- |
| 480–799 px | Document + header; temporary navigation overlay; no minimap; all file/recovery commands reachable |
| 800–999 px | Resizable navigation + document; no minimap |
| ≥1000 px | Navigation + centered prose region + minimap; tables/images may use the wider central workspace |
| Short height | File/outline panes scroll independently; transient menus clamp/scroll; no offscreen recovery buttons |

The current `.max_w(760.)` wraps the entire editor, constraining tables and images as well as prose. Move the reading-measure constraint to prose layout, leaving block-wide content room to use the central workspace. Do not compensate with smaller text or less than the agreed padding.

Review visual fixtures in these states: initial empty, loading, normal, selection, mixed formatting, caret in a cell, rectangular selection, link/image editor, menu near each edge, failed image, save failure, external conflict, recovery available, inactive window and keyboard focus. Screenshots must include real text, not just empty containers.

## 6. Validation protocol and release gates

### Fast checks for every implementation slice

```sh
cargo fmt --all -- --check
cargo test -p document-core --locked
cargo test -p document-view --locked
cargo test -p markdown-app --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Run the affected package first during development. At a milestone, run `cargo test --workspace --all-targets --locked` and `cargo test --workspace --doc --locked` separately. A documentation-only slice does not need repeated full recompilation when source is unchanged. Crusty validation is advisory and does not replace these checks or native qualification.

### Fixture inventory to add

| Fixture | Minimum contents and oracle |
| --- | --- |
| `source-preservation.md` | Setext/ATX headings, unusual spacing, list marker variants, references/comments/front matter, CRLF case; compare untouched bytes after edits elsewhere |
| `literal-inline.md` | Escaped Markdown punctuation, entities, angle brackets, backticks, mixed styles and URLs; compare semantic model after edits/save/reopen |
| `rich-table.md` | 3 columns with each alignment, long cells, links/bold/code, empty cells, multi-block cells and metadata; verify cell geometry and reopened semantics |
| `unicode.md` | Combining marks, emoji families/flags, Arabic/Hebrew + Latin, CJK and tabs; grapheme movement and point/caret mapping |
| `components.md` | All visual component/state roles from section 5, including unsupported inert HTML and inline/standalone images |
| Generated 100 KiB/1 MiB/10 MiB | Many short blocks, many headings, deep lists, a large table and image-heavy variants; stable seed/hash |
| Giant-block stress | One oversized paragraph and one single-line code block; cancellation, bounded shaping and no UI freeze |
| File/network fault scenarios | Real disposable source files, local HTTP fixture server and fault barriers; assert preserved content and retry outcomes |

Model properties should generate valid and invalid operation sequences, validating text runs, IDs, selection, table shape, monotonic revisions and semantic round trips. Include structural edits, table operations, rich paste, composition and undo/redo; the current fixed operation loop covers too little of that state space. Add fuzz targets for import/serialization/clipboard boundaries after deterministic regressions are secured.

### Native scenario matrix

| Scenario | Required assertion |
| --- | --- |
| Open/edit/save/reopen | The exact edited semantic content is present; untouched regions remain byte-identical |
| First-use untitled | New text can be saved through a chooser without a CLI argument |
| Paragraph split/merge | Save/reopen retains paragraph boundaries; undo restores content and selection |
| Table pointer editing | Every column receives edits at the clicked location; no text overlaps adjacent cells |
| Keyboard-only | File actions, formatting, pane navigation, table commands and error recovery all reachable with visible focus |
| Selection tools | Commands preserve selection; overlays stay in viewport; Escape restores focus |
| Async race suite | Held completion cannot overwrite later edits, newer file sessions or newer geometry |
| Save/close fault suite | All committed edits survive failures and normal close; conflicts never silently choose a version |
| Shared windows | One content/history owner, independent view positions, no stuck save or leaked session |
| OS integration | Live light/dark and reduced-motion change; actual Wayland IME composition and clipboard interoperability |
| Accessibility | Actual text/selection/actions through AT-SPI plus an Orca walkthrough, not only roles |
| Scale/resize | 100/125/150/200%, breakpoint boundaries, minimum 480×360, large/short/inactive/multi-monitor windows |

A repeatable table reproduction fixture is:

```markdown
# Audit fixture

| Left | Center | Right |
| :--- | :---: | ---: |
| Alpha | Bravo | Charlie |
| A long cell containing enough prose to wrap over several lines within its own column without painting over its neighboring cells. | Beta | Gamma |
```

Open a disposable copy. Click inside `Bravo`, press End and tap Backspace exactly once. Expected: `Brav`, with `Alpha` unchanged. Check the model and saved file, not only the caret drawing. Long-row text must wrap within column 1 and never draw over columns 2–3. Coordinates vary with the heading/window; automation should derive them from geometry or verified screenshots rather than reuse the audit's larger fixture coordinates blindly.

### Performance qualification

Keep the product gates from `plan.md`:

| Metric | Release gate |
| --- | --- |
| Warm launch, 100 KB, editable first viewport | p95 ≤100 ms |
| Cold launch, 100 KB, editable first viewport | p95 ≤250 ms |
| Open/prepare editable 10 MB | p95 ≤1 second |
| Application frame work during scroll/edit | p99 ≤6 ms |
| Input to presented caret/text | p95 ≤16.7 ms |
| Missed presentation deadlines | <0.1%; no app-caused stall ≥25 ms |

Record whether each fixture uses decimal KB/MB or binary KiB/MiB. Measure on the specified Ryzen AI Max+ PRO 395/Radeon 8060S machine with a real 120 Hz Wayland output, documenting compositor, kernel, driver, resolution, scaling, refresh rate, CPU/power policy, toolchain, dependency revision and source digest.

- Thirty launch/open samples per scenario; report warm/cold separately and first-ever GPU initialization separately.
- Five 60-second interaction runs for 100 KB, 1 MB and 10 MB, including resize, table typing, selection drag, images and simultaneous autosave.
- Timestamp event receipt, transaction completion, projection/layout completion, prepaint/paint, frame submission and presentation feedback. A screenshot series or timer duration is not a presentation-latency measurement.
- Record raw samples and p50/p95/p99/max; do not hide long stalls in average FPS. Track memory/allocations, cache occupancy, queue lengths and idle wakeups.
- Include first/middle/last and nested edit locations, not just the last top-level paragraph. Profile the full failing input path before choosing the next optimization.
- Keep correctness/visual oracles active during performance tests. Dropping content, shaping or accessibility to get faster numbers is a failure.
- Proposed additional diagnostic sub-budgets in A10/A11 guide optimization; they do not replace these product gates. Establish a measured memory baseline and explicit caps for history, decoded images and in-flight work before claiming bounded resource use.

### Packaging/clean-machine qualification

```sh
cargo build --release --locked --bin mineral-markdown
packaging/install.sh /tmp/mineral-stage/usr
desktop-file-validate /tmp/mineral-stage/usr/share/applications/dev.mineral.Markdown.desktop
appstreamcli validate --no-net /tmp/mineral-stage/usr/share/metainfo/dev.mineral.Markdown.metainfo.xml
xmllint --noout /tmp/mineral-stage/usr/share/icons/hicolor/scalable/apps/dev.mineral.Markdown.svg
```

Use a fresh staging directory. These packaging validators were not executed during this audit. Also launch via the staged desktop entry with filenames containing spaces and multiple paths, with no development font setup or prior state/cache. Keep the known optional homepage warning documented until a real homepage exists.

## 7. Execution sequence for the implementing agent

1. **Create red tests and a source baseline.** Read this audit, `plan.md` and current instructions; consult Crusty. Reproduce A01–A04/A08/A10 with the exact described operations. Capture a native rich-table/compact-window baseline. Preserve the user's untracked work.
2. **Stabilize data:** A01 → A02 → A03, with A04's invariant enforcement. Do not broadly rewrite serializers and tree storage together; keep each regression attributable.
3. **Stabilize lifecycle:** A05 → A06 and the minimum A12 session ownership needed to make races deterministic. Test close/save/recovery before polishing their controls.
4. **Unify geometry:** A07, then A08/A09. Remove the second geometry consumer only after editor/minimap/outline/IME tests all run against the shared index.
5. **Restore responsiveness:** A10 then A11, profiling after each bounded change. Complete A12 resource/view ownership and A17 image behavior.
6. **Finish product workflows:** A13, A14 and A18; land A15 tokens/component visuals and A16 semantics on the corrected geometry. Capture approved light/dark/scale fixtures.
7. **Qualify and simplify:** A19/A20 and the entire release matrix. Report unverified gates explicitly; do not mark a task complete because it compiles or a helper test passes.

For each task, the implementation report must state: task ID; changed files/API; exact previous failure; behavior after the change; executed test/benchmark commands; evidence artifact; remaining limitation. A task is complete only when its acceptance criteria pass. If the code has changed since this audit, re-check the named symbol and update the finding rather than mechanically applying stale line numbers.

## 8. Primary references and evidence limitations

- `plan.md` is the local product/visual/performance contract. This audit recommends completing it, with implementation-level choices called out as such.
- [GFM specification: tables](https://github.github.com/gfm/#tables-extension-) allows inline content in pipe-table cells; block-rich cells require another serialization representation. This supports keeping inline formatting in GFM while using semantic HTML for multi-block cells.
- [GFM backslash escapes](https://github.github.com/gfm/#backslash-escapes), [code spans](https://github.github.com/gfm/#code-spans), and [links](https://github.github.com/gfm/#links) are the serializer test references. Use the pinned Comrak implementation plus semantic fixtures as the executable compatibility target.
- [Visual reference](https://www.whichai.dev/with-design-skill/opus-5/4): use the agreed font/mineral direction and the local adaptation; do not import its marketing-page structure into the desktop editor.
- Crusty's floating-dependency findings are qualified by the existing lockfile. Its two blocking-in-async findings were not accepted: the inspected calls already execute inside `spawn_dedicated`. There is a separate, real synchronous serialization call in `save_conflict_copy` (`main.rs:1058`) before the worker is spawned; move that serialization inside the worker under A05/A11.
- Visual inspection establishes clipping/overlap at the tested sizes, not complete international typography or accessibility compliance. AT-SPI inspection establishes the reported missing text/actions/bounds, not a completed Orca evaluation.
- The temporary raw probes/performance reports were produced under `/tmp/mineral-audit-20260906/`. They are supplemental session artifacts and may be removed by the system. The operations, observed results, baseline commands and required permanent regression cases are recorded above so implementation does not depend on those temporary files surviving.
