# Unlabeled list rows

2026-09-10. Layout-first A07 continuation. The bounded Markdown import/layout and
plain-text insertion paths below are implemented and verified. A07 stays active.

## Contract

An authored empty Markdown list item or an item beginning directly with a nested
list must keep its own visible marker and editable blank row. Import supplies an
empty paragraph caret host, as already done for empty tasks. This is not a title
or a summary; it contains no authored text. It precedes, rather than replaces or
borrows text from, the nested list. Canonical list/item identities, order and
depth remain intact. No host is added before existing prose, code, figures,
quotes or tables.

These rows participate in the existing pressure-triggered tree layout. Deep
branches retain the 64 px hierarchy rail when ordinary indentation would leave
less than 320 logical pixels. Comfortable widths retain ordinary nesting.
Blank-parent context uses a role description, not a child's label. Empty rows
remain full-height caret targets, including at increased zoom.

Unchanged source must remain byte-identical. Editing a populated descendant must
preserve the original empty-parent syntax. Typing into a blank host must save a
real introduction at that item, retain descendant semantics, and undo exactly.
Tests must cover import, actual measured layout, local typing versus full layout,
and native narrow/wide/zoom and edit paths before qualification.

Full structural editing, arbitrary semantic HTML/enclosure combinations, RTL,
IME, complete accessibility, paged/static output and release performance remain
separate open work. This checkpoint does not complete L06 or the audit.

## Implementation and red/green evidence

The importer previously emitted no paragraph for ordinary empty items and began
list-first parents directly with the child list. Projection therefore lost the
parent's own marker row and the outline eligibility scan rejected the entire
tree. Import now supplies an empty paragraph in precisely those two cases.
Existing projection, measurement, hit testing and parent-context paths handle it;
no width threshold, second document model or authored label was introduced.
Blank-parent captions say “Within an untitled item”.

`cargo test -p document-core --test source_fidelity --locked unlabeled_list_items`
failed before the change on the minimal `-\n` source: no own blank caret row.
`cargo test -p document-view --locked unlabeled_branches` failed on missing width
recovery. Both now pass. At 360 logical pixels, eighth-level prose recovers
128 px: 288 px of text space rather than 160 px.

The stricter parent edit test then reproduced untouched-child rewriting:
`Child-text.` became `Child\-text\.` when an introduction was entered. The source
reuse path now verifies a marker-only original first line and inserts its new
paragraph there, collecting descendant patches independently. Marker dialect,
spacing, outer rails and line endings remain source-owned. Unsupported or
structural changes retain the semantic serializer fallback; no edit is discarded.

## Verification

- Native-font geometry at logical widths 360/620/1600 and 100/150/200%: each of
  three empty rows has its own marker, identity and full line height; deep text
  fits its recovered measure; wide trees retain ordinary nesting.
- Retained typing versus full-layout matrix includes the eighth-level child and
  returning branch at widths 360/1280 and all three zoom levels.
- Core tests cover empty items, list-first parents, descendant edits, and typing
  into the host. Forty source-equality/reopen/Undo/Redo cases cover five marker
  dialects, LF/CRLF, quote nesting and marker padding. This matrix qualifies
  single-line introductions, not arbitrary multi-paragraph or structural input.
- `scripts/check.sh` passes format, locked all-target checks, strict Clippy,
  workspace tests and doctests: 708 passed, two existing ignored tests.
  Log: `/tmp/tachyon-unlabeled-check.log`. `git diff --check` is clean.
- The UX skill informed readable hierarchy without generated content; the Rust
  and diagnosis skills informed canonical caret ownership and failing regression
  tests at the import, measured-layout and exact-save boundaries.

## Native evidence

Runtime SHA-256:
`522a21fbef7c8f4901a4a237541e57a35113d73948fa12f9b7de8f161bec9be2`.
Fixture `109-unlabeled-tree.md` SHA-256:
`b59eddb725e2bdb2d9b0c29391c931552cdaf004f6d0154294f88c0cba053678`.
Captures use private fixture copies under `layout-previews/`.

| Prefix | Qualified result |
| --- | --- |
| `unlabeled-source-baseline` | Original runtime `bda186dc…`, corrected fixture, 400×1800. Missing blank markers and cramped deep explanation. |
| `unlabeled-narrow` | 400×1800 light. Three visible blank rows, full child text, explicit parent/depth context and readable branch returns. |
| `unlabeled-wide` | 1600×1700 light. Ordinary nesting and all blank rows retained. |
| `unlabeled-dark-200`, `unlabeled-dark-scrolled` | 1000×1900 dark, 200%, initial and scrolled views. Both empty parents, complete deep explanations, empty sibling, returning branches and following section inspected across the two captures. |
| `unlabeled-parent-edit` | Click first blank parent, paste `Introduction`, exact whole-file autosave, one-second focused idle, exact Undo. Caption updates in place; descendant bytes stay untouched. |
| `unlabeled-child-edit` | Click/Home/type in the deepest explanation, exact whole-file expected save (ordinary escaping only in edited paragraph), focused idle and exact Undo. |
| `unlabeled-empty-edit` | Click/type in the empty sibling, exact marker-line insertion, focused idle and exact Undo. |
| `unlabeled-copy` | Twelve unique markers copied once each in canonical order, without duplicated caption text; unchanged source. This is not whole-clipboard equality. |

All final captures pass unchanged-source checks. AT-SPI records supplement the
layout evidence; they do not constitute complete screen-reader qualification.
The parent-edit run logged `wl_display_dispatch: Broken pipe` during teardown,
after successful checks, and exited zero.

Earlier `unlabeled-before` and `unlabeled-baseline` use exploratory fixture
spellings: a marker without a separator can parse as setext/lazy paragraph
syntax. They are not final comparisons. `unlabeled-dark-return` repeats the
initial view because only a step count was supplied; the actual scroll capture
is `unlabeled-dark-scrolled`. An initial multiline core probe expected literal
LF to reopen unchanged despite Markdown soft-break normalization; the passing
matrix deliberately qualifies single-line introductions only.

Crusty context `ctx_9862fe4e248e`; code validation `task_a2357183f637beb1`
completed with 37 existing advisory findings, none new or worsened. Local checks
ran separately. Remaining hierarchy, interaction and audit scope stays open.
