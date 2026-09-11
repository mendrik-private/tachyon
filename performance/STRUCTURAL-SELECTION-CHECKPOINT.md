# A04 active-cell structural selection qualification

2026-09-10. Existing transaction reconciliation already passes the new cases;
this checkpoint adds executable evidence, with no production change.

`tests/structural_selection.rs` checks every cell of a three-by-three table,
upstream/downstream affinity, and five operations: delete active row, delete
active column, move active row, move active column, and paste a two-by-two Unicode
TSV range over the active cell. These 90 combinations include header cells,
trailing cells and TSV growth beyond the original table edges.

After each edit and history transition, text endpoints resolve and lie on valid
UTF-8 boundaries. Moves retain the selected text node and affinity. Undo restores
the exact source and selection; Redo restores the edited selection and source.
Typing a Unicode checkmark immediately after Redo succeeds without a corrective
selection command, and undoing that typing restores the structural result.

Four invalid deletion/move cases use out-of-range indices and assert atomic
failure: source, selection and revision are unchanged, and the previous valid
border edit is still the next Undo/Redo entry.

The shared validity helper also supports rectangular selections, but this first
matrix exercises text selections. A04 remains active for the broader rectangular
coordinate/direction matrix, block/container deletion and public mutation
boundary audit. Existing unit tests for opaque-only imports and invalid explicit
post-edit selections remain available; they have not been substituted for that
remaining audit. No native or performance claim is made.

`scripts/check.sh` passes 821 tests with two existing ignored tests, including
formatting, locked checks, strict Clippy, adapters and doctests
(`/tmp/tachyon-a04-selection-check.log`). The focused matrix passes in
`/tmp/tachyon-a04-table-matrix.log`; diff whitespace checks are clean.
Crusty context: `ctx_702998f7f349`.

## Rectangular endpoints and deleted-table caret

The next matrix covers 1,944 cases: all 81 ordered endpoint pairs on a 3x3
table, eight row/column insert/delete/move/duplicate operations and three mutation
indices. Expected endpoints are checked against stable cell identities, not a
copy of the coordinate-transform implementation. Deleted endpoints choose the
next cell on the deleted axis, falling back to the previous cell at the edge.
This verifies anchor/head direction independently, including reversed and
single-cell ranges. Exact selection/source Undo and Redo and coordinate validity
are checked for every case. Existing production transforms pass this matrix.

A separate regression reproduced deleting a rectangularly selected table sending
the caret to the first paragraph rather than its following neighbor
(`/tmp/tachyon-a04-table-delete-before.log`). Reconciliation now resolves the
original anchor cell's text position and applies the existing text neighbor
repair policy. The regression checks the following paragraph's start, collapsed
selection, exact Undo source/rectangle, and immediate typing after Redo.

The rectangular matrix and deletion regression pass in
`/tmp/tachyon-a04-rect-fixed.log`. A04 remains active for remaining block/container
deletion and public mutation-boundary audit; this is not native or performance
qualification.

Expanded `scripts/check.sh` passes 823 tests with two existing ignored tests,
including formatting, locked checks, strict Clippy, adapters and doctests
(`/tmp/tachyon-a04-rect-check.log`). Diff whitespace checks are clean.
Crusty context: `ctx_281379a40a83`.

## Container deletion and explicit endpoint boundaries

Another 70 cases cover deletion of active paragraphs, quotes, alerts, lists,
code blocks and tables, under LF/CRLF. Surroundings include both text neighbors,
only the preceding or following paragraph, only an opaque comment, and no other
content. Tables exercise both text and rectangular selections. The expected
caret is the following paragraph's start, otherwise the preceding paragraph's
end, otherwise an empty host. Exact source/selection Undo and Redo, valid UTF-8
positions, preserved comments and immediate Unicode typing are asserted. This
also qualifies the previous deleted-table fix at document edges and when no
ordinary text survives.

Explicit `ReplaceText.selection_after` tests reject an endpoint inside a
multibyte character, an out-of-bounds offset and a non-text node. Each rejection
preserves published revision, source and selection. A subsequent valid reversed
selection retains its endpoint affinities, and Undo reaches the original state.

All six structural-selection tests pass in
`/tmp/tachyon-a04-boundary-matrix.log`. No production change was necessary for
these cases. A04 remains active pending the complete public mutation/invariant
and requirement audit.

Audit follow-up: `Document::apply_at` currently skips `validate_tree` only when
`localized_text_node` is a top-level node (`BlockSequence::top_index_of`). A
nested paragraph edit therefore still takes whole-tree validation. A04's
requirement to avoid repeated full-tree scans on ordinary edits needs further
work or qualification before completion; no performance conclusion is inferred
from the functional matrix.

The expanded `scripts/check.sh` passes 825 tests with two existing ignored tests,
including formatting, locked checks, strict Clippy, adapters and doctests
(`/tmp/tachyon-a04-container-check.log`). Diff whitespace checks are clean.
Crusty context: `ctx_185631129c41`.

## Nested text validation cost

A test-only thread-local counter at the actual `validate_tree` entry point
reproduced the nested typing scan (`/tmp/tachyon-a04-validation-before.log`).
The regression covers quote/list/table paragraphs and quoted code, verifies no
full-tree validation during typing, retains both leaf and ancestor dirty IDs,
checks immutable unrelated siblings, valid selection, saved content and exact
Undo/Redo, and confirms that a subsequent structural table insertion still
invokes full validation.

`apply_at` now uses its existing single-leaf text-change classification to skip
whole-tree shape/ID validation at every depth. Those commands retain tree shape
and IDs; RichText mutation validates ranges and selection reconciliation still
validates endpoints. Ancestor dirty propagation remains independent and
unchanged. Structural edits, preview conversions and transient-host retirement
retain the conservative path. This supersedes the top-level-only validation gap
above. The focused regression passes in `/tmp/tachyon-a04-validation-fixed.log`.

This establishes removal of the specific validation scan, not overall constant
time editing: ancestor reconstruction, change tracking and other costs remain.
A04 still needs its final mutation-boundary/requirement audit.

`scripts/check.sh` passes 826 tests with two existing ignored tests, including
formatting, locked checks, strict Clippy, adapters and doctests
(`/tmp/tachyon-a04-validation-check.log`). Diff whitespace checks are clean.
Crusty context: `ctx_9aec632d759d`.

## Composition mutation boundary

The boundary audit found `validate_composition_range` also scanned the entire
tree at every same-leaf composition begin. Extending the actual scan-counter
regression reproduced this (`/tmp/tachyon-a04-ime-before.log`). Same-leaf ranges
now validate endpoints against the actual baseline, including a converted
preview baseline, without constructing a structural probe. Cross-node ranges
retain replacement probing and full shape/ID validation.

The counter regression covers begin, repeated empty/Unicode updates and cancel
inside quote/list/table/code contexts, verifies valid selections and exact
restoration, and still confirms structural insertion invokes full validation.
An eight-case integration matrix adds direct/quote/list/table composition,
commit or cancel, malformed UTF-8 endpoint rejection before ownership is taken,
atomic rejection of structural commands during composition, valid provisional
carets, one-step Undo/Redo after commit, exact source/selection restoration, and
immediate typing afterwards. All seven structural-selection tests pass in
`/tmp/tachyon-a04-ime-matrix.log`; the counter regression passes separately in
`/tmp/tachyon-a04-ime-fixed.log`.

A04 remains active for the final requirement audit; no native IME or end-to-end
performance claim is made.

Composition qualification: `scripts/check.sh` passes 827 tests with two existing
ignored tests, including formatting, locked checks, strict Clippy, adapters and
doctests (`/tmp/tachyon-a04-ime-check.log`). Diff whitespace checks are clean.
Crusty context: `ctx_2547a757f45d`.
