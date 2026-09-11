# Reference definitions retained after container deletion

A02's deletion invariant had a reproduced gap: removing a quote containing
`[ref]: /local "Title"` left an untouched `Outside [link][ref].` paragraph with
an unresolved reference. `/tmp/tachyon-ref-before.log` records the failing
source-preservation assertion.

Import now records reference definitions inside source-backed roots. Extraction
uses the configured Markdown parser to verify that candidate source produces no
rendered blocks, excludes visible code/HTML/math/front-matter spans, and checks
paragraph context so definition-looking text after ordinary prose stays inert.
Comrak 0.54.0 leaves inline source positions at the old paragraph origin after
consuming leading definitions; mismatched text spans are therefore not accepted
as source evidence. The paragraph-prefix check covers that case explicitly.

Definitions belonging to a deleted root join its orphaned source records at the
replacement boundary or tail. Container markers are removed as required to keep
the record non-rendered outside that container; authored labels, destinations,
titles and their internal continuation whitespace remain intact. Existing
prefixes still borrow their original source slices. Successful-save rebasing
remaps record ownership with the rest of the source spine.

Two regression tests cover quote/list deletion, definitions before a paragraph,
LF/CRLF, multiline destinations and titles, exact resolved inline runs after
reopen, no resurrected container, subsequent rebase/edit, and exact Undo/Redo.
Negative cases cover fenced code, HTML, front matter, ordinary paragraph text
and escaped text that resembles a definition; none becomes an active record.
All 39 source-fidelity tests pass (`/tmp/tachyon-ref-final-core.log`). Temporary
reference-position probes were removed.

This is deletion/replacement-boundary qualification, not completion of A02.
Definitions inside a surviving container that is canonically regenerated still
need qualification, as do conflicting duplicate-label precedence under arbitrary
moves, partially overlapping source ranges, list-tightness persistence and the
full structural matrix. Import-time scanning/preparation has no new native
latency or memory qualification.

Full `scripts/check.sh` passes 803 tests with two existing ignored tests, including
formatting, locked checks, strict Clippy, adapters and doctests
(`/tmp/tachyon-ref-final-check.log`). Diff whitespace checks pass. Crusty
validation `task_1cf9e216b34c411e` for `ctx_a7d13d957457` completed with 75 existing
advisory findings and zero new, worsened or resolved findings.

## Surviving-container regeneration

A subsequent regression split `Before` inside a surviving quote that also owned
a reference definition. Canonical container output dropped the declaration and
left the untouched outside link unresolved. The failing-before source/count
assertion is recorded in `/tmp/tachyon-regen-before.log`.

Reference records now retain their absolute source ranges as well as standalone
syntax. A local patch that overlaps a declaration cannot reuse that source unit.
Canonical saved-root output emits its original reference records in order before
the regenerated visible block. Clipboard and generic Markdown serialization do
not acquire document-specific records. An unchanged or safely patched root keeps
its authored source in place. Lifted footnote groups use the same overlap guard,
so definitions and notes are each emitted exactly once.

The two added regressions cover list/quote splits, leading definitions, heading
changes, typing, LF/CRLF, conflicting labels within one owner, lifted footnotes,
untouched setext headings/fences, exact reopened link runs and topology, successful
rebasing, subsequent edits and exact Undo/Redo. All 41 source-fidelity tests pass
(`/tmp/tachyon-regen-matrix.log`). This supersedes the surviving-container gap for
the covered structural and local rewrite cases. Duplicate-label precedence under
arbitrary moves, partially overlapping source roots, loose-list persistence and
the complete A02 structural matrix remain open. No new native/performance claim.

The regeneration change passes full `scripts/check.sh`: 805 tests, two existing
ignored tests, locked checks, formatting, strict Clippy, adapters and doctests
(`/tmp/tachyon-regen-check.log`). Diff whitespace checks pass. Crusty context
`ctx_48d978070376`, validation `task_ff07dc87bdcc13c6`: 75 existing advisory
findings, zero new/worsened/resolved findings.
