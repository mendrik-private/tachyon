# Rapid resize convergence — 2026-09-09

Continues STR-0003/A07 after [height-resize recovery](HEIGHT-RESIZE-CHECKPOINT.md).
The previous turn fixed production behavior. This turn adds qualification and
regression coverage; no additional renderer fix was needed for these cases.

## Contract and implementation

Rapid size requests must settle at the final viewport, not leave old wrapping or
a stale column arrangement. Source order, reading position, selection and saved
text must survive. The complete audit remains active.

`capture-layout.py --resize-burst` sends nine alternating native F9/F10 requests
without settling between them, ending at the intended target. Height mode uses
Shift with these validation-only commands. Reports retain each request's elapsed
time. Width bursts take roughly 10–20ms and height bursts roughly 30ms in these
debug runs; these are command-submission durations, not input-to-present latency.
The compositor may coalesce intermediate requests.

The existing geometry/source oracle still checks row → stack → row, duplicate
resize stability and canonical heading identities. The new trace oracle also
requires the latest committed layout to match the actual AT-SPI viewport in
both dimensions after text zoom. It rejects missing, stale or failed committed
plans; an uncommitted discarded result does not supersede a valid publication.

A deterministic GPUI test, `resize_burst_commits_only_current_viewport_after_in_flight_work`,
keeps the executor undrained across successive width/height paints and asserts
work is in flight. Draining must converge at the last width and actual viewport
height, with unchanged selection/source and no active reflow. Both final wide/tall
and narrow/short states pass. This directly covers in-flight convergence rather
than assuming the native compositor delivered every intermediate request.

## Native evidence

All listed final commands exit successfully. Binary SHA-256:
`a10f3372ad16d331c96bb54edbaadbef9c2a01c622d2c8610bb8b5d424d9aad3`.
No production code changed, so the previous debug `layout-validation` binary
remains the correct native target. Each run uses private Weston, D-Bus and AT-SPI,
a disposable fixture, and 100% text zoom.

| Artifact prefix in `layout-previews/` | Display scale | Verified result |
| --- | --- | --- |
| `resize-burst-final-height-reading` | 100% | Fixture 79, 1040px width fixed; 884 → 304 → 884px viewport; technical row recovery and source unchanged |
| `resize-burst-final-width-reading` | 100% | Fixture 47, 1040 → 650 → 1040px width; deep heading anchor and three-card recovery |
| `resize-burst-height-editing` | 100% | Fixture 79, short-height stack; native insertion, saved text, selection/focus and exact undo |
| `resize-burst-width-editing` | 100% | Fixture 79, narrow-width stack; native insertion, saved text, selection/focus and exact undo |
| `resize-burst-125-width-reading` | 125% | Fixture 47, deep reading anchor and final committed viewport |
| `resize-burst-150-width-reading` | 150% | Fixture 47, deep reading anchor and final committed viewport |
| `resize-burst-125-height-editing` | 125% | Fixture 79, height shrink during editing and exact undo |

Reading reports observe zero anchor displacement; the threshold is one physical
pixel after scaling the AT-SPI logical displacement. AT-SPI's integer bounds are
not a subpixel raster measurement. The technical heading is near the document
start; fixture 47 supplies the deep-heading check, not arbitrary prose anchors.
Caret/selection data come from the validation-only editor report, not an AT-SPI
Text interface. Some editing runs print a clipboard-client broken-pipe message
during private-session teardown; the commands exit zero and saved-source/undo
checks pass.

The initial dark `resize-burst-height-reading` and `resize-burst-width-reading`
runs passed resize checks but failed the separate whole-output palette check.
Their desktop-shell windows were partly off-output, invalidating the palette
area assumption; these are not full passing captures. Their records remain.
The CLI now rejects appearance-pixel checks combined with desktop-shell resize,
boundary or font-reflow tests before launching. Theme checks stay in maximized
kiosk sessions. The fresh `mixed-layout-baseline` dark kiosk screenshot was
inspected and its appearance/source checks pass.

`scripts/check.sh` passes (415 view tests, two intentionally ignored native-font
tests). Ten resize-oracle tests cover invalid traces, geometry mismatch, sequence
targets, drift and source/selection failures. Thirteen capture tests include
invalid burst options and the desktop-shell/appearance incompatibility.
`git diff --check` passes.

```sh
python3 performance/capture-layout.py --fixture 47-editorial-composition.md \
  --binary target/debug/mineral-markdown --width 1920 --height 1200 \
  --scale 150 --zoom-steps 0 --resize-check reading --resize-burst \
  --layout-trace details \
  --output performance/layout-previews/resize-burst-125-width-reading.png
python3 -m unittest discover -s performance -p test_resize_layout_check.py
python3 -m unittest discover -s performance -p test_capture_layout.py
```

## Remaining work

This is burst convergence, not continuous pointer-drag resizing, simultaneous
scroll/resize, latency qualification, full fractional-scale coverage or window
placement/clipping qualification. Broader mixed-document composition, minimap
agreement, accessibility/IME/RTL and source/save dependencies remain open.
