# Complete technical sections — September 10

Contract before implementation. Continue layout-first A07 without narrowing
the full grammar goal. Compact related technical siblings should keep a short
authored continuation below their table/code when placed side by side. The
continuation stays inside its original section and in source order; it is
not extracted as a separate sidebar, dropped or moved below both sections.

Retain current heading-parent/level boundaries, native width and overflow
checks, viewport height and peer balance limits, work bounds and edit locks.
Tables/code remain block-level; no new cards, controls or source syntax.
Long sections, chapters with subsections, warnings and incompatible structures
remain ordinary flow. Narrow/short surfaces stack complete sections. Matching
component tops remain shared anchors despite different prose lengths.

Fixture112 is a complete configuration table and code example with leading
and trailing explanations. Acceptance: a measured wide technical pair contains
all eight roots, source coverage exactly once; both continuations remain below
their component; native narrow/large-text fallback, focused trailing-paragraph
growth, autosave/exact Undo and next-section separation. Native before/after
captures must show the actual full-page space benefit. Full grammar and
state/RTL/paged/performance matrices remain open.

## Implemented and verified

`compact_section` now accepts a trailing paragraph in an otherwise eligible
technical section. It remains part of the complete source-backed entry, not
a separately nominated explanation. The existing four-root/1600-byte bounds,
parent and heading-level checks, measured fit, peer balance, overflow checks
and focused edit locks remain unchanged. Extra continuations beyond those
bounds, child headings, warnings and ordinary chapters still reject pairing.

The native 1600×1700 light fixture places both section headings at y=256,
with a 24px gutter. The left entry is 533px wide and the right 756px wide.
Each trailing explanation follows its own component. The following “Review
the result” heading moves from y=1076 in the baseline to y=691: **385px less
vertical space**, without reducing type, omitting text or rewriting source.

Evidence under `layout-previews/technical-tail-`:

- `before` and `wide`: native baseline and complete-pair result.
- `narrow` (520×2100) and `short` (900×480, scrolled): ordered complete stacks.
- `dark200` and `dark200-bottom` (1280×2100): large-text dark fallback, with
  the second capture showing the trailing content below the initial viewport.
- `control`: fixture79 retains its technical pairs and separate chapter.
- `left-edit` and `right-growth`: native trailing-paragraph edits pass exact
  expected whole-file autosave, one-second focused idle, blur and exact Undo.
  Right-side growth adds 1935 bytes, exceeding the cheap nomination limit;
  its focused entry remains stable and the following section stays below it.
- `copy`: eight unique source-order markers appear once, in order. This is
  a marker-order check, not whole-clipboard or rich-MIME qualification.

All captures pass their unchanged-source and appearance checks. Native
screenshots were inspected, including both edit-idle views and the short
scrolled surface. Runtime SHA-256:
`f2fdcc296b430011dba66dc3b5d40c539e86ac8d7f0b912975c46806f7d5e1b7`.
Baseline runtime: `d53f21db34b7d69e5e002f07f8caade15bbbd68a9cd475552964aff5d0047121`.
Fixture112 SHA-256:
`677755f8fafcf0c76929ab639003fb27b66f7afea51b44e44926f59a792f9d7a`.

Two new native-font geometry tests cover complete entry ownership, source
range coverage, realized versus measured heights and repeated wide/narrow/
short recovery at 100/150/200%. Existing row-local typing/full-rebuild
equivalence coverage now includes both trailing paragraphs at narrow and
wide widths and all three zoom levels, including exact geometry restoration.

Test development exposed three oracle assumptions, not additional runtime
defects: code glyph ranges omit line terminators; narrow fallback need not
have an arranged slot; and fixture12's formerly separate technical sections
can now legitimately share a planning window. The scoped-reflow test now
adds an explicit unrelated H1 appendix. Its original offscreen invalidation,
focused-window, resource-generation and Undo assertions remain intact.

`scripts/check.sh` passes metadata, formatting, all-target checks, strict
Clippy, **723 Rust tests (two ignored)** and doctests. Final log:
`/tmp/tachyon-technical-tail-final-check.log`. Later changes to the scoped
test fixture affect tests only, not the captured runtime.

This completes a bounded technical-composition case, not P02 or A07. Full
mixed-document/state, RTL, accessibility, paged/export and current-runtime
performance qualification remain open. No new renderer, dependency, source
syntax, density control or search-on-scroll behavior was introduced.
