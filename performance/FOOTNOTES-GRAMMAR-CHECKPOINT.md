# Source-linked footnotes — grammar checkpoint

E09 now has native superscripts and editable notes. This is partial family
coverage, not approval of the complete grammar, editorial board or pagination.
The written grammar is authoritative; raster boards are visual specimens.

## Implemented

- Resolved references take the measured width of a superscript number, not
  the width of the Markdown label. Numbers follow first reference occurrence
  in source order, including repeated and adjacent references. Definitions
  retain their authored order; they are not rearranged by visual columns.
- References and inline math share source-indexed measured attachments.
  Wrapping keeps the preceding word, punctuation and reference together.
  Source indices, source copying and ordinary rich editing are retained.
- Notes use 14/20 sans text, a 40 px number rail, 12 px internal paragraph
  gaps and 16 px inter-note gaps. The authored Notes heading has a 24 px
  following gap. Return arrows, digits, insets and gaps scale with zoom.
- Pointer references and Alt+Enter go to the live note destination. Each
  note has a named return link to its first reference. Source-owned AT-SPI
  actions share those destinations; redundant unnamed component buttons
  were removed. The note marker is not inserted into copyable body text.
- Caret movement and selections treat a reference as an atomic source
  object. Adjacent references remain separate objects. Inserting text at
  either edge does not inherit the reference style and hide the insertion.
- Renumbering invalidates relevant list, table and group measurement
  identities. Editing note bodies refreshes aggregate accessible labels.
  Unchanged scrolling consumes prepared geometry, not per-frame analysis.
- Imported note-paragraph spans allow edits to preserve untouched note
  paragraphs, entities, reference-link spelling, blank separators and line
  endings. Safe source-local patches also handle notes Comrak lifts out of
  blockquotes, without duplicating the unchanged definition. Synthetic HTML
  fragment offsets are never treated as original-document offsets.

## Verification

Final native evidence below uses release SHA-256
`9511507bf8f74e4a7c4248e63fe34f4934ce5bf53794bb45fc50e8cea92fb201`.
The immutable test copy is
`/tmp/tachyon-footnotes-repaired.CLld2v/tachyon-canonical`.
Fixture `layout-fixtures/66-footnotes.md` has SHA-256
`c65a88f5495219f39b61054e1ceff08a59bc969c316665e02802afa397e04f9a`.
Later shared-target builds must not inherit these results.

All five `layout-previews/footnotes-canonical-{wide,narrow,narrow-notes,200,200-notes}.png`
captures were visually inspected. Wide and enlarged views are 1600×1200;
narrow views are 600×1100. Enlarged text is 200%, with unscaled app chrome.
The native periodic Weston harness uses 120 Hz and private documents/state.
Every capture has unchanged-source and active AT-SPI sidecars.

`python3 performance/footnote_layout_check.py --prefix footnotes-canonical`
passes across all five views. It checks matching binary/fixture hashes,
reference order 1/2/1/3/3, authored note order 2/1/3, advertised link actions,
absence of duplicate return buttons, measured marker widths and visible ink,
40 px text insets, 20 px leading, and 12/16/24 px gaps with zoom scaling.
The enlarged scrolled capture positively checks all three note markers.
This is a specimen oracle, not an exhaustive contrast or glyph-shape audit.

Native input evidence on that same build:

- `footnotes-canonical-forward-edit.edit.json`: clicking reference 1, typing,
  autosaving the exact expected full Markdown and undoing to the original
  bytes. The untouched second paragraph remains byte-identical.
- `footnotes-canonical-backlink.selection.json`: clicking return 1 then
  Shift+Right copies exactly `[^field-observation]`, proving the destination
  and atomic source selection without changing the document.
- `footnotes-canonical-keyboard-back.selection.json`: Alt+Enter from inside
  note 1, then Shift+Right, produces the same exact source reference.
- `footnotes-canonical-keyboard-forward.edit.json`: Alt+Enter at that source
  reference reaches note 1; exact autosave and complete undo pass again.

Weston MCP launched final build app 28, exposed the named return links
without duplicate buttons, and successfully invoked return 1 through AT-SPI.
`footnotes-canonical.weston.json` records the tree/action. The MCP compositor
still retains provisional startup geometry at its non-periodic refresh
setting: raw reference markers remain there and forward geometric actions
are not yet published. This is a real unresolved startup/rendering issue,
not acceptable final-layout evidence. Final pixels and keyboard/save checks
above use the periodic native compositor. App 28 was stopped normally;
its private document hash remained unchanged.

### Regression checks and failed intermediates

`cargo test --workspace --all-targets` passes: 111 core unit tests,
9 source-fidelity integration tests, 11 tree-selection integration tests,
366 view tests (2 explicitly ignored), and 40 app tests. Workspace Clippy
with `-D warnings`, formatting, diff whitespace checks and both doctest
targets pass. `python3 -m unittest performance/test_capture_layout.py`
passes all eight harness tests, including native Alt+Enter key releases.
Test and Clippy logs are retained under `layout-previews/footnotes-canonical-*`.

New source-fidelity regressions failed before the fix. They now cover exact
unchanged siblings/separators, CRLF, tabs, body-on-next-line notes, wrapped
prose, intentional hard breaks, quoted notes, reopening and exact undo/redo.
The fixture-66 native failure has its own complete-document regression.
Measured view tests cover widths 140/360/760/1200 at 100/150/200%, source
ranges, renumbering, navigation and a 180–600 px wrap sweep in 4 px steps.

Earlier artifacts remain failure evidence, not approved screenshots:
`footnotes-first-*` showed ellipsis-only note markers;
`footnotes-verified-*` stranded punctuation/reference tokens at line starts;
`footnotes-final-*` failed marker scaling at 200% and exact native save;
`footnotes-repaired-*` fixed scaling/save but exposed duplicate unnamed
accessibility buttons. The independent checker rejects these defects.
Some pixel reports contain explicitly selected subprofiles only; they do
not approve omitted profiles. The complete final matrix is
`footnotes-canonical.pixels.json`.

Crusty preparation `task_d8ebc704bfae4789`, context `ctx_74dc7209fa4a`, and
final validation `task_9dbb4671bcc7b50e` completed. Validation reports 36
existing advisory findings with no new, worsened or resolved findings.
Older indexed dark/minimap steering is superseded by the user's explicit
light-first/no-minimap direction.

## Current-build scrolling

`layout-previews/footnotes-canonical-10mib-perf.json` records the same final
binary on 10 MiB mixed Markdown, 1600×1200, 120 Hz, native continuous input:
60.017 measured seconds, 108.803 average presented fps, draw p99 8.438 ms,
presentation p99 12.714 ms. The >60 fps gate passes and source is unchanged.
Three presentation intervals were at least 25 ms (maximum 55.017 ms), with
1.285% missed refresh opportunities. This does not mean every frame meets
16.7 ms, prove 120 fps, or independently revalidate wheel-coast decay.
Our builds and other captures had finished before launch; unrelated host
tests were still running, so this is not a controlled speedup comparison.
Grammar-heavy/footnote-heavy performance remains a separate acceptance case.

## Remaining work

E09 is not complete: attachment fallback still exists for >16 KiB inline
ranges, >64 attachments and unsupported bidi placement. Large reference
counts/multi-digit gutters, footnotes inside all media/table/container
variants, complex structural editing and full keyboard/IME/theme/RTL
coverage still need validation and implementation where necessary. Reuse
falls back to block serialization for structural or unsupported changes;
this is not a universal source-preservation guarantee. Definitions lifted
from containers still need a complete mixed/structural-edit ownership audit.

The MCP non-periodic startup issue remains open. True figure floats, margin
notes, caption/credit grammar, bibliography, all page masters, anchor-page
footnotes/continuations, static export and the complete state matrix remain
part of the original goal. They are not replaced by this specimen.

The Rust skill guided source ownership and regression oracles. The
native-desktop design skill required actual rendering, zoom, source-linked
keyboard/accessibility behavior and explicit rejection of provisional pixels.
