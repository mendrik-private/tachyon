# Reference-section rhythm

September 10, 2026. Contract before implementation, grammar P03 (partial).

Adjacent compact technical sections recognized by the existing structural
classifier, with the same authored heading level and parent, use a 32px gap
when stacked. This separates examples more strongly than their 24px internal
heading/content gap without imposing a 64px chapter break. Unrelated/narrative
chapters, isolated examples, parent transitions and headings with subsections
keep existing rhythm. Wide measured peers retain their aligned row.

No type shrinking, density selector, source rewriting, new card surface or
content omission. The boundary relationship belongs to the shared adaptive
plan/geometry key. Active editing retains existing reference boundaries until
blur, provided source adjacency and heading ownership remain valid. Verify
source coverage, native narrow/wide/200%, exact copy/edit/autosave/Undo and the
workspace gate. Full reading/reference/overview density and pagination remain
open; this is one deliberate reference-flow rule, not complete P03.

## Implementation and automated evidence

`AdaptivePlan` records next-heading/previous-sibling boundaries from the same
bounded `compact_section` classifier used for technical peers. Same level and
parent are required. The boundary map participates in `PlanGeometryKey`, and
`gap_between` reads it for measured geometry as well as ordinary flow. This is
not a title-keyword heuristic or a per-frame section scan. Existing classifier
limits still apply (including 1600-byte compact examples and simple cells).

Both immediate retained geometry and background focused planning preserve the
previous boundary decision while editing; shrinking cannot suddenly introduce
a compact gap, and growing cannot remove one. Blur permits fresh classification.
No source, model, renderer typography, gutter or component-inset change.

- `technical_sibling_rhythm_preserves_chapter_separation`: real native font
  measurement at 360/480 logical px, 100/200% scaling; configuration/environment
  gaps are 32px, narrative/chapter gaps remain 64px, exact serialized source.
  Red command: `cargo test -p document-view --locked technical_sibling_rhythm -- --nocapture`;
  original failure: Configuration file 64px, expected 32px.
- `reference_rhythm_is_structural_source_bound_and_edit_stable`: source
  boundaries, H1/parent/barrier/narrative exclusions, geometry-cache identity,
  growth/shrinkage retention, and exact Undo.
- Focused `rhythm` tests: four passed, including the existing opening-rhythm
  controls. `scripts/check.sh` exited 0: format, locked pins/checks, strict
  Clippy, 705 Rust tests (two ignored), and doc tests. Logs:
  `/tmp/mineral-reference-rhythm-{red,tests,check}.log`.

## Native evidence

Fixture `79-technical-sections.md` SHA-256:
`4ff5072fca0cf1117ef312f493799c14f924b53033e86419ba1a484d87f26963`.
Baseline binary: `057395e0dc07d86b24b918cfb00eb463657208819326700f0bf8c38e98eb2118`.
Final binary: `bda186dcfccfa18db58519a7a4dcd380cbc164a505bbee4d543508e70f803978`.

Captures in `layout-previews/`:

- `reference-rhythm-narrow-before` / `reference-rhythm-narrow`: 520×1900
  light. AT-SPI heading bounds independently show Configuration file moves
  up 32px, following chapters move up 32px, and Environment file moves up 40px
  in total (its own H3 gap changes 40→32px). Every heading width/height is
  unchanged. Native screenshots confirm the compact reference rhythm, retained
  readable table/code/prose and distinct chapter breaks. The final capture has
  a selection highlight for the source-order copy check; it is not a raw
  whole-image comparison with the unselected baseline.
- `reference-rhythm-before` / `reference-rhythm-wide`: 720×1700 light paired
  control. All heading bounds identical; technical siblings retain aligned
  side-by-side placement. These are content-fit peers, not a claim of a 720px
  universal breakpoint.
- `reference-rhythm-dark-200`: 1280×1900 dark, select before zoom, settled
  200% stack. The first table/code pair and its heading gap are visible; later
  material below the viewport is not visually qualified by this capture.
- `reference-rhythm-edit` and `-idle`: 520×1900 light. Paste 1980 bytes at the
  end of `[document]`, exceeding the compact-classifier budget while editing.
  Independently constructed full-file expected source matches autosave after
  one-second idle; Undo restores every original byte. Wide code remains
  horizontally scrollable, not truncated in source. The capture exits 0; a
  `wl_display_dispatch: Broken pipe` message occurs during session teardown
  after the successful edit evidence.

Read-only sources stay unchanged. Narrow and dark-200 captures verify all six
heading clipboard markers once in source order, not whole-clipboard equality.
Logs: `/tmp/mineral-reference-rhythm-*.log`. Native debug captures do not qualify
release performance or the full grammar/RTL/IME/structural-edit matrix.

UX guidance shaped the distinction between reference siblings and chapters;
Rust guidance shaped shared geometry ownership and edit-retention tests.

Crusty preparation `ctx_7f95be4e30cf` and validation
`task_22f7e01221d92d5d` completed: 37 existing advisory findings, none new or
worsened. A07 remains active. Direct raster checks of the following Notes
heading/body band retain the same ink-row coordinates after the large paste;
use saved pixels rather than apparent displacement in scaled image previews.
