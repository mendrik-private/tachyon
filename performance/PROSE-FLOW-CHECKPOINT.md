# Bounded prose columns — grammar checkpoint

The complete document grammar remains the goal. This checkpoint adds the E06
flow primitive; it does not approve the full editorial board or pagination.

## Implemented behavior

- Sustained, top-level narrative prose can flow through paired reading
  columns, including splitting one canonical paragraph across columns. This
  is not a grid of independent paragraphs or a second editable document.
- Native loaded-font measurements select source-byte boundaries. Each
  column retains at least the calibrated 40-character minimum measure. The
  bounded search balances heights, prefers paragraph boundaries, and keeps
  at least three paragraph lines on either side of a split. Columns contain
  at least four lines. Bands are limited to 65% of usable viewport height,
  capped at 560 logical px, before editing.
- The shared 24 px gutter separates columns and successive bands. Narrative
  leading is 28 px; ordinary paragraph gaps are 24 px and section gaps 64 px.
  Prose stays open on paper without cards, borders or shadows.
- Below 900 logical px usable width or 480 px usable height, text remains
  single-column. Zoom reduces usable width rather than reducing font size.
- Planning stays inside measured chapter windows and bounded source groups
  (16 roots / 64 KiB). Lists, code, tables, figures, raw HTML, math, hard-break
  paragraphs, strong RTL and non-narrative contexts do not enter this flow.
  These exclusions do not implement those families' missing layouts.
- The source-linked flow is part of published-geometry identity. Unchanged
  plans reuse native measurements. Scrolling consumes prepared geometry;
  candidate selection and shaping are not added to the scroll path.
- A focused flow retains widths and source anchors while text grows. Anchors
  advance after each edit, preventing two distant changes from being mistaken
  for one replacement of the text between them. Dirty held flows rebalance
  after editing focus leaves. Narrow/short resizing overrides a held flow.
  Source revisions also invalidate formatting-only changes: identical plain
  text is not evidence that bold/italic runs still have the same geometry.
- Localized typing rebuilds the complete affected flow, shifts following
  source/paint geometry, and leaves unrelated lines intact. A structural
  heading insertion invalidates a flow spanning that boundary.
- Down/Up follow source order inside a reading flow, even when the next
  column starts higher on screen. Horizontal caret affinity follows the next
  line at a soft wrap; column transitions retain the local x position.

## Automated checks

`cargo test -p document-view prose --locked --quiet` passes 13 matching tests
on the latest checked worktree, including eight new flow-specific tests.
Coverage includes real paragraph continuation, complete projected bytes,
24 px gutters/band spacing, measured height limits, warm measurement reuse,
Unicode and consecutive-edit anchor rebasing, localized/full-layout
agreement, focus release, exact undo, structural barriers and short/narrow
fallbacks, and formatting-only invalidation. The DP test has positive feasible cases so rejecting every
candidate cannot produce a vacuous pass.

Workspace tests passed before concurrent table/card regression work appeared.
Subsequent full runs were deliberately not described as green: the latest
completed full run has 358 passing, two failing and two ignored document-view
tests. Its failures are `measured_plan_md_entities_fit_a_bounded_card_row`
and `table_controls_have_only_three_pixel_knobs` in the
concurrently changing card/table surface. Those changes are preserved, not
reverted or weakened here. Earlier transient compile errors and four newly
added regression failures were superseded by that latest full run.

The flow implementation passed workspace Clippy before those concurrent edits.
Workspace Clippy was repeated after the final revision-aware cache change and
passed; both workspace doctest targets also passed (zero doctests).
The later workspace formatting check reported only the concurrently modified
table/card files; the two new prose modules were formatted directly, without
rewriting those unrelated edits. `git diff --check` passes.
Crusty validation `task_3670035bfb57a2f3`, context `ctx_2c1b888d775c`, completed:
36 existing advisory findings, no new or worsened findings.

## Native evidence

Specimen: `layout-fixtures/65-reading-bands.md`, SHA-256
`23441aeac5ea36cedc08e13c8d220d90abd5af33d31fb815f917fc14f8899b84`.
Release build used by all captures listed below:
`45f67c34f92e11bdfbe8385a8b8b48a08f6dfe0798593e77ff5875ed987bd8ce`.
Later concurrent builds must not silently inherit these results.

All four `layout-previews/prose-flow-verified-{wide,narrow,short,200}.png`
captures were inspected against boards 05/06 and the written grammar. They
use 1600×1200, 600×1100, 1600×400 and 1600×1200 at 200%, respectively, on
isolated periodic native Weston outputs. Every source hash is unchanged.

`python3 performance/prose_flow_check.py` passes independent native
AT-SPI/pixel checks. Wide semantic bounds prove 24 px gutters and 64 px
section separation. Pixel checks require real glyphs in 28 px line boxes
and paper in the 24 px paragraph/band gaps; disconnected dots and accents
are not mistaken for extra lines. Narrow, short and 200% accessible bounds
prove stacked paragraph order. The checker verifies matching binary and
fixture hashes for all four views.

- `prose-flow-verified-wide.copy.json`: native Select All/Copy retains the
  marked passage order across both bands; this is not an exact full-clipboard
  oracle. Core line-coverage tests independently account for every byte.
- `prose-flow-selection.selection.json`: native Home, Shift+Down from the
  last left-column line copies exactly the authored line through the next
  column's first byte, including the trailing space. Source stays unchanged.
- `prose-flow-keyboard-edit.edit.json`: Home, Down crosses from the last
  left-column line to the first right-column byte. Typing saves the complete
  expected Markdown and undo restores the original bytes. The unrelated
  review paragraph remains byte-exact. Edited plain punctuation still uses
  the existing canonical Markdown escaping policy.
- `prose-flow-growth.edit.json`: a 224-byte native paste at that boundary
  saves the exact expected document and undoes exactly. Inspected `-idle.png`
  retains the first right-column anchor while the band grows; `-blurred.png`
  visibly rebalances after focus moves, retaining the full passage.

Weston MCP launched the same build as app 25, captured screenshots, read the
accessibility tree and clicked into the document. It again retained provisional
single-column geometry, including after the click. It is not final-layout
evidence. App 25 was stopped; its private fixture stayed byte-identical.
The final layout evidence above uses the periodic native compositor instead.

### Final revision-aware rebuild

Build `c57128a6b5f9b7f2e53fc935c895f6b15eca3d7d530cab9996ba1852be8d25d0`
includes the formatting-only cache fix. All four views were captured again as
`prose-flow-final-{wide,narrow,short,200}.png`, with source-unchanged and AT-SPI
sidecars. The final wide view was visually inspected; the independent checker
(`python3 performance/prose_flow_check.py --prefix prose-flow-final`) passes
the same spacing and fallback assertions across this hash-matched matrix.
`prose-flow-final-up.selection.json` adds exact native Up/Shift+End selection
from the right column back to the final left-column line.
`prose-flow-final-edit.edit.json` repeats Down, exact autosave and exact undo
on the rebuilt binary. The full workspace still has the two explicitly listed
concurrent card/table regression failures; these native captures are not a
claim of full workspace or full grammar approval.

## Remaining work

E06 is partial, not a full family sign-off: multi-script/RTL column adaptation,
large-flow stress, arbitrary structural editing, complete accessibility text
geometry and the full width/state/interaction matrix still need coverage.
The grammar's true figure wrapping, margin notes, footnotes/bibliography,
media families, all finite-page masters and export remain separate open work.
No existing source or performance limit has been redefined as completion.

## Scrolling measurement

`layout-previews/prose-flow-10mib-perf.json` records the captured `45f67c34`
build at 1600×1200, 120 Hz, 10 MiB mixed Markdown, native continuous input,
60.017 measured seconds: 108.536 average presented fps; draw p99 8.806 ms;
presentation p99 12.476 ms. The >60 fps gate passes. Three presentation
intervals were at least 25 ms (maximum 43.024 ms), with 1.078% missed refresh
opportunities and no measured application stalls at least 25 ms. This is not
a claim that every frame is below 16.7 ms or an improvement over earlier runs.
The source hash remained unchanged. No compilation or other capture was live
at launch; later host activity was not controlled. The formatting-only cache
hardening was added after this scrolling capture; its rebuilt visual/editing
evidence is recorded above. A post-hardening performance run remains to be
recorded. Another concurrent release build replaced the shared target binary
after the final captures; evidence remains tied to the recorded hashes.

The Rust skill shaped the source/geometry/cache invariants and regression
checks. The native-desktop design skill required real rendering and input
evidence, including explicitly rejecting stale MCP geometry as final proof.
