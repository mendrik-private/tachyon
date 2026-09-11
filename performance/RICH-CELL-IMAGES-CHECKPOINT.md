# Rich-cell image geometry checkpoint

2026-09-09. Layout-priority continuation of A07; the full audit remains open.

## Result

Standalone images in rich table cells now use the fitted inner column width
for both measurement and painting. Column padding, quoted-cell insets, aspect
ratio, zoom, intrinsic-size limits, and the table's horizontal clip agree.
The image placeholder uses the same rectangle. A cell image no longer shrinks
the parent table's measurement canvas to its own intrinsic width.

Fixture `layout-fixtures/83-rich-cell-images.md` exercises actual ImageNode
blocks through supported HTML table import, including a caption and a quoted
image. An earlier pipe-table image probe reached inline images instead and
did not qualify this path.

## Regression evidence

```sh
cargo test -p document-view rich_cell_image -- --nocapture
```

The measurement regression failed before the fix: a 400×240 image in a cell
with 216 px of usable width reserved 96 px instead of 129.6 px of height.
Both new tests now pass:

- Measured geometry covers authored, automatic, and mixed column widths at
  canvas widths 240/400/760/1280, image-containing row height, and exact source.
- The actual paint-bounds helper is checked at 100%/200%, with horizontal pan,
  ordinary and quoted insets, and 400×240/80×48 intrinsic sizes. Small originals
  are not enlarged.

## Native evidence

Final layout-validation binary SHA-256:
`891ad72baae3ae333d799a925dd99393e930828f636a2ea846a36e220c5bceec`.
Only regression-test changes followed this build.

Artifacts in `layout-previews/` were captured in private native sessions and
visually inspected:

| Prefix | Coverage |
| --- | --- |
| `cell-images-before` | Pre-fix baseline, 1440×1100 light: image starts at the cell border and is undersized; quote padding is ignored. |
| `cell-images-after` | Final binary, 1440×1100 light, 100%: first image uses 216×129.6 px; quoted image uses its smaller inset content area. |
| `cell-images-200` | 1440×1100 dark, 200%, scrolled: images and quoted inset scale together; following prose stays below the table. |
| `cell-images-narrow` | 600×1100 light: navigation collapses and authored table widths retain contained horizontal overflow. |
| `cell-images-narrow-pan` | Same narrow window, actual horizontal wheel input: table moves 24 px left and image paint clips at the document viewport. |
| `cell-images-caption-edit` | Native caption typing/autosave and byte-exact undo; both image references and the following paragraph are preserved. |
| `cell-images-gallery-holdout` | Fixture 31: large paired figures remain complete and use available width; smaller originals retain their intrinsic size. |

Final captures pass source and private appearance checks. Fixture 83 retains
SHA-256 `cfe63f99fa958cda53f8dddd6403ed8b3f6a71db3ae1528df5e976d0b2629d36`.
The caption-edit oracle verifies the intended insertion and protected fragments,
not byte identity of the entire edited serialization; undo/final source are
byte-exact. No original user document was opened for editing.

Reproduce the caption check:

```sh
python3 performance/capture-layout.py --fixture 83-rich-cell-images.md \
  --binary target/debug/tachyon --width 1440 --height 1100 \
  --appearance light --atspi-active --source-unchanged-check \
  --select 280 530 280 530 --selection-keys home --edit-check \
  --edit-within 'Caption inside the image cell.' \
  --edit-preserve 'gallery-1.svg' 'gallery-2.svg' \
  'The paragraph after the table must follow the entire row, not overlap the image.' \
  --output performance/layout-previews/cell-images-caption-edit.png
```

## Verification and limits

`scripts/check.sh` passes formatting, workspace checking, strict Clippy, tests,
and doctests. Document-view: 430 passed, 2 ignored. `git diff --check` passes.
No dependency, core serialization, typography, or theme-policy change was made.

This qualifies standalone rich-cell image layout, not all inline media,
accessibility image hit bounds, or the complete mixed-content/release audit.
Further real-document composition and the remaining open audit work continue.
