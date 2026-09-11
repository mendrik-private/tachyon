# Rich dated timelines — September 10

## Contract and implementation

Layout remains the priority. A dated event can include paragraphs, code, tables,
callouts and nested children without losing its timeline presentation. The first
paragraph must still contain an accepted explicit date and a nonempty summary.
Authored order is retained, including descending dates. Ordered lists, task
lists, versions and date-only labels do not acquire invented timeline meaning.

Rich events use a vertical timeline. Their date/summary headers share measured
alignment; under width pressure the header stacks. Supporting blocks start at
the event's leading content edge and retain their normal content-dependent
widths. Code is not squeezed into the summary column, and tables are not widened
merely to fill space. Existing single-paragraph milestones may still form a
measured horizontal timeline. There are no new cards, fonts or palette tokens.

`adaptive::timeline` now accepts supporting blocks. `measure_label_rows` also
admits explicitly dated outline roots with nested children. The compact-group
fast path rejects rich timelines: counting just date headers would omit the
height of supporting code, tables and descendants when evaluating a peer row.

`AdaptivePlan::timeline_owners` maps each projected event leaf to its authored
date header once during planning. It participates in the retained geometry key.
The existing renderer can therefore paint an event connector from visible
supporting content even when the date header is outside the overscan region.
Paint deduplicates by event and retains the existing clipping and palette; it
does not walk source blocks or parse dates during drawing.

## Regression and native evidence

New fixture: `layout-fixtures/119-rich-timelines.md`, SHA-256
`3341916cec6774738cff1db49366eb66ad5221ed251924afcddcd0ad8227bf97`.
Final validation binary SHA-256:
`8db6f11c2380fbdb544804566863727780f6bbbc2a464b3639bec4f763724687`.

Three new Rust regressions cover recognition negatives and exact source order;
complete supporting-leaf ownership, measured vertical placement and geometry-key
invalidation at mock 100/150/200% text and wide/narrow widths; and 60-line code
growth, focused-plan retention, narrow recomposition and exact Undo. The fixture
regression failed before implementation because rich events had no timeline
headers. The mock font system is not evidence of native glyph fit.

An expanded glyph assertion initially included newline delimiters that are not
painted glyph ranges. Its corrected oracle excludes those delimiters from glyph
comparison, separately requires all 61 code lines, and retains byte-exact source
and Undo assertions. This was a test expectation correction, not missing text.

`scripts/check.sh` passes: formatting, locked all-target check, strict Clippy,
workspace tests, adapter tests and doctests. There are **759 passing Rust tests**
and two existing ignored tests. All 18 capture-harness Python tests pass.
Logs: `/tmp/mineral-rich-timeline-check.log` and
`/tmp/mineral-rich-timeline-python.log`.

Native captures use isolated Weston and active AT-SPI, not the user's desktop:

- `rich-timeline-before`: original 1600×1800 light baseline.
- `rich-timeline-verified-wide`, `-narrow`, `-short`, `-enlarged`: final
  1600×1800 light, 768×1800 light, 1600×480 light and 1600×1800 dark at 200%.
- `rich-timeline-flat-control`: existing fixture57's compact dated events.
- `rich-timeline-body-edit` and `rich-timeline-code-growth`: ordinary native
  typing/paste, exact whole-file autosave, blur and byte-exact Undo. The latter
  adds 60 comment lines to the event's fenced shell command.

All four final size/theme variants preserve all 68 canonical semantic nodes,
including IDs, parents, roles, names, descriptions and actions. Source and
appearance checks pass. Native wide code/table x positions, widths and heights
match the baseline; supporting content is not compressed to obtain the timeline.
Both connectors have 100% one-pixel rule coverage between their date markers.
Original-size wide, narrow and enlarged screenshots were inspected; this does
not claim every offscreen glyph was visually reviewed.

`rich-timeline-offscreen-owner` uses the exact grown code content in a temporary
document, a 1600×240 window and settled scrolling. The first date is at y=-375,
height 24, wholly above even one viewport of overscan. The document viewport is
y=54, height 166. Its entire 166-pixel connector remains painted from visible
code support. Source, active accessibility and exact 60-comments-plus-command
checks pass; the original-size screenshot was inspected.

The first offscreen capture's fixed appearance probes landed inside the code
panel and rejected the image as a page-background mismatch. The final stress
capture omits that unrelated appearance oracle; independent exact-color rail
checks remain. It is not counted as an appearance-mode pass.

`rich_timeline_check.py` checks complete canonical identity, unchanged technical
geometry, date order, marker pixels and connector continuity, including the
offscreen owner. It passes for final captures and fails against the pre-change
baseline as a negative control. Report:
`layout-previews/rich-timeline-verified-wide.pixels.json`.

## Remaining scope

The app-UX skill guided content-fit alignment and full-width technical support;
the Rust skill guided canonical ownership, retained geometry and exact regression
oracles. L10 and A07 remain partial. A top-level dated event may contain nested
children, but a dated list itself nested inside a quote or another list is not
enabled by the top-level measurement pass. Full nested timeline, RTL, IME,
selection/copy and resize-interaction matrices remain separate obligations.
Page/export families, playable media and release performance qualification also
remain open. These correctness captures are not performance measurements or a
claim that the full audit is complete.

Crusty validation `task_99cb45443ddd4b4e` for `ctx_c03fd0d5fd5d` completed:
75 existing advisory findings, zero new, worsened or resolved findings.
