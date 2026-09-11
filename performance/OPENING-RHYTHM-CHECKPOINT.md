# Opening rhythm checkpoint

2026-09-09. Layout-priority continuation of A07; the full audit remains open.

## Result

The native-app UX spacing pass now distinguishes a document opening from a
later section break, following the relationships in section 6 of
`LAYOUT-GRAMMAR-PLAN.md`:

- Title plus introductory prose/recognized metadata → first H2: 48 logical px,
  previously 64.
- Consecutive H1 → H2: 28 logical px, previously 48.
- Later major sections retain 64 px; subsections retain 40 px. Body spacing,
  component padding, typography, canvas widths, and theme/minimap policy are
  unchanged.

The opening boundary is resolved once from the canonical source prefix and is
part of the published geometry key. It is not inferred from a heading's text,
the viewport, or a fixed paragraph count. Ordinary lists, timelines, code,
figures, and other components end the opening; a paragraph after them does not
reclassify that content as introductory material.

## Regression evidence

```sh
cargo test -p document-view opening_rhythm_is_tighter -- --nocapture
```

Before the change, the measured-geometry test failed with `opening gap 64 != 48`
at width 360, zoom 1. After the correction it passes at widths 360/1040 and
scales 1/2. Cases include one or multiple intro paragraphs, recognized metadata,
consecutive headings, a preamble before the title, an earlier subsection,
ordinary lists, and code followed by prose. Later section separation and exact
source serialization are asserted. A separate test guards geometry-key
invalidation when the opening boundary changes.

The full suite caught an overbroad first implementation that treated a timeline
under a title as introductory content. The classifier was corrected; the
existing timeline spacing assertion remains 64 and passes unchanged. The older
paragraph-opening spacing assertion was updated from 64 to the intended 48;
its table-padding and paragraph-gap assertions remain unchanged.

## Native checks

Final layout-validation binary SHA-256:
`f3c87e38601bcdb1550dd57b65df692f9901f0fca14405440c3078e4e2776ea8`.
Only a regression test was added after this build.

Artifacts are in `layout-previews/`:

| Artifact prefix | Native coverage |
| --- | --- |
| `rhythm-readme-qualified` | Real README staged into a private editable copy; 1440×1100, dark, 100%. Actual caret placement below the moved heading, typing, autosave, protected fragments, exact undo, source and private appearance checks pass. |
| `rhythm-readme-narrow` | Same real README at 600×1100, light, 100%; source/appearance pass, native screenshot inspected. Mixed explanation/code rows stack; the document retains its available width. |
| `rhythm-reference-qualified` | Fixture 82 at 1440×1100, light, 200%; source/appearance pass. Fixed widths remain unchanged, tables remain stacked, and the first section moves up 32 screen px. |
| `rhythm-consecutive-headings` | Fixture 51 at 1440×1100, dark, 100%; source/appearance pass. Consecutive title/section gap is exactly 28 px in native accessibility bounds; the decision panel and three-column reference row remain intact. |

Native README heading bounds change from y=496 (`rhythm-readme-before`) to
y=480 (`rhythm-readme-qualified`), with the same x=256, width=1160, height=34.
At 200%, fixture 82's heading changes from y=554 in the prior
`table-resize-200-qualified.active-atspi.json` to y=522 in
`rhythm-reference-qualified.active-atspi.json`, with the same width/height.
Fixture 51's title ends at y=120 and its first H2 begins at y=148.

Reproduce the native editing check:

```sh
python3 performance/capture-layout.py --source-document README.md \
  --binary target/debug/mineral-markdown --width 1440 --height 1100 \
  --appearance dark --atspi-active --source-unchanged-check \
  --select 280 702 280 702 --selection-keys home --edit-check \
  --edit-within 'Build and open a document:' \
  --edit-preserve '## Build and run' '## Keyboard commands' 'Run the repository checks with:' \
  --output performance/layout-previews/rhythm-readme-qualified.png
```

The native edit oracle checks the intended insertion and named protected
fragments during the edit, not the entire committed edited source. Undo and
final source checks are byte-exact. An earlier attempt incorrectly named the
non-unique `scripts/check.sh` fragment; the final run uses unique markers.
Unqualified `*-after` images are intermediate visual probes, not final binary
qualification. Final screenshots and the edited README screenshot were viewed.

The workspace README remains
`abca9115a24f8b4293074b510011acfc5d11a036f910ba68d77e6f0054527541`;
`plan.md` remains
`0e6d5a63b803ce3001c558e8747a46b2ffd376ee8a829a473141810deff47079`.
Neither original was edited.

## Verification and remaining work

`scripts/check.sh` passes formatting, workspace checking, strict Clippy, all
tests and doctests. Document-view: 428 passed, 2 ignored. `git diff --check`
passes. No production dependency, font, or serialization change was made.

This establishes the opening spacing rules, not a full mixed-content layout or
release qualification. Further real-document composition and embedded-image
geometry coverage remain, along with all other unclosed audit requirements.
