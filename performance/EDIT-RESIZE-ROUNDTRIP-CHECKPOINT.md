# Editing through resize round trips — September 10

## Scope

The preceding turn fixed a previously visible editing caret being lost when
the window became shorter. This pass keeps the layout-first priority and closes
the next specific evidence gap: shrink and grow the window while retaining the
active edit. Rich events remain vertical, supporting blocks keep their authored
relationships, and the caret must fit horizontally as well as vertically.

The complete, unchanged fixture119 is used. Targets cover an ordinary supporting
paragraph, fenced code, a table cell and a nested bullet. Both width and height
cycles are exercised at 100/150/200% text. This is not a new layout mode or a
smaller substitute for the mixed document.

## Verification changes

`resize_layout_check.py` now restores the original window size before Undo for
rich-timeline edit scenarios. The restored stage must retain focus, the same
collapsed selection, a visible caret, the explicitly expected vertical layout,
both window dimensions, all 68 canonical nodes and complete event-support
geometry. Detailed traces must match the current restored viewport and zoom.
These checks are separate from the existing narrow-stage assertions.

The saved file must be identical immediately after typing, after shrinking and
after restoring. The existing whole-file golden independently checks the exact
insertion and appropriate Markdown escaping, and Undo must restore every
original byte. The cycle does not issue another edit after restoration, so it
does not claim a separate post-resize typing/IME interaction test.

`caret_visible` builds on the previous vertical-line check and rejects a caret
outside either side of the document viewport. Native caret and viewport reports
use the same window coordinates. Tests reject incorrect restored dimensions,
wrong horizontal topology, changed selection, changed source and offscreen
carets. The initial restoration test failed before these checks existed.

## Concurrent evidence-reader defect

The normal-size code height run stopped before resizing because the live caret
report could not be parsed as JSON. The reader used `splitlines()` on a log that
the app was still writing, allowing an unterminated final record into the JSON
decoder. A focused regression of that exact reader reproduced the partial-record
failure. The original transient bytes were not retained, so this does not prove
that no stderr interleaving contributed to that particular capture failure.

`prefixed_records` now frames complete LF-terminated byte records before decoding.
It waits for a partial JSON/UTF-8 tail and does not split valid JSON text on a
Unicode line separator. Complete malformed records still raise an error; it
does not skip corruption, extend deadlines or weaken layout assertions. Both
live layout-trace and caret-state reads use this path. The failing native code
height scenario passes after the change.

## Runtime and qualification

There is no production Rust change in this pass. Native evidence uses the
previously verified, isolated validation binary SHA-256
`1f595c23f184e54bd8c8788d7e96cfd21de60c923502852475be0e1b6d044909`.
Fixture119 remains
`3341916cec6774738cff1db49366eb66ad5221ed251924afcddcd0ad8227bf97`.
Runs use a private Weston desktop at 3200×1400, display scale 120, native
validation resize actions and active AT-SPI. No physical desktop input is used.

Width cycles are 1380→650→1380 document pixels; height cycles are
884→304→884px. The first 200% nested-width round trip passes with selection
812..812 throughout, full caret visibility, exact source and Undo. The original
and restored caret bounds and scroll offset are equal for that specimen.
This is a case-specific observation, not a requirement to preserve a caret's
old offscreen position when the viewport becomes shorter.

All **24 native round trips pass**: four edit targets × two resize axes × three
text sizes. Every narrow/restored caret is fully inside the document viewport;
all three saved snapshots are byte-identical, the restored geometry is committed
for the actual dimensions/zoom, all canonical nodes and event-support boundaries
remain, and Undo restores the exact original file. The complete matrix is
checked by exact `(target, axis, zoom)` tuples, not by a count alone.

Accepted artifacts are `layout-previews/edit-roundtrip-*.resize.json` plus
`width-roundtrip-nested-zoom10.resize.json`. The first three normal-size passes
use `edit-roundtrip-` directly; the other 20 use `edit-roundtrip-verified-`.
The interrupted code-height attempt has no accepted resize report and remains
separate diagnostic evidence. All accepted reports share the exact binary hash
above. Original-size 200% restored nested-text and narrowed code screenshots
were inspected; this does not assert visual review of every offscreen glyph.

Two existing-path controls also pass with the corrected record reader:
`roundtrip-control-specification` checks the complete fixture03 specification
pair's reading cycle at 150%; `roundtrip-control-guidance` checks fixture01's
default width-edit/save/Undo path. They retain their original contracts and are
not counted among the 24 rich-event round trips.

All 23 resize-oracle tests and 19 capture-harness tests pass. The current Rust
visible-caret/scrolled-away resize regression passes. The preceding full
760-test Rust run remains historical evidence, not a rerun claimed here.
`cargo fmt --all -- --check` and `git diff --check` pass. Logs include
`/tmp/mineral-width-roundtrip-unit.log`, `/tmp/mineral-width-roundtrip-capture.log`,
`/tmp/mineral-width-roundtrip-rust.log`, `/tmp/mineral-width-roundtrip-red.log`
and `/tmp/mineral-live-record-red.log`.

Crusty validation `task_b619ab833b3aa753` for `ctx_83467251e81e` completed with
75 existing advisory findings and zero new, worsened or resolved findings.

## Remaining scope

The app-UX skill guided visible editing and complete adaptive cycles. The Rust
skill guided source and Undo evidence; the diagnosis skill separated incomplete
live records from application-layout failure. The full audit, A07 and L10 stay
active. Nested timeline containers, other content families' full resize cycles,
horizontal scrolling within overflowing components, post-restore typing/IME,
RTL/selection/copy, page/export/media and release performance remain open.
These correctness captures do not qualify physical resize timing or scrolling
performance.
