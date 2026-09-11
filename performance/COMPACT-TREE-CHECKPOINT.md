# Compact deep trees

2026-09-09. Layout-first continuation of Audit A07. The full audit and complete
hierarchy family remain open.

## Layout

A pure-prose outline deeper than three levels now switches to a compact tree
when its deepest normal indentation would leave less than 320 logical pixels.
The decision uses the actual arrangement canvas before the readable text cap.
Comfortable-width outlines retain their existing layout. Shallow outlines and
mixed code/table/quote/definition contexts retain their current presentation.

In the compact tree, the first two levels retain their ordinary anchors. Deeper
items share a 64 px leading inset instead of accumulating indentation without
limit. Authored bullets, numbers, checkboxes and source order remain intact.
Local branch connectors cannot run through a re-anchored descendant; numbered
markers retain their extra hanging gutter.

An explicit parent-context caption appears on entering a deep branch or returning
from a deeper child, not above every sibling. It names the actual parent item
and canonical depth, uses the existing 13/18 caption role and a 24 px band, and
ellipsizes within the available width. This is view-only context, not a generated
heading or an extra editable paragraph. The complete parent remains in the
canonical document and accessibility hierarchy. No content is hidden/collapsed.

Projection records the complete prose-outline footprint and parent identities
once. Arrangement uses a temporary segment override; the canonical context stays
unchanged. The override participates in geometry cache identity and is retained
by the local editing path. Parent excerpts read the live projection, bounded to
1 KiB and 96 graphemes, including protection against enormous combining clusters.
No second document model, new dependency, or scroll-time candidate search is added.

The UX skill informed recomposition and the reduction of repetitive context
labels. The Rust skill informed shared geometry ownership and explicit bounds.

## Discovered checklist correctness issue

Fixture 92 exposed a pre-existing misleading summary: a checked root task with
unchecked descendants displayed “1 of 1 complete”. A two-task real-plan regression
reproduced `(1, 1)` instead of `(1, 2)`. The summary counted only immediate items.

It now counts explicit task states throughout the canonical subtree, including
supported semantic containers. Ordinary bullets and literal code are not invented
tasks. Counts are prepared with the plan, not recalculated while painting.
The four-task native specimen correctly shows “2 of 4 complete”; checking its
last descendant produces “3 of 4 complete”. The diagnosis skill supplied the
minimal red/green loop; direct source evidence made speculative instrumentation
unnecessary.

## Automated verification

- A native-font regression failed before the change: a twelve-level unordered
  outline had only 72 px of text space on a 360 px canvas. Ordered/unordered
  twelve-level cases now retain at least 280 px at 100/150/200%, with complete
  source coverage and exact Markdown preservation.
- Existing eight-level readable-width tests still pass at wide sizes, and their
  360/620/1000 width and focused-inset checks cover the compact transition.
- The retained-typing/full-geometry test now additionally exercises both deep
  explanatory paragraphs at width 360 and 100/150/200%, checking range, inset,
  tree-mode, height, local shaping bounds and subsequent shrinking.
- Parent-identity/branch-return tests distinguish adjacent siblings from returning
  branches. Threshold, shallow/mixed fallback and bounded Unicode excerpt tests
  cover adjacent cases.
- Checklist regression cases cover nested incomplete tasks, ordinary intermediate
  bullets, multiple levels/siblings, tasks inside quotes and code-literal exclusion.
- `scripts/check.sh` passes formatting, locked all-target checks, strict Clippy,
  workspace tests and doctests: 457 document-view tests pass, two ignored.
  `git diff --check` is clean.

## Native evidence

Final runtime SHA-256:
`2825669689f52a399731b59777345c0c6eedb437899287e7ed88e3fe02314c8d`.
Fixture 91 SHA-256:
`d21458c7670a48c3e22079dd4906175f96ffa25d620daa6c00ac39404339afda`.
Fixture 92 SHA-256:
`9d736487f73c0dabd3e3e48152ce7e9a74946bcf1704c09087db741c7e520553`.

Artifacts below are in `layout-previews/`, use private source copies, retain
build/source/input sidecars, and were visually inspected.

| Prefix | Evidence |
| --- | --- |
| `compact-tree-before` | Fixture 91, 400×1100 light, baseline runtime `e73c8e843976381bc3be3dc0c4aef9cdee6d6863880076e1cdf3633fde1d4f1d`; deep indentation consumes substantial text space. |
| `compact-tree-final-deep` | Fixture 92, 400×1200 light. Twelve source levels, branch returns, readable final explanation and correct nested task totals. Exact unchanged source. |
| `compact-tree-final-wide` | Fixture 91, 1600×1200 light. PNG is byte-identical (`cmp`) to `paragraph-endings-after.png`: the comfortable-width composition is unchanged. Exact unchanged source. |
| `compact-tree-200-verified` | Fixture 92, 1000×1400 dark, 200%, twenty wheel events. Twelfth-level text, returning branches, next heading and task summary remain legible. Appearance and exact unchanged-source checks pass. |
| `compact-tree-edit` | Native twelfth-level click/Home/type/autosave, preserved neighboring fragments, focused idle and exact whole-file undo. Typed/idle screenshots retain the actual edited state. |
| `compact-tree-ordered-edit` | Native fourth-level numbered-item edit at 400×1100, original numbers/parent context, preserved neighbors, idle and exact undo. Final runtime includes the corrected number/connector clearance. |
| `compact-tree-task` | Native deepest-checkbox click, exact marker-only autosave, all unrelated bytes unchanged, “3 of 4” screenshot and exact one-step undo. |
| `compact-tree-copy` | Twenty-one unique canonical markers copied once each in source order, including parent labels reused in visual captions; captions do not duplicate these clipboard markers. Exact unchanged source. |

Earlier exploratory 200% captures (`compact-tree-200`, `compact-tree-final-200`,
`compact-tree-qualified-200`) failed the general appearance oracle because their
viewports contained no heading pixels. Their inspected page/panel/text colors
were correct. The accepted capture scrolls farther, includes a heading, and
passes the unchanged appearance oracle. They are not counted as passing runs.
Intermediate compact-tree captures retain older runtime hashes and are baselines,
not final-build qualification.

Clipboard marker order/uniqueness is not full clipboard equality. The typing
oracle permits canonical punctuation escaping inside the edited leaf; exact
whole-file undo and marker-only checkbox equality are separate checks. Active
AT-SPI supports the evidence but is not full screen-reader qualification.

## Remaining work

Mixed deep branches containing code, tables, figures or quotations still require
the corresponding compact-tree layout/ownership treatment; this implementation
does not claim they fit narrow canvases. Extremely narrow text surfaces, full
RTL/IME/keyboard/structural-edit/resize/accessibility matrices, optional interactive
branch collapse, other grammar families, static/paged export and sustained release
performance remain open. The next hierarchy work should extend the real tree
treatment to mixed branch content, not weaken the complete-family requirement.

Crusty contexts `ctx_1f1bb13c9828` (layout) and `ctx_c2c544ec73e0` (nested
progress), validations `task_d93b5cb2cfc1f7e4` and `task_641cf8d94a3aa944`:
both completed with 36 existing advisory findings, none new or worsened. Local
workspace checks ran separately. A07 and the full audit goal remain active.
