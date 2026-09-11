# Paged HTML export checkpoint — September 11, 2026

## Result

Tachyon now has a production **Export paged HTML…** command in the application
menu and a `Ctrl+Shift+E` shortcut. It exports the current immutable document
snapshot on a dedicated worker and writes the result atomically. Export does
not change the Markdown source path, saved revision, recovery journal, undo
history, selection, or editor state. If editing continues while the worker is
running, the completion message identifies the output as an earlier revision.

The output is a standalone, inert semantic HTML document with A4 or Letter,
portrait or landscape page options in `document-core`. The application command
currently uses A4 portrait. It supplies print margins, a running document title,
`page / pages` folios, heading/orphan/widow break hints, static tasks, expanded
authored disclosures, language labels for code, semantic table sections, and
repeated table headers. Application navigation, toolbars, search, copy controls,
and other interactive chrome do not enter the document body.

Static export is a read-only projection of the canonical snapshot. GFM heading
anchors, including duplicate suffixes, are generated in canonical source order,
so internal Markdown links remain live in the export. Unsafe resource schemes
are omitted. Preserved HTML passes through the existing inert sanitizer, with a
visible escaped source fallback when it cannot be exported safely.

## Table-width policy

Explicitly saved table widths are retained as exact resolved column values in a
fixed-layout `<colgroup>`; they are not stretched to the prose width. In the
checkpoint fixture, the saved `380.078125 / 230.203125` logical-pixel columns
export as the corresponding internal `f32` values `380.07813 / 230.20313`, with
a total table width of `610.28125px`. Tables without saved widths remain
automatically sized and aligned by normal table layout.

## Paged-renderer evidence

The deterministic checkpoint source concatenates fixtures 27 and 125 without
editing either file. Its SHA-256 is
`1430aa4e33185641f781f17bfb3b83d03329bf16204c0c8cbb8fc9ab83d81077`.
The export harness asserts byte-identical canonical Markdown before and after
projection.

WeasyPrint 67.0 rendered
`layout-previews/paged-export-checkpoint.html` into
`layout-previews/paged-export-checkpoint.pdf` with no warnings or missing-anchor
errors. The HTML SHA-256 is
`34250340443054d2b85a505c555a543e7cc90cc393657f6ad0b7403ec5b9f5d0`;
the PDF SHA-256 is
`607e0eb6845c5dd48dec22d433638e65a3d333f9cf014bbbb1287106fa46dacc`.

The result is four A4 pages at 595.276 × 841.89 points. `pdfinfo` reports no
JavaScript. `pdffonts` reports embedded Unicode fonts, and `pdftotext -layout`
recovers the opening title, source-order list, exact Rust line, final offscreen
heading and paragraph, second document title, both tables, palette values, and
final paragraph. The internal link points to the exported
`7-final-offscreen-heading` ID. The saved-width table crosses pages 3–4 and its
header repeats on page 4.

All four rendered pages were inspected:

- `layout-previews/paged-export-checkpoint-page-1.png`
- `layout-previews/paged-export-checkpoint-page-2.png`
- `layout-previews/paged-export-checkpoint-page-3.png`
- `layout-previews/paged-export-checkpoint-page-4.png`

They retain readable source order, visible static task state, code and formula
labels, non-stranded headings, reserved folio space, automatic table layout and
the saved-width table geometry. No content is truncated to balance a page.

## Automated evidence

- All 128 `document-core` unit tests, 72 integration tests and doc tests pass.
- Three static-export tests cover sanitization, source immutability, page-size
  options, task translation, saved widths, safe language metadata, GFM anchors
  and duplicate heading IDs.
- All 44 `markdown-app` tests pass. New tests cover `.html` target normalization
  and exact atomic creation/replacement without leftover temporary files.
- The application menu action and shortcut compile through the normal GPUI
  action path; export work remains outside the foreground render thread.

## Boundaries

This is the first production static/paged output path, not the complete page
system. Pagination is currently performed by the receiving browser or paged
renderer. Tachyon does not yet provide a finite-page preview, PDF command,
cover/reference/wide-evidence/appendix masters, generated contents/bookmarks,
explicit authored page breaks, landscape selection in the UI, anchor-page
footnote placement, code continuation labels, oversized-row escape handling,
or its own bounded convergence and unresolved-constraint report.

The generated PDF is selectable but is not a tagged PDF. Display math is
preserved visibly and losslessly as source rather than typeset as a real print
equation. Project fonts are named in CSS but not embedded in the HTML artifact;
the paged renderer uses available system fallbacks. Those limits keep P04–P16
partial or open, and A07 remains active.
