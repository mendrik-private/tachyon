# Measured diagram composition

2026-09-09. Layout-first continuation of Audit A07; full grammar acceptance
and the remaining audit requirements are not complete.

## Finding and change

The existing renderer already prepares bounded, source-backed Mermaid
flowcharts, including a source-order prose alternative. The coverage ledger
incorrectly described all diagram rendering as missing. Native review of
fixture 78 confirmed flowchart branches, a cycle/subgraph, source editing,
and readable source fallback for an unfinished diagram. It also confirmed
that the full-page composition still has opportunities for improvement.

The measured row planner had a concrete mismatch: a rendered diagram owns
one atomic line with the entire source range, but candidate measurement
shaped that range as text and derived its preferred width from Mermaid syntax.
This prevented fitting explanation/diagram pairs from competing correctly.

Candidate measurement now uses the prepared figure's width, its actual
container insets, and its realized height including any overflow rail.
When source is visible, both figure width and source text participate.
Normal group gaps still own the external spacing. The source, rendering,
font size, graph topology, and narrow horizontal-scroll behavior are unchanged.
No new diagram dependency, parser, or layout family was introduced.

The UX skill's shared-alignment and readable-component guidance informed the
pair review: use the available width without shrinking graph labels or
stretching ordinary prose across the whole page.

## Regression evidence

`rendered_diagram_rows_use_figure_dimensions_not_source_text` first failed
because the visibly fitting pair was not selected. It now checks the original
source and an additional long invisible comment at logical widths
1040/1312/1632 and font zoom 100/150/200%. Predicted figure height agrees with
realization, columns narrower than the figure are rejected, and width 600/800
or height 200 correctly selects a stack. Source serialization remains exact.

Existing deterministic-rendering, inert/unsupported syntax, topology and
accessible-description tests remain in `diagram.rs`; native editor tests cover
atomic geometry, zoom, preview activation, source editing and undo. These are
bounded flowchart checks, not proof of support for every Mermaid family or
quantitative chart type.

## Native evidence

Baseline binary: `985eef9dd77da6fd612c5865f46593f621ad4fa3ee1e237900f426da2558c4d0`.
Final runtime binary: `9a8106b4ae51b4ad4f7765ecc752bf2115f3322f0e47b4643c79a29d13220f96`.
Only test/document edits followed that build.

All artifacts below are under `layout-previews/`, use private source copies,
and retain source/appearance/build sidecars.

| Prefix | Evidence |
| --- | --- |
| `diagram-layout-review-wide`, `diagram-layout-review-narrow` | Baseline fixture 78, including intrinsic graph sizes and narrow overflow. |
| `diagram-pair-before` | Baseline fixture 84 at 1600×1100 light; explanation and graph stacked. |
| `diagram-pair-after` | Final same source/environment; automatic 422/868 px explanation/figure slots, 24 px gutter, 144/154 px heights. Following section moves up 108 px. |
| `diagram-pair-narrow` | Final 600×1100 light; source-order stack and horizontal overflow, no text shrinking. |
| `diagram-pair-narrow-pan` | Native horizontal wheel reaches the graph's trailing edge. Hover tooltip remains visible in this capture. |
| `diagram-pair-keyboard-pan` | Native focus and End-key panning on the narrow figure. |
| `diagram-pair-200`, `diagram-pair-200-lower` | Final 1600×1100 dark at 200%, upper/lower review of the stacked graph and following content. |
| `diagram-pair-edit` | Native click, temporarily invalid source insertion, autosave and exact undo. |
| `diagram-pair-valid-edit` | Native comment insertion, valid preview alongside exposed source, one-second focused idle, autosave and exact undo. |
| `diagram-layout-holdout-lower` | Final fixture 78: complete cycle/subgraph, following explanation and malformed-source fallback. |

Fixture 84 is a new positive composition witness, not a rewrite of fixture 78
or a user document to make an existing page fit. Its unchanged SHA-256 is
`36fa4ab07f449945b4a818994a143ed2424cac11299ae14073ef4a08b6fa4bb3`.
The native text edit checks verify intended insertion and protected prose/
heading fragments; the complete edited source is not compared byte-for-byte.
Undo and final file checks are byte-exact.

## Remaining scope

Final native screenshots were inspected, including valid focused editing,
the panned trailing edge and the complete enlarged graph/following section.
`scripts/check.sh` passes formatting, locked workspace checks, strict Clippy,
all-target tests and doctests (434 document-view tests pass; two remain ignored).
The final focused regression and `git diff --check` also pass.
Crusty context `ctx_a5385dead6f6`, validation `task_a243051aca71f4de`, reports
36 existing advisory findings with no new or worsened findings.

The full-page fixture 78 remains a stacked holdout; its compact diagram-first
sections are not automatically paired by this change. Other diagram/chart
families, schema-specific trees, complete RTL/nested/state and accessibility
interaction matrices, paged/static export, and release performance remain open.
This fixes measured geometry for an existing supported figure, not all of T08.
