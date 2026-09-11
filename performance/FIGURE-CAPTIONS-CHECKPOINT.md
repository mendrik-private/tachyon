# Source-owned figure captions and credits

The complete document grammar remains open. This pass implements explicit
caption/credit attachment for top-level Markdown images; it does not approve
every media, pagination, or export family in boards 01–07.

## Implementation

- An image followed by an explicitly labeled caption and optional credit forms
  one visual unit. Supported labels are `Caption:`, `Figure:`, `Photo:`,
  `Illustration:`, numbered `Figure 1.` / `Fig. 2a:` forms, and `Credit:`,
  `Credits:`, `Photo credit:` or `Image credit:`. Inline emphasis is retained.
  Plain italics, a short paragraph, image titles and alternative text alone
  never establish a caption. `Figure out the answer` remains ordinary prose.
- Caption and credit remain canonical, editable paragraph nodes, with source
  order and exact untouched Markdown preserved. No annotations or generated
  numbering enter the Markdown. Projection roles are prepared once, not found
  by scanning the document during scrolling.
- Caption/credit text uses the shared 13 px / 18 px sans-serif caption role and
  muted text color. The attachment gaps are 8 px after an image and 4 px before
  a credit; ordinary prose resumes with its 24 px relationship gap. Zoom scales
  these values. Authored headings still clear the whole figure group.
- True wrapping measures the image, caption and credit together. All three
  stay on the figure lane; prose returns to full measure below its total
  height. A caption edit refreshes the complete group while retaining the
  focused arrangement. Changed caption revisions require reconsideration
  after editing rather than retaining an obsolete balancing decision.
- Galleries and explanation/image pairs consume the complete source-owned
  figure range, not caption paragraphs as independent columns. A gallery that
  exceeds its images' intrinsic sizes contracts its canvas, preserving the
  24 px visible gutter without upscaling. The planner still accounts for the
  unused page width, so six small images do not regress from three columns
  to three unnecessarily sparse two-image rows.
- Images have a single intrinsic, uncropped footprint shared by measurement,
  visible image bounds, captions and semantic geometry. Removed horizontal
  image padding and matched the underlay's media radius to the image, avoiding
  the old leading strip and square underlay at rounded corners.
- Captions expose the native caption role. Images retain their alt name and
  gain source-linked descriptions. The pinned AT-SPI adapter does not derive
  a description string from `DescribedBy`; publication supplies the explicit
  authored text as well as the relationship. Neither changes reading order.

The Rust skill guided ownership, invalidation and source-fidelity tests; the
native-desktop-design skill guided actual-app spacing, zoom, narrow-window
and assistive-semantics checks. Crusty consultation returned older dark/minimap
steering; the newer explicit light/no-minimap direction remains authoritative.

## Evidence and remaining work

Fixture: `layout-fixtures/69-figure-captions.md`, with the existing deterministic
non-quantitative botanical illustration. No board lettering or scientific
data was copied or invented. `figure_caption_check.py` checks native geometry,
actual glyph ink around and below the complete media lane, source order,
caption/credit accessibility, zoomed gaps and exact source hashes.

The first Weston MCP capture (app 35) established the caption lane and exposed
the gallery's inflated gutter. App 36 confirmed the corrected 24 px gutter but
exposed the missing AT-SPI description. Those are intermediate observations,
not final accessibility approval. The native pre-fix artifact is rejected by
the final caption-semantics checker rather than accepted as equivalent.

## Verified native build

Release SHA-256:
`5c66c0d8e9b943736622a3e3b04e579d4c5cd3c6787305afa7d6c6f0645edab1`.
Immutable binary:
`/tmp/mineral-caption-weston.EmVYGg/tachyon-verified`.
Fixture SHA-256:
`009bd19857fe6303c3293f5c38d607fbd09631e6eef830a10707fbfd0688b451`.

Visually inspected `layout-previews/figure-caption-verified-*` artifacts:

- `wide.png`: 1600×1200, 160 px square image, 36 px two-line caption and
  18 px credit; nine useful 28 px narrative lines beside the complete lane.
  Full-width prose returns at y=582. Image/caption/credit share x=276 and
  width=160; body gutter is 24 px.
- `narrow.png`: 600×1100, source-order stack, intact image and captions,
  shared leading edge, no old white inset strip, 8/4/24 px attachment gaps.
- `200-top.png`: 1600×1800, valid enlarged wrap, 48 px gutter, 36 px caption
  leading, 16/8 px attachment gaps, nine useful 56 px narrative lines.
- `200-stack.png`: 1600×1200 at 200%, scrolled to the caption. The shorter
  viewport rejects the tall wrap and stacks it. Caption, credit and following
  prose are visible and retain 16/8/48 px gaps. `200-short.png` is the earlier
  insufficient scroll and is not the caption-visibility evidence.
- `gallery-weston.png`: real Weston MCP app 37 after activating the named
  outline item. Both 600 px images retain their captions and credits, with
  an exact 24 px image gutter. No caption became a separate gallery column.
- `exact-edit-typed.png`: typing inside the caption label retains its figure
  lane. `exact-edit.edit.json` proves the complete autosaved Markdown matches
  the expected edit (including the existing serializer's punctuation escapes)
  and undo restores every original byte. The earlier boundary click left the
  entire marker present, correctly failing the interior-insertion oracle;
  the final check clicks inside it and does not weaken that assertion.

`python3 performance/figure_caption_check.py --prefix figure-caption-verified`
passes. The JSON pixel report includes every above geometry mode and matches
binary/fixture hashes across them. It checks caption roles, explicit image
descriptions, exact scaled attachment gaps, actual glyph ink beside and below
the media, clean gutters, gallery caption ink, and uncropped leading pixels.

Weston MCP app 37 exposes caption roles and complete image descriptions with
the original alternative names intact; `.weston.json` records the native tree.
Its source hash remained unchanged after outline navigation. All apps launched
by this pass were stopped normally; the compositor was left running. MCP's
capture-only repaint mode is not used as frame-rate evidence.

`scripts/check.sh` passes: locked metadata, formatting, workspace all-target
check/Clippy/tests and doctests. Counts are 111 core + 9 source-fidelity +
11 tree-selection + 372 view (2 explicitly ignored) + 40 app/bin tests.
Ten Python harness/control-name tests pass. Crusty reports the same 36 advisory
baseline findings with no new or worsened architecture findings. Logs use the
`figure-caption-verified-` prefix. This is bounded fixture evidence, not full
grammar or accessibility-matrix approval.

## Scroll performance

`figure-caption-verified-10mib.json` uses the same release, 10,485,760 bytes,
1600×1200, an isolated periodic 120 Hz Weston output, five-second warmup and
60.024 seconds of real bidirectional continuous Wayland input. No other build
or test application was active during the measurement.

- Average presented rate: 108.39 FPS.
- Draw p99: 8.86 ms; maximum: 30.43 ms.
- Presentation interval p99: 12.18 ms; maximum: 64.85 ms.
- Input latency p99: 11.58 ms; maximum: 54.79 ms.
- One draw and six presentation intervals reached 25 ms; 1.03% of configured
  120 Hz refresh opportunities were missed. The existing >60 FPS gate passes.

This is the generated large-document workload with accessibility inactive,
not a promise of 120 FPS, every-frame 16.7 ms, or a comprehensive large-media
and active-screen-reader performance matrix. Source/renderer fidelity and
native caption checks above use the actual media specimen separately.

Separate native wheel and continuous-input coast checks also pass on this
release (`figure-caption-verified-{wheel,continuous}-coast.coast.json`). After
input release, wheel thumb speed falls from 155.7 to 58.3 px/s and settles at
the same position in the last two samples. Continuous-input speed falls from
226.8 to 89.9 px/s and moves only one pixel between the final samples. These
are real post-release scrollbar observations, not a screenshot-based FPS
estimate or evidence that the capture-only MCP compositor animates while idle.

Still open: shared gallery captions, structured HTML figure conversion,
caption/credit families inside nested containers, figure-reference resolution,
right-side and multi-script wrapping, further intrinsic/missing-media states,
anchored margin notes, bibliography, audio/video/map families, pagination and
exports. The full coverage ledger remains authoritative; this checkpoint does
not replace its requirements with this smaller specimen.
