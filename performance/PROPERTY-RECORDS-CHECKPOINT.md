# Property records — September 9

This advances Board 04, T03 and table editing fidelity. It does not approve
the full grammar or equate one property/value layout with the board's complete
multi-attribute record vocabulary. The written grammar remains authoritative.

## Implementation and visual contract

Explicit property/value tables can now become source-ordered labeled records
when measured columns would be cramped. The canonical Table, rows, cells,
headers and editable paragraph identities remain unchanged. Layout does not
rewrite Markdown, duplicate headers into the document, or invent a value for
an empty field. The original two-column schema header remains visible and
editable above the records.

Recognition happens once during native table measurement. It accepts explicit
Property/Field/Key/Setting/Parameter/Attribute/Term headers paired with
Value/Description/Definition/Meaning, unique nonempty keys, ordinary borders,
and flat paragraph cells. Explicit column widths, nested tables/containers,
ambiguous comparison headers, duplicate keys, inline math and strong RTL keep
their existing table presentation. These exclusions retain the source; they
are not claims that those complete record variants have been implemented.

The candidate requires usable document width below 560 logical units, headers
that fit, preferred column widths exceeding available width by more than 20%,
and minimum cell widths that fit the record inset. Compact property tables
and comparison-sensitive tables remain tables. Genuinely unbreakable values
keep horizontal table overflow instead of being shrunk. All decisions use
loaded native fonts; scrolling does not scan source or rerun candidate search.

Records use existing foundation roles: Paper, a 1 logical px Rule border,
4 px corners, no shadow, 24 px insets, an 8 px label/value gap and 16 px between
records. Natural heights accommodate wrapped values and the editable empty
value line. The reference-table typography is 14/21 sans; property keys are
semibold in both record and table forms. The following major heading retains
64 px space. Zoom scales these document metrics consistently.

Each cell retains separate hit-test/selection bounds even though the two
cells now stack. Whole-record geometry paints the enclosure when a tall
record's label is outside the viewport. Record panels do not inherit zebra
stripes or internal table grid borders. Misleading horizontal column-edge
handles are suppressed in record form; canonical table commands remain.
Measured table classification stays locked while editing, and row-local
updates reproduce the full measured geometry. Blur allows reconsideration.

## Source fidelity and regression fixes

The original exact-source test exposed an unrelated serializer defect:
editing `a` in one GFM table cell regenerated an untouched sibling `Keep.` as
`Keep\.` and normalized source spacing. GFM cell spans now share the source
spine's lifetime alongside list and footnote spans. With unchanged table/row/
cell topology, serialization patches only the changed cell's original span,
retaining untouched delimiters, padding, cells, rows and reference definitions.

A conservative plain-text path preserves harmless punctuation. Rich or
syntax-sensitive edits use the existing inline serializer only for that cell.
Structural edits and missing/unsupported spans retain canonical serialization;
they never silently discard an edit. A line-break round-trip regression also
showed that serialized `<br>` inside a GFM cell was imported as literal HTML.
The importer now recognizes case-insensitive `<br>`, `<br/>`, and `<br />` as
cell line breaks, without changing ordinary raw HTML or inline code handling.

Five new Rust tests cover measured 280/420/760/1280-unit arrangements, original
source/range order, comparison and compact negative controls, exact edited
Markdown, focused classification, local/full geometry equivalence, undo,
CRLF, Unicode, empty cells, rich untouched siblings, pipe/HTML/math-like input,
cell line breaks and semantic reopening. The original minimal source failure
is retained in `layout-previews/property-records-fidelity-red.log`.

The full `scripts/check.sh` run passes **572 tests**, with two existing ignored:
formatting, locked metadata, workspace/all-target checks, Clippy with warnings
denied, all targets and doctests. See
`layout-previews/property-records-checks-complete.log`. The earlier
`property-records-checks.log` failed before the line-break fix; it is not a pass.

## Current native evidence

Fixture: [75-property-records.md](layout-fixtures/75-property-records.md).
SHA-256: `94cc3638be8e7bd42c1c25f588260f3304c6849d2f7bcb9d85a40f79e7c579b7`.

Final release SHA-256:
`a33c9286ac3b62417a336046b504a65b1766c4815fa18b92e8faa0f0c4a90d94`.
Both `target/release/tachyon` and the immutable test copy
`/tmp/mineral-properties-weston.vQROHL/tachyon-final` match this hash.
The optimized build completed in 2m21s:
`layout-previews/property-records-build-verified.log`. Earlier `first`/`final`
build logs and captures predate the complete source fix and are not sign-off.

Personally reviewed final native captures, compared with the written grammar
and Board 04:

| Capture | Surface | Observed result |
| --- | --- | --- |
| [Wide](layout-previews/property-records-final-wide.png) | 1600×1400, 100% | Property/comparison/compact tables retain their columns; property keys are semibold |
| [Narrow](layout-previews/property-records-final-narrow.png) | 600×1800, 100% | Five naturally sized records, visible schema header, retained empty value, compact comparison columns |
| [Large text](layout-previews/property-records-final-200.png) | 1150×3200, 200% | Five records with scaled insets/gaps and retained table relationships; later content remains in the native tree |
| [Short](layout-previews/property-records-final-short.png) | 600×600, 100%, settled scroll | Coherent viewport clipping and outer scrollbar; one complete record is pixel checked |

Matching `.source.json` and `.active-atspi.json` files establish current build,
unchanged fixture bytes and native row/cell/header semantics. The independent
`property_records_check.py` verifies exact labels and values, including the
empty value, 24/8/16 spacing, 64 px next-section spacing, actual glyph insets,
Paper padding, Rule-edge presence and retained comparison/compact columns.
The samples do not prove every corner's curvature or all border thicknesses.

```sh
python3 performance/property_records_check.py performance/layout-previews/property-records-final-wide performance/layout-previews/property-records-final-narrow performance/layout-previews/property-records-final-200 performance/layout-previews/property-records-final-short
```

All four captures pass. There are five fully visible pixel-checked records in
each narrow/200% capture and one in the short capture. The wide table evidence
is semantic geometry, not a claim to pixel-check nonexistent record panels.
Eight synthetic negative-oracle tests reject altered source/build/dimensions,
missing/fabricated values, reordered labels, duplicated tables, wrong insets
and gaps, missing panel boundaries and collapsed comparison columns.

Weston MCP app 51 exercised the same release at 1600×1200: native Zoom in
actions reached 200%, and an advertised outline action navigated to Comparison
remains aligned. `property-records.weston.json` records 83 document descendants
in unchanged source order across the three untruncated trees. At this width,
200% leaves 640 document units, correctly retaining table form.

Weston MCP app 52 separately rendered at 600×1200. Its measured five-record
layout and pointer caret placement were personally inspected. Before/after
click trees have the same 91 source-ordered nodes, neither truncated; see
`property-records-narrow.weston.json`. Both apps were stopped after inspection,
the private source hash remained unchanged, and MCP was restored to 1600×1200.

## Native editing and evidence-oracle correction

`property-records-native-edit.edit.json` passes real pointer placement at the
start of the first description, native `x` typing, autosave with **complete
edited-file equality**, three seconds of focused idle, blur and exact original
bytes after undo. The [idle edited state](layout-previews/property-records-native-edit-idle.png)
was personally inspected: only the intended description changes; neighboring
records retain their placement. Typed/blurred/restored screenshots are retained.

This check initially failed in a secondary target-marker oracle after the
stronger whole-file equality had already passed. The oracle incorrectly
required the original phrase to disappear, rejecting a valid insertion at
its start or end. A reduced test reproduced six endpoint failures before the
fix (`property-records-marker-red.log`). The extracted helper now recognizes
all insertion offsets while still requiring one unique original and edited
target. Negative tests reject missing/duplicate targets and edits elsewhere.
Exact edited-file and undo assertions are unchanged. The original native
scenario was rerun successfully after fixing the oracle.

`property-records-native-copy.copy.json` passes eight native clipboard-order
markers across the record and ordinary-table sections. Complete plain-text
clipboard equality and rich-MIME interoperability remain unverified.

All **73 Python verifier tests** pass in
`layout-previews/property-records-python-suite.log`; `py_compile` and
`git diff --check` pass. No production Rust changed during this final evidence
pass; these native checks use the previously fully tested release hash above.

## Scrolling

Current release, 10,485,760-byte generated mixed Markdown, isolated Weston GL
at 120 Hz, 1600×1200, 100%, native continuous input for 60.0168 seconds:

- Average presented rate: **109.219 fps**; the aggregate >60 fps gate passes.
- Draw p50/p95/p99: 4.567 / 5.816 / **6.717 ms**; maximum 33.260 ms.
- Presentation p50/p95/p99: 9.134 / 10.387 / **11.166 ms**; maximum 42.598 ms.
- Four presentation intervals and two application draws >=25 ms.
- Input latency p99: 10.813 ms; maximum 42.500 ms.
- 19 missed refresh deadlines of 6,574 opportunities (0.289%).

See `layout-previews/property-records-10mib.json` and `.log`. No cargo/rustc,
other capture, or MCP app was live before this run. This is a current-build
gate, not a controlled improvement claim or a guarantee that every frame
meets 60 Hz. Accessibility was inactive; grammar-heavy/AT-SPI-active workloads,
wheel-input timing attribution and recovery remain open.

An attempted 10 MiB wheel-coast pixel check is retained as
`property-records-wheel-coast.json` and seven frames. It fails: the scrollbar
thumb remains at raster row 58. Inspection of frames 0 and 4 proves content
moved after release, but this enormous document's thumb lacks sufficient
pixel resolution for the existing decay oracle. It is not counted as a pass.

The 148,655-byte fixture 17 retry is retained as
`property-records-resolved-wheel-coast.json`. It visibly slows (thumb rows
59,60,61,61,62,62,62), but still fails the existing minimum per-interval pixel
displacement gate; it is not counted as passing or silently overwritten.

With the existing 1,933-byte fixture `02-list-arrangements.md`, unchanged
wheel impulses, binary, 1600×1200 surface, and the original unmodified decay
oracle, `property-records-list-wheel-coast.json` passes. After release the thumb
moves through rows **96,140,169,196,214,228,228**. Early measured speed is
**182.19 px/s**, later speed **64.32 px/s**, and the final 2.86/3.26-second
samples are settled. Seven matching native frames retain the evidence. This
proves visible wheel coasting/deceleration on a resolvable document, separately
from the 10 MiB continuous-input frame-rate run; it does not resolve wheel
input-to-presentation profiling or large-document subpixel decay measurement.

## Repository workflow

Crusty consultation was repeated before the final evidence pass. Its old
September 6 dark/minimap steering is superseded by the user's newer explicit
light/no-minimap decisions and current written grammar. Prepared contexts:
`ctx_382ab8e249c9` (records), `ctx_339952c72c55` (GFM cell fidelity),
`ctx_bcaeec219021` (native verification and checkpoint), and `ctx_8ac12939670e`
(target-marker oracle). Each baseline contains 36 existing advisory findings.
No dependency, unsafe allowance or lint suppression was added.

Validation tasks `task_63f643ece1384c25`, `task_a41d54ba73088214`,
`task_9b0d98579a1dc48e` and `task_9a50dc57b04bbff4` all completed: 36 existing
advisory findings, none new, worsened or resolved, and no matched blocking
quality constraint. These static results supplement the actual test/native
evidence above; they do not substitute for it.

## Remaining scope

T03 remains **Partial**: Board 04's multi-attribute entity records, alternate
table orientations, repeated value/header relationships, complex/nested
content, complete RTL/localization and all state/print variants remain open.
The compact property fallback is not a substitute for them. Full border/corner,
selection/table-command, structural growth and interaction coverage also needs
completion. Broader lists, media, margin notes, diagrams, page masters,
pagination, semantic exports and navigation/state families remain in scope in
`DESIGN-GRAMMAR-COVERAGE.md`. No complete board family is signed off.
