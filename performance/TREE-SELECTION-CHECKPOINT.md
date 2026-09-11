# Semantic range editing in composed documents

September 8, 2026. Progress on L02/E10/X01–X03, not approval of the full
document grammar. The authority remains `designs/document-design-grammar.md`
and boards 01–07. Board 02's term-list specimen was inspected for this pass.

## Change and contract

`document-core::tree_selection` now resolves canonical source-order ranges
through definitions, quotations, alerts, footnotes, list items and table cells.
Copy, replacement and block paste share that traversal rather than requiring
both endpoints to be top-level blocks. This does not introduce a second
editable representation or put layout preferences into Markdown.

- Sibling paragraphs in the same text flow join around the replacement.
- Different semantic roles retain their unselected text in their original
  containers. Replacing across a term and description does not move the
  description suffix into the term. Separate list items and cells likewise
  remain separate.
- Fully selected interior content is removed. Untouched block subtrees retain
  their shared identities. Partially edited tables retain rows, cells, headers,
  column settings and metadata, with editable empty hosts in cleared cells.
- Rich extraction preserves inline styles and selected containers. An ordered
  fragment starting at item 8 still starts at 8; task states remain authored
  states. Partial table fragments keep the relevant rows and column matrix,
  leaving unselected values blank rather than copying or inventing them.
- Block paste can split a nested paragraph and insert headings, lists or code
  in its authored context, with unique IDs and a caret in the inserted content.
- Composition updates restart from the same baseline; cancellation and undo
  restore the original source and selection. Invalid UTF-8 endpoints fail
  before composition acquires ownership.
- Rewriting a source-owned definition list now restores its following block
  separator. Significant edge spaces created by splitting/pasting serialize
  as entities instead of silently disappearing on re-import.

## Automated evidence

The initial two regressions failed with the explicit top-level-only copy and
replacement errors. Subsequent tests exposed the missing definition separator
and trimmed split-paragraph whitespace; their exact round-trip oracles now pass.
The existing view selection test also caught a lost leading newline when an
endpoint lies at the end of a block; empty boundary text remains represented.

`crates/document-core/tests/tree_selection.rs` contains 11 integration tests,
including forward/reversed ranges, styled suffixes, ordered/task lists, partial
table copy and editing, nested block paste, composition, untouched subtree
sharing, and opaque selected content. One matrix exercises all distinct
endpoint pairs in a mixed heading/prose/definition/quote/nested-list/table
document in both directions: copy/re-import, edit/re-import and exact undo.
The matrix excludes blank placeholder cells from its text comparison; separate
table tests assert the complete cell identities, dimensions and empty hosts.

Current workspace result: **504 passed, 2 existing ignored**. Workspace
Clippy with `-D warnings`, rustfmt, diff whitespace checks and release build
pass. No dependencies, unsafe code or renderer hot paths were added.

## Native evidence

Release SHA-256:
`96d0c5cba4cc4f98d696449fad5ae44965c958beb27c5f58b9a5bd4e9003ef0e`.
Fixture: [63-semantic-range-editing.md](layout-fixtures/63-semantic-range-editing.md),
SHA-256 `e33671b259988652e05b643e7c3186258e0eae721aa569db307d120ad7a36108`.
All editing probes use private copies and real Wayland input.

- [Wide](layout-previews/tree-ranges-wide.png),
  [600 px narrow](layout-previews/tree-ranges-narrow.png), and
  [200%](layout-previews/tree-ranges-200.png) captures were inspected. The term
  rail stacks at narrow width; zoom preserves the relationship and readable
  type. These viewport captures do not cover all offscreen content.
- [Cross-column selection](layout-previews/tree-ranges-wide.selection.json)
  copies exactly `che\nReusable `, including the final space. The
  [native edit](layout-previews/tree-ranges-wide.edit.json) replaces it with
  `x`, matches the complete independently specified saved Markdown, and undoes
  to the exact original bytes. The [edited frame](layout-previews/tree-ranges-wide-typed.png)
  and [idle frame](layout-previews/tree-ranges-wide-idle.png) keep `Cax` in the
  term rail and bold `geometry.` in the description rail.
- [Block paste](layout-previews/tree-ranges-block-paste.edit.json) selects
  exactly `geometry.\nA second` across two descriptions and inserts an authored
  heading plus a two-item list through Paste as Markdown. It matches the
  complete expected saved source and restores the original bytes on undo.
  The [edited](layout-previews/tree-ranges-block-paste-typed.png) and
  [idle](layout-previews/tree-ranges-block-paste-idle.png) frames retain the
  inserted structure in the description column and move following content
  down by the added content height without overlap.
- [Two warm replans](layout-previews/tree-ranges-wide.planning.json) reuse
  published geometry with zero wrapping/shaping/segment layout and zero
  anchor displacement. This is a cache/anchor check, not an FPS benchmark.
- Weston MCP app 21 independently exposes the [semantic tree](layout-previews/tree-ranges.weston.json).
  Cache is `(276,330,88,24)` and its description is `(388,330,537,24)`:
  equal tops and a 24 px rail gutter. The preceding H2 ends at y=306, leaving
  24 px before the row. Viewport ends at y=442; the next H2 begins at y=506,
  retaining the 64 px section gap. Definition roles do not become outline
  headings. The explicitly pasted H2 does become an outline entry.

The native-desktop skill shaped the validation around source integrity,
selection, focus, undo and live reflow—not just attractive static captures.

## Remaining work and limits

The initial Weston MCP frames showed provisional stacked geometry until later
input/frames published the measured composition. The controlled follow-up
qualifies that observation: with this same `96d0c5cb` binary, both fixture 57
and fixture 63 paint their measured layouts before input at 120 Hz. Fixture 63
has both aligned term rails and its numbered row, with zero changed document
pixels after the pointer moves (`startup-definitions-periodic.idle.json`).
Changing only the compositor to zero refresh reproduces the stall for both
fixtures (`startup-*-zero.log`). This is a zero-refresh capture-environment
counterexample, not evidence of a definition-specific application defect.
No speculative app repaint workaround was added. Other startup/resource/failure
cases remain unverified.

Native plain copy and Paste as Markdown are verified here. Cross-application
rich-MIME publication/round-trip, the complete native table/IME/RTL matrix,
image-boundary block paste and exotic attributed HTML remain unverified or
unsupported. Nested glossary rail selection, all family states, broad spacing
and token coverage, deep trees, metadata, metrics, records, true wrapping,
prose columns, footnotes, media, page masters and exports remain in the full
coverage ledger. No claim of complete board fidelity is made.

Scrolling was not rebenchmarked in this editing-only pass; previous FPS results
remain historical evidence. The full current-build 10 MiB, grammar-heavy,
timing-tail and recovery acceptance work remains open.
