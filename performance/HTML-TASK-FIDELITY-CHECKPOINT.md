# Mixed task states in rich HTML table cells

2026-09-10, A03. A new import regression reproduced checked and unchecked HTML
list items becoming plain items (`/tmp/tachyon-a03-tasks-before.log`). The HTML
list converter treated input elements as unsupported content instead of task
state. Independently, canonical HTML export used `unwrap_or(false)` for every
item of a task list, which would turn a plain item into an unchecked task.

HTML import now recognizes a leading checkbox directly in a list item or inside
its first paragraph. It carries that state separately while rendering paragraph
boundaries and continuation indentation. Nested lists run the same conversion.
Canonical HTML export emits a checkbox only when the item has a checked state,
preserving `Some(true)`, `Some(false)` and `None` distinctly.

`html_table_mixed_task_states_survive_presentation_edits` asserts imported
states, then changes the table border to force canonical HTML. The fixture
includes checked, plain and unchecked siblings, a nested checked task, multiple
paragraphs, bold text and a link destination, under LF and CRLF. Reopened shape,
successful complete-model source rebasing, exact Undo source/selection and Redo
output are verified. Complete-model rebasing checks rich runs in addition to the
test helper's topology/text comparison.

A03 remains active: HTML-required cell break/whitespace semantics and the final
requirement audit still need qualification. This checkpoint makes no native UI
or performance claim.

Validation: `scripts/check.sh` passes 817 tests with two existing ignored tests,
including formatting, locked checks, strict Clippy, adapters and doctests
(`/tmp/tachyon-a03-task-check.log`). All 52 source-fidelity tests pass.
`git diff --check` is clean. Crusty context: `ctx_1f298a3254e3`.
