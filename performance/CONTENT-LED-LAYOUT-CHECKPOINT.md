# Content-first technical layout — September 10

## Contract before implementation

Layout remains the active priority. The native README shows its keyboard
table at the leading edge with a large empty trailing area, followed by three
explanatory paragraphs. Allow a table or code block followed by a complete,
bounded explanation in the same section to negotiate a two-column module.
The component stays first, the prose second; any section heading spans both.
Do not move prose before its component, add a card, rewrite content, shrink
type, or split/repeat the table. This is a paired module, not text wrapping.

Reuse the twelve-track templates, loaded-font measurements, shared gutters,
overflow/viewport gates and edit locks. Require useful prose (at least four
reference lines), balanced measured heights (at most 2:1), and a comfortable
prose measure. Reject separate sections, authored captions, prose owned by a
following example or margin note, and incomplete/unbounded prose groups.
Narrow or short surfaces and large text stack in original source order;
returning space can recover the pair after any focused edit is released.

Acceptance: unchanged README-section fixture before/after native pixels,
whole-document holdout, narrow/short/dark200 fallback, complete source-order
coverage and copy, native prose editing/autosave/exact Undo, focused geometry
and repeated resize regressions. Existing figure-first and explanation-first
layouts must retain their own semantics. Unit tests use GPUI mock shaping;
only native captures qualify actual fonts. Full grammar/audit remains open.

## Implemented and verified

`ContentExplanation` is a separate measured row kind. Nomination accepts one
table/code root, an optional H2-or-lower heading, and one through four complete
following paragraphs in the same section. Existing sibling sections retain
ownership. Captions, H1 openings, next-example introductions, anchored notes,
long/incomplete groups and non-prose structures are not borrowed into the pair.
The normal 900px canvas floor, native overflow checks, four-line prose minimum,
2:1 height balance and 70%-of-viewport height gate decide actual fit.

The first native implementation left a 184px internal gap because the table
rendered narrower than its allocated track. Final candidate sizing uses the
table's actual fitted width; type, table cells and table dimensions are
unchanged. The final table is 596px wide, the prose track about 419px, and the
visible gutter 24px. This uses about 79% of the 1314px document canvas with
readable prose; the heading still spans the canvas. Whitespace below the
completed document is not treated as missing content or a reason to enlarge it.

Fixture115 copies the complete README keyboard section, including its three
paragraphs, and adds a following-section boundary. In native 1600×1700 light
captures, that next heading moves from y1217 to y881: **336px saved** without
shrinking the table, changing text or lowering font sizes. Native measured
column heights are 697/480px. `content_led_layout_check.py` independently checks
unchanged table bounds, the real 24px gutter, newly occupied glyph pixels,
source/runtime identity, fallback and saved edit/Undo evidence.

Evidence under `layout-previews/content-led-`:

- `before`: original stacked fixture on runtime `c5d22822…f64f`.
- `wide`: intermediate oversized-gutter counterexample, preserved as evidence.
- `fitted-wide`: accepted table-first layout and detailed measured trace.
- `narrow` (520×2000), `short` (1600×480) and `dark200` (1600×2400): original
  source-order stacks; short-window/offscreen content is checked through
  semantic geometry, not claimed to be fully visible in the screenshot.
- `readme`: unchanged full README, 1600×1700 light, twelve wheel steps; the
  complete keyboard table and explanation form the same pair. Private-copy
  and original-file provenance both pass. This is not a same-viewport comparison
  to the older 1280px README capture.
- `prose-edit`: native insertion at the new right column, exact whole-file
  autosave expectation (including existing punctuation normalization), one
  second focused idle, blur and byte-exact Undo. Idle pixels retain the pair.
- `copy`: seven unique markers appear once in source order across the table,
  explanation and next heading. Not a whole-clipboard/rich-MIME claim.
- `code-wide`, `code-narrow`, `code-dark200`: fixture116's complete TOML example
  and following prose pair at wide width and stack at narrow/200% widths.
  Language/Copy chrome, line numbers, code contents and source order remain.
- `code-edit`: native insertion in the left code pane, exact whole-file
  autosave, focused idle, blur and byte-exact Undo.

All native source and appearance checks pass. Wide/narrow/large-text,
whole-README and both edit-idle screenshots were inspected at original detail.

Final runtime SHA-256:
`dbce54a291bbb3d9ffd053bc79cee8461fd3d2590ee2374a24a388a4335d4e89`.
Fixture115 SHA-256:
`ee931aaa64a4175a865226896a4df4b463038c2c367074af7298d53f9398ded4`.
Fixture116 SHA-256:
`0998f8d175a10d1707cec3e15d9bb66210abf154301d9db3d64da73436e995ca`.
README SHA-256:
`abca9115a24f8b4293074b510011acfc5d11a036f910ba68d77e6f0054527541`.

Three new Rust tests cover semantic negatives, exact source partitions,
100/150/200% geometry and repeated narrow/short recovery, focus-delayed
expansion, growth beyond the nomination budget, retained column widths and
exact Undo. These are mock-shaping regressions, not native-font qualification.
`scripts/check.sh` passes formatting, all-target checks, strict Clippy,
**737 Rust tests (two existing ignored)** and documentation tests. Final log:
`/tmp/tachyon-content-led-final-check.log`.

The UX skill drove actual-footprint alignment and width/zoom checks; Rust
guidance kept this in the source-backed planner with existing measurement and
editing ownership. No new renderer, dependency, source syntax or manual layout
control was introduced. Full mixed-document/state/RTL, continuous native
resize, performance, page masters/export and the wider audit remain open.

Crusty validation `task_09c1217207a146b5` against `ctx_d665f99cc3c0` completes
with 37 existing architecture findings and none new, worsened or resolved.
A07 remains active; this checkpoint does not close P02 or the full goal.
