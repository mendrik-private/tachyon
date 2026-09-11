# Technical component alignment checkpoint

2026-09-09. Continues A07 under the user's layout priority. The complete audit
and layout milestone remain open.

## Change

The native review of fixture 79 found that adjacent configuration sections
shared a heading baseline, but their table and code borders were staggered by
24 px when one introduction wrapped. The shared-alignment guidance from the
native-app UX skill informed this correction: technical sibling rows now align
their first table/code border after measuring the headings and introductions.

The extra space belongs between introduction and component, not inside a code
panel or a table cell. Component dimensions, column widths, and authored source
order are preserved. Ordinary paragraphs, galleries, lists, and stacked sections
do not acquire this alignment rule. It does not invent a relationship between
otherwise unrelated content or expand technical-section eligibility.

`GroupMeasurement` records the first root component's top. The row planner
includes alignment padding before checking height limits and scoring, including
retained editing rows. `LayoutSlot` carries the alignment policy as part of the
existing geometry identity. `position_flow` resolves the actual shared anchor
from current shaped lines on complete or local row rebuilds; table borders,
code chrome, text, and following content therefore share the same geometry.
No paint-only offset, document mutation, new dependency, or theme change was
introduced. The additional work is bounded by the technical row, not scrolling.

## Regression evidence

`technical_sibling_components_share_a_measured_anchor` initially failed with
component tops 320.5 / 296.5 at logical canvas width 1632. Its expanded oracle
caught a second incomplete path: the retained editing candidate still predicted
284.5 px while its aligned realization required 308.5 px. That path now applies
the same measured alignment rule.

The test covers the original source, a longer heading, and a longer opposite
introduction at logical widths 1040/1312/1632, in ordinary and focused plans.
It asserts equal component tops, exact agreement of predicted and realized
column heights, idempotent positioning including table-row bounds, and exact
source serialization. Existing tests also cover narrow/short-window fallback,
source traversal, and bounded local typing/growth/undo versus full geometry for
technical introductions, code and table cells at 100/150/200% zoom.

## Native evidence

Final layout-validation binary:
`e6fb3ab3548887507d3b709ef87719499f2f272684e8ac301539e18ffaeaaaa6`.
Only test edits followed that build.

Private native captures are in `layout-previews/`:

| Prefix | Scope |
| --- | --- |
| `composition-technical-before` | Pre-fix 1600×1100 light, binary 891ad72b…: table/code tops staggered. |
| `composition-technical-qualified` | Final 1600×1100 light, 100%; aligned table/code, actual code insertion, autosave and exact undo. |
| `composition-technical-growth-qualified` | Final 1600×1100 light; paste 130 bytes into the left introduction, capture after idle while focused, autosave and exact undo. The columns stay fixed and both components move down 48 px together. |
| `composition-technical-150-qualified` | Final 1920×1200 light, 150%; component borders remain aligned with measured table wrapping and intact code chrome. |
| `composition-technical-narrow-qualified` | Final 600×1100 light; source-order stack, without cross-column alignment padding. |
| `composition-technical-200-qualified` | Final 1600×1100 dark, 200%, scrolled; source-order table/code stack with normal section spacing. |

Final screenshots, including edited states, were inspected. Source and private
appearance checks pass. Fixture 79 remains SHA-256
`4ff5072fca0cf1117ef312f493799c14f924b53033e86419ba1a484d87f26963`.
The edit oracles verify intended insertion and named protected fragments, not
the entire edited serialization; undo and final source checks are byte-exact.
Earlier `*-after`, `*-edit`, `*-narrow`, and `*-200` captures used intermediate
binary ad408fc8… and are not final qualification.

Reproduce the growth check:

```sh
python3 performance/capture-layout.py --fixture 79-technical-sections.md \
  --binary target/debug/tachyon --width 1600 --height 1100 \
  --appearance light --source-unchanged-check --atspi-active \
  --select 280 326 280 326 --selection-keys home --edit-check \
  --edit-paste 'Additional context belongs with these settings. Review the local configuration with the team before opening any shared documents. ' \
  --edit-within 'The configuration keeps the document local and the presentation adjustable.' \
  --edit-preserve '[document]' '## Notes for the team' --edit-idle-seconds 1 \
  --output performance/layout-previews/composition-technical-growth-qualified.png
```

The README's lower build/keyboard sections were also captured before this fix
as `composition-readme-lower-before` from a byte-identical private copy. That
review did not justify stretching the shortcut table or altering its source.
It is a reviewed holdout, not final-build whole-document qualification.

## Checks and limits

`scripts/check.sh` passes formatting, locked workspace checks, strict Clippy,
all-target tests and doctests; document-view has 431 passed and 2 ignored.
`git diff --check` passes. This checkpoint improves technical-row alignment,
not every mixed-content arrangement, all reference-family acceptance gates,
or release performance. Those requirements remain open.
