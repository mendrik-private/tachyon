# A02 structural source preservation: completion audit

2026-09-10. Scope: the six implementation requirements and acceptance criteria
in `audit.md`, A02. Earlier checkpoint notes describe intermediate gaps; this
audit supersedes their outstanding A02 lists, using the regressions now present.

| Requirement | Implementation and verification |
| --- | --- |
| 1. Dirty units and adjacency | `markdown.rs` retains source-backed units and patches eligible leaves; structural fallback assembles unchanged slices and regenerates affected roots. `structural_edit_keeps_untouched_source_units_and_block_boundaries`, item/cell fidelity tests, and insert/move regressions verify unrelated source stays exact. |
| 2. Owned records | `source.rs` records units, prefixes, references and overlapping roots. Serialization anchors orphaned records in source order, independently of moved visible blocks. Deleted/regenerated container tests check surviving reference meaning; repeated owner moves check duplicate-label precedence, including all owners moved before saving. The partial-overlap builder contract has a synthetic regression; actual nested-note continuation fixtures separately verify parser-produced ranges. |
| 3. Exact untouched slices | Source-fidelity tests exercise setext headings, authored list markers, fences, reference definitions, comments, front matter and mixed endings. Nested list/HTML/GFM table edits verify enclosing-unit fallback and local reuse. `joining_blocks_preserves_mixed_ending_source_records_and_inline_meaning` explicitly checks protected records alongside changed rich text. |
| 4. Generated boundaries | Blank-line adjacency is supplied between nonempty generated blocks and source records. Split/reopen and `generated_structural_boundaries_follow_crlf_source` cover paragraph separation and CRLF. No-final-newline reference-move cases verify declaration boundaries. |
| 5. Empty paragraphs | Empty top-level nodes remain live caret hosts; reopening normalizes blank-only topology. Rebase defers when live placeholders cannot map exactly. Two empty-paragraph tests verify later typing and Undo/Redo; empty quotes/alerts keep their own editable host. Empty cells and explicit HTML paragraphs retain their separate structural representation. |
| 6. Successful-save rebase | Background preparation serializes/imports/compares the model and remaps source IDs. Installation guards revision, originating content identity and composition, then updates metadata without replacing content or clearing history. Save and adopted Save As install only after successful writes. Four rebase regressions cover stale edits, foreign equal revisions, nested subsequent edits, selection, composition and exact Undo/Redo. App save completion rebases the captured session; registry dirtiness compares current and saved revisions. |

The structural acceptance matrix includes split, join, insert, delete and move,
complete reopened shape and inline meaning, exact original source/selection on
Undo, and saved output after Redo. The cross-container replacement regression
adds 120 combinations across quote, alert, list, nested list and HTML table,
direction, boundary offset, replacement type and LF/CRLF.

The final singleton test first reproduced a heading-only list retaining a loose
flag after its hidden note moved away. Import and splice now normalize any
one-item/one-block list, where no spacing gap exists; genuine multi-item or
multi-block spacing remains meaningful. The same test then exposed fenced-code
content gaining spaces because list continuations used a fixed four spaces.
Canonical continuations now use the actual marker width; task checkboxes remain
paragraph content. The regression covers headings, quoted content and fenced
code under unordered, single-digit ordered and four-digit ordered markers. It
checks exact initial source, one note, full reopen shape, successful rebase and
Undo/Redo. The initial failure is recorded in
`/tmp/tachyon-a02-singleton-before.log`.

A02 completion is bounded by these persistence requirements and executable
cases. A03's complete rich-table combinations, A04 selection behavior, A05/A06
broader asynchronous file lifecycle, A13 native clipboard, and performance
qualification remain separate queue work. No new native-binary or performance
claim is made by this audit.

Validation: `scripts/check.sh` passes 816 tests with two existing ignored tests,
including locked workspace checks, formatting, strict Clippy, adapters and
doctests (`/tmp/tachyon-a02-final-check.log`). All 51 source-fidelity tests pass.
`git diff --check` is clean. Crusty validation uses context
`ctx_7d11106cedf0`; its final advisory result is recorded in the A02 work item.
