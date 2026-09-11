# Compact technical-section layout — 2026-09-09

This continues the user's layout/available-space priority (STR-0003) and
[document-space work](LAYOUT-SPACE-CHECKPOINT.md). A07 and the full audit remain
open; this is a bounded implementation and native verification checkpoint.

## Layout behavior

Adjacent compact technical sections can share a measured row, including an H2
or H3 heading, a short explanation, and a table or code example. Table/code
combinations retain their ordinary visual treatment: table cells, syntax
highlighting, code header, copy control, line numbers and inset padding.
Columns negotiate their widths from measured content and retain a 24px gutter.

Eligibility does not assert a semantic relationship: these are neighboring
sections, not an invented comparison or request/response object. Named editorial
objects keep precedence. Peers must have the same heading level and parent.
H1, nested subchapters, long examples, large tables, and trailing explanations
remain outside this compact treatment. A technical section must have a compatible
neighbor; isolated examples keep existing explanation layout behavior.

Pairing requires table measurement, adequate width, balanced height and viewport
fit. Narrow windows, large text and short viewports can return to a source-order
stack. It does not rewrite Markdown or detach headings from their own content.

## Verification

`scripts/check.sh` passes: dependency pins, formatting, workspace check, Clippy
with warnings denied, all-target tests and doctests. Regression tests exercise:

- No speculative table pairing before measurement; measured wide-window pairing.
- Source coverage, sibling gutters and stacked narrow/short-window fallback.
- Chapter boundaries, continuations, alerts, oversized code and oversized tables.
- Localized editing versus full geometry, including growth/shrinkage and undo,
  for explanations, code and table cells at 100%, 150% and 200% text zoom.

Native captures use fixture `79-technical-sections.md`, a private periodic Weston
session, light appearance, 100% display scale, and debug binary SHA-256
`f6ca6d5b6f8362d5d7649fcce2f6f7eabb0d4d46f3520d94960fdcac77c99b0d`.
The fixture SHA-256 is
`4ff5072fca0cf1117ef312f493799c14f924b53033e86419ba1a484d87f26963`.

| Capture prefix in `layout-previews/` | Window / text zoom | Configuration sections |
| --- | --- | --- |
| `technical-space-wide` | 1920 × 1000 / 100% | Side by side |
| `technical-space-medium` | 1440 × 1000 / 100% | Side by side |
| `technical-space-narrow` | 600 × 1000 / 100% | Stacked |
| `technical-space-zoomed` | 1920 × 1000 / 200% | Stacked |

These screenshots were inspected. Each has active private AT-SPI evidence and
an unchanged-source report. Native edit captures `technical-space-code-edit`
and `technical-space-cell-edit` verify pointer-targeted insertion in the right
code panel and left table respectively, autosave, preservation of an unrelated
fragment in the other column, and undo restoring every original byte.
`technical-space-code-edit.copy.json` verifies the configuration headings and
following notes retain source order through Select All/Copy; it is not a full
clipboard-equality test.

Reproduce the baseline, replacing width/zoom/output for other matrix entries:

```sh
python3 performance/capture-layout.py --fixture 79-technical-sections.md \
  --binary target/debug/mineral-markdown --width 1920 --height 1000 \
  --zoom-steps 0 --source-unchanged-check --atspi-active \
  --output performance/layout-previews/technical-space-wide.png
```

## Remaining work

Broader mixed-document composition, resize/scroll anchoring, minimap agreement,
fractional display scales, RTL, IME and accessibility action qualification remain
open. Small tables deliberately keep intrinsic cell widths. The current forced
light appearance still needs a separate dark/system-theme pass. This debug build
does not inherit historical release-performance results or close source/save
audit dependencies.
