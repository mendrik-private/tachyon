# Shared card and list columns

2026-09-10. Addresses Crusty `work_930c548dffd0d79b` and
`work_b8e4dec9abbb014e`; the wider visual/audit queue remains open.

## Behavior

Each measured card, feature, or task collection now chooses one column count.
Incomplete rows keep the same widths, leading anchors, and gutters instead of
redistributing their items. Source order, natural heights, semantic task state,
and the existing line-count, overflow, minimum-width and 1.5x height-fit gates
remain intact. Empty trailing tracks are not treated as unequal item heights.
Small permitted height differences use a squared imbalance penalty, preventing
one additional wrapped line from outweighing a fitting compact arrangement.

The exact six plan.md interface descriptions now use 3x2 at a 1314px canvas,
instead of 2x3. They reflow to two columns at regular widths/150% and a vertical
list at narrow widths/200%. Five completed tasks retain shared anchors and
5-of-5 progress; at a 1440px window the final two occupy the first two tracks
of the three-column grid. Four-column short-feature layouts remain supported.

## Verification

- New partial-row regression failed before the fix: item 3 of five reported
  a two-column final row instead of retaining three columns.
- Candidate tests cover counts 2–12, partial rows, whole-collection wider-track
  fallback, bounded measurements, overflow, natural-height rejection and
  hysteresis. Recorded native six-interface footprints exercise the actual
  score distinction; the model-font integration test alone did not reproduce
  the old native 2x3 choice.
- Native task geometry tests check shared track starts, spans and row identity
  at 100/150/200%, retaining task semantics. Existing full-vs-local edit/growth/
  Undo geometry tests cover list grids and task strips.
- `scripts/check.sh` passes: 781 Rust tests, two existing ignored tests, locked
  checks, formatting, strict Clippy, adapter tests and doctests. Log:
  `/tmp/mineral-grid-check.log`. The earlier sandbox run failed on local socket
  permissions; the unrestricted run passed. `git diff --check` passes.
- Crusty `ctx_31fbde83dd68` / `task_0af293b34ef7fa0b`: 75 existing advisory
  findings, zero new/worsened/resolved. Snapshot guidance remains stale.

Native debug layout-validation binary SHA-256:
`7eda4654652e21ab4bf6ff5a8702df18b3f207a7fe050978dcf010fbf9c8f1ac`.
Fixture `123-aligned-card-grids.md` SHA-256:
`120df2bb3122a08f3f097a97da95e6395fa3b5ec08a8b112117dd749016ea790`.

All artifacts are under `performance/layout-previews/aligned-grid-*`:

- `before`: original native 2x3 interfaces and variable-width final rows.
- `final`: inspected 1600x1400 light 3x2 and aligned collections; native trace
  and active accessibility tree. `copy` verifies unchanged source and all six
  interface markers copied once in source order (not exact full clipboard).
- `regular`, `narrow`, `150`, `200`: inspected 1440/600/1600px window captures,
  light and dark, enlarged text, unchanged-source hashes, layout traces and
  active accessibility trees. This is not full screen-reader qualification.
- `task-toggle`: actual click in the incomplete three-column final row changes
  only the intended checkbox marker; one native Undo restores all bytes.
- `edit-wide`, `edit-150`, `edit-200`: real pointer/Home/type, autosave, two-second
  focused idle and byte-exact Undo. Full-file edited expectations allow only
  the intended insertion and the existing serializer's punctuation escaping
  within that edited item; all other source bytes stay exact. The initial
  expectation without those escapes failed and was not treated as a pass.

Example reproduction:

```sh
cargo build --locked -p markdown-app --features layout-validation
python3 performance/capture-layout.py --fixture 123-aligned-card-grids.md \
  --binary target/debug/tachyon --width 1600 --height 1400 \
  --layout-trace details --source-unchanged-check --atspi-active \
  --output /tmp/aligned-grid.png
```

These are layout and interaction checks, not release performance, continuous
resize, IME/RTL or complete application qualification. Existing source-spine
serialization work remains open; this change does not alter the serializer.
The older LIST-PARTITIONS-CHECKPOINT.md describes superseded per-row stretching.
