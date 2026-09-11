# Code selection readability

September 10, 2026. Contract before implementation for work_64cc1bebafa6ce50.

Selection must preserve readable code glyphs on the actual resolved code-panel
surface, including contrasting executable panels in light appearance. Keep
syntax roles legible, selection visibly distinct, source/copy unchanged and
geometry stable. Use shared semantic palette ownership, not per-language paint
exceptions or shaping on selection changes. Exercise native light/dark,
100/200%, partial/whole/inactive selections and edit/Undo.

## Result

Contrasting executable panels now resolve selection fill from their own code
palette, just as their glyphs already did. Ordinary text selection no longer
puts pale executable syntax on the page's pale selection surface. Whole rich
table-cell selection restores the embedded code panel's selected surface above
the cell fill while keeping neighboring prose on its normal selected surface.
Shared light/dark syntax shades were also adjusted for ordinary code panels;
these shades apply to unselected code too. Prose colors, layout geometry,
document model and selection-independent text shaping are unchanged.

## Regression evidence

- `selected_code_glyphs_contrast_with_the_painted_selection`: actual GPUI
  painted selection quads and renderer-styled glyph runs, Rust/JSON/TOML/plain
  code, light/dark, 100/200%, active/inactive (32 combinations).
- `selected_rich_cell_keeps_code_and_prose_readable`: actual painted whole-cell
  selection in light appearance at 100%, sampling prose before/after an embedded
  executable panel as well as code. Exact serialized source is preserved.
- Original executable selection failed at 1.40:1; expanding the regression
  exposed ordinary light code failing at 4.28:1. Both tests pass after the fix,
  with each tested glyph run at least 4.5:1 against its painted selection fill.
  Red logs: `/tmp/mineral-code-selection-{red,expanded-red,cell-red}.log`.
  Green focused log: `/tmp/mineral-code-selection-tests.log`.

The contrast calculation uses underlying sRGB colors, not anti-aliased edge
pixels, following [W3C contrast guidance](https://www.w3.org/WAI/WCAG22/Understanding/contrast-minimum.html).
This is a bounded code-selection check, not a whole-application accessibility
claim or a test of every possible syntax category and language.

## Native verification

Fixture `108-code-selection.md` SHA-256:
`8aae486e275344cf1730e4f3a1a963d5cca1b354768573a9cc0262aec7f5fa5d`.
Final layout-validation binary SHA-256:
`7a375b868f254ca1ec3d34396d589eca31212c5fb780418fe622461bbbe2b077`.

Captures under `layout-previews/`:

- `code-selection-before.png`: visible pale-on-pale failure before the fix.
- `code-selection-light.png`, `code-selection-dark.png`: 1600×1500, 100%,
  selected executable/configuration panels and embedded rich-cell code remain
  readable. Existing peer arrangement and panel dimensions are retained.
- `code-selection-dark-200.png`: 1280×1700, stacked enlarged code panels; the
  lower mixed table is below the captured viewport, so its pixels are not
  qualified at this zoom by this screenshot.
- `code-selection-light-200.png`: 1280×1700, selection colors remain readable,
  but the native layout is **not acceptable**: peer headings overlap, prose
  clips, and narrow code/table tracks retain insufficient width at 200%.
  Unlike the dark capture, this run did not settle to a suitable stacked
  arrangement. Root cause is not established; treat zoom/font settlement and
  layout invalidation as the next priority, not as a passing layout result.
- `code-selection-light-200-settled.png`: a separate cold run with eight seconds
  allowed after zoom stacks the panels correctly. **Correction on subsequent
  inspection:** the initial visual review reported missing executable text,
  but the saved panel pixels are identical to the good dark capture. Reopening
  at original resolution and counting actual foreground pixels confirms the
  text is present. The missing-text finding is withdrawn; the first run's
  overlapping geometry remains a real failure in `work_dde44398e8ea83e5`.
- `code-selection-original.png`: fixture 105, 1600×1700 light, the original
  extended-explanation reproduction now has readable selected code and retains
  its technical peer layout.
- Those read-only captures preserve source bytes and all seven requested copy
  markers exactly once in canonical order; this is not whole-clipboard equality.
- `code-selection-pointer.png`: native mouse drag selects/copies exactly `let`
  without changing source.
- `code-selection-keyboard-edit.png` and `-idle.png`: native Home plus three
  Shift-Right presses selects/copies exactly `let`; typing replaces it with `x`.
  An independently constructed whole-file expectation matches the autosaved
  result, and Undo restores the exact original bytes after a one-second idle.

Native logs: `/tmp/mineral-code-selection-*.log`. Inactive windows and rectangular
whole-cell selection were checked by actual GPUI drawing tests, not by native
input screenshots. This is not release-performance qualification.

UX guidance shaped local surface ownership and the native selection review;
the diagnosis skill required reproduced failures before changing the palette;
Rust guidance shaped renderer-level regression tests. Context7 was unavailable,
so the pinned GPUI sources supplied the drawing-test API.

Full `scripts/check.sh` exited 0: format, dependency pins, locked workspace
checks, strict Clippy, 701 passing Rust tests (two ignored), and doc tests.
Log: `/tmp/mineral-code-selection-check.log`. Passing automated checks do not
override the failed native 200% light layout described above.

Crusty validation `task_d3e9baf7b3539c1e` for `ctx_cd54bbbd3bf6` completed:
36 existing advisory findings, none new or worsened. Code-selection work remains
active pending clean native 200% verification after the zoom/publication defect;
the broader layout goal remains active.

Follow-up: [selected zoom verification](SELECTED-ZOOM-CHECKPOINT.md) fixes the
selection-blocked reflow and verifies clean native 200% light/dark captures,
exact code copy, autosave and Undo. The code-contrast and reproduced zoom issues
are now verified; the full layout/audit scope remains open.
