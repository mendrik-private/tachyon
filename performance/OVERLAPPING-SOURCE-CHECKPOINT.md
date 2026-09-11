# Structural saves with embedded footnote source

A minimized A02 regression split `After` after a quote containing a footnote.
The model held a quote, a lifted definition, and the following paragraph. Saving
emitted the original quote (already containing the definition) followed by a
second canonical copy of the definition. The failing test observed two `[^n]:`
markers; `/tmp/tachyon-boundary-before.log` records the exact duplicate output.

Source import now records fully contained top-level source ownership: the lifted
semantic root belongs to the enclosing source unit. During structural saving,
that group can reuse its authored bytes only when all members remain adjacent
in canonical order and every edit has a proven local patch. The serializer
patches the group once and suppresses separate emission of embedded members.

If a member moves, is deleted, or cannot be patched, the enclosing container is
regenerated without embedded definitions. Surviving definitions serialize at
their canonical positions. This prevents both duplication and resurrection of
a deleted definition. Successful-save rebasing remaps these ownership records
along with ordinary and nested ranges.

Two regressions cover quote/list ownership, multiple notes in one owner,
unrelated structural splits, LF/CRLF, moving the owner or note, deleting either,
simultaneous note edits and splits, subsequent save/rebase/edit, original setext
heading spelling, and exact Undo/Redo. All 37 source-fidelity tests pass. Full
`scripts/check.sh` passes 801 tests with two existing ignored tests, including
formatting, locked checks, strict Clippy, adapters and doctests
(`/tmp/tachyon-boundary-check.log`). A later multiple-note extension to the same
regression passes separately (`/tmp/tachyon-boundary-multiple.log`); no production
code changed after the full check. Diff whitespace checks pass.

## Remaining A02 scope

The matrix also reproduced an independent tightness mismatch: moving an
originally loose single-item list away from its nested note serializes a list
that reopens tight. The ownership test verifies retained text/runs, root count,
exactly one definition and Undo/Redo for that case; it does not claim tightness
preservation. The initial matrix failure is the identified evidence (the rerun
log now records the corrected, explicitly scoped oracle). This needs a deliberate
live-model/Markdown persistence policy and remains unresolved.

This fix covers fully contained source roots. Partially overlapping spans,
reference-definition ownership inside regenerated containers, arbitrary
replacement boundaries and the full structural persistence matrix remain open.
No new native or performance claim is made by these core serialization tests.
