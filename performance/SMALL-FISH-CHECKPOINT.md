# Four small visual fixes — 2026-09-11

1. Sibling prose columns use equal tracks. Opening/peer row nomination excludes
   asymmetric templates; readable stacked fallback, source order, editing locks
   and the existing gutters remain. The opening in the user's screenshot is
   reproduced in `layout-fixtures/129-small-fish.md`.
2. Task items stay in canonical vertical order. Removed the separate checklist
   grid planner and its diagnostic state. Progress-to-first-item spacing is
   70% of the shared 32px body leading: 22.4 logical pixels.
3. The shared checkbox-to-text gap is 13 logical pixels, with a 32px task indent.
   Task ancestry preserves that inset for supporting paragraphs and nested
   content; table insets and geometry-cache identity use the same ancestry.
4. Outline sits above Files, takes its natural content height up to available
   space, and leaves at least five 28px file rows visible. The keyboard/pointer
   splitter can reduce the outline height. Its limit persists separately from
   the old Files-first split, which no longer describes the new arrangement.

Crusty: complete `work_975591829cd95f77` and `work_c0ca7ae1e0871ae9`.
The Outline/Files criterion of `work_dd79fc8521c80ffb` is implemented, but the
larger navigation audit remains open.

## Verification

- `scripts/check.sh` passes: locked dependencies, formatting, workspace check,
  strict Clippy, workspace tests, accessibility adapter/publication tests and
  doc tests. Document-view has 596 passing tests and 2 pre-existing ignored tests.
- `cargo build --release --locked --bin tachyon` passes. Final binary:
  `3efdd97958fd2143b1b050bf3f7d850ee038e5467bddf0962693298ea5ca822a`.
- Native private-Wayland captures at 960 and 1600 output width, each at 100%,
  150% and 200% text zoom, plus a 960×360 window. Artifacts:
  `layout-previews/small-fish-{width}-{zoom}.*` and `small-fish-nav-short.*`.
- Native geometry and pixel checks pass in `layout-previews/small-fish.geometry.json`.
  Horizontal siblings differ by at most 1px of compositor rounding; gutters are
  24 logical pixels. All six tasks have distinct rows. Measured progress gaps
  are 22px, 33px and 44px at 100%, 150% and 200%. The short window's Files
  viewport is 141px, exceeding the required 140px for five rows.
- Native select-all/copy preserves all six semantic markers in canonical order
  at every width/zoom. Every capture preserves exact fixture bytes.
- `small-fish-toggle-{100,150,200}.task-toggle.json` verifies native checkbox
  clicking changes only the selected task marker, autosaves, and one Undo
  restores the exact original document at each text zoom.
- Rust regressions cover equal opening widths, responsive/edit-locked recovery,
  vertical task counts 2–12, nested/rich/long task content, checkbox bounds,
  text edits against full geometry, task toggling/Undo and navigation minimums.
- Crusty preparation: `ctx_a6409332ff05`; architecture validation
  `task_92d9494e5ecc9dc7` reports no new or worsened findings.

Reproduce a representative capture and inspect its native geometry:

```sh
python3 performance/capture-layout.py --fixture 129-small-fish.md \
  --width 1600 --height 2400 --appearance dark --atspi-active \
  --source-unchanged-check \
  --output performance/layout-previews/small-fish-1600-100.png
python3 performance/small_fish_check.py performance/layout-previews/small-fish-1600-100
```

The legacy README-shaped opening had relied on unequal columns to balance
substantially different paragraph lengths. It now stacks. Opening/resize
regressions use the user's reference opening, whose equal columns fit.
