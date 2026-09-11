# Mixed content in compact trees

2026-09-09. Layout-first continuation of Audit A07; the full audit remains active.

## Layout and ownership

The compact deep-tree presentation now accepts labeled branches containing code,
tables, figures and quotations. The existing pressure threshold, 64 px deep
inset, source-backed parent captions and comfortable-width outline remain intact.
Each item must still begin with a paragraph label. This is automatic presentation,
not a Markdown rewrite, generated hierarchy or collapsed-content feature.

Previously, introducing supporting content disabled the compact tree. Merely
extending eligibility exposed a real second defect: a nested table still began
at 192 px while its branch began at 64 px. Table positioning, wrapping and viewport
clipping now share the published compact inset. Cell-local padding and relative
nesting remain separate; nested tables retain their existing grid semantics.

Figures now account for ancestry consistently in their outer span, painted
position and reserved aspect ratio. Narrow images use the remaining branch width;
wide images can reach intrinsic size without losing indentation twice or being
upscaled. Local outline connectors remain beside prose, not inside code, table,
quote or figure interiors. Supporting content stays in canonical source order.

The UX skill informed the shared alignment and space allocation. The Rust skill
informed geometry ownership and regression verification. No dependencies or
scroll-time layout searches were added. An exploratory nested-record path was
removed after source inspection confirmed that record recognition intentionally
excludes nested tables; this checkpoint does not expand record eligibility.

## Automated verification

- Native-font mixed-branch geometry regression: the table's old 192 px outer edge
  failed the expected 64 px compact edge before the correction.
- The passing test covers logical widths 360/620/1600 at 100/150/200%, compact
  transitions, table outer position and clip span, preserved grid topology, image
  aspect ratio/paint span and exact Markdown serialization.
- Retained-typing versus full-rebuild tests now include mixed-branch code, table
  cells, quotation text and explanatory prose at widths 360/1280 and all three
  zoom levels, including growth and shrinking.
- `scripts/check.sh` completed successfully: formatting, locked checks, strict
  Clippy, workspace tests and doctests. Document-view: 458 pass, two ignored.
  `git diff --check` is clean.
- Crusty context `ctx_e08c8a5c51ae`, validation `task_aae1d7eda9d03359`:
  completed, 36 existing advisory findings, none new or worsened. Workspace
  checks ran separately.

## Native evidence

Final runtime SHA-256:
`bdcd8e54a11b4732b2f658292f11e18301fdbaf9eeeb524a8f52619768193716`.
Fixture `93-mixed-tree.md` SHA-256:
`1dc3b44298ed1f2b468ce4a1b2a5deefa0bcc5d835ee5836c70c01540e44e083`.

Artifacts in `layout-previews/` use private source copies, retain runtime/source
and input evidence, and were visually inspected.

| Prefix | Evidence |
| --- | --- |
| `mixed-tree-before` | Baseline runtime `28256696`, 400×1200 light: accumulated indentation clips code/table content, severely narrows quotes and disconnects the image from the branch. |
| `mixed-tree-after` | Intermediate runtime `5f545170`, narrow visual comparison; not final-build qualification. |
| `mixed-tree-quote-edit` | Final build, 400×1200 light: native click/Home/type inside the quote, preserved neighbors, autosave, focused idle and exact whole-file undo. |
| `mixed-tree-table-edit` | Same narrow surface, actual table-cell edit and row-height growth, preserved neighbors, autosave, idle and exact undo. |
| `mixed-tree-code-edit` | Same narrow surface, actual code edit, preserved neighbors, autosave, idle and exact undo. |
| `mixed-tree-wide` | Final build, 1600×1600 light: ordinary eight-level outline, aligned support components and intrinsic-width 960×360 figure. Appearance and exact unchanged-source checks pass. |
| `mixed-tree-dark-200-verified` | Final build, 1000×1600 dark at 200%, thirty wheel events: aligned quote, image, explanatory prose and branch return; next heading remains separate. Appearance and exact unchanged-source checks pass. |
| `mixed-tree-copy` | Eighteen unique markers copied once each in canonical order, spanning parent labels, code, table values, quote, image alt text and branch returns. Exact unchanged source. |

The exploratory `mixed-tree-dark-200` capture used twenty wheel events and
contained no heading pixels, so the general appearance oracle rejected it.
Its page/panel/text colors were correct on inspection. The accepted capture
scrolls farther to include the next heading without changing the oracle; the
exploratory run is not counted as passing source/appearance qualification.

Clipboard marker uniqueness/order is not full clipboard equality. The edit
oracle permits canonical escaping within the edited leaf; exact whole-file undo
is a separate check. Active AT-SPI is not complete screen-reader qualification.

## Remaining layout work

Unlabeled/code-first items and outlines rooted inside quotations or other outer
containers still need explicit compact-tree treatment. Rich cells, deeper mixed
containers, extreme narrow widths, RTL/IME, structural edits, live resize and the
complete accessibility matrix are not qualified by this fixture. Nested record
recomposition remains outside this change. Other grammar families, static/paged
export and sustained release performance remain required; A07 is not complete.
