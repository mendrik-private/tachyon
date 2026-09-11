# Height-aware layout recovery — 2026-09-09

Continues STR-0003 and [width-resize recovery](RESIZE-SPACE-CHECKPOINT.md).
This is a bounded A07 layout fix, not completion of the audit.

## Reproduced defects

Native height-only resizing of fixture 79 exposed two failures:

- Reading: shortening stacked the technical sections, but restoring height did
  not recover their shared row. A trace also showed a discarded tall-viewport
  request with no final tall commit.
- Editing: the editing lock retained a technical row that no longer fit the
  short viewport.

Pre-fix reports remain in `layout-previews/height-before-{reading,editing}`.
The measured planner regression demonstrated both failures before their fixes.

## Changes

The recovery exception now permits measured technical and matched three-card
rows to compete after gaining viewport height as well as canvas width. Existing
measurement, overflow, hierarchy and fit checks remain authoritative.

Editing locks release a non-fitting multi-column row when the viewport shrinks;
ordinary typing at unchanged height still retains its arrangement. Unknown
heights do not establish fit. Source-order stacking remains the fallback.

The paint-publication boundary now records the actual scroll viewport height
and schedules reflow when it changes. It does not confuse document bounds with
viewport bounds or wait for a subsequent input event.

Validation-only Shift-F9/F10 commands resize height while preserving width.
The native oracle's `--resize-axis height` mode requires an unchanged width,
changed height, stable identities, row/stack recovery and preserved source.

## Verification

- `scripts/check.sh`: passes, including formatting, locked checks, Clippy,
  workspace tests and doc tests. Document-view: 414 passed, two intentionally
  ignored native-font tests.
- Eight resize-oracle tests and eleven capture-harness tests pass.
- `height_only_resize_commits_current_viewport_without_input` checks repeated
  height changes, current committed viewport, unchanged selection/source and
  settled reflow. The measured-row test covers short/tall recovery and releasing
  a non-fitting editing lock.
- `git diff --check` passes.

Native debug `layout-validation` binary SHA-256:
`a10f3372ad16d331c96bb54edbaadbef9c2a01c622d2c8610bb8b5d424d9aad3`.
The private Weston/AT-SPI sessions use 100% display/text scale and a
1920 × 1200 output. Editor width stays 1040px; viewport height changes
884 → 304 → 884px.

`height-fixed-reading.resize.json` passes paired → stacked → paired, duplicate
short resize, stable heading identities and unchanged source. The observed
heading displacement is zero physical pixels. The heading is near the document
start, so this is not qualification of arbitrary deep prose anchors.

`height-fixed-editing.resize.json` passes native insertion, short-viewport
stacking, stable selection/focus, saved-source preservation and exact-byte undo.
Caret geometry comes from validation-only editor state, not an AT-SPI Text
interface. The caret moves horizontally with its column becoming a stack; its
vertical position remains within one pixel.

Restored-height screenshot inspected: technical sections again share the row
with readable table/code chrome. Desktop-shell window placement can leave the
expanded window partly off-output. These captures therefore establish layout
recovery through local AT-SPI bounds and traces, not full window-clipping or
placement qualification. Stage filenames retain the oracle's historical
`wide`/`narrow` labels; `axis: height` in the report identifies this sequence.

```sh
python3 performance/capture-layout.py --fixture 79-technical-sections.md \
  --binary target/debug/tachyon --width 1920 --height 1200 \
  --zoom-steps 0 --resize-check reading --resize-axis height \
  --layout-trace details \
  --output performance/layout-previews/height-fixed-reading.png
```

## Still open

Continuous resize/scroll, arbitrary prose anchors, fractional display scales,
minimap agreement, broader mixed-document composition and the audit's unresolved
source/save dependencies remain open. System-theme progress is separately
recorded in [the appearance checkpoint](SYSTEM-APPEARANCE-CHECKPOINT.md).
No release-performance claim follows from this debug build.
