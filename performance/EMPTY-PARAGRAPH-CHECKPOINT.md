# Empty Markdown paragraph persistence

A02 requires an explicit contract because Markdown cannot persist an arbitrary
number of empty paragraph nodes. The existing behavior is now documented at the
paragraph serializer and covered by regression tests; no production behavior
change was needed for this qualification.

Empty top-level paragraphs remain live caret hosts while editing. Their Markdown
serialization is empty text plus the ordinary block separators. Reopening
normalizes blank-only topology: nonempty paragraphs remain distinct, while an
otherwise empty document receives one editable caret host. Save does not remove
live nodes, move the selection, alter content revision or create an undo entry.
Source-spine rebasing deliberately defers when those live IDs have no equivalent
parsed nodes. Typing into a live empty paragraph after saving creates an ordinary
nonempty paragraph on the next save. There is no invented HTML placeholder.

Two new tests cover repeated splits between nonempty paragraphs under LF/CRLF,
multiple empty live caret nodes, exact nonempty reopen order, selection/revision
preservation, rebase deferral, later typing into an empty host, full Undo/Redo, and
an all-empty document followed by typing and exact Undo. All 48 source-fidelity
tests pass (`/tmp/mineral-empty-core.log`).

This qualifies top-level Markdown editing placeholders. It does not discard
structural list markers, empty table cells, or explicit empty paragraphs in rich
HTML containers; those have separate serialization contracts. Cross-container
structural editing and the remaining complete A02 matrix are still open. No new
native/performance claim is made.

Full `scripts/check.sh` passes 813 tests with two existing ignored tests, including
locked checks, formatting, strict Clippy, adapters and doctests
(`/tmp/mineral-empty-check.log`). Diff whitespace checks pass. Crusty context
`ctx_0d605d2cfb82`, validation `task_7afcc6952a789c03`, completed with 75 existing
advisory findings and zero new, worsened or resolved findings.
