# Source-backed inline enumerations

2026-09-09. Layout-first continuation of Audit A07; full grammar and audit
acceptance remain open.

## Presentation and scope

Explicit introductory paragraphs such as `Consider: (a) ...; (b) ...; (c) ...`
can now use measured open columns. The introduction remains attached above the
clauses, with a 16 px gap; additional clause rows use the shared 24 px gutter.
No labels, headings, punctuation or canonical list nodes are invented. Every
fragment refers to an exact byte range in the original editable paragraph.
Code/link punctuation remains inside its rich-text run rather than becoming a
clause boundary. Layout alone does not rewrite Markdown.

Recognition is deliberately bounded: an explicit colon introduction, 2–12
semicolon-delimited clauses, at most 4096 text bytes, a 160-byte introductory
prefix and 512-byte clauses. Comma-only sentences, missing introductions,
empty/unfinished clauses, quoted or nested ambiguous delimiters and unsupported
rich media retain ordinary flow. Native measured line counts, overflow and
height balance decide whether a recognized paragraph actually gains columns.
Strong RTL text does not nominate this new presentation yet.

The UX skill informed the shared anchors, readable measures and attached
introduction. The Rust skill informed source-range ownership, bounded work and
geometry-cache/editing tests. Candidate diagnostics are not geometry inputs;
published fragment anchors and row capacities are.

## Editing regression found during native review

The first native editing preview exposed a real focus transition defect:
the outer row planner installed a generic focused-stack slot before internal
clause retention, collapsing the paragraph. A focused-plan regression test
failed before the fix. Retained internal fragments now take precedence over
that generic slot, just as other source-backed internal flows do. Clicking a
clause, Home, typing and focused idle retain the column geometry.

The debugging skill guided the native repro and targeted planner probe. Its
temporary `[DEBUG-inline]` instrumentation was removed. A separate failed
protected-fragment assertion included parentheses which the existing edited
Markdown serializer escapes. The qualified invocation protects the neighboring
words, checks insertion inside the intended third clause, and still requires
byte-exact whole-file undo. It does not claim byte-identical untouched Markdown
punctuation after an edit; layout-only preservation is tested independently.

## Automated evidence

- Recognition/source-range tests preserve every byte and rich span, reject
  ambiguous prose and enforce count/byte bounds.
- Native-font geometry tests cover logical widths 360/620/1280 at 100/150/200%,
  complete source coverage, exact track limits, row gaps, focused retention and
  cache invalidation. The test font environment promotes three wide paragraphs;
  the actual application fonts reject the uneven labeled example, correctly.
- Retained typing-growth/full-geometry comparison includes the introductory
  fragments of fixture 89 at 100/150/200%, bounded local shaping and undo.
- `scripts/check.sh` passes formatting, locked all-target workspace checks,
  strict Clippy, tests and doctests: 445 document-view tests pass, two ignored.
  `git diff --check` is clean.

## Native evidence

Fixture: `layout-fixtures/89-inline-enumerations.md`, unchanged SHA-256
`5ef39259d50ce89f130b601132558463f91f298960631e67fe300bf994ff12e1`.
Baseline binary:
`ffd5681d3499be828ee25cbb9ad1e705bb41d5f7577ea977a9db1c5323e6549b`.
Final binary:
`90791b78f42fc4252bd09708d4af90864c115f2a2bfbce9e3c28f844cca511dc`.
Captures use private source copies and native Wayland input, not user files.

Artifacts in `layout-previews/`:

| Prefix | Evidence |
| --- | --- |
| `inline-enumeration-before` | 1600×1200 light baseline; all three paragraphs remain narrow flowing text. |
| `inline-enumeration-edit-final` | Final wide light layout: evidence clauses and literal code/link clauses each use three columns; the uneven labeled example remains a paragraph. Third-clause click/Home/edit/idle, unchanged neighboring words, autosave and exact undo. Thirteen unique clipboard markers retain source order. |
| `inline-enumeration-narrow-final` | 600×1100 light; ordinary readable paragraph flow, source unchanged. |
| `inline-enumeration-200-final` | 1600×1200 dark at 200%, scrolled; readable fallback, source unchanged. |
| `inline-enumeration-rich-selection-final` | Click in the second literal clause's text, Home and four Shift+Right presses select and copy exactly `A; B`, including its protected semicolon. Source remains byte-identical. |

Intermediate `inline-enumeration-edit-diagnostic-typed` is the failed focus
transition witness, not acceptance evidence. `inline-enumeration-focus-fixed-typed`
shows the repaired focused columns. Clipboard marker order does not prove exact
whole-document clipboard equality; active AT-SPI is supporting evidence, not a
complete screen-reader test matrix.
The initial rich-selection probe clicked directly on the active hyperlink,
leaving the editor caret at the title; it is not selection acceptance evidence.
The qualified probe enters through the adjacent clause text before navigating.
The final screenshots, including typed/idle and selected states, were inspected.

## Remaining work

Additional authored label/stage conventions, nested inline enumerations, the
full keyboard/IME/RTL/structural-edit/resize/accessibility matrix and further
grammar families remain open. Static/paged export and sustained release
performance qualification remain separate open gates. This checkpoint adds
one conservative paragraph presentation, not semantic splitting of prose.

Crusty context `ctx_eab805fc18ee`; final source validation
`task_8f98058000879fdb`: 36 existing advisory findings, none new or worsened
(local checks run separately).
