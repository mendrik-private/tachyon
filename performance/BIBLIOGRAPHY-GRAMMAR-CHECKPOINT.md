# Bibliography grammar checkpoint — September 9

Scope: E10 and the bibliography specimen on board 01. This is progress toward
the complete design grammar, not a sign-off of all boards or editorial families.
The fixture's authors, publications and destinations are synthetic.

## Source-owned representation

An authored `Bibliography`, `Works cited` or `References` heading establishes a
citation section. Numbered chapter prefixes and `:`/`·`/`—` heading qualifiers
are accepted. Subheadings inherit the citation context; a peer or ancestor
heading ends it. Paragraph entries and flat ordinary/ordered lists keep their
canonical nodes, rich runs, order and authored numbering. Introductory labels
ending in a colon remain ordinary prose. Tasks, nested lists and technical
containers are not reinterpreted as bibliography entries.

Citation paragraphs use Liberation Serif 18/28, 24 logical px hanging
continuations, a loaded-font reading measure, and 12 px entry gaps. Existing
ordered lists retain their 32 px number/body rail; bullet citations retain
their 24 px rail. Numbers are quiet reference labels, not step badges. Section
headings retain 24 px following and 64 px preceding gaps. No card surface,
shadow, new numbering, sorting, citation-style rewrite or generated metadata is
introduced. Authored emphasis and underlined links remain intact.

The projection owns citation context; adaptive selection excludes those roots
from resource/feature/step arrangements and flowing prose bands. Measurement
and geometry cache keys include citation ownership. Focused citation roles
are retained through punctuation edits, while width changes still reflow text.
Native bibliography-entry roles preserve the actual list hierarchy; embedded
links do not mislabel an entire citation as a standalone resource link.

## Verification

- Five focused Rust tests cover section recognition/boundaries, nested/task
  exclusions, source numbering and exact untouched serialization, loaded-font
  glyph fit, hanging indentation, entry gaps, whole-fixture source order,
  serif runs, every paragraph/heading focus target, citation growth, retained
  editing roles, exact undo and native semantic entry hierarchy.
- Measured geometry exercises 1280 px, 420 px and 640 px logical canvases with
  100%/200% font environments. This is not by itself native pixel evidence.
- `scripts/check.sh` passes formatting, locked check, warning-denying Clippy,
  workspace tests and doctests; see `layout-previews/bibliography-grammar-final-checks.log`.
- Ten capture/titlebar harness tests pass. Crusty context `ctx_8cafdcc44983`
  validation reports 36 existing advisory findings, no new or worsened findings.
- Workspace results: 111 core, 9 source-fidelity, 11 tree-selection, 380 view
  and 40 app/bin tests pass (551 total; two existing ignored view tests).

## Native visual and editing evidence

Final release SHA-256:
`4ff76f75b5a4f8abd4b80261b7fe1024f995096a0e3cd8591b4a917a614efbc5`.
Fixture 71 SHA-256:
`908c1a6138849af4761825f7841f3b6c9ba047ece567a4335191272c22611420`.
The isolated immutable binary is
`/tmp/mineral-bibliography-weston.OzzaiY/mineral-markdown-final`.

The final screenshots were opened and visually inspected:

- `layout-previews/bibliography-grammar-wide.png`: 1600×1800, 100%; all eight
  citations, ordinary/numbered/bullet variants and final unindented prose.
- `layout-previews/bibliography-grammar-narrow.png`: 600×2400, 100%; navigation
  collapses, all citation entries retain their hanging/marker alignment.
- `layout-previews/bibliography-grammar-short.png`: 1150×600, 100%; two complete
  visible citations are pixel-checked, with all offscreen source/semantic
  relationships retained. The window does not shrink text to fit its height.
- `layout-previews/bibliography-grammar-200.png`: 1600×2400, 200%; logical
  24 px hangs become 48 px, 12 px gaps become 24 px. Seven whole entries are
  visible and pixel-checked; the eighth remains below the viewport.
- `layout-previews/bibliography-grammar-weston.png` and `-weston-focused.png`:
  actual Weston MCP app 42 at 1600×1200, including native outline navigation
  to Works cited and the end of the document. The matching `.weston.json`
  stores both native AT-SPI trees and proves all eight entry widths/heights
  stay identical on focus (528 px allocated reading columns at 100%).

`bibliography_grammar_check.py` passes against all four size/zoom captures.
It independently checks source/build identity, native role/order/line heights,
12/64 px gaps, actual glyph ink after the hanging rail, empty continuation
rails, and paper pixels without card tint. It does not infer a visual pass from
the renderer's own geometry helpers or silently accept a new golden.
The `-before` negative-control capture uses the prior quotation build
`3372f5fe…05d`; the same verifier rejects its absent citation entry roles.
That image was also inspected: it lacks hanging paragraph continuations and
uses ordinary sans-serif list typography and green markers instead of the
coherent serif citation treatment.

`bibliography-grammar-edit.edit.json` verifies real Wayland insertion inside
`Vale, M. (2025).`, autosave, unchanged named sibling source fragments, and
exact original bytes after undo. The typed screenshot was inspected. This is
not a claim that every edited-region punctuation byte remains untouched:
the existing serialization path can canonicalize the changed paragraph.
`bibliography-grammar-copy.copy.json` verifies unique clipboard markers for
all eight authors in source order; it is a marker-order check, not a claim of
full rich-MIME interoperability or exact full-document clipboard equality.

The first native inspection exposed a pre-existing whole-resource link role
being applied to citation paragraphs with embedded links. The final build
corrects that role and has a dedicated semantic regression. The earlier
feature-label font leak also has a failing-before/passing-after regression.

## Performance

`layout-previews/bibliography-grammar-10mib.json` records the final release on
10,485,760 bytes of generated mixed Markdown, native bidirectional continuous
input, isolated Weston 14 GL at 1600×1200/120 Hz, five seconds warmup and
60.020 seconds observed duration. Accessibility was inactive. No repository
build or other Mineral capture was running during this benchmark.

The >60 fps gate passes at **107.20 presented fps average**, with draw p99
**9.85 ms** and presentation p99 **13.41 ms**. This is not a claim of every-frame
60/120 Hz: ten presentation intervals were at least 25 ms (maximum 69.01 ms),
two draws were at least 25 ms (maximum 32.42 ms), and 2.47% of configured
120 Hz refresh opportunities were missed. Input p99 was 12.94 ms, maximum
68.75 ms. These tails are retained, not discarded or called an improvement
over the earlier quotation benchmark. Scrolling code was not changed.

The final binary also passes the unchanged native coast protocol on the longer
figure-caption fixture 69 at 1600×1200. In
`bibliography-grammar-continuous-coast.json`, scrollbar-thumb speed declines
from 394.11 to 101.24 px/s, then settles to a one-pixel change over the last
350 ms. In `bibliography-grammar-wheel-coast.json`, it declines from 144.69 to
60.33 px/s, then settles with no final movement. Both retain their full sampled
positions/timestamps and binary hashes; these measure fading scroll motion,
not a frame-rate guarantee. The two bounded coast checks ran after the separate
10 MiB throughput measurement.

## Remaining scope

Arbitrary citation schemas/import formats, citation cross-reference resolution,
complex annotated/nested bibliographies, multiline inline math, long numeric
labels, RTL and the complete accessibility/interaction matrix are not approved
by this specimen. Pagination, bibliography page masters and print continuations
remain part of the full grammar goal. Margin notes, technical/media families,
page/export implementation and the remaining coverage ledger remain open.
