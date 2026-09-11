# Technical sibling partitions — September 10

## Contract before implementation

Continue the full layout-first audit. Native fixture76 at 1100×1600 shows a
readable service table and two compact technical tables stacked below it.
The latter can potentially share available width, but the planner's blanket
three-sibling rule prevents any pair from being measured. Common technical
shape is not evidence of an indivisible three-card editorial unit.

Permit complete source-order partitions of technical siblings. Keep the
existing matched nontechnical trio rule, parent/heading boundaries, native
width/height/overflow/balance gates, source coverage and edit locks. Never
split a section's heading from its table or move a table across another
section. Three technical sections may still share a row when that is the
better measured result. Narrow/short/large-text environments retain normal
stacked fallback. No source rewriting, new chrome or typography changes.

Acceptance: actual-font regression using unchanged fixture76; complete
range ownership and realized geometry; repeated width/height/zoom recovery;
native before/after whole-document evidence and exact source/edit/Undo.
Numerical comparisons remain semantic tables. Paired entity-record grids
were considered, but the native baseline does not justify converting this
already readable directory to cards; that separate family stays open.

## Implementation and regression evidence

`adaptive/rows.rs` now limits the indivisible matched-trio exclusion to
nontechnical peer cards. Technical siblings enter the existing bounded
source-order partition search, including the existing three-column candidate.
Only nomination changed: no new score override, width threshold, relaxed
overflow/height/balance gate, renderer, source syntax or dependency.

The unchanged fixture76 regression failed first because the desired complete
pair was absent. A sharper candidate assertion failed before measurement:
this distinguished exclusion from failed native fit or an unfavorable score.
Removing the technical exclusion makes both candidate and selected-layout
assertions pass. Logs: `/tmp/tachyon-technical-partitions-red.log`,
`/tmp/tachyon-technical-partitions-candidate-red.log`, and
`/tmp/tachyon-technical-partitions-green.log`. The native fixture was retained
whole to verify its preceding wide table and following prose, not just an
isolated pair divorced from the triggering sibling context.

Three new tests cover:

- Complete `4..6` and `6..8` section roots, native realized/measured heights,
  exact source ranges once, 24px slot gutter, full-width first section and
  following-section separation.
- Repeated 833→520→833 logical widths and 1500→180→1500 logical heights at
  100/150/200% text scaling, with valid pair/stack/pair recovery.
- Left/right table-cell edits adding 2000 bytes, beyond compact nomination
  size. The retained pair matches a complete geometry rebuild, preserves
  exact expected Markdown and restores original bytes on Undo; no more than
  two wrap requests. Paint order, table rows, slots and coordinates agree.

The existing nontechnical matched-three-card wide/narrow/recovery test still
passes. `scripts/check.sh` passes formatting, all-target checks, strict Clippy,
**726 Rust tests (two existing ignored)** and doctests. Final log:
`/tmp/tachyon-technical-partitions-check.log`. `git diff --check` is clean.

## Native evidence

Fixture76 SHA-256:
`5c607ea0536a188cd6591478010394890b1cc75612408eb1ace34ab19c7395c9`.
Baseline runtime: `f2fdcc296b430011dba66dc3b5d40c539e86ac8d7f0b912975c46806f7d5e1b7`.
Final runtime: `cc1a4414c43ab89de9f237f8dd1e989a1ecd4db8e1e116b2931b6b6b31b1a1f5`.

The baseline is `layout-previews/paired-records-before` (the initial exploration
name; no record-grid code was implemented). Final artifacts use
`layout-previews/technical-partitions-`:

| Capture | Native result |
| --- | --- |
| `wide`, 1100×1600 light | Complete lower pair; next heading moves y1094→881, **213px reclaimed** |
| `ultrawide`, 1600×1800 light | Wide directory stays a table; lower pair retains natural table widths |
| `narrow`, 520×2100 light | Complete source-order fallback; existing entity records and numerical table semantics preserved |
| `dark200` / `dark200-bottom`, 1600×2400 | Complete stacked tables at 200%; scrolled capture includes the final prose |
| `short`, 1100×480, scrolled | Compact pair still fits the available height and scrolls coherently; extremely short fallback is separately tested above |
| `left-edit` | Native insertion before Standard, full-file expected equality, one-second focused idle, blur and exact Undo |
| `right-growth` | Native 76-byte paste before Atlas; full-file expected equality, focused idle, blur and exact Undo |
| `copy` | Nine unique markers once in canonical order; not a whole-clipboard/rich-MIME qualification |

Source-preservation and appearance checks pass for these captures. Screenshots
were personally inspected, including both edit-idle views and right-growth
after blur. The latter keeps its cell width while focused, then expands its
column within the same section slot after blur. Temporary focused wrapping is
visible, not clipped. No existing test oracle was weakened.

Independent AT-SPI geometry confirms the large directory remains at
(249,324), 834×269 in both baseline and final. Both smaller tables retain their
454×103 and 200×103 extents; only the second table's placement changes. Their
headings share y635 and table text extents share y704; the two slots are
476/333px wide with a 24px gutter. These are semantic text extents, not border
pixel bounds. Source/native view evidence, not schema attributes alone,
supports the visual claim.

P02 and A07 remain partial/active. Broader mixed-document ranking, record-grid
and transposed comparison families, all state/RTL/IME/accessibility matrices,
media/page/export and current-runtime release performance remain open.
