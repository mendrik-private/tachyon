# Selected document zoom and measured reflow

September 10, 2026. Work `work_dde44398e8ea83e5`.

## Contract

A reader's zoom, font or viewport change must eventually publish matching
measured geometry even when text remains selected. Preserve canonical selection
endpoints and direction, source bytes and edit/Undo. Continue deferring optional
recomposition while a selection is held, and all reflow while a pointer drag or
text composition is active. Do not introduce synchronous HTML/layout work on
zoom input, disable automatic compositions, or weaken stale-worker guards.

## Reproduction

`performance/zoom_publication_check.py` checks native fixture 108 at 1280×1700,
200%. The first executable panel must have at least 800px available width and
paint actual code glyphs. It distinguishes the narrow overlapping original
(`code-selection-light-200.png`, 316px) from the correctly stacked capture
(`code-selection-dark-200.png`, 1003px). It is a targeted fixture pixel check,
not a general grammar or full-content visibility verifier.

Native `zoom-selection-red.png` uses `--select-before-zoom --zoom-steps 10`
with the previous binary. It reliably retains the same 316px overlapping track
after waiting two seconds. Source bytes remain unchanged. Native log:
`/tmp/mineral-zoom-selection-red.log`.

The previously reported missing code in the longer-wait light capture was a
visual-inspection error: its executable panel pixels equal the good dark
capture's pixels exactly. The finding is withdrawn, not claimed fixed.

## Cause and implementation

Zoom immediately scales retained lines and dispatches native remeasurement in
the background. Both dispatch and publication formerly rejected every nonempty
text selection. Selecting before zoom prevented dispatch; selecting while the
worker was running prevented publication. Either could strand old tracks with
enlarged glyphs indefinitely.

The shared selection guard now distinguishes required environment correction
from optional recomposition. Pending font/zoom changes and mismatched viewport
dimensions can reflow a settled selection. Dragging and composition still defer
both dispatch and publication. A matching successful commit clears the pending
text-environment flag; existing source/generation/focus/resource/viewport guards
still reject stale work. No additional worker or synchronous preparation path
was added.

The pending environment also participates in the current-layout check: a
selection can expand a code block's trailing editable row and refresh retained
geometry, which must not falsely declare old-width tracks current. Publication
anchors a nonempty selection at the reading viewport, not at its head as though
it were a caret; otherwise the newly permitted commit could scroll the page.

## Verification

- `selected_zoom_publishes_readable_geometry`: real GPUI view/worker, a reduced
  two-example source, measured peers at 100% becoming stacks at 200%. Covers
  selection before dispatch and selection after dispatch with a real worker
  held by a oneshot. Verifies cleared environment-pending state, measured
  geometry, exact source/selection and top-of-document reading position.
  Original red: `/tmp/mineral-zoom-selected-test-red.log`; focused green:
  `/tmp/mineral-zoom-selected-test-green.log`.
- `selected_reflow_distinguishes_environment_from_active_input`: settled
  reverse selection continues deferring optional recomposition, while width,
  height and zoom changes can proceed; active drag/marked text still defer.
- `scripts/check.sh` exited 0: formatting, locked pins/workspace checks, strict
  Clippy, **703 Rust tests passed, two ignored**, plus doc tests. An existing
  gutter assertion exposed a 0.0001px floating-point difference after real
  remeasurement; it now uses the same 0.01px tolerance as its neighboring fixed
  rail assertion, rather than bitwise equality. No gutter geometry was changed.
  Full log: `/tmp/mineral-selected-zoom-check.log`.
- Python oracle/harness tests: one test with three pixel-oracle cases plus
  seventeen capture-harness tests pass. The pixel test rejects missing body
  glyphs even when header glyphs remain, and rejects the original narrow track.

Final native binary SHA-256:
`057395e0dc07d86b24b918cfb00eb463657208819326700f0bf8c38e98eb2118`.
Fixture 108 SHA-256:
`8aae486e275344cf1730e4f3a1a963d5cca1b354768573a9cc0262aec7f5fa5d`.

Captures under `layout-previews/`:

- `selected-zoom-light.png`, `selected-zoom-dark.png`, and separate cold
  `selected-zoom-light-repeat.png`: 1280×1700, select before zoom to 200%,
  two-second settlement allowance. Native pixel oracle passes: 1003px main
  executable panel, 8099 body foreground pixels, stacked headings and visible
  complete examples. The mixed table below the viewport is not visually
  qualified by these captures. All sources unchanged; first two also verify
  seven clipboard markers once in source order, not whole-clipboard equality.
- `selected-zoom-wide-100.png`: 1600×1500 light retains the useful paired code
  layout and complete mixed table; source/copy-marker checks pass.
- `selected-zoom-edit.png` and `-idle.png`: 200% light, after selected zoom,
  click code then Home/three Shift-Right presses copies exactly `let`.
  Replacing with `x` matches an independently constructed whole-file expected
  source after autosave and one-second idle. Undo restores exact original bytes.
  Header and number-rail pixels are byte-identical to the pre-edit capture.

Native logs: `/tmp/mineral-selected-zoom-*.log`. The initial partial-fix capture
`zoom-selection-green.png` is not accepted evidence: it exposed the selected
endpoint scroll jump fixed before the final captures.

## Limits and next work

This closes the reproduced selected-zoom layout stall, not every font/selection
or responsive-layout state. Native font-switch, resize-during-drag, arbitrary
HTML/IME/RTL, all grammar families and release performance remain part of the
broader audit. Immediate retained geometry during asynchronous remeasurement is
still provisional; these checks establish settled correctness, not frame-time
or reflow-latency qualification. The full goal remains active.

UX guidance shaped readable fallback and viewport anchoring; debugging guidance
required a red native pixel oracle and a minimized worker-timing regression;
Rust guidance shaped shared gating and preservation of stale-work checks.

## Crusty review

Preparation `ctx_b4a5d647d61a`, validation `task_f9d4ae4a26906ddc` completed:
36 existing advisory findings, one new boolean-state-cluster observation
`af_3ea9b3d27d2f`, none worsened. The two flags are independent obligations,
not mutually exclusive states: neither, optional replan only, required text
environment correction only, and both pending are all valid. In particular a
selected code-tail refresh can supply retained geometry while text-environment
correction still remains pending. Dispatch/publication share one selection
guard, and the regression exercises that combination. The advisory has not
been promoted, suppressed or claimed resolved; an enum conversion is not
required to prove this scoped fix. Full audit work remains open.
