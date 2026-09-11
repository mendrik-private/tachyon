# A04 selection validity: completion audit

2026-09-10. Scope: all five implementation requirements and acceptance criteria
in `audit.md` A04, work `work_3afbe7b914c94f81`. This audit supersedes the pending
A04 qualification notes in `STRUCTURAL-SELECTION-CHECKPOINT.md`.

| Requirement | Implementation and current evidence |
| --- | --- |
| 1. Reconcile before publication | `apply_at` mutates a private snapshot, runs `reconcile_selection`, then publishes. Existing text nodes clamp to UTF-8 boundaries; removed cell text chooses the nearest surviving cell; deleted containers use surviving text neighbors or create a transient caret. Rectangular table deletion now uses the same neighbor policy. The 90 active-cell and 70 container-deletion cases verify valid endpoints, exact Undo/Redo and immediate typing. |
| 2. Rectangular transforms | Selection mutation descriptors cover insert/delete/move and duplication on each axis. Endpoint coordinates transform independently and clamp at table edges. The 1,944-case matrix compares resulting endpoint cell identities for all ordered pairs on a 3x3 table, including reversed and collapsed ranges. A deleted endpoint chooses the next cell on its axis, otherwise the previous cell at the edge. Moves retain text caret identity and affinity in the active-cell matrix. |
| 3. Mutation-boundary validation and cost | Supplied post-edit selections are validated before publication. Invalid UTF-8, out-of-range and non-text endpoints reject atomically; valid reversed selections retain affinity. Single-leaf text/style operations preserve shape and IDs, validate RichText ranges and reconcile selections. The actual validation-entry counter verifies nested typing and same-leaf composition avoid whole-tree shape scans, while structural insertion still invokes validation. Ancestor dirtiness remains intact. |
| 4. No ordinary text on import | Import adds a transient editable host when needed without changing original serialization. `source_with_no_editable_nodes_gets_a_transient_valid_caret` now covers thematic-break-only, comment-only, front-matter-only and preserved-HTML documents through typing, exact Undo/source/selection and Redo. Container deletion also tests empty and opaque-only remainders. |
| 5. Atomic rejection | Private snapshot mutation prevents publishing failed edits. Invalid table operations and explicit endpoint regressions check unchanged source, selection and published revision, plus retained history. The 300-step mixed edit sequence now checks selections as well as tree invariants after edits, Undo/Redo and rejection. Composition rejects invalid endpoints before taking ownership and structural commands while active. |

Public mutation boundary review:

- `from_markdown`, `from_html` and `empty` build imported models with an editable
  selection, including transient-host cases above.
- `apply` validates selection-only commands directly; other commands use the
  private-state reconciliation/publication path. Structural changes validate
  shape/IDs; classified leaf edits cannot alter those invariants.
- `begin_composition` validates endpoint ranges; preview/HTML composition first
  resolves source-verified positions into a private converted baseline. Same-leaf
  updates use range-validated text replacement. Cross-node ranges probe the
  structural replacement; the tree splice creates its resulting caret.
  Composition regressions cover provisional updates, cancellation, commit and
  history, including cross-role Unicode replacement and invalid boundaries.
- `undo`, `redo` and composition cancellation restore previously validated
  snapshots with fresh revisions. Tests verify restored endpoint validity and
  exact selection/source; commit records the provisional result as one entry.
- `rebase_source` only changes source metadata after revision/origin/composition
  checks. A02 tests verify live nodes, selection and history stay intact.

The acceptance operations include deletion/movement of active rows, columns and
blocks, TSV replacement and growth, last-container deletion, history transitions
and typing without another selection command. The added validity assertions in
the 300-step sequence and opaque-import tests pass without production changes
during this final audit.

This completes A04's model-level selection and mutation contract. Overall queue
work remains, including A05/A06 file lifecycle and broader native interaction
and performance qualification. Avoiding a validation scan is not a claim that
all editing operations are constant time or that native IME behavior was tested.

Final validation: `scripts/check.sh` passes 827 tests with two existing ignored
tests, including formatting, locked checks, strict Clippy, adapters and doctests
(`/tmp/mineral-a04-final-check.log`). This includes the strengthened 300-step
mutation sequence, opaque-only import cases, seven structural-selection tests,
and the validation-entry regression. `git diff --check` is clean. Crusty context
`ctx_ac6ac3ecd700`; the final advisory result is recorded in the work item.
