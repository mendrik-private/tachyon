# Very short open-feature columns

2026-09-09. Layout-first continuation of Audit A07; full grammar and audit
acceptance remain open.

## Change and rationale

The approved layout plan permits four open columns for very short independent
features. Native measured candidates now include that family and complete mixed
2/3/4-column partitions within the existing 2–12 item work bound. Only open,
unordered, non-resource features nominate four columns; numbered items and
labeled explanation/entity cards retain their existing families.

Every cell assigned four columns must occupy one actual measured text line.
The existing minimum text width, overflow, height-balance, task/nesting and
dependency safeguards still apply. Wider rows do not shrink the text or change
source order. The existing explicit row identity and focused editing geometry
carry the new widths. Diagnostics identify `MEASURED_FOUR_COLUMN_FIT` and each
row's capacity. Measurements are cached at at most four widths per item;
tests bound candidate enumeration to at most forty alternatives for counts
2–12. These are work bounds, not sustained-performance qualification.

The UX skill's density and shared-anchor guidance informed the scope: short
entries use more available space; longer explanations retain readable measures.

## Automated evidence

- The native-font positive test first failed on the old two-column result.
  Counts 4/8/12 now select four columns at logical widths 1112/1280/1700;
  widths 620/1111 reject four. Realized lines preserve row-major source order,
  exact twelve-track placement, 24 px gutters, nonoverlap and complete text.
- Candidate tests cover bounded work for counts 2–12 and reject four-column
  cells that need more than one text line, even if their reported height fits.
- Numbered/labeled lists do not nominate four columns. The task/nested/
  dependent/long/uneven negative matrix now includes counts 4 and 8.
- Retained typing/full-geometry comparison includes fixture 88 at 100/150/200%,
  bounded local shaping, growth and undo, alongside the mixed-row witnesses.
- A cache-invalidation test exposed an old assumption: assigning span 3 did
  not necessarily change a slot anymore. Its mutation now always changes the
  width, independently of HashMap iteration order; the oracle is unchanged.

## Native evidence

Baseline binary: `a22b318ababbb2f4d7379609145d13be6c78dcf5c4c5e23689e625f21695950a`.
Final runtime binary: `ffd5681d3499be828ee25cbb9ad1e705bb41d5f7577ea977a9db1c5323e6549b`.
Only formatting/tests/documentation changed after that runtime build.
Fixture 88 remained byte-identical before/after, SHA-256:
`928cfa57c525cbcf58299deafb991059579b78ff5de723672215f31048c3fa3a`.

All artifacts below are in `layout-previews/`, use private source copies and
retain build/source/edit/appearance sidecars. Screenshots were inspected.

| Prefix | Evidence |
| --- | --- |
| `four-columns-before`, `four-columns-after` | Same 1600×1200 light source. Four/eight/twelve entries choose 4, 4+4, 4+4+4. Longer explanations stay 2+2 and start 144 px higher. |
| `four-columns-narrow` | 600×1100 light, scrolled: two-column short collections and vertical longer explanations. |
| `four-columns-200` | 1600×1200 dark at 200%, scrolled: readable two-column features; no four-column squeeze. |
| `four-columns-edit-qualified` | Native last-column edit in the eight-item group, focused idle, autosave and exact undo. Eight unique item markers and following content copy in source order. |
| `four-columns-twelve-edit` | Native edit in the last cell of the twelve-item group, with neighboring text preserved and exact undo. |
| `four-columns-mixed-holdout` | Unchanged fixture 87: five keep 3+2; seven now use measured 4+3. Dependent instructions remain vertical. |

An initial clipboard invocation used “Longer explanations”, which also occurs
in the introduction; its order check correctly failed. The qualified invocation
uses unique markers and yields the same clipboard hash. No fixture content or
application copy behavior was changed to accommodate the check. Native edit
assertions cover intended insertion/protected fragments, not complete edited
file equality; final undo is byte-exact. Active AT-SPI is supporting evidence,
not a complete screen-reader interaction matrix.

## Remaining scope

`scripts/check.sh` passes formatting, locked workspace checks, strict Clippy,
all-target tests and doctests (441 document-view tests pass; two ignored).
`git diff --check` is clean. Crusty context `ctx_3075c4a7450d`, validation
`task_ee333696d4e92715`: 36 existing advisory findings, none new or worsened;
checks were run separately in the local workspace.

Inline enumerations, additional list/presentation families, complete keyboard/
RTL/structural-edit/resize/accessibility coverage, static/paged exports and the
release performance gates remain open. This checkpoint adds the short plain
open-feature family, not four columns for every kind of labeled content.
