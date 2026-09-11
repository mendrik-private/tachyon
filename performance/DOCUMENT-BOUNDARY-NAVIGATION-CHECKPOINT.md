# Document-boundary keyboard navigation — September 10

This continues critical A07 and the table keyboard matrix. It closes the
document-boundary command gap; A07 and the full audit remain active.

## Behavior

The editor now provides the conventional Linux desktop bindings:

- `Ctrl+Home` moves to the start of the canonical document.
- `Ctrl+End` moves to the end of the canonical document.
- `Ctrl+Shift+Home` and `Ctrl+Shift+End` extend the selection to those bounds.

The actions leave any temporary HTML-preview selection before resolving a
canonical projection offset. They then use the existing measured visual lines,
table viewport calculation and document scroll handle to reveal the destination.
No parallel caret or scroll geometry was added.

The focused GPUI regression dispatches the actual four key chords in a viewport
shorter than the document. Its final table has three explicitly saved 400 px
columns in a 500 px window. At 100% and 200% zoom it verifies the exact logical
selection, vertical visibility, local table-scroll visibility and byte-identical
Markdown. Saved table widths remain unchanged; the earlier automatic
reading-edge alignment continues to apply only to auto-sized tables.

## Verification

- Focused regression: 1 passed.
- Complete `document-view` suite: 581 passed, 2 existing native-font tests
  ignored.
- `scripts/check.sh`: formatting, locked checks, strict Clippy, 833 Rust tests,
  adapter tests and doctests passed; the same 2 tests were ignored. Log:
  `/tmp/mineral-document-boundary-check.log`.
- `git diff --check` passed.

This is deterministic GPUI coverage at two text scales. It does not claim a
native compositor capture or release-performance qualification. Broader table
column operations, nested/RTL/IME cases, export, and performance work remain
open.
