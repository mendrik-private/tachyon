# Measured extended explanations

September 10, 2026. Layout contract before implementation.

Source-adjacent explanatory prose should not be forced above its table, code
or figure merely because a paragraph exceeds 420 UTF-8 bytes. Nominate bounded
complete prose sequences in the same section; use the existing loaded-font
measurement, readable widths, overflow, height, balance and edit-lock rules to
choose a pair or a stack. Do not turn byte counts into visual fit decisions.

Retain the existing measurement work budget, complete canonical source order,
full-width section headings, code/table fidelity, captions and gallery
ownership. No invented relationship across a heading or other block, no
clipping/smaller type, no automatic wrapping of essential technical evidence.
Verify narrow/short/200% fallback, original source, edit/Undo and native pixels.

## Result

`adaptive::groups` now recognizes up to four adjacent paragraphs structurally,
without the 420-byte visual cutoff. The prose-group boundary uses the same
recognizer, retaining a complete introduction rather than only its final short
paragraph. The existing row planner, native shaping, 4096-byte-per-text-leaf
measurement budget, source-order validation, scoring and edit locks remain the
owners of fit and placement. Oversized candidates still fall back without
truncating their content. No dependency, type role, renderer or model changed.

An important comparison: fixture105 already admits a compact technical peer
row. Both newly eligible explanation pairs now compete, but that existing row
still wins. Fixtures106/107 contain the same complete table/code sections in
isolation; those now select prose/component pairs. This change expands the
measured alternatives, not a requirement to choose one particular arrangement.

Two added tests cover structural nomination (421-byte, multibyte and oversized
prose), complete source ownership, legal alternatives and retained technical
peer selection, isolated table/code pairs, full-width pair headings, complete
realized glyph ranges, measured/realized heights, narrow/short fallback, and
focused growth beyond the shaping budget. Active column widths remain stable;
after blur the oversized content returns to a complete stack. Exact Undo is
checked. Existing heading/break/gallery/five-paragraph negative tests remain.

Both final regressions were rerun against the restored old 420-byte predicates
and failed for missing structural/measured alternatives:
`/tmp/mineral-extended-final-red.log`. Final `scripts/check.sh` passes formatting,
locked pins/metadata, all-target check, strict Clippy and 699 tests: 117 core,
29 source-fidelity, 11 tree, 502 view (2 existing ignored), 1 consumer, 39 app,
plus doctests. Log: `/tmp/mineral-extended-check.log`. `git diff --check` is clean.

## Native evidence

Final runtime SHA-256:
`4f2d8022b4aeea2011b4c4bf057b5320ede4b22680448dcfa2a8bba706c4540e`.
Matched isolated baselines use the old predicates on runtime
`dfe911c5a21cd2cf1cbe753f2d525e26fc8bdddca8df757ca10e79bfc57d77a9`.

Fixture hashes:

- 105: `710a71c72d604925ffe2b613cb842664168de57dd309c00705dad7e285f72acc`
- 106: `664ad8ec5bd1c46f835970ca69104ec5434ae7b11656a8e6a0d6f7863d5b1fcf`
- 107: `69884696d12c819489fc9b2a3d2adc439888985236c8340ce376e4dea9dad6b3`

Inspected `layout-previews/` artifacts:

- `extended-table-before` / `extended-table-final`, 1600×1000 light: complete
  explanation beside the unchanged readable table. AT-SPI table bounds move
  from (259,406,343,226) to (816,130,343,226); dimensions are unchanged.
- `extended-code-before` / `extended-code-final`, 1600×1000 light: open prose
  beside a complete 11-line example, with shared top alignment and retained
  syntax colors, gutter and Copy. The code panel moves up 192 px (Copy y316→124)
  without enlarging type or compressing its internal spacing.
- `extended-table-narrow`, 520×1700 light: navigation collapses and all prose
  and table rows stack, without dropping columns or shrinking text.
- `extended-code-dark-200`, 1280×1700 dark: enlarged prose and code stack.
  Long source lines retain horizontal code overflow; the offscreen tail of
  line8 is not claimed visible in this screenshot without panning.
- `extended-table-edit` / `extended-code-edit`: native first-paragraph Home/
  type `x`, independently constructed full edited-file expectation, one-second
  idle autosave and exact-byte Undo. Both inspected idle PNGs retain their pair.
  Expected serialization includes the existing escaping of punctuation only
  within the changed paragraph. Initial expectations omitted comma escapes;
  corrected after inspecting `escape_inline`, with the whole-file oracle kept.
  No serializer change was made. Initial oracle logs are retained as
  `/tmp/mineral-extended-{table,code}-edit-oracle.log`.
- `extended-combined-final`, 1600×1700 light: existing technical peer layout
  retained, all seven authored copy markers once/in canonical order, source
  bytes unchanged. This is a marker-order check, not whole-clipboard equality.

All read-only final captures retain original source bytes. Native logs use
`/tmp/mineral-extended-*.log`; focused tests and captures are not release
performance qualification. UX guidance shaped the comparison against existing
better layouts and preserved reading measures; Rust guidance shaped bounded
measurement and focused-growth/full-source tests.

## Remaining scope and observed issue

The combined select-all capture exposes pale code glyphs on a light selection
highlight inside the dark code panel. Copy/source checks pass, but selected
code contrast is not acceptable visual evidence. Recorded separately as
`work_64cc1bebafa6ce50`; no selection-palette change was made here.

September 10 follow-up: the separate
[code-selection checkpoint](CODE-SELECTION-CHECKPOINT.md) fixes and verifies
this contrast defect, including a recapture of the combined fixture. The
historical failed capture above is retained as evidence.

Full layout/media/nested/RTL/IME/print/export and release performance matrices
remain open. This checkpoint does not qualify arbitrary prose/component
relationships or claim the complete grammar is finished.
