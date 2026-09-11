# Source-local HTML cell edits

September 10, 2026. Contract before implementation for work_db42e4aa5947ef08.

Preserve unaffected authored HTML table bytes when editing source-addressable
rich-cell text: metadata, wrappers, attributes, whitespace, neighboring cells,
images and quotes. Source addresses must follow structural order and be checked
against the existing canonical importer, not selected by matching repeated text.
Reuse the source spine and existing transaction/Undo model. Converted Markdown
offsets must never become original HTML offsets.

Prove source reuse before applying it. Preserve inline markup for text edits
when a verified local text patch represents the new canonical rich text; use
the existing HTML inline serializer for intentional formatting changes. Reject
uncertain source mappings rather than patching the wrong text. Test repeated
labels, quoted cells, entities, attributes, UTF-8, multiple edits, reopening,
Undo and the original fixture104 full-file native caption expectation.

## Implementation and regression

The minimized one-cell caption test reproduced whole-table rewriting before
the fix (`/tmp/tachyon-html-cell-red.log`). Imported rich cells are converted
through temporary Markdown; those offsets do not address original HTML.

`markdown::html_source` now extracts strict lexical paragraph-body addresses
and verifies them with the existing HTML importer. Addresses are attached to
the existing source spine. Cells and their leaves are paired in structural
order, never located by searching for matching text. Source-local serialization
patches only verified paragraph bodies. Ordinary text changes retain original
inline markup, attributes and entity spelling when their resulting rich text
is proven equal; intentional formatting uses the existing HTML inline
serializer, also verified before reuse. Named entities use html5ever's existing
entity table. There is no second semantic document, transaction path or cache.

Four new integration tests cover the minimal table and original fixture104,
empty cells, LF/CRLF, comments, quoted tag-like attribute values, blockquotes,
duplicate labels in different cells, styled/link text, numeric/named/multiple-
codepoint entities, UTF-8, sequential insert/replace/delete, formatting,
reopening and exact Undo. The fixture regression also checks retained table,
row/cell/block identities, metadata/column/header semantics and pointer-shared
unmodified image/quote/header/neighbor subtrees. One internal regression rejects
uncertain lexical input and distinguishes real comments from attribute text.

## Native evidence

Original fixture104 SHA-256 remains
`d4fe58ab8d9fbe62ae21620e6598669452974a7c68da761ecd26f2dc2ffdfd51`.
Runtime SHA-256:
`102e6280156b2f34cfaf727ed90f9eb726ffad8e2763d331f9f4c49193c2c877`.
Only tests/docs changed after this build.

Both 1600×1700 light native runs independently compare the entire edited file
against the original source with one literal character changed:

- `layout-previews/html-cell-fidelity-caption`: append `x` to the original
  narrow caption. This is the original stronger oracle that failed in
  CRAMPED-IMAGE-STATES-CHECKPOINT.md, not its weaker fragment-preservation check.
- `layout-previews/html-cell-fidelity-neighbor`: prepend `x` to the adjacent
  explanatory cell. All other caption, image, quote and table bytes remain.

Both prove one-second idle autosave and exact-byte Undo. Both idle PNGs were
inspected: captions and neighboring text remain readable, and the loaded SVG
retains complete proportions. AT-SPI failure-panel bounds/descriptions match
the preceding wide baseline exactly: (271,363,176,180) and (295,645,152,180).
Logs: `/tmp/tachyon-html-cell-native.log` and
`/tmp/tachyon-html-cell-neighbor.log`.

`scripts/check.sh` passes formatting, locked dependency checks, workspace/all
targets check, strict Clippy and tests: 117 core, 29 source-fidelity, 11 tree
selection, 500 document-view (2 existing ignored), 1 consumer, 39 app and docs.
Log: `/tmp/tachyon-html-cell-check.log`.

## Boundaries

This closes the reproduced fixture104 caption/adjacent-cell fidelity defect;
it is not arbitrary HTML source editing. Addresses currently cover paragraphs
in verifiable top-level HTML tables. Structural edits, changed heading/code
leaves, implicitly repaired/misnested HTML, raw/foreign/template content and
unverifiable cell/leaf correspondence retain canonical serialization fallback.
Unsupported inline whitespace normalization may require serialization of the
edited paragraph body. No uncertain lexical address licenses a source patch.
The full X01/media/layout/export/interaction matrix remains partial.
