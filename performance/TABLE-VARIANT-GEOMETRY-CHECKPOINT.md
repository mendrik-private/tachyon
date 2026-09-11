# Aligned, nested and rich-table geometry — September 10

This continues critical A07's shared table-geometry acceptance. It audits the
current aligned, nested and rich-cell paths after the horizontal caret-reveal
correction. No additional production correction was required.

## Current geometry paths

- Right-aligned table links derive their hit region from the shaped line's
  aligned text origin and remain clipped to the same cell/table content mask.
- Automatically sized near-reading-width tables expand to the shared reading
  edge without squeezing measured minimum widths.
- Nested table edits rebuild only the affected measured row and match a fresh
  complete geometry build across ordinary, quoted, list, callout and nested
  quote/list cell content.
- The first edit of imported rich HTML cell text locks the focused table's
  measured columns through conversion and focused reflow; exact Undo restores
  the authored HTML source.
- Rich-cell prose, code panels and images retain their table insets, fitted
  inner width, zoom and intrinsic-size constraints.

Explicit saved widths remain on the separate authored-width path and are not
expanded to the reading edge. The prior column-history and IME/RTL regressions
continue to assert exact `[400, 400, 400]` metadata. Automatic reading-edge
alignment applies only to tables without saved widths.

## Verification

- Nested table boundary typing/full-geometry comparison: 1 passed across its
  full marker matrix.
- First rich HTML cell edit/focused-column geometry: 1 passed.
- Aligned table link hit/clipping geometry: 1 passed.
- Automatic near-width reading-edge/minimum-width matrix: 1 passed.
- Rich-cell geometry filter: 4 passed.
- The immediately preceding `scripts/check.sh` run passed 839 Rust tests, strict
  Clippy, formatting, adapter suites and doctests with 2 existing native-font
  tests ignored. Log: `/tmp/tachyon-rtl-ime-selection-check.log`.

Native compositor/system-IME injection, candidate-popup placement, static or
paged export and controlled release-performance qualification remain open.
