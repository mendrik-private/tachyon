# Table IME caret geometry — September 10

This continues critical A07's shared-geometry and IME acceptance. It closes the
reproduced preedit-caret defect below; A07 and the complete audit remain active.

## Reproduction and correction

A reduced GPUI case places the caret in the trailing cell of a three-column
table with explicitly saved 400 px widths, inside a 500 × 220 px viewport at
200% zoom. It starts a 40-character Unicode preedit, replaces it with 280
characters, and selects the last composed character.

Before the correction, the canonical selection and measured wrapped line were
updated, but the assertion that the preedit caret fit inside the table viewport
failed. The IME update path refreshed the projection and selection without
revealing the new caret.

After each successful preedit update, the editor now sends its canonical cursor
offset through the existing `keep_offset_visible` path. That path reads the
published visual line, table-cell viewport and document scroll geometry. No IME
layout estimate or parallel geometry was added.

The regression now verifies:

- the current preedit caret is painted inside both table and document viewport
  bounds;
- candidate-window bounds come from that same painted line;
- replacing an active Unicode preedit retains a valid marked selection;
- cancellation restores exact source with no undo entry;
- commit is one undoable edit;
- the saved `[400, 400, 400]` widths remain exact throughout.

## Verification

- The reduced regression failed before the reveal call and passes afterward.
- Focused composition filter: 19 passed.
- `scripts/check.sh`: formatting, locked checks, strict Clippy, 835 Rust tests,
  adapter tests and doctests passed; 2 existing native-font tests were ignored.
  Log: `/tmp/mineral-table-ime-check.log`.
- `git diff --check` passed.

Native input-method integration, candidate popup placement, RTL preedit,
nested/rich-cell tables, resize during an active system composition, export and
release-performance qualification remain open.
