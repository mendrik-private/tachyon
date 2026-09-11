# Quotes, notes and tables can lead a branch

2026-09-09. Layout-first continuation of Audit A07. The full audit remains active.

## Layout

A list item beginning with a quotation, alert or table no longer disables the
whole outline's pressure-triggered tree layout. Its first authored text leaf
supplies the bounded parent excerpt. A table uses its first cell, preserving the
actual row/column structure rather than inventing a prose title or converting it
into a card. Source order, depth and markers remain canonical.

The first container's identity is retained separately from its text label. Parent
captions, markers and local guides use the container's published outer geometry,
outside quote rails, note headers and table borders. The table caption is external
row spacing: all first-row cells retain the same 10px top/bottom padding. Ordinary
wide layouts retain their natural indentation, but their container-leading markers
also sit outside the component. Checkbox clipping uses the marker's own mask.

At a 360px logical canvas the eighth-level prose gets 288px of usable width:
64px hierarchy rail plus the ordinary 8px trailing inset. That recovers 128px
compared with its original eight-level indentation. Containers retain their own
semantic insets; this does not stretch prose without a readable-width limit.

The UX skill informed shared outer anchors and separation of hierarchy guides
from component chrome. The Rust skill informed source-backed identities, reuse
of published geometry, and native measured/editing regression tests.

## Verification

- The new measured regression failed on the old whole-tree eligibility gate,
  then passed with the implementation. It covers quote/note/table parents and
  their children at logical widths 360/620/1600 and 100/150/200% font measurement.
- Assertions cover parent identity, pressure selection, real shaped line fit,
  panel containment, caption clearance from preceding text, equal table-cell
  insets/row height and unchanged Markdown. Existing source-led, enclosed and
  mixed-tree tests also pass: seven compact-tree tests in total.
- The retained-typing/full-geometry matrix now includes the quote and note body,
  the container-leading table header and both note/table explanations, at
  widths 360/1280 and all three zoom levels. The table target explicitly requires
  a container-leading cell; an earlier generic `Reading` lookup selected another
  fixture and was corrected rather than weakening the arranged-node assertion.
- `scripts/check.sh` covers formatting, locked all-target checks, strict Clippy,
  workspace tests and doctests. The completed workspace pass has 461 view tests
  passing (two ignored), 116 core, 25 source-fidelity, 11 tree-selection, one
  external-link and 39 app tests. The final table-target refinement also passes
  its focused retained-typing test. `git diff --check` is clean.
- Crusty context `ctx_1393c492871c`, preparation `task_1894d8e17330ec7b`,
  implementation validation `task_105cabde38f01c1c`: completed, 36 existing
  advisory findings, none new or worsened.

## Native evidence

Runtime SHA-256:
`06880f614f6578f8f4c1f96b776ccbd93e8036d7080b9cd1ec2c3e49c3cca28d`.
Fixture `96-container-first-tree.md` SHA-256:
`92c7e1d667edb22df29cea0dbd6f1ea9b35fa2cc6a24805cccb6c8bbc5e76982`.
Artifacts in `layout-previews/` retain runtime/source/input sidecars and use
isolated native sessions with private source copies.

| Prefix | Evidence |
| --- | --- |
| `container-narrow` | 400×1400 light, lower outline and following heading. Inspected: outside markers/guides, readable deep prose, unobstructed note header and table cells. Source/appearance pass. |
| `container-wide` | 1600×1800 light, complete outline. Inspected: ordinary indentation, bounded prose, natural compact table and no invented parent captions. Source/appearance pass. |
| `container-dark-200` | 1000×1600 dark at 200%. Inspected: note/table child context, full table relationships, branch returns and following section. Source/appearance pass. |
| `container-quote-verified`, `container-note-verified` | 400×1400 light native click/Home/type/autosave/idle/undo. Complete expected edited-file equality and exact original-file undo. |
| `container-table-edit` | Same sequence editing the leading `Reading` header; full edited-file equality and exact undo. Inspected typed pixels show the child caption updating without moving the branch. |
| `container-copy` | Nineteen unique markers occur exactly once in canonical order, including parent text also used in visual captions. Source/appearance pass. This checks marker order, not entire clipboard equality. |

For quote/note edits, the expected edited paragraph uses the existing canonical
`escape_inline` punctuation spelling (`changed\.` / `record\.`). Every byte
outside that edited leaf is checked against the original file, including untouched
children. The initial `container-quote-edit` and `container-note-edit` expectations
omitted this established escaping and failed; they are diagnostic only. No saving
implementation was changed in this checkpoint.

`container-before` uses previous runtime `85d42112` and shows squeezed deep prose.
`container-first` predates the guide alignment correction and is not final evidence.

## Remaining

Container combinations whose first projected content is itself a list, empty or
non-textual parents, arbitrary nested enclosure combinations and richer table
cells need further qualification. This does not sign off all hierarchy contexts,
structural edits, native task interactions, RTL/IME, resize/accessibility,
other grammar families, static/paged export or sustained release performance.
