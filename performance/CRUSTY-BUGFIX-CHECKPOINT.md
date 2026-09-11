# Accepted Crusty bugs — native verification

2026-09-09. Scope: the four accepted `kind: bug` work records, not the separate
grammar epics or automatically captured historical problem reports. Existing
unrelated changes in this shared worktree are preserved.

## Fixes

| Crusty record | Change | Regression evidence |
| --- | --- | --- |
| `work_9e198803d61f7b7c` | Table controls contain a real round 3 px mark, not a text label. The hit target and accessible name are separate. Pointer movement into the outside half of the edge target retains the control. Enter/Space and Tab/Shift+Tab have a control-specific key context so they do not edit the document. | `table_controls_have_only_three_pixel_knobs`; native idle/hover/focus and keyboard checks at 100%, 150%, 200%. |
| `work_c9821aed7acb4abe` | Independent labeled list cards use the existing quiet surface/top-accent treatment and symmetric 24 px insets. Their bullets and obsolete marker indentation are removed together; ordinary lists retain their bullets. | `labelled_entity_cards_have_an_enclosure_and_no_marker_indent`, `measured_plan_md_entities_fit_a_bounded_card_row`; real `plan.md` entity text at wide/narrow widths, native typing, canonical list semantics and exact undo. |
| `work_7b9c267f187aca22` | Reject an explanation/table pair when its prose is less than four useful lines or below 60% of the table's height. A short lead-in remains above its table, with the following paragraph directly below the table. Substantial explanation pairs and paired tables remain supported. | `short_table_leadins_do_not_become_stranded_columns`; native typography/palette excerpt at 1600, 1024 and 560 px, selection/copy reading order and unchanged source. |
| `work_6674b8abddc2b2c6` | Reapply the retained row template to its current descendants after structural edits, including newly allocated table cells. New cells immediately inherit the same table placement as existing cells. | `inserted_cells_inherit_the_current_table_placement_immediately`; 12 native beginning/middle/end row/column insertions across inline and paired tables, geometry comparison, clicking/typing in each new cell, exact undo/redo. |

The actual three-crate example needs one title plus up to four body lines at a
1600 px window. That bounded allowance is intentional: the guide's “about three
body lines” is not treated as a reason to reject the explicitly requested
three-card example. The measured 1.5 height-ratio limit, overflow checks, source
order, narrow fallback and edit locking remain enforced.

The table fix deliberately does **not** turn tables into true CSS-style floats.
The authoritative grammar keeps tables block-level. It removes the reported
stranded-caption arrangement using a coherent stacked fallback.

## Visual review

Reviewed against `designs/03-cards-and-signals.png` and the card, spacing,
available-width and block-level-table rules in `document-design-grammar.md`.
The native-desktop design skill informed the restrained enclosure, separate
hit/visible geometry, visible focus and accessible names; the debugging skill
required failing regressions before the corresponding fixes.

- [Wide entity cards](layout-previews/crusty-cards-wide-before.png): three
  intentional surfaces, 24 px inter-card gaps/insets, no bullets in cards;
  ordinary uneven bullets remain below.
- [Narrow entity list](layout-previews/crusty-cards-narrow-before.png): readable
  source-order fallback, no forced three-column layout.
- [Wide table flow](layout-previews/crusty-table-flow-wide-before.png),
  [medium](layout-previews/crusty-table-flow-medium-before.png),
  [narrow](layout-previews/crusty-table-flow-narrow-before.png): attached captions,
  contained tables, following prose, no stranded short-text column.
- Controls: [100% idle](layout-previews/crusty-knobs-100-knobs.png),
  [150% hover](layout-previews/crusty-knobs-150-hover.png),
  [200% focus](layout-previews/crusty-knobs-200-focus.png). Marks snap equally in
  both axes at fractional zoom; tooltip text is not button content.
- Paired-table new column: [immediate](layout-previews/crusty-insertion-paired-column-end-immediate.png)
  and [settled](layout-previews/crusty-insertion-paired-column-end-settled.png).
  The `.bugs.json` report records all twelve insertion cases, including empty
  inserted-cell geometry and native typing into those exact cells.

Screenshots are the first available capture **after the inserted state is
published**, not a claim to capture the compositor's very first frame. The Rust
regression checks synchronous geometry before a background replan can repair it.

## Verification

The results below describe the tested bug-fix snapshot and retained release
binary. After native verification finished, concurrent math/footnote changes
made a fresh focused test build fail with 14 compiler errors: a missing
`editor/footnotes.rs` module / `render_footnote_numbers` method and stale
`Attachment.light` / `.dark` accesses after the attachment-model change. Those
in-progress changes are outside these four bug fixes and were not modified.
The current shared worktree must not be described as build-clean.

- `cargo test --workspace --locked`: 527 passed, 2 existing ignored.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: passed.
- `cargo check --workspace --locked`: passed.
- `cargo fmt --all -- --check`: passed at the tested bug-fix snapshot. A later
  check caught concurrent, unrelated math/footnote formatting edits in
  `editor/inline_math.rs`, `editor/measurement.rs`, `footnotes.rs`,
  `projection.rs`, and the shared `editor.rs`; those changes were not reformatted
  by this bug-fix task. A non-recursive check of the six touched Rust files
  reports only the concurrent formula/reference/footnote hunks in `editor.rs`.
- `python3 -m unittest discover -s performance -p 'test_*.py'`: 61 passed.
- Release binary built with `layout-validation`; native proof records identify
  each running executable by SHA-256, not only its pathname.
  Final native matrix: `8426c5add4149d2b0ac8ce968eedbdd16c928f14b3d30e0d4805b6a8a331b0ac`.
- Two explicit warm card replans: zero shaping/wrapping/new segment layout,
  published-geometry reuse and zero anchor displacement.

All native runs use private Weston, a private accessibility bus/Wayland seat,
and private fixture copies. Each passing `.bugs.json` and `.source.json` records
source integrity and the tested binary. No real document is edited by the checks.

Reproduce, for example:

```sh
cargo build -p markdown-app --release --locked --features layout-validation
python3 performance/capture-layout.py --fixture crusty-cards.md --recorded-bugs-check cards --width 1600 --height 1200 --source-unchanged-check --layout-trace details --layout-samples 2 --cached-geometry-check --output performance/layout-previews/crusty-cards-wide.png
python3 performance/capture-layout.py --fixture crusty-insertion.md --recorded-bugs-check insertion --width 1600 --height 1200 --source-unchanged-check --output performance/layout-previews/crusty-insertion.png
python3 performance/capture-layout.py --fixture crusty-insertion.md --recorded-bugs-check knobs --width 1600 --height 1200 --zoom-steps 10 --source-unchanged-check --output performance/layout-previews/crusty-knobs-200.png
python3 performance/capture-layout.py --fixture crusty-table-flow.md --recorded-bugs-check table-flow --width 560 --height 1600 --source-unchanged-check --output performance/layout-previews/crusty-table-flow-narrow.png
```

This is a bounded bug-fix verification, not a complete design-grammar/pixel-perfect
audit or a new scrolling FPS benchmark. Crusty change context:
`ctx_739171341432`. Final validation task `task_9f9fbc1c67e7c57b` completed:
zero new/worsened architectural findings, no matched blocking constraint.
Compiler checks were run separately; Crusty's advisory static result does not
override the current concurrent-build caveat above.
