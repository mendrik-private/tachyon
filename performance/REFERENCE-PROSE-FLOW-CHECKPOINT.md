# Reference prose flow — September 10

## Contract before implementation

User priority: improve document appearance and use the available canvas without
stretching ordinary prose into unreadably long lines. Native fixture27 shows
four complete reference-font paragraphs after a formula occupying only the
leading reading column. Existing reading bands require narrative typography,
even though the paragraphs are ordinary continuous prose.

Extend the existing bounded source-linked flow, not the source model or font
mode. Measure each passage using its actual reference/narrative font. Headings,
equations, code, tables, authored hard breaks, captions, metadata, notes,
resources and other specialized presentations remain boundaries. Reference
passages require multiple paragraphs and sufficient measured lines; short
instructions remain stacked. Narrow/short/large-text surfaces retain readable
fallback. Preserve source order, focused anchors, selection, editing and Undo.

Native before: `reference-flow-before`, fixture27 at 1600×1200 light,
runtime `c843a87868574c406653af6efe109e20b578540af75f8b9f068ae5081172a98b`,
231 active AT-SPI nodes; source SHA-256
`a6a2ca1d038460fac618de55484181e7688ede02c013f155e1345adfb0b08f9a` unchanged.

## Implementation and regression findings

`editor/prose_flow.rs` now separates ordinary-prose eligibility from typography.
Each source-contiguous same-mode passage uses its own loaded-font measure for
the existing 40-character floor, 84-character cap, two/three-column selection,
and bounded balance alternatives. Specialized paragraph roles are explicitly
excluded. Existing source-linked fragments, three-line split guards, 24px gaps,
height limits, edit locks and warm measurement reuse remain in use. Retention
also checks the published reading mode, so unchanged plain text cannot conceal
a typography change.

Reference candidates require at least two paragraphs, six shaped lines per
column and three lines per paragraph on average. The first implementation
counted only aggregate lines; repeated one-line paragraphs then qualified and
compressed the scroll range. Four existing HTML-navigation/image-arrival anchor
tests caught this. A minimal repeated-short-instruction regression failed
before the average-line guard and passes after it. All four existing tests
pass unchanged; no anchor or navigation implementation was modified.

The first new coverage assertion incorrectly expected a code fence's terminal
newline to have a painted glyph range. It now checks exact coverage of the
changed prose flow, with whole-document byte fidelity and canonical semantics
checked separately. The pixel checker checks ink within each 24px line box;
the first ink row varies with glyph ascenders and is not itself a line baseline.

## Final native evidence

Runtime: `766a66e7bb7e332e8222cab6599a9cb6fe181ba43b41a0e93fdae1d7e8e3018a`.
All final captures use the unchanged fixture27 hash above on isolated Weston,
requested scale120 (1x), not the user's physical desktop. Prefixes below are
under `layout-previews/`.

- `reference-flow-verified-wide`, 1600×1200 light: two 639px text columns,
  24px gutter, 1302px span on a 1314px canvas (99.09%). Before/after paragraph
  widths and line-box heights are identical. Column boxes are 192/168px;
  actual glyph extents are 192/171px. The passage saves 192px of vertical
  space without changing type, source or surrounding section spacing.
- `reference-flow-verified-regular`, 1280×1200 light: the same four paragraphs
  form two readable columns. Native screenshot reviewed at original size.
- `reference-flow-verified-narrow` (768×1200), `-short` (1600×480), and `-200`
  (1600×1800 dark, 200% reader zoom) stack in canonical order. Narrow and dark
  final images are pixel-identical to the reviewed intermediate captures;
  wide/short differences are confined to the transient outer scrollbar.
- `reference_prose_flow_check.py` compares all **82 canonical document nodes**
  across before/after and stacked cases: IDs, parent paths, roles, labels,
  descriptions and actions match. It checks actual glyph rows in both columns,
  balance, gutters, source/runtime identity and height savings. It deliberately
  fails when given the stacked baseline as the supposed improved result.
- `reference-flow-verified-actions`: all ten headings available before scroll,
  offscreen reveal with stable IDs/order, table headers/cells, list traversal,
  formula source, task activation, exact task-source change and Undo pass.
- `reference-flow-verified-word-boundary`: pointer placement plus Home and two
  Ctrl+Shift+Left actions selects/copies exactly `body.\n` across the column
  boundary. The exploratory one-step golden incorrectly assumed word-plus-
  double-newline selection; the editor's punctuation boundary and single
  projection separator are unchanged.
- `reference-flow-verified-edit`: typing at the right-column paragraph start,
  focused idle, blur, complete expected autosaved Markdown and byte-exact Undo
  pass. The golden includes the existing punctuation escaping of the edited
  paragraph; unrelated source bytes remain exact.

All listed native source checks pass. Preserved HTML's light surface in the dark
capture is unchanged; this checkpoint does not qualify its theme treatment.
Broader real-document/RTL/IME/inline-math flow coverage, single-long-reference
paragraphs, full grammar/page/export variants and performance remain open.

## Verification and pending framework staging

`scripts/check.sh` passes: formatting, workspace all-target check, strict Clippy,
740 workspace tests plus five adapter tests (745 total; two existing ignored),
and doc tests. Log: `/tmp/mineral-reference-flow-final-check.log`. Seventeen
capture-harness Python tests pass. Two Rust tests were added and the existing
focused editing/resize/Undo test now also covers reference prose.

Crusty layout validation `task_1167951eb7f97662` against `ctx_c56813aad733`
reports 75 existing inferred findings, none new/worsened/resolved. The earlier
GPUI-staging context `ctx_6cb2d58ac4eb`, validation `task_3955696117254135`, has
an expanded indexed inventory (37→75) and one widened environment-read finding
whose locations include untouched upstream build/example sources. No application
configuration reads changed. These are advisory findings, not approval to
refactor upstream GPUI.

Retained-accessibility framework work remains unfinished and paused for layout
work. GPUI Rust sources still exactly match the pinned upstream package; only
its manifest normalization and provenance note differ. Rechecked complete
dependency topology/features retain the same 968-node normalized graph hash
recorded in `vendor/gpui/MINERAL-PATCHES.md`. No retained API, framework test
harness, startup responsiveness fix or scrolling speedup is claimed. A07, A16
and the full audit remain active.
