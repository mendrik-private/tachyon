# Source-backed schema trees and complete technical explanations

2026-09-09. Layout-first Audit A07 continuation. T09 is now partial; the full
document grammar, audit and release-performance qualification remain open.

## What changed

Explicitly identified JSON Schema fences have a bounded structural tree view:
original fields, values, object/array nesting, aligned value columns and quiet
nesting guides. Labels use the bundled 14px monospace font, 26px rows, 20px depth
steps, 24px label/value separation and 16px insets. Labels are actually shaped
before sizing the figure; text is never scaled down to fit. Schema trees align
with their component's leading edge; Mermaid figures retain their centering.

This is a structural JSON view, **not a schema evaluator or a condensed property
summary**. Unknown keywords, literal reference destinations, duplicate ordinary
keys, empty containers and exact number spellings survive. The field traversal
is authored order, not a sorted map. JSON strings retain visible quotes/escapes.
All canonical Markdown and code remain owned by the existing document; the
derived figure is not a second editable model.

Recognition requires a `json` fence with an explicit `$schema` key and a supported
dialect URI (2020-12, 2019-09, draft-07/06/04). Ordinary configuration JSON,
unknown/ambiguous dialects and malformed input keep the source pane. Recognition
is conservative about escaped spellings of the `$schema` key. The renderer is
limited to 8192 source bytes, 64 rows, 12 nested levels, 1600px width and two
million logical pixel area. Exceeding a bound retains the full source, never an
ellipsis or partially displayed tree.

Reference URIs are not fetched; SVG has no file/network/image resolver, and the
existing pipeline turns bundled-font text into inert vector outlines before
painting. The shared bounded figure cache distinguishes Mermaid from schema
entries. This adds the already locked workspace `serde_json` dependency to the
view crate with `raw_value`; no new package version is introduced. Borrowed raw
values preserve number lexemes without globally enabling ordered JSON maps.

The dialect boundary follows the [official JSON Schema basics](https://json-schema.org/understanding-json-schema/basics).
The rendering deliberately does not interpret the validation meaning of
[`properties` and `required`](https://json-schema.org/understanding-json-schema/reference/object).

## Available-space composition

The first native schema capture was legible but stacked a two-paragraph
introduction above its tree, leaving the wide canvas mostly empty. Review found
two nomination limitations: explanation groups accepted one paragraph, and
compact technical-section merging unconditionally disabled their inner
explanation/example candidate.

Contiguous introductions of one to four short paragraphs (each at most 420
bytes) now form a complete source-order group with their adjacent code, table or
single figure. Heading, section and gallery boundaries remain intact. The row
planner compares the entire explanation/example layout with whole-section peer
alternatives instead of suppressing either. The introduction's first root is
explicit, and partition validation checks the complete range.

Regression tests exposed two required safeguards during implementation. A short
colon-ended code label is measured separately even when it follows other prose;
it stays above the code. A compact technical sibling needs at least four
measured explanation lines to nominate its own reading column. Brief section
descriptions retain their existing whole-unit peer composition. The README label
test and technical-section narrow-to-wide recovery test pass unchanged.

Fixture 100 at a 1600px window has 1314px of document canvas. Its complete
explanation/tree pair uses 422/868px slots with a 24px gutter, measured heights
264/546px, and no invalid row fallback. Ordinary prose remains constrained;
the tree uses its natural readable size inside the wide component slot.
The first section's stack measured 808.5px before the composition correction.

## Verification

- New deterministic tests cover ordered fields, parent links, exact numeric
  spelling, unknown/duplicate members, empty containers, dialect rejection,
  malformed/oversized/deep/wide input, inert vector output and light/dark geometry.
- Native-font geometry checks 360/760/1312/1632 logical widths and 100/150/200%
  font environments; both schema previews cover their canonical code ranges,
  ordinary JSON remains code, accessible alternatives retain hierarchy.
- The wide composition regression first fails because no complete explanation
  pair exists. It now proves both paragraphs participate, peer candidates remain
  available, narrow tree layouts stack, and row ownership validates.
- Group tests cover one to four paragraphs with/without a heading, section/rule
  barriers, complete galleries, long paragraphs and bounded recognition.
- Native pointer activation exposes canonical code. Editing a valid leading
  space in the first JSON fence, and separately typing into the paired prose,
  both pass **full edited-file equality**, one-second idle, autosave and exact
  original-file undo. The prose expectation includes existing punctuation escaping.
- Native clipboard checks nine unique markers exactly once in source order.
  This is not a whole-clipboard equality claim.
- `scripts/check.sh` exits 0: formatting, pinned/locked checks, strict Clippy,
  482 view tests (two ignored), 116 core, 25 source-fidelity, 11 tree-selection,
  one external-link, 39 app tests and doctests. Log:
  `/tmp/tachyon-schema-final-qualified-check.log`. Additional parser boundary
  assertions were added afterwards and are checked separately.
- Crusty contexts `ctx_753cf926e44f` and `ctx_95d1fb939fbf`, validations
  `task_77d0cfa366542fca` and `task_b0a9204e47f069fa`: 36 existing advisory findings,
  no new/worsened findings. No canonical serialization changes.

The UX skill guided source-preserving composition, shared leading edges and
readable overflow. The Rust skill guided borrowed raw-value ownership, bounded
work and independent source/edit oracles. Context7 was unavailable; pinned local
library source and official JSON Schema documentation supplied the API reference.

## Native artifacts

Runtime SHA-256:
`fb1d400ecb29252f01557a1653ff17270b715ccc31cfe1263889d2f5da76d1a5`.
Fixture `100-schema-tree.md` SHA-256:
`c62b675e0a16dd41c61888ff3eb25104039c806c388456c331a2568c976e6d80`.
Only tests/docs changed after this runtime build. Artifacts are under
`layout-previews/`, with private source copies and runtime/input/source sidecars.

| Prefix | Evidence |
| --- | --- |
| `schema-tree-final-wide` | 1600×1700 light; complete paired introduction/tree, aligned code pane, conservative short-explanation stack. |
| `schema-tree-final-narrow-pan` | 520×1700 light; native End pans to the trailing values without changing text size or source. |
| `schema-tree-final-dark-200` | 1600×1700 dark at 200%; heading, stacked explanation, large labels and nesting guides. |
| `schema-tree-final-dark-200-lower` | Lower enlarged-tree rows and subsequent document flow. |
| `schema-tree-final-edit` | Full-file-exact valid JSON edit/idle/undo; source and retained preview inspected. |
| `schema-tree-final-prose-edit` | Full-file-exact edit/idle/undo in the paired introduction; the tree stays alongside it. |
| `schema-tree-final-copy` | Source-order clipboard markers and unchanged fixture. |
| `schema-technical-holdout` | Existing fixture 79, retained compact technical peers. |

`schema-tree-wide`/`schema-tree-narrow` use the initial d42c236e runtime before
composition; `schema-tree-composed*` use intermediate 6f550fbc before alignment
and the regression safeguards. They are comparison evidence, not final sign-off.

## Remaining

This does not implement interactive per-field tree editing/disclosure, a schema
validator, condensed semantic property views, arbitrary dialects, general anchored
margin notes, schema exports, full RTL/IME/state/accessibility matrices or page
masters. Wide overflow is a pan surface; it does not yet reflow individual value
rows. Full reference-board/held-out composition and sustained release scrolling
qualification remain open. No new frame-rate claim follows from these captures.
