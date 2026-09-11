# Cross-container replacement and persistence

A02's new replacement matrix exposed two concrete model/reopen mismatches.

1. Deleting from the start of a quote into the following paragraph left a live
   empty caret paragraph in that quote. Reopening the saved quote produced no
   child paragraph. `/tmp/mineral-cross-source-before.log` records the mismatch.
2. Pasting two paragraphs into a list retained the old tight flag, although the
   required blank separator reopened as a loose list.
   `/tmp/mineral-cross-source-fixed.log` records that second failure after the
   quote fix (the filename denotes that intermediate run, not a passing result).

Empty quote and alert imports now create their own editable paragraph when the
parser supplies no children. Authored source remains unchanged until an actual
edit. The caret stays inside the container instead of being supplied by an
unrelated document-level placeholder. The new host has no invented source span;
typing uses existing canonical/source-record preservation behavior.

Tree-range splicing now updates spacing on the affected list. A singleton
paragraph has no spacing gap, while inserted non-list block siblings require
loose separators; paragraph-plus-nested-list structure can remain tight. The
change occurs only during splice, not read-only selection extraction, and touches
only containers already rebuilt by that operation.

The 120-case matrix covers quotes, alerts, lists, nested lists and HTML tables;
forward/reversed ranges; full/partial start leaves; deletion/plain replacement/
multi-paragraph rich paste; and LF/CRLF. It checks exact reference count and
outside link runs, untouched setext/fence/source text, full reopen shape, exact
Undo/Redo and original selection. A separate test covers empty quotes, alerts,
nested quotes and reference-only quotes through open, typing, save/rebase and
exact Undo/Redo. All 50 source-fidelity tests pass
(`/tmp/mineral-cross-source-matrix.log`).

This qualifies the covered cross-container cases; it does not close A13's full
native clipboard/editing contract or A04's complete selection matrix. A02 still
needs its final requirement audit and remaining non-paragraph singleton list
spacing check. No new native or performance claim is made.

Full `scripts/check.sh` passes 815 tests with two existing ignored tests, including
locked checks, formatting, strict Clippy, adapters and doctests
(`/tmp/mineral-cross-source-check.log`). Diff whitespace checks pass. Crusty
context `ctx_6f34ff8e5aa4`, validation `task_226fdc6755de5a8b`, completed with 75
existing advisory findings and zero new, worsened or resolved findings.
