# Specification beside its authored example — September 10

## Layout contract and baseline

Continues the full audit under the user's document-layout/space priority.
Native review of the complete README's opening and lower prose bands found
those compositions useful. The API-reference fixture03 exposed another gap:
a compact parameter table was stacked above a full-width request example,
leaving substantial unused width. Preserve both complete sections and their
authored identities, but let a demonstrably related specification/example
pair negotiate the existing native technical row.

Before: `layout-previews/layout-next-technical`, 1600×1600 light, runtime
`eee6271bf25e69d28721fe059ce9c1e727ee128c1099073436ac6a71922f196e`.
The original fixture03 remains unchanged, SHA-256
`075a04766b464776cb052a4c077be0e0817cd5b9d2de2520eb0d307777c36205`.

## Source-grounded composition

`adaptive/rows.rs` records adjacent specification/example relationships before
building units. Both must be complete bounded technical sections at the same
heading level and under the same parent. One contains exactly one simple table,
the other one non-math code block and an existing authored Example, Request or
Response role; optional accompanying paragraphs stay in their own unit.

At least two distinct first-column field identifiers must occur as complete
identifier tokens in the code. A substring or repeated duplicate field is not
enough. Initial identifiers are ASCII letters/digits/underscores with a valid
leading letter/underscore. The code is not executed, parsed into an invented
schema or rewritten. Explicit adjacent request/response counterparts take
precedence. Different chapters, heading levels, warnings, unrelated fields and
unlabelled examples do not acquire this new relationship.

The pair competes through existing Technical row widths, native measurement,
overflow/height/balance guards, scoring and responsive retention. The table
stays open; the named example retains its own enclosure, inset, syntax colors,
line numbers and Copy action. Existing component alignment is reused. No new
theme, renderer, font substitution, user-facing selector or dependency.

The fixture-backed test failed before implementation because the specification
had no shared row. Three new Rust tests now cover its measured placement and
editorial identity; both source directions and recognition negatives; and
width/height recovery at 100/150/200% measurement scales with large code edits,
complete painted code coverage and exact Undo. Mock GPUI shaping is not treated
as native font proof.

## Native evidence

Final runtime: `26416b6a68d3360ccea181ea18e76ffbb2de7dac67d1ae038e6600715ddc81fa`.
All runs use isolated Weston, display scale120, not physical desktop input or
release performance qualification. Prefixes are under `layout-previews/`.

- `specification-verified-wide`, 1600×1600 light: the 623px parameter table
  retains its exact native width/height. The request uses a 645px outer track;
  all six code lines remain intact. The technical row uses 99.92% of its
  1314px canvas and saves **295px** before Configuration. Table/code painted
  top edges align within one pixel; the example keeps its 24px title inset.
- `specification-example-regular` (1280×1600), `specification-verified-narrow`
  (768×1600), `-short` (1600×480), and `-200` (1600×1800 dark, 200% text)
  correctly stack because the paired code/geometry no longer fits. Regular
  capture uses the same final runtime, not an older implementation.
  `specification-verified-200-code` scrolls to the complete enlarged table and
  request; original-size review confirms all six code lines remain readable.
- `specification_example_check.py` checks all **157 canonical document nodes**
  across these states: IDs, parent paths, roles, names, descriptions and actions
  match the before capture. Independent pixels verify aligned borders and every
  code line; native bounds prove unchanged table size, occupancy and savings.
  Giving it the original stacked baseline as an improvement correctly fails.
- `specification-verified-table-edit` and `-code-edit`: native pointer/Home
  placement, typing, one-second focused idle and blur pass. Both require the
  entire autosaved Markdown to equal the expected file, then byte-exact Undo.
  The table change stays within its original inline-code cell; code whitespace
  and all unrelated source remain exact.
- `specification-verified-copy`: native select-all/copy preserves all five
  supplied markers in source order across table, example and following sections.
  This is a marker-order check, not a full clipboard golden.
- `specification-verified-readme`: whole README canonical semantics and all
  heading/prose/table/code bounds remain identical to `opening-verified-readme`.
  Its original SHA-256 `abca9115a24f8b4293074b510011acfc5d11a036f910ba68d77e6f0054527541`
  remains unchanged. The new rule does not rearrange unrelated prose or code.

## Verification and remaining work

`scripts/check.sh` passes: formatting, locked all-target checks, strict Clippy,
747 workspace tests plus five adapter tests (**752**, two existing ignored),
and doc tests. Log: `/tmp/mineral-specification-example-check.log`.
Seventeen capture-harness tests pass. Crusty `ctx_da07eb3e5ef2`, validation
`task_c925eb812f4ca3b1`, reports 75 existing advisory findings, none new,
worsened or resolved.

App-UX guidance informed shared content alignment and preservation of each
unit's visual identity; Rust guidance required recognition negatives and
source/edit/resize regression evidence. Generalized schemas/Unicode field
identifiers, richer/nested specification units, complete native continuous
resize/IME/RTL matrices and grammar/page/export/performance qualification
remain open. This is partial A07 progress, not completion of the full audit.
