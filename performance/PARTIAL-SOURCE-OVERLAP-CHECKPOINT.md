# Partial source-range ownership

A02's partial-overlap concern was checked in two distinct ways.

Six actual quote/list/nested-quote footnote continuation forms, each under LF and
CRLF, already pass an unrelated paragraph split, exact unchanged prefix, one
footnote declaration, reopen shape and exact Undo/Redo. The initial corpus result
is `/tmp/tachyon-partial-before.log`; it did not reproduce a parser-generated
partial-overlap failure.

Inspection nevertheless found an uncovered source-spine branch: a source-sorted
root beginning inside the preceding unit but ending beyond it was skipped.
A direct source-position contract regression supplies a partial overlap, a
contained following root and a separate final unit. Before the fix, the first
unit ended at byte 10 instead of covering the shared range through byte 16
(`/tmp/tachyon-partial-unit-before.log`). This is an explicit builder-input
regression, not a claim of a currently reproduced native/parser document defect.

The builder now extends that source unit to the union end and assigns each
covered semantic root to the same owner. Following prefix/tail boundaries use
the extended end. Existing shared-unit serialization can therefore emit the
range once, or regenerate separated semantic roots when the group changes.
The regression verifies exact unit boundaries, owned root identities, following
prefix ownership and byte-identical reconstruction from all source slices.

All document-core suites pass (`/tmp/tachyon-partial-core.log`), including the
new corpus and ownership regression. Remaining A02 work includes cross-container
structural editing, deliberate empty-paragraph persistence, non-paragraph list
spacing, and the complete structural/source-boundary qualification matrix. No
new native or performance claim is made.

Full `scripts/check.sh` passes 811 tests with two existing ignored tests, including
locked checks, formatting, strict Clippy, adapters and doctests
(`/tmp/tachyon-partial-check.log`). Diff whitespace checks pass. Crusty context
`ctx_2fe2126434b5`, validation `task_4b040cbb33c1301b`, completed with 75 existing
advisory findings and zero new, worsened or resolved findings.
