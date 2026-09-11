# Supporting figures and true text wrapping

This is partial E04/E05 coverage, not approval of the entire document grammar
or board 05. The written grammar remains authoritative. The full goal is open.

## Implemented

An authored supporting photographic or illustrative role can nominate a
small image for a true wrap. Adjacency or file extension alone is insufficient;
explicit diagram, chart, map, screenshot, schema, evidence and essential roles
override the nomination. Alternative text remains an accessibility description,
not a fabricated visible caption. Supporting images no longer force otherwise
sustained narrative prose into compact reference typography.

Native font measurement selects a 4/8-track first row inside the reading
measure, with a 24 px gutter. The figure must occupy 25–35% of that measure,
leave at least the measured equivalent of 40 characters for prose, have known
intrinsic dimensions and enough text for at least four useful adjacent lines.
No upscaling is used. Height is capped at the smaller of 40% of the viewport
or 336 document pixels; narrow, short and unsuitable groups remain stacked.

The paragraph continues in a full-span second region below the figure, without
an artificial paragraph gap at the width change. Subsequent paragraphs retain
their normal separation. Headings and other non-prose blocks end the wrap.
Canonical source order and byte ranges remain unchanged; there is no new card,
manual layout preference, generated label or duplicated text.

The plan owns source-linked break anchors. While a paragraph is edited, those
anchors follow the edit instead of choosing a different arrangement. Localized
geometry rebuilding includes the complete figure/text group. Normal keyboard
movement follows source lines across the changing measure, and plan publication
includes the new geometry in its identity. Candidate selection and shaping do
not run in scrolling/painting.

## Native visual and interaction evidence

Release SHA-256:
`daba40e9890473c94229b684aeb9d7408cf650946e30aa0d526ce05b98aef2e9`.
Immutable verification binary:
`/tmp/tachyon-figure-flow.ldei8f/tachyon-verified`.
Fixture `layout-fixtures/67-supporting-figure-flow.md` SHA-256:
`ba307554e1e5258d5bfaec6cc8600a55a721c224c235f2709691f474b8cc7a38`.
The local botanical SVG is a deterministic, non-quantitative synthetic
illustration; it is not an extracted board asset or a scientific diagram.

Visually inspected native captures:

- `layout-previews/figure-flow-verified-wide.png`: 1600×1200, six 18/28
  narrative lines beside a 160 px square illustration, then full-width prose.
- `layout-previews/figure-flow-verified-narrow.png`: 600×1100, original-order
  stacked image and narrative text, no compressed side corridor.
- `layout-previews/figure-flow-verified-200-top.png`: 1600×1200 at 200% text,
  still enough room for a valid wrap, 48 px gutter and 36/56 narrative type.
- `layout-previews/figure-flow-verified-200.png`: scrolled enlarged view,
  including the full-measure continuation. This is visual evidence only;
  ongoing scroll settlement can offset its later AT-SPI snapshot slightly.

`python3 performance/figure_flow_check.py` passes independently across wide,
narrow and static 200% views. It checks matching binary/fixture hashes,
unchanged source, source-order image/paragraph semantics, no alt-caption
fabrication, actual glyph ink in six narrow lines and below the image, a clean
gutter, 24 px paragraph gaps and 64 px section gaps with zoom scaling. The
figure occupies approximately 30.3% of the measured prose width. The checker
does not establish full font/contrast/state conformance or caption support.

Native input on the same binary:

- `figure-flow-verified-boundary.selection.json`: End then Shift+Right from
  the last narrow line selects exactly the first `i` below the image.
- `figure-flow-verified-down.selection.json`: Down, Home, Shift+Right crosses
  the wrap in source order and selects that same character.
- `figure-flow-verified-source-edit.edit.json`: pointer placement and typing
  autosave the exact expected whole Markdown; undo restores every original
  byte. The changed paragraph uses the existing canonical punctuation escapes;
  untouched paragraphs, headings and image syntax remain byte-identical.

The first edit expectation incorrectly expected the changed paragraph to retain
its original punctuation spelling. That failure was inspected, not ignored;
the final complete-document expectation explicitly includes the serializer's
existing period/comma escaping, without relaxing untouched-source checks.
Earlier `figure-flow-first-*` captures show the compact reference face and are
not final typography evidence. The checker originally assumed 200% must stack;
actual native measurements show a readable valid wrap at this viewport, so the
final oracle checks its scaled geometry and glyphs rather than forcing a
window-size-only breakpoint.

Weston MCP apps 29 and 30 were launched on private copies and stopped normally.
Final app 30 exposed image and paragraph semantics in canonical order, but the
non-periodic MCP compositor still showed provisional geometry and a blank image.
This unresolved startup/rendering defect is not accepted as final visual proof.
The approved specimen pixels use the existing periodic native Weston harness
(120 Hz), real loaded images/fonts, private state and native input.

## Automated checks

Workspace all-target tests pass: 111 core unit tests, 9 source-fidelity tests,
11 tree-selection tests, 369 view tests (2 explicitly ignored), and 40 app tests.
The three new native-measurement tests cover exact source coverage, image/text
alignment, full-measure continuation, heading clearance, narrow rejection,
same-environment retention, localized editing/full rebuild agreement, exact
undo, and exclusion of captions, warnings, tables, hard breaks, unknown roles
or dimensions, and very tall figures. Supporting narrative versus essential
reference typography is asserted explicitly.

Workspace Clippy with `-D warnings`, formatting, diff whitespace checks and
both doctest targets pass. Build/test/Clippy logs use the
`layout-previews/figure-flow-verified-*` prefix. Crusty preparation
`ctx_6d3d0554a03d` and validation `task_afbecc7571550a36` are terminal;
36 existing advisory findings, no new/worsened findings.

## Still open

### Scrolling checkpoint

`layout-previews/figure-flow-verified-10mib.json` records the same release
binary on 10,485,760 bytes of generated mixed Markdown, 1600×1200, 120 Hz,
and 60.0205 seconds of real continuous Wayland scrolling. It passes at
108.7794 presented fps; draw p99 is 8.8637 ms and presentation p99 is
12.7386 ms. There are three presentation intervals of at least 25 ms
(maximum 71.50 ms), so this does not claim every frame is below 16.7 ms or
that 120 fps is met. The source remained unchanged. Our builds and other
captures had finished; an unrelated host Cargo test process was still live,
so this is a target check, not an isolated speedup comparison. It does not
replace image/grammar-heavy performance, wheel-coast or startup validation.

### Remaining family scope

Attached caption and credit lanes; explicit right-side/portrait/pullquote
variants; broader optionality recognition and non-English descriptions;
full RTL/CJK/bidi/IME, structural edits and image replacement coverage;
mixed footnote/media flows; figure numbers/cross-references; margin notes;
page masters, pagination and selectable exports. Caption-like paragraphs
currently retain the stacked fallback rather than being floated incorrectly.
Ordinary stacked-image background/corner treatment still needs a board-level
polish pass; this change does not approve the existing hero/image fallback.
The non-periodic native startup defect remains a separate real blocker to
unconditional visual approval. Current evidence proves this bounded wrapping
path, not all board rules or every document.
