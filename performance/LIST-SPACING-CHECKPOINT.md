# Singleton paragraph list persistence

The A02 overlap matrix had a weakened check for a known mismatch: moving a
single-item list away from its authored nested footnote reopened the list as
`tight: true`, while the live model retained the parser's `tight: false`.
Restoring the full shape assertion reproduced the failure in
`/tmp/tachyon-tight-before.log` before changing production code.

The list has one visible item containing one paragraph. There is no inter-item
or inter-paragraph gap to distinguish tight from loose spacing in this shape.
The document renderer does not consume `ListBlock.tight`; the field controls
canonical Markdown separators and source/model comparisons. The parser's loose
flag came from the hidden definition's source context, which canonical Markdown
cannot retain as a spacing distinction once that definition moves away.

Import now normalizes this specific singleton-paragraph shape to tight. It keeps
the complete original source spine unchanged. Multiple items and multiple blocks
within an item retain their authored tight/loose flag. No content revision,
dirty state or undo entry is created during import.

The original move matrix now uses the full reopen shape assertion for both lists
and quotes, with no tightness exception. Successful rebasing after those moves is
also asserted. A new matrix covers unordered/ordered/task lists, LF/CRLF, exact
unchanged source bytes, meaningful loose multi-item and multi-paragraph lists,
editing, reopen, successful rebasing and exact Undo/Redo. All 42 source-fidelity
tests pass (`/tmp/tachyon-tight-matrix.log`); the strengthened move/rebase case
passes in `/tmp/tachyon-tight-rebase.log`.

This supersedes the singleton-paragraph mismatch recorded in
OVERLAPPING-SOURCE-CHECKPOINT.md and the later reference checkpoints. It does not
qualify all list restructuring or every non-paragraph singleton shape. A02 still
requires duplicate-label move precedence, partially overlapping source ranges
and the remaining complete structural matrix. No new native/performance claim.

Full `scripts/check.sh` passes 806 tests with two existing ignored tests, including
locked checks, formatting, strict Clippy, adapters and doctests
(`/tmp/tachyon-tight-check.log`). The final move/rebase assertion was rerun after
that check without further production changes. Diff whitespace checks pass.
Crusty validation `task_84eef12acc8a3291`, context `ctx_d32e08195522`, reports 75
existing advisory findings and zero new, worsened or resolved findings.
