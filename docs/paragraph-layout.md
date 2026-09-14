# Paragraph line breaking and justification

Investigated 2026-09-14 after the justification screenshots from
`IMPLEMENTATION_SPEC.md`.

## Current correction

Justified paragraphs keep their naturally short final line. The final-line
balancer previously moved words from the preceding line to make the last two
lines more similar in width, without accounting for the resulting stretched
word gaps. `FontMeasurement::wrap` now excludes justified prose from that
balancer. Painting already excludes final lines and authored hard breaks from
justification. The regression test reproduces the word movement at 425 logical
pixels and checks the corrected breaks over widths from 300 to 695 pixels.

## Cargo library assessment

These are integration candidates, not newly installed dependencies.

| Crate | Capabilities | Fit here |
| --- | --- | --- |
| [paragraph-breaker 0.4.4](https://docs.rs/paragraph-breaker/0.4.4/paragraph_breaker/) | Knuth–Plass paragraph optimization with boxes, stretchable/shrinkable spaces, and discretionary penalties; returns breakpoints and adjustment ratios. | Best algorithmic match for fuller justified-paragraph composition while keeping GPUI font shaping. Small, old release with sparse API documentation; requires careful integration and validation. |
| [textwrap](https://docs.rs/textwrap/latest/textwrap/wrap_algorithms/fn.wrap_optimal_fit.html) | Paragraph-wide optimal fit with custom floating-point fragment widths and configurable penalties. | Convenient integration, but its objective penalizes leftover line width rather than space stretch/shrink ratios. Default penalties are intended for monospace text. |
| [Parley](https://docs.rs/parley/latest/parley/) | Shaping, bidi, alignment, and editing geometry; justification excludes the final line. | A much broader text-stack migration. Its [current line breaker is greedy](https://docs.rs/parley/latest/src/parley/layout/line_break.rs.html), so adoption alone would not provide Knuth–Plass composition. |

For a future paragraph composer, evaluate `paragraph-breaker` using GPUI-measured
word and hyphen widths and original byte ranges as item metadata. Its
[implementation](https://docs.rs/paragraph-breaker/0.4.4/src/paragraph_breaker/lib.rs.html)
uses integer widths and can return no feasible solution: bound conversion and
paragraph sizes, verify final shaped widths, and define a deliberate fallback.
The final paragraph line should have flexible trailing space, not a requirement
to fill the column. Preserve Unicode break rules, selection mappings, and the
existing virtual hyphen behavior.

The [textwrap Fragment API](https://docs.rs/textwrap/latest/textwrap/core/trait.Fragment.html)
can also preserve byte ranges with custom measured widths. Its
[penalties documentation](https://docs.rs/textwrap/latest/textwrap/wrap_algorithms/struct.Penalties.html)
explicitly calls for font-size-adjusted penalties for proportional text and
documents that finite overflow penalties may allow overflowing lines. Neither
library removes the need to validate the final font-shaped geometry.
