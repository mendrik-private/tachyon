# RTL caret and table geometry — September 10

This continues critical A07 after the table IME correction. It qualifies the
bounded RTL cases below; A07 and the complete audit remain active.

## Overflowing table regression

A focused GPUI regression uses Arabic and Hebrew text in a three-column table
with explicitly saved 400 px widths, rendered in a 500 px viewport at 200%.
Actual key dispatch verifies that:

- Left and Right follow shaped visual caret order within an Arabic run and
  return to the identical physical position;
- Tab and Shift+Tab follow canonical source-cell order across Arabic and Hebrew
  cells;
- each destination caret remains inside the painted table and document
  viewport;
- source bytes and all three saved widths remain unchanged.

No production correction was needed. The test consumes the same shaped lines,
selection mapping, local table scroll and clipping geometry as painting.

## Native fixture qualification

The existing fixture-48 native oracle passed on immutable layout-validation
binary SHA-256
`85706ab0eb54b50735c8666dbdf0825bdfe29cf9642ac0c45b50da45b71a018d`.
Both runs use a private Weston seat and AT-SPI bus. Screenshots were inspected.

| Artifact prefix | Scale | Result |
| --- | ---: | --- |
| `rtl-geometry-final-100` | 100% | RTL edge hit tests reverse logical offsets; Left/Right move visually; source unchanged |
| `rtl-geometry-final-200` | 200% | Same checks pass after enlarged wrapping; source unchanged |

The fixture source SHA-256 is
`d0e1f5d556b12843de35d7c1c475b6346bb6a005f517d5d701525207e18fb2f7`.
At 100%, the selected shaped line is 796.28 × 32 px. At 200%, the measured
line is 1192.32 × 64 px inside the enlarged paragraph. Both reports retain
collapsed carets and exact source.

## Verification

- Focused RTL table regression: passed.
- Native RTL oracle: passed at 100% and 200%.
- RTL oracle unit tests: 2 passed.
- The production state, including the preceding IME fix, passed
  `scripts/check.sh` with 835 Rust tests, strict Clippy and all repository gates;
  this slice adds one passing test and native artifacts only.
- `cargo fmt --all -- --check` and `git diff --check` passed.

Native RTL composition, selection spanning mixed-direction table cells,
right-aligned saved-width columns, nested/rich cells, resize during IME, export
and release performance remain open.
