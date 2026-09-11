# Compact command strips

2026-09-09. Layout-first continuation of Audit A07, technical family T04.
The full grammar and audit remain active.

## Layout and ownership

Short top-level shell examples can share one row with their authored language
and Copy action. Native font measurements must fit the complete command, a
measured language track, equal 16px outer padding and a fixed 96px control track.
The source viewport must retain at least 80px. This removes the separate 32px
header without shrinking text or inserting content. Existing composition can
place a strip beside its explanation; narrow layouts stack those components.

Recognition is deliberately limited to sh/shell/bash/zsh/fish/console, one
nonempty line, no tabs, at most 4096 bytes. A terminal LF or CRLF is invisible
presentation, not removed source. Long commands that do not fit, multiline
scripts, unlabelled fences, other languages, and nested/table code retain their
full pane. A longer command can still use a strip when the full canvas fits it.

During active editing, the published strip/pane choice and language origin are
retained across local refresh and worker publication. Growth uses the existing
horizontal source viewport; language and Copy remain outside its clip. Blur
allows normal remeasurement, and a width that cannot retain the minimum source
viewport falls back to a pane. Mode-sensitive gutter retention prevents a prior
strip's label width from becoming the fallback pane's line-number gutter.

The lock belongs to view projection/cache keys, never Markdown. Published strip
choices are limited to measured geometry windows, including retained/editing
coverage, rather than shaping every offscreen command. A missing context field
was also added to the geometry cache's manual hash (`list_item_container`);
equality already included it, so this fixes hash distribution, not source data.

The UX skill informed compact shared controls, padding and responsive fallback.
The Rust skill informed view-only state, cache identity and measured native tests.

## Verification

- The initial measured test failed with 56px rather than 24px top spacing,
  proving the old separate header was present. It passes with the strip.
- Fit/fallback tests cover ten source/width cases at 100/150/200%, equal padding,
  native command width, control clearance, CRLF and unchanged source.
- Active growth retains origin through immediate refresh and a prepared worker;
  blur returns an overflowing command to a pane, and undo restores exact source.
- A 100-section scoped regression proves only measured command nodes acquire
  strip choices. Actual pointer Copy tests cover both a pane and strip, exact
  payload including newline, and unchanged caret/selection and document.
- `scripts/check.sh` completed successfully: formatting, locked all-target
  checks, strict Clippy, 465 view tests (two ignored), 116 core, 25 source-fidelity,
  11 tree-selection, one external-link and 39 app tests, plus doctests.
  Log: `/tmp/tachyon-command-strips-qualified-check.log`. `git diff --check` passes.
- Crusty preparation `task_c78be1914e960a1e`, context `ctx_c3a95bf8c231`,
  validation `task_cab40d91c80412ba`: completed with 36 existing advisory findings,
  none new or worsened. No upstream dependencies or serialization were changed.

## Native evidence

Final runtime SHA-256:
`e015639369bd05899f94229f774d0915eff4246f4fe019bf76ed97cdc2387971`.
Fixture `97-command-strips.md` SHA-256:
`28b4d5c28171419d149ad9b07b23c23927b4ee7f63c332a241ecd2ef864dcee2`.
All final artifacts live in `layout-previews/` with runtime/source/input sidecars.
Private native sessions use fixture copies, not user documents.

| Prefix | Evidence |
| --- | --- |
| `command-strips-qualified-wide` | 1000×1400 light. Inspected: short command/explanation pairs, shared language/Copy row, full-width longer command, ordinary multiline and Rust panes. |
| `command-strips-qualified-narrow` | 400×1800 light. Inspected: 52px short strips, stacked explanations, long-command pane fallback and unmodified multiline source. |
| `command-strips-qualified-dark-200` | 1000×1600 dark, 200%, ten wheel steps. Inspected: short strips, long/multiline full panes and separated controls at enlarged text size. |
| `command-strips-qualified-growth` | 1000×1400 light. Native click/Home/paste of 432 bytes into the second strip, autosave, one-second idle and undo. Full expected edited-file equality and exact original-file undo pass. Typed pixels retain the strip and unobstructed language/Copy tracks. |

Final captures pass source and appearance checks. The earlier
`command-strips-verified` additionally tests single-character typing on runtime
`a860ae47`; it predates the measured-window scope guard and is supplemental, not
final-build qualification. `command-strips-before` is the previous implementation;
`command-strips-first` is an intermediate build. The initial `command-strips-edit`
oracle failed because its `--edit-within` marker occurred twice in the fixture.
Verified runs use complete edited-file equality instead of that ambiguous marker.
The first CRLF fit oracle also included the control byte in a native text-width
query; the corrected oracle excludes only terminal CRLF from measurement and
continues checking the full original source.

## Still open

Nested/table command strips, the complete multiline-growth/resize/selection/
RTL/IME/accessibility state matrix, static/paged continuation, other document
grammar families and controlled sustained release-performance qualification.
These examples do not sign off all technical layouts or the full audit.
