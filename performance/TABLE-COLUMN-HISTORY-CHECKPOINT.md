# Table column history and saved widths — September 10

This continues A07/T03 after row history and document-boundary navigation. It
qualifies the bounded column case below; the full table and layout audits remain
active.

## Contract exercised

A focused GPUI regression performs real editor structural transactions on an
overflowing three-column table whose columns each have an explicitly saved
400 px width. At both 100% and 200% zoom it covers:

- insert a column before the active trailing cell;
- delete a preceding column;
- Undo, Redo, and final Undo after deliberately invalidating the old local
  horizontal scroll position;
- exact surviving and restored text selections;
- full horizontal and vertical caret visibility in the measured table viewport;
- byte-exact source restoration after Undo.

Insertion creates one auto-sized column and shifts the three saved widths
without changing them: `[auto, 400, 400, 400]`. Deletion removes only its target
and retains the two surviving saved widths as `[400, 400]`. Undo restores the
original `[400, 400, 400]` metadata exactly. This confirms the user decision
that saved widths remain authoritative while automatic alignment and fitting
apply only to auto-sized columns/tables.

No production correction was needed. Structural column commands already use
the shared transaction refresh, measured visual lines, table viewport geometry,
and caret-reveal path introduced for row history.

## Verification

- Focused column-history regression: 1 passed at both zoom levels and for both
  structural operations.
- Complete `document-view` suite: 582 passed, 2 existing native-font tests
  ignored.
- The immediately preceding production state passed `scripts/check.sh` with
  formatting, locked checks, strict Clippy, 833 Rust tests, adapter tests and
  doctests; `/tmp/mineral-document-boundary-check.log` records that run. This
  slice adds only the passing regression and checkpoint.
- `cargo fmt --all` and `git diff --check` pass.

This does not qualify move/duplicate column commands, rectangular selections,
nested/RTL/IME tables, native compositor behavior, export, or release
performance.
