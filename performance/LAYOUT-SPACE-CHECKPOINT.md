# Document-space utilization — 2026-09-09

The user reprioritized layout and available-space utilization (Crusty STR-0003).
This is a verified layout slice, not completion of A07 or the full audit.

## Change and evidence

The current renderer already uses native font measurements and reading bands;
the older A07 work note claiming only character-count wrapping was stale.
The shell nevertheless capped the entire document at 1,280 logical pixels.
Its fixed 28px side insets also consumed unnecessary compact-window space.

- The document canvas now fills its pane. Side padding follows pane width,
  from 12 to 28px; navigation is subtracted only when it occupies a real pane.
- Standalone text may expand from the preferred loaded-font 72-character
  measure to a bounded 84-character measure. This is a calibrated average,
  not an exact character limit per line. Typography sizes are unchanged.
- Sustained narrative uses two measured columns at intermediate widths and
  can use three above 1,440 document pixels if minimum reading widths fit.
  Each column needs at least four lines; breaks avoid paragraph widows.
  Shorter content can fall back to two columns, then ordinary stacked prose.
- Reading bands retain the 24px gutter and viewport-height limit. Their
  column count participates in retained geometry. Editing holds boundaries;
  narrow/short reflow still returns to one column. Source order is unchanged.
- Tables and code can negotiate the entire canvas; small tables still use
  their intrinsic width rather than stretching cells gratuitously.

## Verification

`scripts/check.sh` passes: locked dependency pins, formatting, workspace check,
warnings-as-errors Clippy, all-target workspace tests and doctests.
The view suite has 410 passing tests and two intentionally ignored native-font
tests. New regressions cover three-column source coverage, gutter placement,
minimum content, height bounds, invalid column counts, and adaptive insets.
The localized-edit/full-layout/undo comparison now runs for two and three
columns. Existing width assertions were updated to the deliberate 84-character
bound; glyph-size, gap, attribution and overflow checks remain intact.

Native captures use the debug binary SHA-256
`9c4dd7aca6b8f7c8fc5c2ac4e60f3bbdc0ede72a92e92e3422de7ee5e6e12e59`,
isolated periodic Weston, light appearance and 100% display scale. Fixture
copies are disposable. All captured source hashes are unchanged after undo.

| Capture prefix in `layout-previews/` | Window | Document width | Reading columns |
| --- | --- | --- | --- |
| `layout-space-wide` | 1920 × 1000 | 1632px | 3 |
| `layout-space-medium` | 1440 × 1000 | 1160px | 2 |
| `layout-space-narrow` | 600 × 1000 | 576px | 1 |
| `layout-space-zoomed` | 1920 × 1000, 200% text zoom | 1632px | 1 |

These represent 96–97% of the document pane, excluding navigation, not of the
whole application window. At 1920px the canvas is 27.5% wider than the old cap.
The technical-reference capture `layout-space-technical` also was inspected;
its source remains unchanged and its code panel reaches the full canvas.

`layout-space-wide.copy.json` verifies marked passages remain in source order
through native Select All/Copy (not a complete clipboard-equality claim).
`layout-space-wide.edit.json` verifies a pointer-targeted insertion inside
"while someone is writing" in the third column, autosave, preservation of an
unrelated paragraph, and undo restoring every original byte.

Run the independent captured-layout oracle and its negative tests:

```sh
python3 performance/layout_space_check.py
python3 -m unittest discover -s performance -p test_layout_space_check.py
```

The oracle checks real AT-SPI bounds, source/build identity, pane utilization,
column placement and actual glyph rows in every column. Its five tests reject
the old width cap, a missing third column, absent accessibility evidence, and
reversed single-column reading order.

## Remaining layout work

Mixed technical documents still have sparse single-column stretches: measured
pairing of suitable independent sections was addressed in the subsequent
[compact technical-section checkpoint](TECHNICAL-SPACE-CHECKPOINT.md); broader
mixed-document composition still needs visual qualification.
This slice does not qualify all breakpoints, fractional display scales, RTL,
IME, resize anchoring, minimap consistency, accessibility actions or performance.
The current app forces light appearance; dark/system-theme qualification remains
open. No prior release-performance results are inherited by this debug build.
The remaining source/save audit and A07 dependencies remain open in Crusty.
