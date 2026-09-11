# Recent layout pairs: native resize verification — September 10

## Scope

Continue the layout-first audit by checking the recently implemented optional
guidance row and compact Configuration/Comparison table tracks in unchanged
whole fixtures 01 and 03. This turn extends validation, not the production layout
planner. The app-UX skill guided actual window/reading-state checks; the Rust
skill guided feature isolation and source/Undo regression coverage.

The validation-only F5 action requests a 1710px-wide window (1380px document
canvas). Existing F9/F10 width controls, Shift-F9/F10 height controls and
F11/F12 small-width controls remain unchanged. The first implementation reused
F11 and failed to enlarge the native window: the existing small adjustment
won. Assigning the unused F5 corrected that test-control collision.

`resize_layout_check.py` now recognizes both source-backed scenarios. Short
Note/Tip notices must retain their row at 304px document height; taller table
sections must stack. Both must stack at 650px width and recover their pair at
1380px. Editing requires a real paired starting state, stable editor identity,
focus and collapsed caret, the intended final topology, an exact whole-file
autosave golden and byte-for-byte Undo. Only the deliberately edited paragraph
or cell may receive canonical Markdown punctuation escaping.

## Reading setup correction

The first table height run paired/stacked/recovered correctly but failed its
selected-heading displacement assertion: offsets 793→1088→793. The target
heading was already visible near the bottom after enlarging the initial
window; ScrollIntoView did not establish it as the top reading anchor. Waiting
an extra second did not change this result and that attempted workaround was
removed. Establishing the reading position before enlarging the initial window
makes this an actual top-reading-anchor test for the table scenario. Guidance
retains its existing 182px heading offset in the height case, not a top anchor.
No app scroll behavior or
one-physical-pixel displacement tolerance was changed.

Initial 1920px desktop captures could put the enlarged window partly outside
the screen. Final runs use a 3200×1400 private Weston desktop so the complete
1710px window remains available for visual inspection. This is test-display
space, not an increase in the app's document width requirement. Initial failed
and partially visible captures remain diagnostic evidence, not final acceptance.

## Evidence and remaining scope

Runtime SHA-256:
`975061b0fee2e48b96d251f8e3e5ad26acf3d8aa38fe1d157860ecd53b42c3e6`.
All scenarios use disposable copies of the original fixtures, private input
and a private session bus with active AT-SPI. Native resize bursts alternate
nine requests and verify the latest committed geometry at the actual viewport.
They do not qualify continuous physical pointer-drag latency or release speed.

Final evidence is under `layout-previews/recent-resize-final-`, named by
fixture, reading/editing mode and width/height axis. The existing opening
scenario is checked separately as `recent-resize-opening-control` to retain
coverage of the original F10 target.

All eight final pair scenarios and the opening control pass. Width sequences
are 1380→650→1380px; height sequences are 884→304→884px with width unchanged.
Observed reading-heading displacement is zero in all four pair reading runs.
All four editing runs start paired, retain focus/caret, match their complete
autosave golden and restore the exact original source with Undo. Every final
burst passes the latest committed viewport check. Original-size desktop
screenshots were inspected for the complete wide table window and short
guidance window. The latter intentionally retains the reader's position, so
the viewport cuts through the callout panels; it is not whole-callout pixel
qualification (the preceding settled kiosk checkpoint supplies that evidence).

`scripts/check.sh` passes formatting, locked workspace/all-target checking,
strict Clippy, 756 Rust tests, two existing ignored tests and doctests; log
`/tmp/tachyon-recent-resize-check.log`. Python resize-oracle tests cover the
new keys/source targets, required short guidance row, rejection of an incorrect
stack and rejection of an initially stacked editing scenario. The existing
capture-harness suite remains separate: 15 resize-oracle tests and 17 capture
tests pass. `git diff --check` passes.

This does not complete C01 or P02. Specification/example pairs elsewhere in
fixture03 are not this scenario's asserted peers. Arbitrary reading anchors,
broader mixed-document states, RTL, large-text resize bursts, physical resize
timing and the full release/performance gates remain open. The audit goal and
A07 remain active.

Crusty validation `task_17bea73dab66c8ce` for `ctx_b2806899e88a` completed.
Its architecture comparison is advisory; runtime and source checks above
remain the verification evidence for this change.
