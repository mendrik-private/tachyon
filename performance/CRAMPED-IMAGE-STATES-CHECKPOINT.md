# Image states in narrow containers

September 10, 2026. E01/N06 contract before implementation.

An image's local width is not a reason to discard its status and alternative
description when its reserved height can contain them. Preserve the full image
reservation and canonical source, including rich-cell/quote insets, authored
table widths, captions and neighboring text. Use tighter padding and wrapping
in a narrow state panel, not smaller type or a wider parent column. Recovery
controls must stay inside that panel and scale with reader zoom.

Retain bounded metadata previews with complete accessible descriptions; do not
turn preview truncation into canonical source truncation. Reuse actual toolkit
loading/failure callbacks and the existing Retry command. Do not infer a failed
image from unknown dimensions or add loader/layout state. Verify ordinary and
quoted table cells, widths around the old breakpoint, long alternatives,
100/200% text, native source/edit/Undo and a loaded-image control.

Very small known-size media need a separate state/inspection presentation;
this checkpoint does not qualify metadata inside every possible tiny image.

## Result and regression

The initial native fixture showed only Retry in both narrow reservations.
`cargo test -p document-view narrow_cell_keeps_visible_description --locked`
failed because the actual failed-image callback created no visible title or
alternative-description elements. Log: `/tmp/mineral-cramped-image-red.log`.

The former 240 px width gate discarded metadata even with 180 px available
height. Narrow panels now retain the same content roles, with 8 px rather than
16 px insets. Description line allocation uses the remaining height after
reserving the title, destination, button and gaps; it remains a bounded preview,
with the complete alternative in the accessible description. Controls cannot
extend beyond the panel's width. No image sizing, table sizing, loading state,
canonical model, font scale or Retry command path was replaced.

One new actual-render regression covers 60 combinations: authored column widths
160/200/263/264/265, ordinary/quoted cells, 100/200% zoom through the real zoom
method, and ordinary/long/multiscript alternatives. Visible title, description
and Retry bounds remain inside the unchanged image reservation, with exact
source bytes retained. Long/multiscript coverage here is geometry, not a claim
that all scripts' native glyphs were visually qualified.

## Native evidence

Fixture `104-cramped-image-states.md` SHA-256:
`d4fe58ab8d9fbe62ae21620e6598669452974a7c68da761ecd26f2dc2ffdfd51`.
Final binary:
`fa4a39b660fe6acf2bb6a4352392ca7854acfe7491b01d8f4c0dce1ad61a6fc9`.
Only test whitespace cleanup followed this build; focused tests were rerun.

Inspected native PNGs in `layout-previews/`:

- `cramped-image-before`: 1600×1700 light, old runtime `fc4fb7a…`; both narrow
  image reservations contain only Retry.
- `cramped-image-final-wide`: 1600×1700 light; complete short descriptions in
  ordinary and quoted cells, contained controls, same captions/neighboring text,
  loaded SVG at its complete normal proportions.
- `cramped-image-final-narrow`: 600×1700 light; navigation collapsed, authored
  column widths retained, status content and the complete loaded figure fit.
- `cramped-image-final-dark-200`: 1280×1700 dark at 200%; complete short
  descriptions and scaled controls remain within both image columns. The wide
  authored table retains horizontal overflow; its offscreen right-hand text is
  not claimed visible without panning. The loaded SVG is below this viewport.
- `cramped-image-body-edit`: full edited-file equality against an independent
  one-character paragraph mutation outside the table; one-second idle autosave
  and exact-byte Undo. This keeps every authored HTML table byte intact.
- `cramped-image-caption-content-edit`: targeted caption insertion, six protected
  image/alt/following-content fragments, one-second idle autosave and exact-byte
  Undo. This is NOT full edited-source equality; see the open fidelity gap below.

The final wide/narrow/200% captures preserve all original source bytes. Before
and final wide AT-SPI failure groups have identical bounds and complete
descriptions: 176×180 and quoted 152×180 logical px. Final content and both
edit-idle PNGs were inspected.

`scripts/check.sh` passes: formatting, locked dependency checks, workspace/all
targets check, strict Clippy, 500 document-view tests (2 existing ignored),
116 core, 25 source-fidelity, 11 tree-selection, 1 consumer, 39 app and doc tests.
Log: `/tmp/mineral-cramped-image-check.log`. Focused final tests also pass.

## Open source-fidelity finding and remaining scope

Follow-up: [HTML-CELL-FIDELITY-CHECKPOINT.md](HTML-CELL-FIDELITY-CHECKPOINT.md)
fixes the reproduced caption defect and reruns the original whole-file oracle
successfully, plus an adjacent-cell edit. The historical failure below remains
as the regression record; arbitrary HTML editing is still not fully qualified.

The stronger `cramped-image-final-caption-edit` oracle expected exactly the
authored source with `x` appended to the caption. It failed: the existing rich
HTML-table serializer rewrites the entire edited table, including decimal width
metadata, indentation, header paragraph wrappers and removal of the original
`thead`/`tbody` wrappers. Actual text/image references remain, but preservation
of unaffected authored markup is not proven and is contradicted by this check.
Failure log: `/tmp/mineral-cramped-image-final-caption-edit.log`.
`document-core/src/markdown.rs::serialize_html_table` is the current whole-table
serialization path; no core change was made in this layout pass. Do not replace
the stronger oracle with the weaker content/Undo check or mark this gap closed.
The next fidelity task should preserve source-local rich-cell text edits and
rerun this original whole-file expectation.

E01/N06 and the full audit remain partial. Very small known-size images,
accessible inspection of long previews, inline/linked media, native successful
retry and post-replacement focus, live announcements, export and full mixed
state/interaction coverage remain open.
