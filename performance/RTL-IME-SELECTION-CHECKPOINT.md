# RTL IME and spanning selection geometry — September 10

This continues critical A07's shared input-geometry acceptance. It qualifies
the editor/platform composition interface with RTL preedit and fixes selection
collapse across an overflowing mixed-direction table. A07 remains active.

## RTL composition

A GPUI platform-input regression places the caret after Arabic text in the
trailing cell of a three-column table with explicitly saved 400 px widths. At
200% zoom in a 500 × 220 px viewport it supplies a long Arabic/Hebrew preedit,
including a selected UTF-16 range inside the composition.

The regression verifies that the canonical RTL preedit caret and candidate
window bounds come from the same painted line and clipped table viewport. It
also verifies cancellation with no undo entry, commit as one edit, exact Undo,
and unchanged `[400, 400, 400]` widths.

## Mixed-direction spanning selection

A second regression selects canonical text from a cell containing Latin,
Hebrew, numerals and Arabic through a Hebrew/English middle cell to an
Arabic/English trailing cell. It checks the complete logical range and source
order before collapsing the selection with actual Left and Right actions.

Before the correction, Left collapsed to the correct leading source offset but
left the table scrolled to the trailing cell. The new caret was absent from the
painted lines. Horizontal move and extend actions now send their resulting
canonical caret through the existing measured reveal path. Both selection
edges are painted inside their table viewport, and source plus saved widths
remain exact.

No bidi-specific layout, selection model or alternate hit geometry was added.

## Verification

- The spanning-selection regression failed before horizontal actions revealed
  their resulting caret and passes after the correction.
- RTL table filter: 2 passed.
- Directional-collapse filter: 1 passed.
- `scripts/check.sh`: formatting, locked checks, strict Clippy, 839 Rust tests,
  adapter tests and doctests passed; 2 existing native-font tests were ignored.
  Log: `/tmp/tachyon-rtl-ime-selection-check.log`.
- `git diff --check` passed.

Native compositor/system-IME injection, cross-process candidate-popup
placement, aligned/nested/rich table variants, export and release-performance
qualification remain open.
