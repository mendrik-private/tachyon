# Nested reading measures

2026-09-09. Layout-first continuation of Audit A07. The complete hierarchy
family and broader audit remain open.

## Change

Nested paragraphs previously paid their ancestry indentation out of an already
capped reading column. Each deeper level therefore narrowed the text even when
the document canvas had unused width. The extra nesting gutter is now reserved
separately from the readable text measure. The first-level treatment is unchanged;
deeper unordered and ordered levels retain its text width when space permits.
The hard canvas constraint still wins on smaller windows.

The shared calculation feeds both row-candidate measurement and final line
geometry. Explicit card slots, tables and other wide components keep their
existing width ownership. No list levels are flattened, no ancestors or labels
are invented, and the existing marker/guide geometry remains source-backed.
There is no new layout search, document tree or persistent state.

The UX skill informed the separation of text width from hierarchy gutters;
the Rust skill informed shared measurement ownership and regression coverage.

## Verification

- A native-font regression failed on the original implementation at depth 2.
  It now checks equal readable width through eight unordered and ordered levels
  at 100/150/200%, source-complete line ranges and exact Markdown preservation.
- Logical widths 360/620/1000 retain positive text space, stay within the
  canvas, and keep widths/insets stable when focused. These are bounded depth
  checks, not proof that arbitrary nesting fits every narrow window.
- The retained typing/full-geometry test now includes both deep explanatory
  paragraphs of fixture 91, at 100/150/200%, with local shaping bounds and undo.
- `scripts/check.sh` passes formatting, locked all-target checks, strict
  Clippy, workspace tests and doctests: 449 document-view tests pass, two ignored.
  `git diff --check` is clean.

## Native evidence

Fixture: `layout-fixtures/91-nested-reading-measures.md`.
Unchanged source SHA-256:
`d21458c7670a48c3e22079dd4906175f96ffa25d620daa6c00ac39404339afda`.
Baseline runtime:
`78e8f5964e4fa6155d8b4993c1f8b4105926cd82565111c6f008ae4bbbe0972c`.
Final runtime:
`887e56cb1d1bf78e43c790c43284695371095e2d19bfaf7a5033e1b9c0870681`.
Only tests/formatting/documentation changed after that runtime build.

All artifacts below are in `layout-previews/`, use private source copies and
retain build/source/appearance/edit sidecars. Screenshots were inspected.

| Prefix | Evidence |
| --- | --- |
| `nested-measures-before`, `nested-measures-after` | Same 1600×1200 light source. Deep explanations use more of the available width; the following “Review sequence” section starts 72 px higher. Parent-child guides and marker positions remain intact. |
| `nested-measures-narrow` | 600×1100 light, scrolled; content stays inside the window and returns to the ordinary reading column afterward. |
| `nested-measures-200` | 1600×1200 dark at 200%, scrolled; hierarchy, labels and explanations remain visible and source-complete. |
| `nested-measures-edit` | Native deep bullet click/Home/type/idle, preserved neighboring text, autosave and exact whole-file undo. Eleven unique clipboard markers retain source order. |
| `nested-measures-ordered-edit` | Native fourth-level numbered-item edit, preserved neighboring instructions, stable focused idle and exact undo. |

Clipboard marker order is not exact whole-document clipboard equality. The
typing checks verify intended insertion and neighboring fragments, then exact
undo; layout-only captures prove byte-identical source. Active AT-SPI is
supporting evidence, not the full screen-reader matrix.

## Remaining layout work

This fixes width accounting, not the required deeper responsive tree
presentation. Very narrow windows with arbitrary nesting still need a designed
tree fallback. The wide preview also exposes isolated final words (for example
“document.”); native paragraph widow control is a concrete typography follow-up.
Neither issue is hidden by the green width tests.

The full keyboard/IME/RTL/structural-edit/resize/accessibility matrix, other
grammar families, static/paged export and sustained release performance gates
remain open. Crusty context `ctx_fa48ef777562`, validation
`task_8d70548e47f4b4ce`: 36 existing advisory findings, none new or worsened;
local workspace checks ran separately.
