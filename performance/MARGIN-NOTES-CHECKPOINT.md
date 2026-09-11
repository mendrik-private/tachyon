# Anchored margin notes

2026-09-09. E08 implementation contract, prepared before code changes.

An explicit top-level blockquote beginning `Margin note:` attaches to the
immediately preceding top-level paragraph. The marker and all text remain
visible and editable Markdown. Recognize one or two paragraph children, each
at most 480 bytes; unsupported/nested/ambiguous structures stay ordinary quotes.
No inferred links, footnotes, numbering or hidden source directives are added.

Use an optional measured main/rail pair with a 24px gutter, loaded-font prose
measure and quiet 14/20 sans note text with the existing quotation rail/insets.
Do not lift the note above an earlier paragraph or across a heading. Insufficient
width, height, or readable main text leaves it inline immediately after the
anchor. Never shrink text to retain the rail. Ordinary quotes and Note/Tip alerts
retain their current behavior.

The anchor relationship is source-owned and available to assistive technology.
Focused note typography must remain stable during nonstructural edits, and all
selection, editing, autosave, Undo and source-order copy routes remain canonical.
Verification must cover semantic eligibility, group ownership, actual measured
wide/narrow/zoom geometry and native pixels/input. Arbitrary distant anchors,
multiple colliding notes, nested/HTML notes, RTL rail placement and paged exports
remain part of the full E08 requirement, not claimed by this bounded encoding.

## Implemented and verified, September 10

`quotes::margin_note_anchor` recognizes the explicit role and binds it to a
canonical paragraph ID. Projection context and the existing retained quotation
roles carry it through focused edits. Removing the label releases the role after
blur; deleting the anchor cannot leave a retained description pointing at it.
The note uses the existing 14/20 metadata tokens, quotation inset/padding and
quiet rail surface; no new drawing system or user-facing template control.

Grouping isolates the exact anchor paragraph from earlier prose. The existing
bounded main/aside planner now measures recognized paragraph-only margin notes,
without admitting arbitrary quotations to that layout. Source order, section
barriers and the normal height/width/overflow gates remain authoritative. The
main passage is not widened beyond its loaded-font reading measure merely to
fill the window.

The semantic tree publishes a Note role (mapped to `comment` by this AT-SPI
adapter) and a source-linked description on the anchor paragraph. Shared figure
descriptions and margin-note descriptions use one `authored_descriptions` path.
The actual native AT-SPI record contains the complete first note in its anchor
paragraph's description. Every canonical note paragraph has one semantic node;
ordinary container labels still aggregate their children, as before.

## Tests and diagnosis

Six tests cover explicit/negative/nested eligibility, focused label edits and
deleted anchors, exact grouping ownership, real measured two-rail placement,
inline fallback at narrow/short/200% sizes, 14/20 note geometry, and semantic
attachment without duplicate canonical nodes. Existing optional Note/Tip and
quotation tests remain unchanged and pass.

The first new layout-test attempt used a private group field and failed to
compile; that was a test-seam error, not a rendering regression. After correcting
the seam, the test went red with zero rails: candidates were rejected as
`Unmeasured` because row measurement excluded all blockquotes. Supporting only
recognized margin notes closes that gap. The initial semantic test incorrectly
searched only top-level nodes; its corrected recursive oracle verifies the
actual nested paragraph nodes. Those test-development failures are retained in
`/tmp/tachyon-margin-red.log`, `/tmp/tachyon-margin-candidates.log` and
`/tmp/tachyon-margin-focused.log`, not represented as production failures.

`cargo test -p document-view --locked margin_note -- --nocapture` passes six
tests (`/tmp/tachyon-margin-complete-focused.log`). The final `scripts/check.sh`
exits 0: formatting, source pins/locked metadata, all-target check and Clippy,
116 core tests, 25 source-fidelity tests, 11 tree tests, **491 document-view
tests (2 ignored)**, 1 external-consumer test, 39 app tests and doctests.
Log: `/tmp/tachyon-margin-qualified-check.log`. `git diff --check` passes.

## Native evidence

Runtime SHA-256:
`4e98264c0432e803d6d1c9c9710414e61422033e2ad3c3041d845b94cd9c79c5`.
Fixture 101 SHA-256:
`88f1b3fbf7520e5d6ddb36ea27da52a87930f535c3074ca6eee5d4e45e93ca80`.
All native runs use private fixture copies and preserve the original bytes.
Artifacts below are under `layout-previews/`. Wide/narrow/200%, note-idle and
anchor-typed screenshots were inspected.

| Prefix | Evidence |
| --- | --- |
| `margin-notes-wide` | 1600×1700 light: both explicit notes beside their own paragraph; earlier prose stays above, later text clears the pair. Ordinary and unanchored quotations remain quotations. |
| `margin-notes-narrow` | 520×1700 light: source-order inline notes after complete anchors, including the two-paragraph note. |
| `margin-notes-dark-200` | 1280×1700 dark/200%, scrolled: first complete enlarged anchor and its inline note with legible text and rail. |
| `margin-notes-note-edit` | Type `x` before the first note label; full edited-file equality, autosave, one-second idle, exact Undo. Inspected idle pixels retain the original rail and note typography while focused despite the temporarily changed label. |
| `margin-notes-anchor-edit` | Type `x` at the anchor start; complete edited-file equality, autosave, idle and exact Undo. Inspected typed pixels retain the main/rail pair. |
| `margin-notes-copy` | Seven unique source-order markers, including both anchors and notes, occur once in canonical order. This is not a whole-clipboard equality claim. |

The edit oracles account for the existing serializer's punctuation escaping and
compare the entire file, not just inserted markers. Undo restores the original
exact Markdown. The layout tests also exercise a short viewport; that geometry
test is not presented as an additional native short-window screenshot.

UX guidance shaped the local anchor, readable measure, visual hierarchy and
fallback behavior. Rust engineering guidance shaped source identity, cached
roles and complete geometry/semantic/native verification. E08 moves from open
to partial for this implemented encoding. All broader scope listed above and
the full audit goal remain active; no release-performance claim is made.
