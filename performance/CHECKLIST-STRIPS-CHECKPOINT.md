# Compact authored checklists

2026-09-09. Layout-first continuation of Audit A07. The complete grammar and
audit acceptance remain open.

## Change

Short, flat task lists now nominate measured wrapping rows of two through four
columns. Checkbox states remain canonical task data, not decorative marks.
The original task order, editable paragraphs and source ranges are unchanged.
One completion summary spans the strip; its 48 px header is reserved across
the whole first row. Subsequent rows use the shared 24 px gutter. Ordinary
bullets do not acquire checkboxes and task strips do not acquire card borders.

This implements the compact checklist strip/panel family in the approved
layout plan using existing twelve-track slots and native controls. The UX
skill informed grouping, shared text/control anchors and readable fallback;
the Rust skill informed ownership, measurement bounds and retained-edit tests.

Candidate search covers 2–12 tasks. Each must be one paragraph with an explicit
task state. Cold nomination caps individual text at 4096 bytes and requires one
actual native line in every selected cell, including its checkbox gutter and
trailing inset. Rich supporting blocks, nested tasks, mixed task/plain lists,
strong RTL, media/math/HTML and long explanations do not enter this family.
Text is never shrunk to achieve fit. The existing bounded candidate search
measures at at most four widths per item.

Focused geometry remains stable during typing and while widening the window.
Returning to a width that still fits uses the published strip width, not the
intermediate window width. Growth can exceed the cold nomination bound while
focused (up to the existing 64 KiB local-node guard); cold layout then returns
the longer task to ordinary flow. Structural changes invalidate incompatible
strip shapes. Candidate diagnostics do not independently invalidate geometry;
the resulting slots and canonical completion counts do.

## Automated evidence

- Native-font tests cover counts 2–12 at 100/150/200%, source order, task states,
  row baselines, exact summary reservation, row gaps, focus/widen/restore,
  narrow fallback and diagnostic-vs-geometry cache ownership.
- Negative cases cover nested, mixed, supporting paragraphs, inline media,
  RTL, long tasks and 13-task complexity bounds.
- Focused task growth past 4096 bytes retains published slots; a fresh plan
  declines the strip for that same content.
- Existing retained typing/full-geometry comparison now includes the first
  and last tasks of fixture 90 at 100/150/200%, local shaping bounds and undo.
- `scripts/check.sh` passes locked all-target workspace checks, formatting,
  strict Clippy, tests and doctests: 448 document-view tests pass, two ignored.
  `git diff --check` is clean.

## Native evidence

Fixture `layout-fixtures/90-checklist-strips.md`, unchanged SHA-256:
`efe1d3e83f6ca653c0fc58fab51f6cd77d7ddc809f2b2ba5809f1b2dc4742fd0`.
All captures use private source copies and native Wayland input.

| Prefix in `layout-previews/` | Evidence |
| --- | --- |
| `checklist-strips-before` | 1600×1200 light baseline; all six short tasks stack. |
| `checklist-strips-after` | Same source, measured 4+2 rows. Detailed work starts 112 px higher without smaller text. Rich/nested work remains vertical. |
| `checklist-strips-narrow` | 600×1100 light; short tasks use two columns, rich tasks remain vertical. |
| `checklist-strips-200` | 1600×1200 dark at 200%; two-column short tasks with readable labels and real state. |
| `checklist-strips-small` | 400×1100 light; complete single-column fallback with no task-text clipping. |
| `checklist-strips-final-toggle` | Native last-task checkbox click changes exactly its single source marker, completion becomes 3 of 6, and one undo restores every original byte. |
| `checklist-strips-qualified` | Final-runtime last-task click/Home/type/idle, preserved neighbors, autosave and byte-exact undo. Eight unique clipboard markers preserve source order. |

Baseline runtime: `90791b78f42fc4252bd09708d4af90864c115f2a2bfbce9e3c28f844cca511dc`.
Initial wide/narrow/200% runtime: `59987f6df2ff2938a6866168a1e3848a9014f64a6b30333889400c3a6314a2d6`.
Small-window and final-toggle runtime: `bdbbb898a32ca3d71c1ff76126a8c7201ea1fdeca8ed0295b230f487eabd3996`.
Final runtime, including published-width focus retention:
`78e8f5964e4fa6155d8b4993c1f8b4105926cd82565111c6f008ae4bbbe0972c`.
Build hashes and source/edit/appearance details are retained in sidecars.
Screenshots, including the changed checkbox and typed/idle states, were inspected.

Clipboard marker order is not exact whole-document clipboard equality. Native
typing checks intended insertion and protected neighbors, then exact undo;
the toggle check proves exact edited source. Active AT-SPI supports these
checks but does not close the screen-reader interaction matrix.

## Remaining scope

This is a track-aligned checklist strip, not arbitrary intrinsic-width packing
of every task into a single row. Nested/long tasks retain their authored tree;
deep tree presentation and full keyboard/IME/RTL/structural-edit/resize/
accessibility coverage remain open. Other grammar families, static/paged
export and sustained release performance gates remain open as well.

Crusty context `ctx_a6f7940745b3`; final source validation
`task_a3e687b8a548f8ca`: 36 existing advisory findings, none new or worsened.
Local workspace checks ran separately.
