# A03 rich table persistence: completion audit

2026-09-10. Scope: the five implementation requirements and acceptance criteria
in `audit.md` A03 and Crusty work `work_ecb785aee94bb4f0`. A01 and A02 dependencies
are completed. This audit supersedes earlier A03 outstanding-case notes.

| Requirement | Current implementation and executable evidence |
| --- | --- |
| 1. Rich GFM cells | `serialize_pipe_table` uses the rich inline serializer, then escapes pipes and maps line breaks. `requires_html_serialization` checks block/header structure, not ordinary inline styles. `table_presentation_edits_preserve_complete_gfm_inline_semantics` changes border, width and alignment, retains GFM, and checks bold, strike, links, image title/alt/source, code, pipes, backticks, breaks, empty cells and CRLF. `table_metadata_edit_preserves_rich_inline_cells_and_literal_pipes` adds literal syntax coverage. |
| 2. Block-rich HTML and adjacent metadata | Import recognizes metadata before either GFM or supported HTML tables, checks column count, and attaches metadata to the table's source unit. `block_rich_html_table_reopens_with_metadata_and_alignment` splits a bold cell paragraph, saves/reopens HTML, then joins and saves/reopens GFM. Complete-model source rebase succeeds at both boundaries, including rich runs and the neighboring link destination. |
| 3. Presentation and unknown fields | Both serializers use the same versioned metadata writer; HTML exports column alignment and header tags. Metadata, authored HTML and transition tests assert widths, borders, alignment, unknown fields and headers. Source-fidelity tests cover zero/one/two header rows, nested lists, multiple paragraphs, code and linked figures with destinations/titles. Mixed checked/plain/unchecked tasks retain their distinct states. |
| 4. Metadata safety | JSON output escapes double hyphens, so unknown string content cannot terminate the comment. The transition fixture includes an unknown field containing a comment terminator and asserts exactly one closing comment. `malformed_table_metadata_remains_preserved_source` now covers invalid widths, unknown version, malformed JSON, missing border and column-count mismatch across GFM/HTML and LF/CRLF (20 combinations), before and after presentation edits. Invalid metadata stays a separate, byte-preserved source block. |
| 5. Breaks and empty cells | GFM rich-cell tests check imported breaks and empty cells. HTML-required whitespace coverage checks 96 combinations of eight whitespace/punctuation values, six inline style choices and LF/CRLF, with complete-model rebase and Undo/Redo. Text-only pre-wrap spans preserve significant whitespace; numeric newline entities prevent blank lines ending the Markdown HTML block. Inline and block code share literal newline encoding; a separate code-block regression verifies blank/trailing newlines, language and following content. |

The acceptance round trip is exercised in both directions: adding the second
paragraph selects HTML while retaining rich content and metadata; joining back
selects GFM and retains the complete model. Presentation-only border/width/
alignment changes preserve cell text, style and destinations. Source-fidelity
tests additionally check exact original source and selection on Undo and saved
output after Redo; A02 supplies source ownership and revision-safe save rebasing.

This completes A03's bounded table serialization/import contract. Broader
selection behavior (A04), asynchronous file lifecycle (A05/A06), clipboard/native
interaction (A13), and performance qualification remain separate queue work.
This audit does not claim a new native-binary or performance result.

Final validation: `scripts/check.sh` passes 819 tests with two existing ignored
tests, including formatting, locked checks, strict Clippy, adapters and doctests
(`/tmp/mineral-a03-final-check.log`). This includes all 54 source-fidelity tests
and the expanded metadata/transition regressions. `git diff --check` is clean.
Crusty context `ctx_8e0ee4b2a638`; final advisory result is recorded in the work item.
