# Visual tweaks — 2026-09-11

Completed Crusty work:

- `work_9fe974aa7f81dbc9`: Decision, Selected option, Pros and Valid use the same
  accent border, thickness and radius. The light theme uses its semantic accent.
- `work_e9c3e30ee94c1293`: pull-quote text and attribution share the panel's left
  content inset while retaining the readable text measure.
- `work_2957fa3e61b7161b`: recognized label colons use a shared translucent text
  token over the actual surface. Other text, rich styles and source bytes remain
  intact; code and ordinary prose punctuation are unchanged.
- `work_59f2d818b935d594`: horizontal editorial cards paint the tallest peer's
  height. Internal content remains top aligned; stacked cards use natural height.

The app starts with native maximized window bounds and retains 1100×720 restore
bounds. The native window-control replay verifies initial maximization, restore,
keyboard/pointer maximize and minimize, dragging, reactivation and clean close.
The replay now drags down/right because the compositor may restore near the
output's upper-left edge, where the previous up/left drag was clamped.

## Verification

- `scripts/check.sh` passed: locked dependency checks, formatting, workspace
  check, strict Clippy, workspace tests, accessibility adapter/publication tests
  and doc tests. Document-view: 597 passed, 2 pre-existing ignored tests.
- `cargo build --release --locked --bin mineral-markdown` passed.
- Quote and editorial tests verify alignment, shared row height, natural stacked
  padding, source coverage, editing stability and exact Undo. A text-run test
  checks Unicode boundaries, colon-only styling in both themes and exact source.
- Private Wayland captures of `117-visual-tweaks.md` at 960 and 1600 output width,
  each at 100%, 150% and 200% text zoom, preserve exact fixture bytes. Light-theme
  captures also cover 100% and 200%; the narrow 200% quote has a scrolled capture.
- `layout-previews/visual-tweaks.geometry.json` records pixel-measured card
  extents and AT-SPI quote/attribution alignment for the seven initial captures.
  Every horizontal pair has equal top/bottom extents within 1px. Stacked pairs
  have different natural heights. At 1600/100%, Decision and Selected option
  both have 147px fill height; Pros and Cons both have 203px fill height.
- `layout-previews/visual-tweaks-startup.window-controls.json` records native
  startup/restore and all window-control checks passing.
- Release binary SHA-256:
  `e7569dc048849f984a14060cb8a2d71aa51e847fd023478e70f3c65f0e08246b`.
- Crusty preparation `ctx_794b1fccdf42`; validation
  `task_42fcd44dfd5d8e51` found no new or worsened architecture findings.

Reproduce a capture:

```sh
python3 performance/capture-layout.py --fixture 117-visual-tweaks.md \
  --width 1600 --height 2400 --appearance dark --atspi-active \
  --source-unchanged-check \
  --output performance/layout-previews/visual-tweaks-wide-100.png
python3 performance/capture-layout.py --fixture 41-find-document.md \
  --width 1600 --height 1200 --window-controls-check \
  --output performance/layout-previews/visual-tweaks-startup.png
```

Equal-width prose columns and the checklist arrangement/spacing task remain
open. Those change layout selection and are outside this small visual batch.
