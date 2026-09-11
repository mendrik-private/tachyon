# Short-list count and available width

2026-09-09. Layout-first continuation of Audit A07; full grammar acceptance
remains open.

## Change

The list analyzer and native measurement gate both retained an obsolete 3–9
item limit. They now share the specified 2–12 item work bound. Candidate search
considers no more columns than there are items. Loaded-font measurements,
overflow, line count, height balance and orphan penalties still decide fit;
eligibility does not force a grid. No source rewriting or font shrinking occurs.

An initial implementation exposed a regression in the existing two-term
alignment tests. Authored term–description pairs now retain their aligned-row
vocabulary; plain independent pairs and explicit resource identities remain
eligible for measured grids. The UX skill's semantic grouping guidance informed
this distinction: using width should not turn every labeled pair into cards.

## Automated evidence

- `measured_short_lists_consider_two_through_twelve_items` covers counts 1–13,
  plain/bold-label variants, widths 360/1280, measured candidate validity,
  row-major placement and complete visual source coverage. Wide balanced
  2/10/12 witnesses select grids; labeled pairs retain aligned rows.
- `expanded_list_counts_keep_semantic_and_fit_guards` checks 2/10/12 tasks,
  nested, dependent, long and uneven lists remain vertical, with exact source.
- The existing oversized negative witness now uses 13 items, not 10.
- Retained-row typing versus full geometry now includes fixtures 85/86 and
  growth in first/last items at 100/150/200%, bounded shaping and exact undo.
- `scripts/check.sh` passes formatting, locked workspace checks, strict Clippy,
  all-target tests and doctests: 436 document-view tests pass, two remain ignored.

## Native evidence

Baseline binary: `9a8106b4ae51b4ad4f7765ecc752bf2115f3322f0e47b4643c79a29d13220f96`.
Final runtime binary: `b1c707f2774ffa06edc550690b88cc97d5838550f44346d34f14020adcf01a2c`.
Only tests, fixtures and documentation changed after the final runtime build.

Artifacts are under `layout-previews/`, with private source copies and native
appearance/build/source or edit sidecars. Final screenshots were inspected.

| Prefix | Evidence |
| --- | --- |
| `list-count-before` | Fixture 85, 1600×1200 light: ten/twelve short items unnecessarily stack. |
| `list-count-qualified` | Same source/environment: ten items use two columns, twelve use three. The twelve-item section starts 96 px higher. The two-term section stays aligned. |
| `independent-pair-qualified` | Fixture 86, 1600×1000 light: two plain observations share a row; two authored terms keep aligned rails. |
| `list-count-narrow-qualified` | 600×1100 light, scrolled: ten/twelve lists stack and the final single item remains ordinary flow. |
| `list-count-200-qualified` | 1600×1100 dark, 200%, scrolled: twelve readable items use two columns, with the following section visible. |
| `list-count-edit-qualified`, `independent-pair-edit-qualified`, `list-twelve-edit-qualified` | Native click/typing in trailing grid items, focused idle, autosave and exact undo; neighboring authored fragments preserved. |

Fixture 85 SHA-256:
`3849a2b3f845d6510f6cbd0df436b9db376bc81df1e81e6361f83283ae71f761`.
Fixture 86 SHA-256:
`40a5543fa95692fd5c6ce8741c258c631252cc82dd5ba04fe061767493efbf05`.
These are new witnesses; the before/after content is unchanged. Intermediate
`list-count-after` predates the term-pair regression fix and is not final evidence.
Native edit assertions verify intended insertion and protected fragments, not
the entire edited file; final undo is byte-exact. AT-SPI was active, but these
captures do not establish a complete screen-reader interaction matrix.

Crusty context `ctx_16090859838d`, validation `task_3ea26ae9eeda6833`:
36 existing advisory findings, none new or worsened; local checks run separately.

## Remaining work

Variable row partitions (such as 3+2+2), inline enumerations, additional list
families, full RTL/keyboard/structural-edit/resize matrices, static export and
release performance qualification remain open. This removes a count gate from
the existing measured family; it does not complete the list grammar or A07.
