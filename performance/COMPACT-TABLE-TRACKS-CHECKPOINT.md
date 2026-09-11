# Compact reference tracks — September 10

## Layout contract and change

Continue the layout-first audit. The previous specification/example turn made
verified progress; this turn addresses the remaining empty track beside a
compact property table. In fixture03's 1600×1600 native baseline, Configuration
occupied a 422px track but its table needed only 271px. The adjacent comparison
received 867px and wrapped two otherwise compact data rows.

The existing twelve-track row planner now considers 3:9 and 9:3 alongside its
existing patterns. These two additional patterns require two already-related,
complete table units: a table alone or a heading immediately followed by one
table. Neither prose, code, figures nor compound sections acquire a quarter-width
rail. Native measurement must fit the narrow unit's complete preferred table
and heading width. The existing 260px floor, overflow, height, balance, scoring,
source-order validation and focused-edit locks remain in force.

This is a bounded candidate extension, not a manual layout mode or a general
arbitrary-width solver. Tables retain their intrinsic widths, existing column
allocation and semantics. No font, theme, dependency, source rewrite or renderer
change. The app-UX skill guided shared alignment and content-fit adaptation;
the Rust skill guided the red/green test and existing ownership boundaries.

## Native result

Baseline: `layout-previews/specification-verified-wide`, binary
`26416b6a68d3360ccea181ea18e76ffbb2de7dac67d1ae038e6600715ddc81fa`.
Current: `layout-previews/compact-track-wide`, binary
`565b0ecc9667d6fd932ab5ee749c21815762303bd27a58d8e085f66f8887c094`.
Unchanged fixture03 SHA-256:
`075a04766b464776cb052a4c077be0e0817cd5b9d2de2520eb0d307777c36205`.

- Configuration's semantic table rectangle stays exactly 271×185px, at the
  same position. Its track falls from 422 to 310px; unused inner track space
  falls from 151 to 39px, followed by the ordinary 24px inter-track gutter.
- Comparison gains 111px of track width and 112px of integer-rounded semantic
  table width (867→979px). Its height drops from 228 to 186px. The following
  heading moves up **42px**, without reducing text size or hiding content.
- The pair spans 99.92% of the 1314px document canvas. Heading tops and actual
  painted table rules align. Earlier title, authentication, parameter table and
  request geometry are unchanged in the wide capture.
- `compact-track-regular` (1280×1600), `-narrow` (768×1600), `-200`
  (1600×1800 dark, 200% text) preserve all prior heading/prose/table/code bounds
  and all 157 canonical document nodes. The complete README capture `-readme`
  likewise preserves the preceding whole-document semantics and bounds.
- `compact-track-edit`: native pointer placement in `light`, Home, typing `x`,
  one second of focused idle, blur, whole-file autosave golden and exact Undo
  all pass. Expected edit is precisely `light`→`xlight` in the appearance cell;
  first changed byte1076. Private fixture copies only.

All captures use the isolated Weston seat. Original-size wide, narrow,
dark200 and short-table screenshots were inspected. The dark200 opening does
not show the offscreen configuration/comparison tables; their unchanged bounds
and full semantics are checked, not represented as a visual inspection of them.

## Short-window discrepancy checked, not suppressed

The initial verifier incorrectly expected every 480px-high geometry to remain
identical. Both ordinary and traced captures showed the new pair instead of the
baseline stack. Following the diagnosing-bugs loop, hypotheses were stale
offscreen bounds, retained startup height, and a scoring interaction. Committed
native traces showed the correct 406px document viewport, real measurements,
and no rejected/estimated paired rows. Scroll-to-section evidence retained
the same geometry, excluding stale offscreen/accessibility bounds.

The new comparison row is 264.5px high, within the unchanged 284.2px height gate.
Its shorter geometry makes the paired solution viable and changes the bounded
whole-window optimum. The existing parameter/request row also fits (280.5px).
No height guard was relaxed and no runtime fix was needed for this discrepancy.
`compact-track-short-tables` scrolls nine steps and visibly shows both complete
tables and headings in the 480px window. `compact-track-short-stack` uses a
400px window and correctly stacks again. Unscrolled `-short` and `-short-traced`
are retained as diagnostic evidence, not falsely claimed unchanged layouts.

## Verification and remaining work

- Fixture-backed regression failed before implementation with spans4:8, then
  passed with3:9. A second test covers reversed9:3 order, long-value and long-title
  rejection, accompanying-prose rejection, width/height fallback and recovery,
  large focused cell growth with stable slots, source fidelity and exact Undo.
  GPUI's mock shaper is topology evidence, not loaded-font validation.
- `scripts/check.sh` passes formatting, locked workspace/all-target checking,
  strict Clippy, **754 Rust tests**, two existing ignored tests and doctests.
  Log: `/tmp/mineral-compact-track-check.log`. `git diff --check` passes.
- `compact_table_track_check.py` verifies full canonical semantics, unchanged
  compact-table bounds, width/height gain, aligned pixel rules, canvas coverage,
  unchanged regular/narrow/dark200/README geometry, and short-height fit/stack
  predicates. Baseline-as-after correctly fails; the original failed short
  equality check was replaced by the actual fit contract, not silently omitted.

Full rail/master selection, arbitrary measured widths, continuous native
resize/edit bursts for this new pair, broader mixed-document coverage and
release performance remain open. The full audit goal is not complete.

Crusty validation `task_c29a0d1733ebd9fa` for `ctx_8dd9a626da55` completed:
75 existing advisory findings, zero new, worsened or resolved. A07 remains
active with this evidence appended; no full grammar family is signed off.
