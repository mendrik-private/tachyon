# Reading measure inside wide table cells

2026-09-10. Completes Crusty `work_971aef14b72c6a0b`.

Paragraph shaping inside table cells now uses the smaller of the actual inner
cell width and the loaded font's reference reading measure at that paragraph's
font size. Normal text and inline attachments use the same width. Cell bounds,
authored column widths, comparison structure, alignment and horizontal scrolling
remain independent. Row height follows the resulting shared visual fragments;
painting and caret hit testing consume those same fragments. Code, images and
HTML previews retain their component-specific sizing.

## Evidence

The new regression failed before the fix: a 1108.8px shaped line exceeded the
model font's 604.8px reading measure. It now passes at 600/1280/1800 logical
canvas widths and 100/150/200% zoom, asserting all text bytes, unchanged source,
and exact authored [240,1200] column widths. All document-view tests pass,
including existing rich-cell, local/full geometry, editing and Undo coverage.

Fixture `124-table-reading-measure.md` uses the reported Component/Appearance
and behavior descriptions with explicit 240/1200px columns, exercising the
requirement to separate outer width from internal prose measure. The current
plan.md has narrower saved widths and was not rewritten. Fixture SHA-256:
`b8f16f7dbac75a55d754d55c41048264105708d75b0df34f8e71f463a5a890ff`.

Final native debug layout-validation binary:
`d82e831163d6ecd059fd7febc842894fd51ef8bd34110735c78447a373c6b249`.
Native artifacts are `performance/layout-previews/table-reading-*`:

- `before`: reproduced overly long description lines at 1920x1500.
- `wide`, `regular`, `narrow`, `150`, `200`: 1920/1440/600px and enlarged text,
  exact unchanged-source sidecars and active AT-SPI trees. Wide, narrow, 150%
  and 200% screenshots inspected. The semantic table remains 1440px wide at
  100%; its first description cell remains 1199px wide with 42px content height.
- `edit-wide`, `edit-150`, `edit-200`: pointer placement on the newly wrapped
  second line, Home, type x, autosave and two seconds of focused idle. Full-file
  goldens require only `scroll anchor.` -> `xscroll anchor.`; one native Undo
  restores every byte. No serializer-escape exception was needed.
- `narrow-pan`: actual horizontal scrolling moves the fixed-width table while
  the surrounding document stays put. Six copied table markers remain in
  source order. This is marker-order evidence, not full rich MIME equivalence.

`scripts/check.sh` passes: 782 Rust tests, two existing ignored tests, locked
checks, formatting, strict Clippy, adapter tests and doctests. Log:
`/tmp/mineral-table-reading-check.log`. `git diff --check` passes.
Crusty `ctx_bcd3d679c9dc` / `task_89ce22d7208c9930`: 75 existing advisory
findings, none new/worsened/resolved; stale indexed guidance only.

Reproduce the native capture:

```sh
cargo build --locked -p markdown-app --features layout-validation
python3 performance/capture-layout.py --fixture 124-table-reading-measure.md \
  --binary target/debug/mineral-markdown --width 1920 --height 1500 \
  --source-unchanged-check --atspi-active --output /tmp/table-reading.png
```

The separate near-reading-width table alignment item remains open. This result
does not claim complete IME/RTL, export, continuous resize or release performance
qualification. Fixed-width tables still intentionally scroll in narrow panes.
