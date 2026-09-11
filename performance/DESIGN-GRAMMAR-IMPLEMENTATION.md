# Document design grammar: native implementation

September 8, 2026. The authority for this pass is
[`designs/document-design-grammar.md`](../designs/document-design-grammar.md)
and its seven companion boards. Their written tokens supersede the earlier
palette, heading shadows, and default boxed-feature treatments. The generated
boards are visual references, not canonical typography or document content.

## Implemented on the continuous editing surface

| Grammar role | Native implementation |
| --- | --- |
| Foundations | Exact paper, surface, ink, body, muted, rule, moss, sage, information, warning, caution, and dark-code colors in `MineralPalette`. Shared typography, gutter, inset, radius, and width tokens in `DocumentStyle`. No heading shadows. |
| Typography | Fraunces display/section faces; bundled Liberation Serif 18/28 for sustained narrative sections; Spline Sans 16/24 for reference sections; mono code 14/21; compact tables 14/21; metadata 13. Display titles use 52/58, or 44/50 on narrow surfaces. |
| Rhythm | 64 before major sections, 40 before subordinate headings, 24 after major headings and between paragraphs, 16 after subordinate headings. Internal component padding is separate from external spacing. |
| Open features | Measured two-/three-column, row-major groups; 19 px authored labels with hanging explanations; consistent green bullet dots. No invented labels, sequence numbers, or completion icons. |
| Ordered stages | Larger authored numbers above short stages in quiet tiles. Longer/dependent actions retain vertical steps; nesting retains outline relationships. |
| Selection | Native shaping evaluates width, actual line count, height balance and overflow. At most three body lines after a label and a 1.5 height ratio. Short labels cannot override those limits. Below 560 usable logical pixels, lists and peer sections stack. |
| Sibling sections | Wider eligibility for repeated, bounded H3+ sections. Measured unequal-track explanation/example arrangements, table pairs, and image galleries remain source ordered. Peer prose stays borderless. |
| Tasks | Existing vector checkboxes, readable completed text, and a progress bar/count computed only from authored task states. |
| Signals | Exact pale backgrounds and separate foregrounds; 4 px corners and a single continuous thin border. Info, lightbulb, flag, warning-triangle, and error-circle icons. Wide single-paragraph callouts use aligned label/body rows; narrow or structured callouts stack. |
| States | Small chips only for recognized literal values in an authored `Status` or `State` table column. No status is inferred from ordinary prose. |
| Technical content | Crisp default table rules, preserved authored borders/alignment, light configuration panes, dark command/source panes, language labels, and real Copy/Copied feedback. Code content and whitespace remain exact. |
| Media | Full-image containment and 8 px media corners; existing measured paired figures and galleries. Alt text is not fabricated into captions. |
| Heading numbers | Source-authored chapter prefixes remain visible; wrapped title continuations hang under the title rather than its number. |
| Editing | Presentation stays outside Markdown. Labels use the same font runs in measurement and paint. Section reading modes stay pinned while an edit has focus. Geometry keys include the typography mode. |

The source document decides what is present. This implementation does not add
metadata, success states, summaries, captions, or decorative cards just to
resemble the boards' demonstration content.

## Visual evidence

These are captures of the real release binary on an isolated native Wayland
compositor, not HTML recreations or generated mockups. Each capture's `.source.json`
records the binary and source hashes and confirms unchanged Markdown.

- [Before](layout-previews/grammar-before.png) and
  [after](layout-previews/grammar-after.png): the same labelled-list fixture,
  at the same 1600 × 1200 window size.
- [Overview](layout-previews/grammar-overview.png): open features and ordered tiles.
- [Relationships](layout-previews/grammar-relationships.png): three sibling
  sections, mixed list relationships, and a true task summary.
- [Narrow](layout-previews/grammar-narrow.png): compact title and single-column fallback.
- [Enlarged text](layout-previews/grammar-large-text.png): 150% document zoom,
  measured label wrapping, and unchanged Markdown.
- [Signals](layout-previews/grammar-signals.png): semantic callouts.
- [Narrow signals](layout-previews/grammar-signals-narrow.png): stacked callouts
  with the same source, colors and icon semantics.
- [Reading](layout-previews/grammar-reading.png): narrative serif, inset quotation,
  compact table and source-derived completion meter.
- [Technical content](layout-previews/grammar-technical.png): code, tables, and explicit states.

Fixtures 53–55 cover the new visual vocabulary. Geometry, source order,
heading wrapping, status eligibility, reading-mode stability, and authored
table borders have automated regression coverage.

## Validation

- Rust workspace/all targets: 449 passed, 2 intentionally ignored.
- Python validation-oracle suite: 55 passed.
- Workspace formatting and Clippy with warnings denied: passed.
- Bundled narrative-font hashes verified; unmodified fonts retain their OFL license.
- Crusty: no new/worsened advisory architecture findings or blocking constraints;
  its indexed snapshot is stale, so compiler/tests/native captures remain authoritative.

The native [editing record](layout-previews/grammar-overview.edit.json) confirms
typing inside a feature, autosave, and byte-exact undo. The
[copy record](layout-previews/grammar-overview.copy.json) confirms source order
across open grids and ordered tiles. All final fixtures retained their original
bytes. Seven flat-surface pixel samples matched the grammar exactly: paper,
all five callout surfaces, and dark code.

The final release binary is
`65f6b09b1cd3c3ae459ef962678b78217e1fb1401fcb99f7e1e16567059cc120`.
At 1728 × 1080, 100% zoom, on a private 120 Hz Weston Wayland output:

| 10 MiB, 60 seconds | Presented FPS | Draw p99 | Presentation interval p99 |
| --- | ---: | ---: | ---: |
| [Bidirectional wheel](layout-previews/grammar-scroll-wheel.json) | 108.1 | 6.60 ms | 13.25 ms |
| [Bidirectional continuous](layout-previews/grammar-scroll-continuous.json) | 109.4 | 4.50 ms | 10.79 ms |

This is measured sustained throughput on this machine, not a universal minimum
frame-rate guarantee. The compositor/input seat was isolated; CPU resources
were shared with other host workloads. The wheel run recorded zero application
draw stalls of 25 ms or more, but three presentation intervals exceeded 25 ms.
The continuous run also recorded zero application draw stalls, and one long
presentation interval.

The [dedicated wheel-coast probe](layout-previews/grammar-coast-wheel-list.json)
also passes: the scrollbar continues moving after input release, slows from
169 to 53 px/s between early/late sample windows, then settles. An initial
1 MiB coast probe could not resolve the thumb's subpixel travel, even though
the document image continued moving. The existing small-fixture probe is used
for decay; the separate 10 MiB runs above measure sustained throughput.
The [continuous-input coast probe](layout-previews/grammar-coast-continuous-list.json)
passes too, slowing from 274 to 86 px/s and settling at the same thumb position
in the final two samples. These speeds describe the measurable scrollbar thumb,
not document-pixel velocity.

## Boundaries

This is the continuous-editor implementation, not a claim that every proposed
extension in the grammar is finished. True float wrapping that returns beneath
an image, flowing one long prose section across balanced columns, margin
footnotes, richer inferred resource/decision/timeline components, and paginated
screen/print masters still require dedicated layout primitives and editing
validation. Tables, code, equations, and critical warnings are not floated.
Existing supported Markdown/HTML/math remains fully visible through conservative
rendering rather than being silently replaced with an approximation.

The written token values can match exactly; pixel identity with generated boards
cannot be promised across different source content, fonts, zoom and displays.
