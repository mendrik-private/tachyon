# Authored table widths match painted geometry — 2026-09-09

Continues A07 and the user's document-layout priority. This closes the specific
audit instruction not to silently scale a specified small column to fill the
table; it does not close every table or geometry requirement.

## Reproduction and cause

The native lower-section capture of unchanged `plan.md` showed an oversized
property table despite its saved first-column width of about 237px. A minimal
regression drove the real measurement/placement path: with authored widths
100/300 and a 1040px canvas, the first column measured 100px but painted 260px.
At a small canvas, the defect disappeared because no enlargement was applied.

The diagnosis distinguished final placement stretching, font measurement
changing the width, and a later transform rescaling the table. Resolved widths
remained 100/300 before placement. `position_flow` then multiplied them by
`max(available / total, 1)` without rewrapping text. Removing that second scaling
step makes painting, cell extents and pointer geometry consume the same resolved
logical widths used for wrapping. Intrinsic automatic fitting remains upstream;
fixed-width overflow remains local rather than shrinking text.

The old stable-width test asserted only 1:3 column proportions. Those proportions
passed even when both columns were incorrectly stretched. It now checks actual
100/300px widths. The new regression checks fully specified, partly specified
and automatic columns at 240/400/1040px canvas widths, both top-level and inside
a quote, at 100/200% text scale. It checks cell width, text inner width and
preceding-column/ancestor offsets, with nonempty table geometry and exact source.

The UX and diagnosis skills informed the shared-geometry invariant and the
red-before/green-after test, rather than compensating with a different font size.

## Native evidence

Final layout-validation binary SHA-256:
`ea6592851a08a9ef42841cfc4e5159fd3d679c487ffbe2190ac15552a40cfb91`.
Only regression-test expansion followed this build, not production changes.
Fixture `82-authored-table-widths.md` SHA-256:
`6108ef32c7358cf0a1e7c1d8714b2da3b5c78d4a3a2cf2f7cf4eedc1225032b4`.

Private Weston/D-Bus/AT-SPI captures in `layout-previews/`:

| Prefix | Evidence |
| --- | --- |
| `table-widths-before` | Old binary `9ad0dde3…`, 1440×1100; saved 160/320 columns exposed as roughly 189/378px in the peer row |
| `table-widths-after` | Final binary, same window; first fixed column is 160px and second about 320px (integer accessibility rounding); automatic peer table remains fitted |
| `table-widths-narrow` | 600×1100, light, 100% text; source-order stacked tables and retained fixed widths |
| `table-widths-200` | 1440×1100, dark, 200% text; fixed columns scale with text to 320/640px |
| `holdout-plan-dense` / `holdout-plan-dense-after` | Same native scroll through unchanged real plan; saved-width property table no longer stretches to the entire canvas |
| `table-widths-fixed-edit` | Click in the corrected second column, Home, native typing; complete autosaved file equals the expected one-character edit, then Undo restores exact original bytes |
| `table-widths-partial-edit` | Native edit in the partly specified table, preserved width metadata and byte-exact undo |

Final wide/narrow/200% and real-plan screenshots were inspected. Their source
and appearance checks pass. The fixed-column edit oracle compares the entire
edited Markdown, not just a marker; its sibling automatic table stays unchanged.
The real plan's SHA remains
`0e6d5a63b803ce3001c558e8747a46b2ffd376ee8a829a473141810deff47079`.

`scripts/check.sh` passes: locked dependency checks, formatting, workspace check,
Clippy, all-target tests and doc tests. Document-view: 423 passed, two ignored.
`git diff --check` passes. No debug instrumentation or throwaway test remains.

## Boundaries

This is fixed/resolved-width agreement, not a redesigned automatic-column
allocator or a full table-size/drag qualification. Rich embedded-image sizing
still needs its own source-to-component reproduction: a preliminary pipe-cell
image probe did not produce the standalone image node that its hypothesis
required, so it was removed rather than reported as a reproduced defect or pass.
No image-sizing behavior was changed in this slice.

Complete rich-cell/IME/RTL/resize/drag and release-performance matrices, deeper
hierarchy layout, all remaining grammar families and source/save audit work
remain open. The theme-policy question and current minimap exclusion are
unchanged. The full audit remains active.
