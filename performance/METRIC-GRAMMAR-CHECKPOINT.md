# Contextual metrics — September 9

This advances C06 and Board 03. It does not approve the complete card family
or the full document grammar, and does not remove any remaining scope.

## Authored meaning and presentation

An explicitly named `Metric: …` H2-or-deeper section with a compact quantitative
paragraph and one or two contextual paragraphs becomes a source-owned metric
object. The original heading, value, units, separators, context and inline
formatting remain canonical and editable. Ordinary numeric paragraphs,
unlabeled quantities, missing context, and sections merely discussing metrics
do not receive this treatment. No percentage, denominator, period, baseline,
trend, success state or number is calculated or invented.

The view uses a 18/24 serif label, 40/44 serif value and 14/20 sans context,
8 px internal gaps, 24 px insets, 1 px Rule borders, 4 px corners, Paper fill
and no shadow. Roomy cards have a quiet 48 px document-symbol tile, with a
24 px symbol and 24 px gap before text; compact tracks omit that decorative
rail before compromising text measure. The symbol is not a completion check
or progress indicator and does not add an accessible action.

The existing native measured row solver arranges matching siblings in source
order, using actual glyph widths and natural heights. Peer candidates that
split a metric value are rejected. Card geometry is not rebuilt during scroll.
Stacked sibling metrics use the same 24 px modular gap as peer gutters; major
section spacing remains 64 px. Typography and card tracks are retained while
editing an incomplete value or label; structural changes release the object.
All preferences remain view-local, with no Markdown layout annotations.

## Tests and discovered issues

Four new tests cover semantic recognition/rejection, locale-shaped literal
values, units and context, native-measured typography/fit, three-column and
narrow/short/200% layouts, the modular stack gap, incomplete typing/blur,
real pointer caret placement, full edited-source equality and exact undo.
Shared geometry keys include metric roles, so reused geometry cannot silently
retain ordinary body metrics after a role change.

The pointer regression initially compared literal serialized `7x5%` with the
serializer's equivalent `7x5\%`. Inspection proved the click hit the correct
node/offset. The final assertion checks canonical text and the entire edited
Markdown with its existing conservative escape policy, plus exact original
bytes after undo. It does not weaken the source-ownership requirement.

The first native narrow capture exposed a genuine 40 px gap inherited from
the next metric's H3. A geometry regression failed before fixing the shared
gap policy. Only adjacent metric siblings with the same heading parent/level
use the 24 px module gap; actual chapter boundaries keep the heading rhythm.

## Evidence protocol

Fixture: `layout-fixtures/73-contextual-metrics.md`.
The baseline `metric-grammar-before.png` is from equation build `cb05beb8`.
It fails the independent metric oracle because the value is still 24 px high
instead of 44 px. The initial candidate `8958fa94` passes wide pixels but
fails the narrow 24 px stack-gap assertion (actual 40 px).

`metric_grammar_check.py` checks unchanged source/build identity, complete
native label/value/context order, 44 px value leading, 8 px internal gaps,
24 px module gutters and insets, 24/64 px heading rhythm, actual visible glyph
ink, hairline border pixels, quiet paper corners, and untreated negative
controls. It is not a candidate-generated image golden.

Final release SHA-256:
`774f5e588b81369c01af4870d0c971f911590a71d2f5ae60fb68d842cb4c1585`.
Fixture SHA-256:
`ffe74bc75fe5aedbfd864359c05c8d9f3d02859942672c37edd0ae516dac78ca`.
Immutable native binary: `/tmp/mineral-metric-weston.AT7TBa/tachyon-final`.
`scripts/check.sh` passes locked metadata/check, formatting, Clippy with
warnings denied, all workspace tests and doctests: **560 tests pass**, two
existing view tests ignored. See `layout-previews/metric-grammar-checks.log`.
Crusty context `ctx_87beda28e3fe`, final validation
`task_b827a3d2901c2e4d`: 36 existing advisory findings, none new/worsened.

Personally inspected final native screenshots in `layout-previews/`:

- `metric-grammar-wide.png`, 1600×1800, 100%: compact three-card overview,
  two context-bearing cards, all values and negative controls visible.
- `metric-grammar-narrow.png`, 600×2600, 100%: all five natural-height cards
  stack with the corrected 24 px gaps; every source paragraph remains present.
- `metric-grammar-200.png`, 1600×2800, 200%: all five metric objects visible;
  first trio stacks, the shorter final pair fits at real large-text sizes.
- `metric-grammar-short.png`, 1150×600, 100%: all three initial cards fit in
  the viewport with natural, unequal heights; remaining content is scrollable.

All four have matching `.source.json`, `.active-atspi.json` and `.pixels.json`
and pass the independent checker:

```sh
python3 performance/metric_grammar_check.py performance/layout-previews/metric-grammar-wide performance/layout-previews/metric-grammar-narrow performance/layout-previews/metric-grammar-200 performance/layout-previews/metric-grammar-short
```

Actual Weston MCP app 47 verifies native rendering and activation of the
authored metric heading from the outline. `metric-grammar.weston.json` records
the complete before/after native source order and bounds: sizes/leading edges
remain identical, with a uniform -171 px navigation scroll translation. The
private fixture hash remains unchanged. The app was stopped normally.
Raw MCP pointer/key injection was also attempted, but the captured caret did
not establish placement inside the value; it is not counted as typing proof.

`metric-grammar-copy.copy.json` passes eight ordered native clipboard markers,
including locale separators, units, context and untreated controls. Its exact
clipboard field remains null: this is not full rich-MIME interoperability
proof. The initial native edit check used the non-unique marker `75%` (also
present in ordinary prose), so its strict marker oracle rejected the run even
though the changed bytes show the correct `7x5\%` target. The unique-marker
rerun is recorded separately; neither failed attempt is called a pass.

`metric-grammar-edit-unique.edit.json` passes native typing/autosave, exact
original bytes after undo, retained neighboring `128 ms` / `€1.234,50` source,
three seconds of focused idle, and blur. The unique marker includes the value's
surrounding newlines. The typed screenshot was personally inspected and shows
`7x5%` at the original value position with the three-card track unchanged.
Native edited-file equality is not claimed (`expected_edited_source_exact`
remains null); the full edited-source equality assertion is in the pointer
unit regression. Both proof scopes are recorded rather than conflated.

## Scrolling evidence

The final binary completed a 60.024-second continuous native 10 MiB mixed
Markdown run on a 120 Hz isolated Weston GL output: **107.56 fps**.
Presentation p50/p95/p99 are 9.16/11.04/12.93 ms; draw p50/p95/p99 are
6.07/8.30/9.47 ms. Three presentation intervals reached 25 ms (maximum
51.54 ms); no draw reached 25 ms (maximum 22.81 ms). The >60 fps average gate
passes, not a tail-free or sustained 120 fps claim. See
`layout-previews/metric-grammar-10mib.json` and its log.

Our builds and native interaction sessions were terminal before this run;
no Cargo/rustc was active at the launch check. Another build was observed
during the run. This is not an uncontended comparison or evidence of a
performance improvement. Accessibility was inactive, and the generated mixed
workload is not an exhaustive metric-heavy or grammar-heavy document.

## Remaining scope

Additional authored metric encodings (list/property/table forms), complex or
nested quantitative objects, explicit unit/value subtypography, comprehensive
locale/RTL and extreme-length behavior, every focus/state transition, and
semantic paged/print translation remain incomplete or unverified. The parser
recognizes a literal shape; it does not validate the truth of authored data.
The entire remaining coverage ledger and persistent goal stay active.
