# Significant whitespace in HTML-required cells

2026-09-10, A03. Replacing cell text with a bare newline and changing its border
reproduced the newline becoming a space on reopen
(`/tmp/mineral-a03-break-before.log`). Raw HTML text passed through an
intermediate Markdown paragraph, whose soft-break import normalizes to a space.

Canonical HTML now wraps whitespace-sensitive non-code runs in a text-only span
with `white-space: pre-wrap`. This gives the whitespace an explicit HTML display
meaning. The HTML converter recognizes that span and writes whitespace entities
to its intermediate Markdown, preserving characters without applying this policy
to ordinary authored HTML. Newlines in the generated span are numeric entities:
an intermediate regression with consecutive raw newlines exposed premature
termination of the Markdown HTML table block. Code runs use their separate code
conversion and do not acquire a nested whitespace span.

`html_required_cells_preserve_edited_breaks_and_edge_whitespace` covers bare and
hard breaks, repeated newlines, CRLF characters, leading/trailing spaces, tabs,
space-only content, Unicode and literal HTML/Markdown punctuation. Each runs in
LF and CRLF documents with a second cell paragraph forcing HTML serialization.
After a presentation change it verifies full reopened shape, successful
complete-model source rebasing, exact Undo source/selection and Redo output.

A03 stays active for styled whitespace combinations, inline-code newlines in
HTML-required cells, and the final requirement audit. This is persistence
qualification, with no new native or performance claim.

`scripts/check.sh` passes 818 tests with two existing ignored tests, including
formatting, locked checks, strict Clippy, adapters and doctests
(`/tmp/mineral-a03-break-check.log`). All 53 source-fidelity tests pass; diff
whitespace checks are clean. Crusty context: `ctx_f13e8dc96aac`.

## Styled runs and code qualification

The matrix now applies plain, bold, italic, strike, code and link styles to all
eight values under LF/CRLF (96 combinations). It verifies complete-model source
rebasing, which checks rich runs and link destinations as well as text/topology,
and restores formatting operations through Undo/Redo too. The expanded test
reproduced multiline inline code ending the table's Markdown HTML block at a
blank line (`/tmp/mineral-a03-styled-before.log`). Encoding CR/LF as numeric
entities fixes this while retaining literal code characters; all 96 cases pass.

A separate presentation-edit regression reproduced the same truncation for a
code block inside a table cell (`/tmp/mineral-a03-codeblock-before.log`). Block
code and inline code now share literal HTML text escaping with numeric newlines.
The LF/CRLF fixture checks blank lines, trailing newline, language, a following
paragraph, complete-model rebase and exact Undo/Redo.

These cases supersede the styled-whitespace and inline-code qualification gaps
above. A03's final requirement audit remains outstanding; no native or
performance claim is added.

Expanded validation: `scripts/check.sh` passes 819 tests with two existing
ignored tests, including locked checks, formatting, strict Clippy, adapters and
doctests (`/tmp/mineral-a03-styled-check.log`). All 54 source-fidelity tests pass;
diff whitespace checks are clean. Crusty context: `ctx_1673cf260581`.
