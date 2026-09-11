# Rounded title-bar controls checkpoint

2026-09-10. Crusty `work_42ef149a3f234400`: title-bar scope qualified and completed.

Application-owned chrome now uses a compact dark surface and named toolkit
buttons with circular 28px targets. The zoom reset uses a 50px pill and displays
100% without truncation. Close routes through the existing application close
handler. Document recovery, conflict and error actions occupy a wrapping status
row below the title bar.

Native keyboard inspection exposed a pointer-only application-menu trigger.
The local controlled popover now handles keyboard Click events, reuses its menu
while open and clears it on dismissal. It retains the existing menu actions.

`scripts/check.sh` passed: 787 tests passed, two existing tests ignored, with
formatting, locked checks, strict Clippy and doctests passing. Log:
`/tmp/tachyon-title-menu-full-check.log`. `git diff --check` passed.

Release binary SHA-256:
`809a65732f1a7b210bc3775c72d1f23a9713cb45ccc80acc5ea1ba16d27f0dda`.
Native fixture 41 at 480x800 verifies named controls, keyboard focus on Close,
Shift+Tab navigation to the application menu, Space opening the menu, Escape,
pointer Close and exact unchanged document bytes. The normal, hover, pressed,
focus and menu screenshots were inspected. Artifacts:
`performance/layout-previews/title-menu-keyboard*`; execution log:
`/tmp/tachyon-title-menu-native.log`.

The first checkpoint left pressed feedback, native window operations and
effective display scales unqualified; the following checks resolve those gaps.
The subsequent checkpoints qualify pointer window controls, unsaved-close file
outcomes, recovery/status controls and disabled presentation. These title-bar
checks do not qualify the broader persistence/recovery lifecycle.

Crusty validation `task_cd02eb958f40f83f` for `ctx_f0ec0da7f262` completed with
75 existing architecture findings and zero new, worsened or resolved findings.

## Native controls and display-scale correction

The visual preview understated the difference between hover and pressed states.
Pixel measurements establish distinct normal `(42,46,49)`, hover `(72,80,87)` and
pressed `(91,101,109)` interiors. The harness now requires three distinct samples;
no button event-handler change was needed.

The desktop harness verifies Enter on Maximize, Space on Restore, Enter on
Minimize and Enter on Close. It checks changed/restored accessible geometry,
disappearance of the title bar while minimized, reactivation with Weston's
documented Super+Tab binding and exact source preservation. Blank-title-bar
dragging moves the native window by 160px left and 80px up, established from
screenshots independently of the app's window-local accessibility coordinates.
Artifacts: `title-desktop-drag*` and `title-desktop-final*`.

Scale checking exposed an actual event-order bug: before the correction, one
requested 150% run reported 100% (`title-states-scale-180.titlebar.json`), while
other fractional runs succeeded. On older wl_surface versions, output Enter/Leave
could overwrite the fractional preference with the output's integer scale.
Those handlers now defer to fractional scaling, as PreferredBufferScale already
did. Output membership and subpixel updates remain intact; compositors without
fractional support retain the existing fallback.

Corrected release SHA-256:
`8e5f2fd02f59100dabfdcc794535e668a15954d09a2c95b8a65dd14b780f768b`.
All eight native state checks pass at 480px and 1440px widths, each with effective
100%, 125%, 150% and 200% scaling. Each checks actual button bounds against the
requested scale, distinct pointer feedback, keyboard focus/menu, pointer Close
and exact unchanged source bytes. Artifacts: `title-scale-fixed-*` and
`title-scale-regular-*`. Narrow focus and regular menu screenshots at 200% were
inspected. The compositor presents these scaled buffers at logical output size;
the coordinate-scale assertion prevents a 100% fallback from qualifying.

Full checks pass again: 787 tests, two existing ignored tests; log
`/tmp/tachyon-fractional-precedence-check.log`. Capture-harness tests: 23 passed,
log `/tmp/tachyon-title-controls-python.log`. Crusty validation
`task_7cc64cd404b27637` / `ctx_6533b7086827` reports 75 existing findings, zero delta.

## Pointer controls and accessible unsaved-close dialog

`title-pointer-controls*` verifies pointer minimize/maximize/restore as well as
the earlier keyboard actions, drag and exact source preservation on release
`8e5f2fd02f59100dabfdcc794535e668a15954d09a2c95b8a65dd14b780f768b`.
The pointer oracle locates chrome in compositor screenshots and translates the
window-local accessibility bounds, accounting for client decoration insets.

The unsaved-close test exposed GPUI's inaccessible fallback prompt: its Save,
Discard and Cancel rows were not keyboard-reachable, and its detail text clipped.
The application now uses the toolkit dialog with actual named buttons and renders
the toolkit dialog layer. Cancel and Escape preserve pending edits; Save queues
the existing save-before-close path; Discard retains the existing file behavior.
The dialog width follows the viewport, its footer can wrap, and backdrop clicks
do not dismiss it. Keyboard confirmation requires a focused choice.

Final release SHA-256:
`b4f2ae5e50f3b93fd213de8a153c8a52be55983eb7e9f9545a4714bbab44f21a`.
Native `title-unsaved-{save,discard}` at 600px verify a real pending edit and
failed autosave in a private fixture directory, Cancel, Escape, reopening the
dialog, and exact final bytes: Save writes only `Find` -> `xFind`, Discard leaves
the original file intact. Typing while the dialog is open cannot alter the
background document. `title-unsaved-narrow` repeats Save at the minimum 480px
window width and captures visible keyboard focus. Prompt and focused-Cancel
screenshots were inspected. This checks the file outcome, not recovery-journal
cleanup or shared-session discard semantics.

Full final checks: 787 tests pass, two existing ignored; log
`/tmp/tachyon-close-dialog-final-check.log`. Capture harness: 23 tests pass,
`/tmp/tachyon-close-dialog-python-final.log`. Native logs:
`/tmp/tachyon-title-unsaved-{save,discard,narrow}.log`.
Crusty `task_204823a80edd2c57` / `ctx_b09bef4fdb87` completed with 75 existing
findings and zero architecture delta. `git diff --check` passed.

## Final title-bar qualification

Final release SHA-256:
`a40d709d5afc383862ed61cc3c135604205125a9b3d5df1c27c5f9b825c3a03b`.

The editor now supplies zoom availability from its own limits. At 75%/200%,
Zoom out/in is visibly dimmed, cannot activate and is excluded from keyboard
focus. Native checks exposed two accessibility gaps: GPUI lacked an aria-disabled
property, and the AT-SPI adapter marked disabled buttons Enabled/Sensitive.
The property now reaches the existing button node through the shared
ButtonAccessibilityExt helper, and the adapter preserves the disabled state.
The adapter regression exercises the actual consumer/state conversion.

Pointer menu activation also required using the button's Click handler, since
title-bar pointer-down is consumed to prevent dragging. Both pointer and
keyboard now drive the same popup state. The old build fails the pointer test
even after waiting for Escape dismissal; `title-menu-pointer-before*` records it.

Final native evidence:

- `title-controls-complete-{-3,10}`: disabled controls at both zoom limits,
  unchanged geometry, distinct normal/hover/pressed pixels, focus, pointer and
  keyboard menu, pointer and keyboard zoom/reset, keyboard Search, pointer Close,
  exact source preservation and restoration of the initial zoom.
- `title-final-scale-{150,180}`, `title-final-narrow240`,
  `title-final-regular-{120,150,180}` and `title-controls-complete-scale240`:
  effective 100/125/150/200% scaling at narrow/default widths, with all the
  applicable state, action and source checks above. Actual button bounds must
  agree with the requested compositor scale.
- `title-final-desktop`: pointer and keyboard minimize/maximize/restore, native
  blank-bar dragging, task-switch reactivation and keyboard Close.
- `title-search-final`: pointer Search, native query/result navigation, nested
  disclosure reveal, copy, edit and exact undo on the preceding build whose
  only subsequent production change was pointer menu activation.
- `title-final-dark`: matching dark title-bar/document palette and full title-bar
  interaction checks. Light and dark menu/state screenshots, both disabled limits,
  and narrow status-row focus screenshots were inspected. The appearance oracle
  now requires the intended title-bar color in the actual title-bar region;
  matching document pixels cannot satisfy that check.

Recovery actions now use toolkit buttons and return focus to the editor before
the selected button can disappear. `status-final-{reload,overwrite}` proves
keyboard Restore, all three conflict buttons' focus/action availability, no
disk mutation before confirmation, and exact Reload/Overwrite outcomes at 480px.
These use release `77c6a6fb15ca5557bae575f658e8cd6fbf6cd4917285ee11f9197682b6d5aae0`;
subsequent changes concern disabled metadata and pointer menu activation. External
Save-copy chooser completion and shared-session/recovery cleanup remain under
their existing persistence work, rather than being claimed by these UI checks.

Final `scripts/check.sh`: 788 tests passed, two existing ignored, formatting,
locked checks, strict Clippy and doctests passed; log
`/tmp/tachyon-title-final-menu-check.log`. Capture tests: 23 passed. Appearance
tests: six passed, including rejection of matching chrome pixels outside the
title-bar region. `git diff --check` passed. Architecture validations retain
75 existing findings with zero new/worsened/resolved findings.
