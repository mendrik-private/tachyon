# Request/response objects and code overflow — checkpoint

Scope: Board 04's explicit exchange examples and the code-in-card overflow
defect found during native validation. This does not approve the full board
or the complete document grammar.

Final release SHA-256:
`e42a16b48e52f58078de35f3d484bb96dc28f2d48d70d765ed2587a4409fe260`.
Fixture: `layout-fixtures/60-request-response.md`, SHA-256
`70ce3f78754b9e66c786c746b1c119b0792136c4dc9ea24dde78283d6d1957e7`.

## Implementation

Authored Request/Response (also HTTP Request/HTTP Response) headings, with an
optional colon subtitle and actual code content, identify bounded exchange
objects. A prose-only request remains open narrative. Ordinary headings,
mixed heading levels, intervening topics and reversed/repeated request labels
do not create false pairs. H2 and H3 objects both participate; canonical
heading levels and parents remain intact.

Requests pair only with a following response in the same semantic scope. A
response cannot attach to the next request for a lower visual score. A pair
followed by another request is not a three-item feature family. Actual-font
fit, equal 6/6 tracks, height balance and overflow gates select pairs; narrow
or unsuitable content stacks in source order. Authored methods, paths,
statuses, code and whitespace remain editable, not synthesized labels/data.

The existing shared editorial geometry supplies 24 px card insets, 24 px
gutters, 8 px heading/body spacing, natural heights, reference body type,
4 px corners, Paper/Rule boundaries and language-specific code surfaces.
Typing into an object preserves its track, including when its heading is
temporarily changed from Response to Result; blur releases that eligibility.

### Defect caught by visual validation

At 200%, horizontal scrolling inside a card still used the full card width,
while clipping excluded both 48 px outer insets. A painted-editor regression
reproduced a 1920 px scroll viewport versus an 1824 px content mask. The fix
uses the same inner viewport for both and retains 16 logical px of trailing
code padding. It also corrects the old 8 px trailing allowance for standalone
code, without changing HTML/table scroll geometry or source content.

The original native capture left only 2 raster pixels after the final glyph;
the identical input after the fix moves that glyph 36 px left and leaves
38 px. The advance-box inset is independently asserted as 32 physical px
at 200%; the remaining difference is font side bearing.

## Final-build evidence

- [Wide and Copy hover](layout-previews/exchange-final-wide-hover.png),
  1600 × 1200: correct pair direction, shared insets and fine rules.
- [Narrow stack](layout-previews/exchange-final-narrow.png), 600 × 1100:
  both first exchange objects fully visible with attached labels/payloads.
- [200% horizontal overflow](layout-previews/exchange-final-200-code-scroll.png):
  the complete line ending is reachable with the correct trailing inset.
  Left-edge clipping at the maximum horizontal offset is intentional;
  scrolling back reveals the beginning without changing copied source.
- [Unpaired and rejected examples](layout-previews/exchange-final-rejection.png):
  the unpaired request remains separate; the rejected response retains its
  literal status and explanation without relying on red/green coloring.
- [Raster measurements](layout-previews/exchange-final.pixels.json): 24 px
  gutters and three code-pane insets, Paper/Rule/Surface tokens, single-pixel
  sampled boundaries at 1×, plus the before/after overflow comparison.
- [Native editing](layout-previews/exchange-final-edit.edit.json) and
  [stable border probes](layout-previews/exchange-final-edit.layout.json):
  typing hits the requested JSON field, autosaves, and exact undo restores
  Markdown bytes without moving the paired borders.
- [Exact whole-document copy](layout-previews/exchange-final-copy.copy.json):
  clipboard equals the independently authored `60-request-response.copy.txt`
  golden, including code whitespace and all source-order labels.
- [Warm geometry](layout-previews/exchange-final-copy.planning.json): two
  explicit warm replans pass the existing cached-geometry oracle (no new
  shaping/wrapping/segment layout or anchor displacement).
- [Weston MCP semantics and Copy](layout-previews/exchange-final.atspi.json):
  source-order headings and literal payloads are exposed. The named Copy
  action puts exactly the authored first payload on the isolated clipboard.
  After settling the zero-refresh compositor with a bounded screenshot series,
  a real pointer click showed the Copied checkmark/label and AT-SPI also
  published Copied. The whole application accessibility/state matrix is not
  approved; the initial provisional MCP raster is not settled-layout evidence.

All corresponding final capture `.source.json` checks pass. Pre-fix
`exchange-verified-*` artifacts use binary `4d998273…244ec`; retain them as
historical evidence, not as final overflow approval. The exact-copy capture
can retain selection highlights; the separate hover image is the appearance
reference. No screenshot is a substitute for the native interaction checks.

## Checks and performance

`cargo test --workspace --all-targets --locked`: **474 passed, 2 ignored**.
`cargo clippy --workspace --all-targets --locked` and
`cargo fmt --all -- --check` pass. The five added regressions cover semantic
classification, actual 1200/760/480/230 px geometry and source coverage,
heading levels/direction/barriers, edit locks, and painted code scroll bounds.
The classifier, direction/H2 and overflow tests were observed failing before
their respective fixes. Crusty validation reports no new/worsened architecture
findings (36 pre-existing). Compiler and lint checks were run directly.

[60-second, 10 MiB continuous scrolling](layout-previews/exchange-final-perf-continuous.json):
107.50 average presented FPS; presentation p99 12.79 ms; draw p99 8.75 ms;
input p99 12.35 ms over 4937 qualifying samples. Four presentation intervals
were ≥25 ms, maximum 62.23 ms; no application stall was ≥25 ms. This passes
the existing gate, not a guarantee for every frame or the 120 Hz stretch goal.
The generated mixed workload is not an exchange-heavy grammar stress test.

[Wheel fade-out](layout-previews/exchange-final-wheel-coast.json) passes:
the thumb continues after release, slows from 171.46 to 54.26 px/s and settles
at y=204 in the final two samples. This pixel test does not resolve the known
zero-qualifying-input wheel-profiler attribution limitation documented in the
editorial checkpoint. No performance gate was weakened.

Larger/complex exchange objects, contextual status badges, code gutters,
all technical variants, the full interaction/RTL/contrast matrix and the
remaining grammar families are still incomplete. The full goal remains active.
