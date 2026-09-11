# Literal signals — September 9

This advances C07 and Board 03, not the full grammar completion claim. The
written grammar remains authoritative. Page masters, exports, missing media,
remaining relationship variants and the complete state/RTL matrix remain open.

## Authored meaning and visual rules

Short values in explicit Status, State, Classification and Maturity columns,
Property/Field/Key–Value rows, and recognized document metadata can receive
badges. Draft, Mandatory and unknown classifications are neutral: a requirement
is not a completed result. Accepted/Approved/Ready are positive, Beta is
informational, In review/Pending are warnings, Deprecated/Failed are critical.
No label, outcome, icon action or status is added to source. Ordinary prose,
owner fields, links and long sentence-like values are not converted to badges.
Wrapped values stay readable text instead of fragmented pills.

Badges use the existing semantic foreground/background roles, medium-weight
authored text, 8 px horizontal and 4 px vertical inset, a 1 px outline and
concentric pill corners. Metadata accounts for the horizontal inset during
measurement and retains it in the edit-locked placement. There is no manual
layout preference, invented checkbox or color-only state.

Explicitly named Color/Colour/Color token H2-or-deeper objects with a literal
hex paragraph and one or two contextual paragraphs receive color-token cards.
CSS #RGB, #RGBA, #RRGGBB and #RRGGBBAA forms preserve exact case, shorthand and
alpha in editable monospace text. The source supplies the name and explanation.
No color names or invalid values are guessed. Alpha previews over Paper;
transparent and paper-colored tokens keep a visible Rule outline.

Cards have natural height, Paper fill, 1 px Rule borders, 4 px corners, no
shadow, 24 px insets/gutters, 18/24 serif labels, 14/20 literal/context type and
8 px internal gaps. A square swatch is 48 px in a >=360 px card, 24 px in a
compact card, with a 24 px text gap. Actual native glyph widths determine peer
fit; a peer template cannot split the hex literal. Narrow layouts stack in
source order with 24 px gaps. Actual major headings retain the 64/24 rhythm.

## Editing and regressions

Recognition is prepared with the projection, never by scanning headers or
parsing values on each scroll frame. Geometry cache keys include signal roles.
The named value's role/track remains stable while a literal or label is being
edited. A malformed in-progress literal keeps its position but clears the
preview immediately; blur can release the object. Structural edits release
ownership rather than attach unrelated paragraphs.

The first actual pointer edit regression found a stale color in the existing
single-leaf refresh path. Full projection tests passed, but real editor typing
retained the prior RGBA despite correct source bytes. The fast path now updates
the current literal and badge data. This is covered by a red-before-fix native
editor test with complete edited-source equality and exact undo.

The first header-invalidation implementation was too broad: a two-column
status value and an unrelated Setting header triggered whole-tree rebuilding.
The existing row-local geometry test caught this. Invalidation now applies
only when an edited field/header can change sibling signal meaning. Both the
new header/value regression and existing nested-table geometry test pass.

Native review exposed two further issues. The sum of metadata text widths did
not account for rounding individual fields onto twelve tracks, causing a
needless vertical fallback on a wide canvas. A red geometry assertion now
requires the short strip to fit. The solver chooses the smallest feasible
track-boundary width from at most 72 candidates without reshaping or a
per-pixel search. Separately, the text-line clip cut the top/bottom from table
pills. An independent pixel assertion samples the required 4 px padding
outside the native glyph bounds and fails on that intermediate build. Badge
chrome now uses the complete cell/viewport clip; text keeps its own line clip.

Seven new tests cover exact hex decoding, semantic positive/negative cases,
short/unknown status classifications, header invalidation, source equality,
native-shaped wide/narrow/short/200% geometry, focused invalid literals,
pointer hit testing and exact undo. `scripts/check.sh` passes all 567 workspace
tests (two existing ignored), formatting, locked metadata/check, Clippy with
warnings denied and doctests. Final run: `layout-previews/signals-grammar-checks-complete.log`.

## Native evidence

Fixture: `layout-fixtures/74-literal-signals.md`.
SHA-256: `45ba2891cd81fec86ba96feb69116272ee68bf785d8177f7b1338269a6f9dac2`.

`signals_grammar_check.py` independently checks source/build identity, native
source order, actual label/literal leading, 8 px inner gaps, 24 px insets and
modular gaps, 64/24 heading rhythm, square swatch pixels (including alpha),
literal glyph ink and single-logical-pixel boundaries. The old metric build
`774f5e58` is captured in `signals-grammar-before.png`: it fails the label
geometry oracle and visibly has no swatches. It is not an accepted golden.

The initial candidate was personally inspected through Weston MCP app 48;
the app was stopped after read-only inspection. Intermediate build `ac4f315e`
was inspected through app 49, including a real advertised outline action to
the status registry. It passes swatch checks but fails the subsequently added
metadata-strip and complete-pill checks; those captures are not sign-off.
Final release SHA-256:
`e8110201343775eb6b7c482697879b131d9bd2a87324c9e9c71a75eb10af65c5`.
Immutable binary: `/tmp/tachyon-signals-weston.Zhqmoe/tachyon-final`.
The optimized build completed successfully in 2m32s; see
`layout-previews/signals-grammar-build-complete.log`.

Personally inspected final captures, all with matching source/AT-SPI/pixel
reports and unchanged fixture bytes:

- `signals-grammar-final-wide.png`, 1600×2200, 100%: measured three/two-card
  groups, compact metadata strip, all five swatches and nine table badges.
- `signals-grammar-final-narrow.png`, 600×3600, 100%: navigation collapses;
  five source-ordered cards stack; all badges and negative controls remain.
- `signals-grammar-final-200.png`, 1600×3800, 200%: the first trio stacks, the
  two compact literals pair, and labels/values/context stay legible. All nine
  table badges are pixel checked. Later negative controls are below the
  screenshot but present in the native tree.
- `signals-grammar-final-short-settled.png`, 1150×600, 100%, after a 100 px
  native scroll and a three-second settling wait: the compact first three
  cards are fully visible, with their 24 px swatches and natural heights.
  Table badges are offscreen and are not counted as pixel-verified here.

The first short capture sampled AT-SPI after 600 ms while momentum still
moved the document: screenshot geometry differed by two pixels. It failed
the oracle and is retained as `signals-grammar-final-short.png`. The capture
harness now offers a bounded `--scroll-settle-seconds` option; the rerun uses
three seconds without weakening the pixel tolerance. This is static visual
evidence, not a scroll-latency measurement. Nine harness unit tests pass.

The complete final matrix passes:

```sh
python3 performance/signals_grammar_check.py performance/layout-previews/signals-grammar-final-wide performance/layout-previews/signals-grammar-final-narrow performance/layout-previews/signals-grammar-final-200 performance/layout-previews/signals-grammar-final-short-settled
```

Weston MCP app 50 renders the same final binary. An advertised outline click
to Status registry succeeds. `signals-grammar.weston.json` records 98 native
document nodes before/after: names, source order, widths and heights remain
identical; y positions translate uniformly by -661 px. Neither tree is
truncated. The private source hash remains unchanged; the app was stopped.

`signals-grammar-final-edit-exact.edit.json` verifies an actual pointer click
inside the first hex literal, native typing, autosave with **complete edited
Markdown equality**, three seconds of focused idle, blur and exact original
bytes after undo. The typed screenshot was personally inspected: `#3xF6247`
is visible in its original track and the invalid preview is blank, not stale.
At (324,454), the swatch is (63,98,71) before editing, Paper (250,249,246) while
typed and idle, and (63,98,71) after undo. The card top at (500,406) remains
Rule (218,221,213) in all four captures. Both neighboring literal sources are
byte-exact. The initial edit invocation accidentally passed literal backslash-n
characters in its marker and failed with marker_count=0; it is not a pass.

`signals-grammar-final-copy.copy.json` passes nine native clipboard order
markers, including original shorthand, alpha and the invalid negative control.
Full rich-MIME/native clipboard equality remains unverified (`exact_clipboard`
is null); do not confuse marker-order proof with that broader requirement.

## Scrolling and workflow evidence

Final build `e8110201`, 10,485,760-byte generated mixed Markdown, isolated
Weston GL at 120 Hz, 1600×1200, native continuous scrolling for 60.0209 seconds:

- Average presented rate: **108.962 fps**; the >60 fps aggregate gate passes.
- Draw p50/p95/p99: 5.341 / 6.103 / **7.176 ms**; maximum 17.416 ms.
- Presentation p50/p95/p99: 9.142 / 10.412 / **11.518 ms**; maximum 46.137 ms.
- Two presentation intervals >=25 ms; no application draws >=25 ms.
- 22 missed refresh deadlines of 6,562 opportunities (0.335%).

See `layout-previews/signals-grammar-10mib.json` and `.log`. Our builds,
capture sessions and MCP apps were stopped first; the cargo/rustc process
inventory was empty immediately before the run. This is a current-build gate,
not a controlled performance-improvement claim. Accessibility was inactive
for this run; grammar-heavy and active-AT-SPI performance remain open.

Crusty preparation `ctx_091db0966309`; validation
`task_4dfc423cc36268bd` completed with 36 existing advisory findings and none
new, worsened or resolved. An earlier validation failed because the live tree
changed during analysis; it was retried after implementation/captures settled.

## Remaining coverage

The named card and explicit field forms do not yet cover arbitrary inline hex
swatches, CSS functional/named colors, every nested or raw-HTML signal encoding,
standalone badge strips, or every compact/side-by-side internal card template.
Native rich-MIME interchange, exhaustive RTL/localization/state combinations
and paged/print translations remain unverified. Keep C07 partial until those
requirements have authoritative evidence; do not shrink the full objective.
