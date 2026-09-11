# Authored opening composition — September 10

## Contract and baseline

The user prioritizes document appearance and useful canvas occupancy. Native
whole-README review showed a title, introductory lead and substantial overview
stacked in the leading reading column, with unused space beside them. Compose
these authored paragraphs below the full-width title using the existing row
planner; do not invent a summary, change typography or add layout controls.

Fixture118 is an exact README excerpt, including its following build section.
The before capture is `layout-previews/opening-overview-before`, 1600×1000 light.
Whole-file before captures are `layout-next-readme` and `layout-next-plan`, both
1600×1600. Before runtime:
`766a66e7bb7e332e8222cab6599a9cb6fe181ba43b41a0e93fdae1d7e8e3018a`.

## Implementation and regression

`adaptive/rows.rs` adds a measured opening candidate: actual leading H1, its
existing lead, then one to four complete same-section overview paragraphs,
ending at the next heading or EOF. A preamble, missing overview, following
list/object, specialized paragraph role, hard break, strong RTL, inline image,
math or preserved HTML prevents nomination. No source model changes.

The existing twelve-track alternatives compete using native loaded-font
measures. Both columns enforce a 40-character floor and 84-character cap.
Canvas must be at least 900 logical pixels and viewport at least 480. The
lead needs three measured lines, overview four, with height ratio at most 1.5;
existing overflow, viewport-height and measurement-budget gates also apply.
Title and next heading remain outside the columns. Lead stays 21/32 serif,
narrative overview 18/28; no cards, decorative shadows or new text.

Four Rust tests cover source-order geometry and typography, structural
negatives, measured short/uneven negatives, focus on either paragraph, width
and height recovery, three text-scale measurement environments, long typing
growth and exact Undo. GPUI's test text system is a mock, not loaded-font proof.

The new resize test exposed a real ownership conflict. After widening while
the lead remained focused, its unfocused overview could become a reading band.
On blur the opening row returned, but cached prose flow removed its overview
slot. The reproducer stayed red after removing everything except title and
two paragraphs. `editor/prose_flow.rs` now refuses an **unfocused** cached flow
when a newly measured row owns its source. Held editing still retains its flow.
The minimized and full tests pass; temporary geometry instrumentation is removed.
Logs: `/tmp/mineral-opening-minimal-red.log`, `-minimal-green.log` and
`/tmp/mineral-opening-geometry-red.log`.

## Final native evidence

Runtime: `eee6271bf25e69d28721fe059ce9c1e727ee128c1099073436ac6a71922f196e`.
All captures use isolated Weston at display scale120, not physical desktop
input. These are correctness/appearance checks, not performance qualification.
Prefixes below are under `layout-previews/`.

- `opening-verified-wide`, 1600×1000 light: 433/616px text columns, 24px gutter,
  160/168px heights. Opening occupies 1073px of 1314px (81.66%), saving 120px
  vertically. Remaining width protects the loaded-font reading measure.
- `opening-verified-regular`, 1280×1000 light: 405/576px columns, 24px gutter,
  99.90% of the available canvas. Native screenshot reviewed at original size.
- `opening-verified-narrow` (768×1200), `-short` (1600×480), and `-200`
  (1600×1800 dark, 200% text) stack in source order. Narrow/dark screenshots
  reviewed at original size. Font size is not reduced to force columns.
- `opening_layout_check.py` verifies actual ink in every line box, title and
  section spacing, gutter, occupancy, height savings, source/runtime identity
  and all seven canonical specimen nodes across every size. It fails against
  the stacked baseline used as a purported improvement.
- `opening-verified-readme`: same 120px saving in the untouched whole README;
  all 134 canonical document nodes keep IDs, parent paths, roles, names,
  descriptions and actions. `opening-verified-plan`: its single-lead opening
  stays unchanged; whole-document canonical semantics and all heading, prose,
  table and code bounds match the before capture.
- `opening-resize-reading`: actual 1040→650→1040 document-width changes,
  duplicate resize, stable heading/paragraph identities and reading anchor.
  `opening-resize-height`: actual 884→304→884 height changes at fixed1040 width,
  restored pair, identical 8px anchor offsets in all four snapshots.
- `opening-resize-editing`: native overview typing, narrower stacked fallback,
  stable focus/caret, entire expected autosaved Markdown and byte-exact Undo.
- `opening-verified-edit`: pointer placement in the right column, Home,
  typing, one-second focused idle and blur; entire expected autosave and exact
  Undo pass. `opening-verified-boundary`: Home and two Ctrl+Shift+Left actions
  copy exactly `service.\n` across the authored paragraph/column boundary.

All original source hashes remain unchanged: fixture118
`da3a88e60b2a710b5b0c77e990d9fd9b39163f66b5acd5a1655c0305575ecb05`, README
`abca9115a24f8b4293074b510011acfc5d11a036f910ba68d77e6f0054527541`, plan
`5234b880bb32ee63ce7b70c76bc40a84eef6d851bb584c9155bfdfce8f0c470b`.
Intermediate `opening-overview-after/regular/narrow/short/200` captures predate
the flow-retention fix; final claims use the verified prefixes above.

## Verification and remaining scope

`scripts/check.sh` passes formatting, workspace all-target checking, strict
Clippy, 744 workspace tests plus five adapter tests (**749**, two existing
ignored), and doc tests. Log: `/tmp/mineral-opening-final-check.log`.
Seventeen capture-harness and thirteen resize-oracle Python tests pass; the
resize oracle now includes both opening paragraphs and its authored soft breaks.

Crusty validation `task_bf8b27d4aa3cacc8` against `ctx_7d7a8adce67e` reports
75 existing advisory findings, none new, worsened or resolved. The full audit
and layout work item remain active; this checkpoint is partial progress.

This advances continuous-document composition, not paged cover masters.
Broader real-document and multi-paragraph overview specimens, RTL/IME cases,
remaining grammar/page/export families and full release performance remain
open. Retained-accessibility framework work remains staged and paused for the
user's layout priority. No GPUI runtime or dependency changes were made here.
