# Table IME resize geometry — September 10

This continues critical A07's shared-geometry and IME acceptance. It qualifies
resizing while a table composition is active; A07 and the complete audit remain
active.

## Reproduction and correction

A reduced GPUI case starts a 280-character Unicode preedit in the trailing cell
of a three-column table with explicitly saved 400 px widths. The editor begins
in a 500 × 220 px viewport at 200% zoom, then shrinks to 360 × 160 px while the
composition remains active.

Before the correction, composition correctly deferred topology reflow, while
paint immediately clipped the old lines to the new viewport. Caret reveal still
used the old committed width and height, leaving only the following paragraph
in the painted set and removing the active IME caret from candidate geometry.

Paint-bound publication now reveals an active composition against the newly
published window width and current scroll viewport. The table target helper
accepts that width explicitly, so horizontal scroll, vertical reveal, paint
clipping and candidate bounds use one geometry. The line topology remains
frozen until composition commits; no second layout or IME estimate was added.

The regression verifies:

- composition stays active and reflow remains deferred through the resize;
- the preedit caret remains painted inside the resized table viewport;
- candidate-window bounds come from that same painted caret line;
- commit publishes layout for the latest 360 px width and current height;
- the saved `[400, 400, 400]` widths remain exact before and after commit;
- commit is one undoable edit and Undo restores the exact original source.

## Verification

- The reduced regression failed before the paint-bound reveal and passes after
  the correction.
- Focused IME filter: 20 passed.
- Focused resize filter: 18 passed.
- `scripts/check.sh`: formatting, locked checks, strict Clippy, 837 Rust tests,
  adapter tests and doctests passed; 2 existing native-font tests were ignored.
  Log: `/tmp/tachyon-table-ime-resize-check.log`.
- `git diff --check` passed.

Native system-composition coverage, RTL preedit, mixed-direction spanning
selection, aligned/nested/rich tables, export and release-performance
qualification remain open.
