# Reference ownership during block moves

The A02 move regression showed that moving a source unit carrying reference
records could lose the expected outside-link paragraph after save/reopen
(`/tmp/mineral-move-ref-before.log`). Two declaration owners also expose
first-definition precedence: changing their source order must not silently
retarget the resolved link in an untouched block.

Document snapshots now track which original roots have explicitly moved since
the source baseline. Non-rendered prefixes and reference declarations from those
roots stay anchored before the next unmoved original source unit, or at the tail
when none remains. The records are collected in original source order, preserving
precedence while visible blocks move independently. This also covers repeated
moves without an intervening save. Successful rebasing clears the move metadata;
Undo/Redo retain each historical snapshot's metadata and source spine.

A moved root containing declarations regenerates its visible content without
those declarations. Roots whose only records were in their prefix retain their
original body slices. Lifted-note groups cannot reuse an embedded declaration
that has been relocated. Record insertion now supplies a missing blank-line
boundary when needed, including files without a terminal newline, so source
records cannot merge into the preceding paragraph.

Three regressions cover standalone prefixes and quote-owned definitions, all six
orders of three visible roots, LF/CRLF, repeated moves with save/rebase between
steps, moving all roots before any rebase, exact link runs/definition counts,
complete reopen shapes, stable setext spelling, and exact Undo/Redo with original
selection. All 45 source-fidelity tests pass
(`/tmp/mineral-move-ref-final-core.log`).

This supersedes the duplicate-label precedence gap for explicit top-level block
moves in the covered source-prefix/container forms. It does not complete A02:
partially overlapping source roots, cross-container restructuring, non-paragraph
list spacing and the remaining structural/source-boundary matrix still need
qualification. No new native or performance claim is made.

Full `scripts/check.sh` passes 809 tests with two existing ignored tests, including
locked checks, formatting, strict Clippy, adapters and doctests
(`/tmp/mineral-move-ref-check.log`). Diff whitespace checks pass. Crusty context
`ctx_87e24bab4161`, validation `task_8016ff4ec78119b4`, completed with 75 existing
advisory findings and zero new, worsened or resolved findings.
