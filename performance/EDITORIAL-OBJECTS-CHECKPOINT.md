# Editorial objects — verified checkpoint

Scope: named decisions, selected options, worked examples, pros/cons and
valid/invalid objects from Board 03. This is partial grammar implementation,
not approval of the complete board. Written grammar tokens remain authoritative.

Release binary SHA-256:
`ffcb49831aa99a3bca00af8ae74279b3a9c59c21f9a69e8f63268e69b36d644a`.
Fixture: `layout-fixtures/59-editorial-objects.md`, SHA-256
`e67df8f2ca1190f8358beabd936796884d1ddef09637faf248528ed09497c8ca`.

## Implementation and boundaries

Explicit authored headings identify the object. Ordinary narrative headings,
document titles, nested chapters, tasks and long arguments stay in open flow.
Each object retains its canonical heading, paragraphs, lists and code; there
is no secondary editable document or invented completion state. Pros and cons
remain labeled unordered lists, not checklists. Paired comparisons use equal
tracks; other bounded objects may use measured asymmetric tracks. All keep
source order, natural heights and reference-mode body text.

One native-font geometry constructor supplies candidate measurements and final
lines: 24 px card insets, 8 px title/body gap, 12 px between list items and
16 px between supporting paragraphs/code. Cards use 4 px corners and 1 px
boundaries without shadows. Code retains its independent header, Copy action,
exact whitespace and contained overflow. Focused editing retains object/track
geometry; blur may reconsider eligibility after substantial text growth.

Native review found that the embedded code pane and its Copy action reached
the outer card's right edge. Rendering, hit-test/scroll bounds and the actual
candidate overflow measurement now account for the containing 24 px inset.

## Native and automated evidence

- [Wide layout and real Copy hover](layout-previews/editorial-verified-copy-hover-still.png),
  1600 × 1200: measured pairs, natural heights, explicit labels and tooltip.
- [Code and validation objects](layout-previews/editorial-verified-code.png):
  code panes keep equal left/right/bottom insets.
- [Narrow](layout-previews/editorial-verified-narrow.png) and
  [narrow code](layout-previews/editorial-verified-narrow-code.png), 600 × 1100:
  objects stack without changing source order.
- [200% complete card](layout-previews/editorial-verified-200-complete.png):
  full first decision card visible at 600 × 1100; enlarged text wraps and its
  insets scale to 48 px. Earlier `200`/`200-card` captures were viewport-clipped
  and are not the full-card evidence.
- [Independent pixel samples](layout-previews/editorial-verified.pixels.json):
  Paper/Rule/Code tokens, exactly 24 px code insets and a single raster row of
  border at 1×. This is not an exhaustive contrast/corner audit.
- [Prose edit](layout-previews/editorial-verified-edit.edit.json) and
  [code edit](layout-previews/editorial-verified-code-edit.edit.json): native
  typing autosaved; undo restored exact bytes. Isolated `.source.json` checks
  alongside the captures pass.
- [Copy order](layout-previews/editorial-verified-wide.copy.json): eight unique
  markers appear once in source order. Not an exact full-clipboard golden.
  The retained `editorial-copy-ambiguous-markers.json` failure used `Cons`,
  which also matched the earlier `Considered trade-offs` heading; changing
  the marker, not document content, resolved that query ambiguity.
- [Warm geometry](layout-previews/editorial-verified-wide.planning.json): warm
  validation reused published geometry/cache with zero new shaping/wrapping,
  zero segments laid out and zero anchor displacement.
- [Weston MCP AT-SPI](layout-previews/editorial-verified.atspi.json): canonical
  headings, list semantics and source order remain exposed. Activating Copy
  produced the accessible `Copied` feedback. Clipboard bytes for that isolated
  button activation were not independently inspected.

`cargo test --workspace --all-targets --locked`: **469 passed, 2 ignored**.
Workspace Clippy and formatting passed. Native geometry tests cover 1200,
760, 480 and 230 logical px, exact projection coverage, measure/paint height
agreement, zoom and focused text growth with exact undo. Crusty validation
reported no new/worsened architecture findings (36 pre-existing findings).

## Scrolling evidence and limitations

[60-second 10 MiB continuous-scroll run](layout-previews/editorial-verified-perf-continuous.json):
108.57 average presented FPS, presentation p99 11.68 ms, draw p99 7.74 ms,
input p99 10.93 ms over 4944 qualifying samples. Six presentation intervals
were ≥25 ms (maximum 47.55 ms); no application stall was ≥25 ms. This meets
the measured average/percentile gate, not a guarantee for every frame or the
120 Hz stretch target. The generated mixed document is not editorial-heavy.

[Wheel coast pixel check](layout-previews/editorial-verified-wheel-coast.json):
after release the thumb continued through y=92,128,154,177,192,204,204;
early speed was 154.99 px/s and later speed 54.27 px/s, then settled. The
seven associated PNG frames and timing samples pass the existing decay oracle.

The retained [60-second wheel profiler run](layout-previews/editorial-verified-perf.json)
**failed** because it recorded zero qualifying input-latency samples, despite
delivered wheel events and 109.67 average FPS. Short probes reproduced this
at both 3-second and 8-second startup waits; changing only the native input
kind to continuous produced samples. GPUI qualifies input only when dispatch
directly invalidates a frame; wheel impulses joining an existing momentum
animation need not do so. This previously recorded limitation is documented
in `ADAPTIVE-SPEC-IMPLEMENTATION.md`. No gate or profiler/editor code was
weakened to count the wheel run as passing. Full wheel-latency attribution
remains unverified; separate pixel decay is not a substitute for it.

Complex/nested editorial objects, every structural editing operation, RTL,
contrast/state matrices, the rest of Board 03 and the full grammar remain
incomplete. The goal is unchanged and active.
