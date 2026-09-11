# Entity records — September 9

This advances Board 04 / T03 beyond two-column property/value records. It does
not approve the complete grammar or all table/record orientations. The written
`designs/document-design-grammar.md` remains authoritative over raster lettering.

## Implementation

Independent entity directories can now use naturally sized labeled records
when measured columns become cramped. Recognition requires a named identity
column and a descriptive field, unique headers and nonempty unique identities.
The supported identity labels are Name, Service, Component, Resource, Document,
Package, Endpoint and Dataset. Description/Purpose/Summary/Responsibility/
Details/Notes provide the descriptive signal. Three to eight flat paragraph
columns are supported; multiple right-aligned comparison columns are excluded.

The existing record-fit gate uses native font measurements: usable width below
560 logical units, schema headers fitting the width, preferred table width
exceeding available width by 20%, and individual minimum widths fitting the
record inset. Explicit widths, complex/nested content, unsupported math and
strong RTL retain tables. Already compact directories and numerical comparison
tables remain aligned columns. These are conservative nomination boundaries,
not claims that the remaining variants have been implemented.

Each record keeps its source identity first, then fields in source order:

- Identity: 18/24 sans semibold; other fields: 14/21 reference typography.
- Paper surface, Rule border, 4 px corners, no content shadow, 24 px inset.
- 16 px after the identity, between fields and between natural-height records.
- Short repeated labels share a measured rail with a 16 px value gap.
- When that rail leaves insufficient value width, labels stack above values
  with an 8 px gap. Large text reflows before being reduced.
- The next major section retains 64 px leading space.

The original schema header remains visible and editable. Repeated field labels
refer to that header's canonical node and local byte ranges; they are not new
paragraphs, Markdown, clipboard entries or accessibility nodes. Empty values
retain their empty caret line. Status chips use the existing source-backed
status semantics, without inventing completion or filling missing values.

All canonical table/row/cell identities remain. The existing record hit-test,
selection and table-command geometry is reused. Dependent label geometry keys
include the original header allocation identity so header edits invalidate all
dependent label ranges. Prepared labels are scaled with document zoom and
glyphs use the existing shape cache; there is no source classification or
candidate search on the scroll path.

## Native visual regression found and fixed

The initial release `168314ae…` passed the projection-level lock test, but
native header growth made unchanged entity titles lose their semibold weight.
`entity-records-native-title-red.log` and `entity-records-title-oracle-red.log`
record the actual screenshot failure. The unchanged first title's ink width
changed from 136 to 130 pixels, with the same 18 px size.

A worker-publication assertion added to the existing header-edit test failed
at the same boundary (`entity-records-worker-title-red.log`). Immediate refresh
restored both frozen measurements and entity-title roles, while background
reflow assigned the raw lock and omitted the roles. Both now call the same
`install_table_layout_lock` method. No debug logging, new dependency, unsafe
code or lint exception was added.

The final native edit oracle compares all four original title glyph masks
with the focused, edited screenshot, allowing vertical growth but no changed
weight, missing title or duplicate title. It passes after the fix, including
three seconds of idle reflow: `entity-records-final-header-edit.titles.json`.
The long edited label remains in its frozen rail while focused; its temporary
word wrapping is visible rather than clipped. Blur releases obsolete semantic
classification and permits a fresh table arrangement.

## Final build and checks

Release SHA-256:
`4f10b9f57d6e4b65c2ae432e52f632d96dc020b1fd2d8c8b6510702560b79f33`.
Immutable native test binary:
`/tmp/mineral-entities-weston.QS8eZ6/tachyon-final`.
Build: `layout-previews/entity-records-final-build.log` (1m26s including lock wait).

`scripts/check.sh` passes: locked metadata, formatting, all-target workspace
check, Clippy with warnings denied, **575 Rust tests**, two existing ignored,
and doctests. Log: `layout-previews/entity-records-final-checks.log`.
The six focused property/entity tests pass in
`layout-previews/entity-records-worker-title-green.log`. Coverage includes
native measured rails/stacks, exact source/range order, negative comparisons,
header label growth, worker publication and existing property row-local edits.

All **85 Python verifier tests** pass in
`layout-previews/entity-records-python-suite.log`. Twelve new tests exercise
entity evidence rejection and the unchanged-title glyph-mask oracle. They
reject altered/missing values, duplicate schema content, wrong gaps/insets,
missing borders/label ink, invented empty-field content, changed title weight
and duplicated titles. `git diff --check` and Python compilation also pass.

## Native visual evidence

Personally inspected the final native images against Board 04 and the written
grammar, not just source constants:

| Specimen | Surface | Result |
| --- | --- | --- |
| [Wide](layout-previews/entity-records-final-wide.png) | 1600×1600, 100% | Entity, comparison and compact directory tables retain columns |
| [Narrow](layout-previews/entity-records-final-narrow.png) | 600×2400, 100% | Four records, aligned labels, retained empty steward and status chips |
| [Large text](layout-previews/entity-records-final-200.png) | 1150×4000, 200% | Four records with scaled insets/gaps; comparison headers wrap legibly |
| [Short](layout-previews/entity-records-final-short.png) | 600×600, settled scroll | One complete record, coherent clipping and outer scrollbar |
| [Stacked labels](layout-previews/entity-records-final-stacked.png) | 600×3000, 200% | Two records; four labels above their own values when a rail cannot fit |

Fixtures:

- [76-entity-records.md](layout-fixtures/76-entity-records.md), SHA-256
  `5c607ea0536a188cd6591478010394890b1cc75612408eb1ace34ab19c7395c9`.
- [77-stacked-entity-labels.md](layout-fixtures/77-stacked-entity-labels.md), SHA-256
  `bfd9e5b950d59cee0b4c143d3aeaed1eddf7f54f717891e1eaa466bce710962c`.

Each capture has matching `.source.json` and `.active-atspi.json` build, source
hash and dimension evidence. `entity_records_check.py` verifies all original
headers/values and their canonical paragraphs, actual label/value ink alignment,
24 px insets, 16 px field/record gaps, 64 px section spacing, Paper padding and
four Rule-edge samples. There are four complete pixel-checked records in each
narrow/200% frame, one in the short frame, and no record-pixel claim for wide
tables. The two stacked records separately pass
`stacked_entity_records_check.py`, including 8 px label/value spacing. These
sample checks do not prove every border pixel or rounded-corner curvature.

```sh
python3 performance/entity_records_check.py performance/layout-previews/entity-records-final-wide performance/layout-previews/entity-records-final-narrow performance/layout-previews/entity-records-final-200 performance/layout-previews/entity-records-final-short
python3 performance/stacked_entity_records_check.py performance/layout-previews/entity-records-final-stacked
python3 performance/entity_record_edit_check.py performance/layout-previews/entity-records-final-narrow performance/layout-previews/entity-records-final-header-edit
```

Weston MCP app **55** independently rendered the final binary at 600×1200.
Native pointer placement at (125,515) put the caret in the first purpose value.
Before/after trees are untruncated, contain 111 nodes and retain the same source
order. Evidence: `layout-previews/entity-records-final.weston.json`. The app was
stopped, the private source remained unchanged, and MCP was restored to
1600×1200. Initial apps 53/54 belong to the pre-fix evidence and are not final
build sign-off.

## Editing and clipboard

`entity-records-final-native-edit.edit.json` verifies native pointer insertion
of `x` at the first purpose value, autosave with **whole edited-file equality**,
three seconds of focused idle, blur and exact original bytes after undo.
`entity-records-final-header-edit.edit.json` verifies a 31-byte native paste at
the Purpose header with the same complete-source/undo checks. The corresponding
typed/idle/blurred/restored images remain available.

`entity-records-final-copy-unique.copy.json` passes eight native clipboard-order
markers, including the original Purpose header. Complete clipboard equality
and rich-MIME interoperability are not established by this marker test.
The first copy probe used the non-unique marker `Service`, which also matches
the opening title at offset zero. Its failure is retained as
`entity-records-final-copy.copy.json`; the corrected probe uses unique `Purpose`
without changing the copier or weakening the ordering check.

## Scrolling and remaining scope

The final release passes the aggregate >60 fps gate on 10,485,760 bytes of
generated mixed Markdown, 1600×1200 at 100%, isolated Weston GL / 120 Hz,
native continuous input for 60.0168 seconds:

- Average presentation: **109.703 fps**.
- Draw p50/p95/p99: 2.310 / 3.250 / **3.930 ms**; maximum 23.970 ms.
- Presentation p50/p95/p99: 9.069 / 9.814 / **10.346 ms**; maximum 43.680 ms.
- Four presentation intervals >=25 ms; no application draw >=25 ms.
- Input latency p99: 9.921 ms; maximum 42.828 ms.
- 18 missed refresh deadlines of 6,602 opportunities (0.273%).

Evidence: `layout-previews/entity-records-final-10mib.json`, `.log` and
`.source.json`. No Cargo build, other capture or MCP app was live before the
run. Accessibility was inactive. This is a current-build gate, not a controlled
improvement claim or proof that every frame meets 60 Hz. Grammar-heavy and
AT-SPI-active workloads, full wheel latency attribution and recovery remain open.

The final release also passes the real wheel-release decay check on fixture
02, whose scrollbar has enough raster resolution for this oracle. Thumb rows
progress 96, 140, 169, 196, 214, 228, 228 after release; early motion is
182.68 px/s, later motion 64.24 px/s, settled by the last two samples around
2.86/3.26 seconds. See `entity-records-final-wheel-coast.json` and its seven
native frames. This is visible coasting/decay evidence, not full input-latency
attribution or a claim about 10 MiB thumb-pixel resolution.

T03 remains **Partial**. Board 04's transposed option/criterion records, paired
record-card grids, complex/nested/RTL entities, exhaustive header and editing
states, and print/pagination variants are not complete. This checkpoint also
does not close the full grammar's remaining page masters, media, diagrams,
navigation/static translations or cross-family acceptance matrix.

Crusty validations completed for `ctx_dcef412895ae` (entity implementation,
task `task_b35fe5ac99c9cba9`) and `ctx_e5250d056176` (worker title correction,
task `task_2fa7f9480e5b484d`): 36 existing advisory findings, no new, worsened or
resolved findings, no newly blocking constraint. No full-goal completion claim.
