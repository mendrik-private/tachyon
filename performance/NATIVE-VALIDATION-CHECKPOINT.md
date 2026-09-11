# Native validation: repaint mode and named controls

This checkpoint resolves the apparent idle-startup failure in the Weston MCP
session and fixes unnamed controls discovered there. It does not approve the
complete document grammar. Captions/credits, margin notes, bibliography, media,
technical variants, all page masters/export and the full interaction matrix
remain in `DESIGN-GRAMMAR-COVERAGE.md`.

## Root cause, not an application redraw workaround

The live MCP compositor is Weston 14.0.2, headless GL, kiosk, 1600×1200,
started with `--refresh-rate=0`. In this backend, zero explicitly selects
**repaint only on capture**, not ordinary unsynchronized rendering. The native
application gets its first configure, schedules a frame callback, finishes its
initial layout worker, and then waits for the next compositor frame. An idle
wait cannot supply that frame. See the [versioned Weston implementation](https://gitlab.freedesktop.org/wayland/weston/-/blob/14.0.2/libweston/backend-headless/headless.c).

The minimal reproduction is an empty Markdown file. On release
`daba40e9890473c94229b684aeb9d7408cf650946e30aa0d526ce05b98aef2e9`:

```sh
python3 performance/capture-layout.py --fixture 68-idle-empty.md \
  --width 1600 --height 1200 --refresh-rate 0 --startup-wait 0.5 \
  --startup-ready-width 1280 --layout-trace summary
```

It fails with zero committed layouts, as expected for capture-only mode without
capture. Changing only refresh to 60000 passes before any screenshot or native
input. At zero, explicitly requesting startup captures permits the real frame
callbacks and measured layout to complete. No timer, frame pump, refresh-rate
special case, or retry loop was added to the app/backend. Temporary tagged
Wayland probes were removed; the vendored window file has no remaining diff.

The harness now names this mode accurately and exposes bounded
`--startup-capture-frames 1..12`. Artifacts distinguish capture-driven from idle
readiness, count startup captures and record binary/source hashes and input.
Capture-driven setup is rejected for the idle-layout check; the performance
gate continues to require 120 Hz. Readiness checks the latest usable committed
publication with resolved image dimensions, not an obsolete correct-sized
result. An original failure is no longer overwritten by the cleanup's missing
publication check.

`startup-capture-{figure,definitions,footnotes}` records successful six-capture
startup with the same unchanged release and source; those PNGs were inspected.
The unpumped failure remains `startup-capture-unpumped.startup.json`. This is a
positive/negative diagnostic pair, not a claim of idle rendering at zero.

## Actual implementation change

The pinned component button supplies a visible-label name, but icon tooltips
do not become accessible names. A small `controls.rs` adapter applies public
GPUI metadata to the existing button node. It creates no additional rendered
or accessible wrapper and does not modify the shared dependency checkout.

- Application menu, zoom out/in and reset now have action names.
- Reset retains its percentage visually and exposes the current percentage
  separately as a description; actual keyboard shortcuts are annotated.
- Link, Bold, Italic, Strikethrough and Inline code have names on their existing
  formatting buttons.

The native name/action checker fails on the pre-change tree and passes on the
new tree. Weston MCP app 33 verified Zoom in through its advertised click
action (100→110%), then Reset (100%), and exposed all five formatting actions
after a real pointer selection. The private Markdown remained byte-identical.
`startup-toolbar-debug.weston.json` records these observations. Application and
Text dropdowns still expose no click action: naming is fixed, complete dropdown
accessibility is **not**. That remains an explicit next interaction defect.

## Release visual evidence

Final release: `14dea702a5c193254ecea6244f8fc8a6e826ce490d852e8590601602884df9a9`.
Immutable test binary: `/tmp/mineral-native-startup.GN6qPB/tachyon-release`.

`startup-toolbar-final-{wide,narrow,200-top}.png` and their native/source
sidecars cover fixture 67 at 1600×1200, 600×1100 and 200% document text.
All three were visually inspected and pass `figure_flow_check.py --prefix
startup-toolbar-final` and the native title-bar checker. Wide idle startup
also passes at 60 Hz before input or capture.

The figure occupies 30.3% of a 528 px reading measure, with a 24 px gutter,
six useful 28 px lines beside it, a full-measure continuation, 24 px paragraph
gap and 64 px section gap. At 200%, these distances double. Narrow content
stacks in source order. These checks cover this specimen, not all media forms;
the ordinary stacked-image leading inset still reveals a white underlay that
needs attention in the media-surface pass.

Before/after native harness comparison of the document rectangle and the full
title bar reports **zero differing pixels** between the pre-change release and
the debug metadata fix at 100%. No visual restyling was intended by this fix.

Final-release Weston MCP app 34 rendered real superscript footnotes and named
forward/return actions after six startup captures. Activating Footnote 1 placed
the caret at its note, confirmed visually. The tree is in
`startup-toolbar-final.weston.json`; the screenshot is
`startup-toolbar-final-footnotes-weston.png`. Source hash remained
`c65a88f5495219f39b61054e1ceff08a59bc969c316665e02802afa397e04f9a`.
All our MCP apps were stopped normally; the compositor was left running.

## Checks

Workspace tests: 111 core, 9 source-fidelity, 11 tree-selection, 369 view
(2 explicitly ignored), and 40 app/bin tests pass. Formatting, diff whitespace,
workspace Clippy `-D warnings`, and workspace doctests pass. Ten Python harness
and name-oracle tests pass. Logs are `layout-previews/startup-toolbar-*.log`.

Crusty preparation contexts `ctx_a389c09ae41b` and `ctx_dfc7c8a2a6f8` were
validated by terminal tasks `task_615d169613ec7ca3` and
`task_358dceb3e378173f`: 36 existing advisory findings, none new/worsened/resolved.
Older indexed dark/minimap steering remains superseded by the user's explicit
light-first/no-minimap direction.

## Current-build scrolling

`layout-previews/startup-toolbar-final-10mib.json` records the final release on
10,485,760 bytes of generated mixed Markdown, 1600×1200, 120 Hz, 60.027 measured
seconds of real bidirectional continuous Wayland input. Average presentation
is 108.65 fps; draw p99 is 8.52 ms, presentation p99 12.26 ms, and measured input
latency p99 11.44 ms. The >60 fps gate passes with unchanged source bytes.

There were four presentation intervals ≥25 ms (maximum 83.36 ms), one draw
≥25 ms, and 1.017% missed refresh opportunities. This is not an every-frame
16.7 ms guarantee or 120 fps. Builds and other test apps had finished before
this run. Accessibility was inactive for this measurement. It does not replace
the separate AT-SPI-active, wheel-coast, recovery or grammar-heavy gates.
