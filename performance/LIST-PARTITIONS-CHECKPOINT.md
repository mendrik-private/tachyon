# Measured complete list rows

2026-09-09. Layout-first continuation of Audit A07. The complete document
grammar and remaining audit gates are not yet accepted.

## Implementation

The short-list planner now compares complete source-contiguous combinations of
two- and three-column rows, alongside the existing stack/uniform candidates.
Five items can use 3+2 or 2+3; seven can use 3+2+2 and its source-order variants.
Every item uses its actual row width for native shaping. The 2–12 work bound
limits enumeration; cached measurements require at most three widths per item.
Existing line-count, global height-balance, overflow and semantic exclusions
remain in force. Uniform partial rows are still legal when wider rows do not
fit; this change does not force three columns into a narrow document.

`LayoutSlot` now has explicit row identity. Painting, component/outline geometry
and retained local editing no longer infer a row from source index divided by
one global column count. The chosen row capacities are retained with the
measured decision and appear in diagnostics. Focused text edits preserve those
capacities and track widths; changes in item count leave the focused grid.
Hysteresis compares the complete partition, not merely its maximum columns.

The UX skill's shared anchors and source-order grouping guidance shaped the
result: fill a short final row across the page without inventing emphasis,
reordering by height, reducing type, or wrapping every item in a card.

## Verification

- Candidate tests cover complete partitions for counts 2–12, bounded native
  measurement calls, exact placement, width-dependent rejection, narrow escape
  and stable full partitions across nearby widths.
- Native-font arrangement tests cover plain/labeled counts 1–13 at wide/narrow
  widths, row-major ordering, nonoverlap, canvas bounds and all source text.
- The negative matrix for long, uneven, dependent, nested and task lists passes.
- Retained row growth/full-geometry comparison now includes final items in
  fixture 87 at 100/150/200%, bounded local shaping and exact undo.
- Shared-geometry regression tests caught an incorrect figure continuation row
  during implementation. The full-width continuation now retains its distinct
  row; figure wrapping and caption/credit geometry tests pass again.
- `scripts/check.sh` passes formatting, locked workspace checks, strict Clippy,
  all-target tests and doctests (438 document-view tests pass; two ignored).

## Native evidence

Fixture `87-balanced-list-rows.md` was kept byte-identical before/after:
`fad9a291e82c3b254f48d20f8b0ab2d6d7134773e38b44b63f9c83456b9210f9`.
Baseline binary: `b1c707f2774ffa06edc550690b88cc97d5838550f44346d34f14020adcf01a2c`.
Final runtime binary: `a22b318ababbb2f4d7379609145d13be6c78dcf5c4c5e23689e625f21695950a`.
Artifacts below are in `layout-previews/`, with private source copies and
build/source/edit/appearance sidecars. Final screenshots were inspected.

| Prefix | Evidence |
| --- | --- |
| `list-partitions-before` | 1600×1200 light baseline: five items leave an empty third track; seven items stack. |
| `list-partitions-final` | Same source/environment: five choose 3+2, seven choose 2+2+3; following section moves up 96 px. Native final-item typing, focused idle, autosave and exact undo. |
| `list-partitions-pair-final` | Native editing in the widened final pair, stable geometry and exact undo; all twelve independent items plus the following instruction copy once in source order. |
| `list-partitions-narrow-final` | 600×1100 light: two-column candidates remain readable; no illegal three-column partition. |
| `list-partitions-200-final` | 1600×1200 dark at 200%, scrolled: readable two-column list and following vertical instructions. |
| `list-partitions-figure-holdout` | Final build, fixture 69, 1600×1200 light: figure, caption/credit lane and full-width prose continuation. |

Native edits assert intended insertion and protected neighboring fragments;
they do not compare the entire edited file. Undo is byte-exact. Clipboard
evidence asserts every supplied marker in source order, not exact full rich
clipboard equivalence. Active AT-SPI trees are supporting evidence, not a full
screen-reader interaction test. Intermediate `after`/`edit` captures predate
the final measurement-cache iteration cleanup and are not final build evidence.

Crusty context `ctx_4817385e67c0`, validation `task_1167f9e7f2b99ce0`: 36 existing
advisory findings, no new or worsened findings. Checks were run locally.

Remaining: four-column short-content variants, inline enumerations, additional
list families, full RTL/keyboard/structural-edit/resize and accessibility
matrices, paged/static export, and release performance qualification. The
overall grammar/audit goal remains active.
