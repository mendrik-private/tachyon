# Table resize interaction checkpoint

2026-09-09. Continues the authored-width layout correction; this is not closure
of A07 or the full audit. Theme policy and minimap policy are unchanged.

## Reproduced defect and correction

The actual editor mouse handlers previously added screen-space drag distance
directly to logical table widths. The regression command was:

```sh
cargo test -p document-view column_resize_mouse_release -- --nocapture
```

Before the correction, an 80-pixel drag at 200% changed a 100-unit column to
180, not 140. The starting width and painted boundary were correct; dividing
the movement by document zoom made the same test pass.

The native-app UX pass also added a provisional vertical guide, accessible
logical-width readout, and column-resize cursor. Moving does not mutate the
document. Release applies one structural transaction. Escape, lost-button
movement, focus loss, changed zoom, changed canvas width, scrolling, source
transactions, and document replacement discard provisional drag state. A click
or tiny pointer jitter does not turn an automatic column into a fixed one.
Cursor and pointer boundary geometry are shared, and offscreen column edges
are excluded from the visible resize targets.

The preview is a guide, not continuous table reflow: text reflows on release.
Centered table insertion buttons retain their separate action and target.

## Native qualification

Fixture: `layout-fixtures/82-authored-table-widths.md`.
SHA-256: `6108ef32c7358cf0a1e7c1d8714b2da3b5c78d4a3a2cf2f7cf4eedc1225032b4`.
Binary: `target/debug/mineral-markdown`, layout-validation build,
SHA-256: `908eef5752a1675c6538f1383f21908ab86f465b9a280b627a096bfdb901ac3c`.
Only regression tests changed after that build.

```sh
python3 performance/capture-layout.py --fixture 82-authored-table-widths.md \
  --binary target/debug/mineral-markdown --width 1440 --height 1100 \
  --appearance dark --atspi-active --source-unchanged-check \
  --select 416 384 496 384 --table-resize-check \
  --output performance/layout-previews/table-resize-100-qualified.png

python3 performance/capture-layout.py --fixture 82-authored-table-widths.md \
  --binary target/debug/mineral-markdown --width 1440 --height 1100 \
  --appearance light --zoom-steps 10 --atspi-active --source-unchanged-check \
  --select 576 787 656 787 --table-resize-check \
  --output performance/layout-previews/table-resize-200-qualified.png
```

Both isolated Wayland runs pass:

- Holding the preview beyond autosave debounce leaves exact source bytes intact.
- The live logical-width status is exposed through native accessibility.
- Escape followed by mouse release does not commit the canceled drag.
- An 80-screen-pixel drag changes 160 to 240 at 100%, and to 200 at 200%.
- The second fixed column remains 320; unrelated source remains byte-exact.
- One native undo restores the entire original source byte-for-byte.
- Private appearance and final source-preservation checks pass.

Preview and committed screenshots were inspected at both zoom levels. The
guide tracks the pointer, the label sits above the header, and committed column
edges match the indicated width. At 100%, the neighboring automatic table
remains beside the fixed table; at 200%, the tables stay stacked.

The existing structural table serializer escapes the two terminal periods in
the edited table as `\.`. The native oracle permits exactly that known canonical
form and rejects changes elsewhere, including equivalent normalization in the
neighboring table. Therefore this is not a claim of metadata-only byte changes
during the committed resize. Undo remains byte-exact.

Early captures without `-qualified` were diagnostic failures: the 100% pointer
landed on an insertion button, and the first 200% readout lacked an accessible
name. These are not passing evidence. Intermediate qualified attempts also
exposed the pre-existing period escaping; the final reports and screenshots
replace those attempts after the oracle was made explicit and unit-tested.

## Checks and remaining scope

`scripts/check.sh` passes formatting, workspace checking, strict Clippy, all
workspace tests, and doctests. Document-view: 426 passed, 2 ignored. Python:
15 capture-harness tests and 3 resize-oracle tests pass. `git diff --check` passes.
The resize tests cover actual start/move/release, guide movement, correct zoom
conversion, one undo, cancellation/no-op on automatic widths, and clipped cursor
targets. Native tests qualify Escape and release, not every cancellation branch.

Further layout work remains: vertical spacing and mixed-block balance on real
documents, broader resize/zoom combinations, and the unqualified embedded-image
measurement seam recorded in the authored-width checkpoint. This bounded
interaction fix does not establish full-grammar or full-lifecycle correctness.
