# Quotations, attribution and editorial emphasis

Scope: grammar §2/4/8 and board 05, requirement E07. This is progress toward
the complete grammar, not approval of all editorial, media or page families.

## Source and visual contract

- Ordinary Markdown quotations retain all paragraphs and source order in a
  quiet reading panel: 18/28 serif prose, 24 px horizontal inset, 16 px panel
  padding, 16 px between paragraphs, 4 px corners and a 1 px editorial rail.
- Only an explicit final attribution paragraph (`— `, `– `, `Attribution: `,
  or `Source: ` followed by text) receives the 14/20 sans metadata role and an
  8 px relationship gap. A lone dash-led quotation and an ordinary final
  paragraph remain quoted prose. Authored inline formatting and links survive.
- An immediately preceding heading labeled `Pull quote` or `Pull quote: …`
  explicitly establishes editorial intent for a paragraph-only quotation.
  Its body uses 24/32 italic serif, with a centered, constrained reading column
  inside a full-span Sage surface. The heading and passage remain authored,
  editable content: no copied passage, generated attribution or annotation.
- The centered column has a stable leading edge; individual live lines are
  not re-centered as their text grows. Focused source-order stacks use the
  same reading width and x position as the unfocused quotation.
- Nested quote ancestors each own their rail, 24 px indentation and padding,
  including a quotation consisting only of another nested quotation.
- Spacing is in document units and scales once with zoom. Preparation owns
  source analysis and component bounds; scrolling only clips and paints them.

The Rust skill guided source-owned roles, exact undo and geometry-cache keys.
The native-desktop design skill guided shared tokens, wide/narrow/200% checks,
the explicit editorial treatment and the native focus/selection review.

## Findings corrected during native review

The old quote painter used a 20 px leading inset against 24 px text geometry,
and quote endings mixed paragraph margins with panel padding. Nested quotes
without direct outer text could share the wrong rail origin. The first new
Weston render then revealed an unbalanced left-aligned full-span pull quote;
the text column is now centered. A subsequent native outline navigation found
that edit-locked stacks reverted to the reference-text width. The common
stack path now uses the quotation measure and retains centered placement.

## Verification

Fixture: `layout-fixtures/70-quotation-grammar.md`, SHA-256
`f62705402bf1efad13aa6b6bc906ea05105e265e8defa1721992b6432f70c86d`.

The current workspace checks pass: formatting, locked metadata, all-target
check, Clippy with warnings denied, workspace tests and doctests. Counts:
111 core + 9 source-fidelity + 11 tree-selection + 375 view (2 explicitly
ignored) + 40 app/bin tests. Ten Python harness/control-name tests pass.
Crusty reports 36 existing advisory findings, no new or worsened findings.

Three quote regressions cover source recognition, actual loaded-font fit,
attribution gaps/type, nested ancestor geometry, normal/narrow/200% layout,
focus at every specimen heading, role retention during marker editing,
source membership and exact undo. The independent native oracle is
`quotation_grammar_check.py`. It rejects the initial uncentered build as a
negative control, rather than treating every generated screenshot as a golden.

Final binary SHA-256:
`3372f5fe19d83afd7c5800679bda605d955a9f14345b60000fef5eb907fef05d`.
Artifacts use the `layout-previews/quotation-grammar-stable-` prefix:

- `wide.png`: 1600×1200, 100% text, ordinary and centered pull quotation.
- `narrow.png`: 600×1800, single column with the complete specimen visible.
- `200.png`: 1600×1800, native 200% text, 48/32/16 px inset/padding/attribution
  gap, without reducing typography to fit the surface.
- `long-weston.png`: native MCP outline navigation exposes the complete long
  quotation and following unquoted section. Companion `.weston.json` contains
  before/after AT-SPI trees and verifies unchanged paragraph x/width/height.
- `exact-edit-typed.png` and `exact-edit.edit.json`: pointer placement plus
  native Home/Right/Right and typing edits the centered pull quotation;
  autosave matches the entire expected document, undo restores all original
  bytes, and the text column does not move while typing.
- Each width has source hashes, an active native AT-SPI tree and independent
  `.pixels.json` evidence. All three pass `quotation_grammar_check.py`.

At 100%, the ordinary panel starts at x=276, text at x=300, with a 528 px
allocated reading column. Pull text uses the same 528 px column centered in
the 1280 px document area. Its body/attribution line boxes are 32/20 px with an
8 px gap. Heading focus preserves these measurements; the earlier 20 px
width change is absent. Nested rails have x=276/300/324 and real 1 px strokes.

Every final screenshot above was visually inspected. Weston MCP app 40 used
bounded capture-driven frames; its compositor was left running and the private
test application was stopped normally. Separate periodic Weston sessions
provide the width/zoom/edit captures. Earlier `first` and `verified` artifacts
preserve the review sequence and are not the final-build evidence.

The initial edit probe failed because it asked the harness to treat a repeated
attribution as a unique marker. The final probe compares the complete expected
document instead; the oracle was not weakened. The expected edited region
explicitly includes canonical `\.` escaping and a `> ` blank quote line.
All bytes outside that edited quote region remain identical, and undo restores
even that region exactly. This is not a claim that every edited Markdown byte
retains its original spelling.

## Scrolling performance

`quotation-grammar-stable-10mib.json` uses the same final binary, 10,485,760
bytes of generated mixed Markdown, a periodic 120 Hz native Weston output,
1600×1200, five-second warmup and 60.021 seconds of real bidirectional
continuous Wayland input. No build or other test app was active during the run.

- Average presented rate: 108.96 FPS; existing >60 FPS gate passes.
- Draw p99: 7.52 ms; maximum: 11.91 ms; no draws reached 25 ms.
- Presentation interval p99: 12.12 ms; maximum: 39.35 ms; two intervals reached
  25 ms. Missed configured refresh opportunities: 0.70%.
- Input latency p99: 11.40 ms; maximum: 39.26 ms.

Accessibility was inactive for this large-document workload. This is not a
claim of guaranteed per-frame 60/120 FPS, active-screen-reader performance or
coverage of every large-media/grammar mix. Native quote semantics were checked
separately on the actual fixture with accessibility active.

Post-release coast checks also pass on this binary. Fixture 70's wheel-input
thumb speed falls from 274.9 to 83.7 px/s and settles within one pixel between
the final samples (`quotation-grammar-stable-wheel-coast.json`). The continuous
impulse reaches the end of this short document and its scrollbar fades before
the last sample; that run is retained as an unsuccessful coast measurement,
not counted as a pass. Repeating the unchanged protocol on the longer fixture
69 produces a measurable 241.6 → 81.6 px/s decay and one-pixel final settling
(`quotation-grammar-stable-continuous-long-coast.json`). No scrolling code was
changed to accommodate either check.

## Still open

This does not close HTML `<cite>`/`<figure>` quotation conversion, quotations
with mixed technical/nested attribution structures, very deep/multiscript
layout, quotation floats, pagination or the complete accessibility/state
matrix. The current semantic tree retains canonical quotation nesting and
source order, but its paragraph bounds describe allocated text columns, not
precise glyph ink bounds. Edited quote regions still use canonical Markdown
escaping/blank-line serialization; untouched-region fidelity for arbitrary
structural changes remains in X01. The full coverage ledger stays authoritative.
