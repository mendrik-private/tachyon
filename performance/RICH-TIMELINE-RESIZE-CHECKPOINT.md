# Rich timeline resize and editing — September 10

## Scope and acceptance

The preceding goal turn made verified production progress on rich dated events.
This follow-up keeps the layout-first priority and exercises the complete
fixture119 while the native window changes size. It does not replace the
document with a smaller date-only specimen or force its rich events into rows.

The three rich events must remain vertical at 100/150/200% text. Every
supporting leaf, including nested descendants and table-cell paragraphs, must
stay between its authored date header and the next event (or following section).
The complete 68-node canonical document must retain IDs, parents, roles, names,
descriptions and actions across each resize. Reading position, exact source,
editing focus/caret and current committed dimensions/zoom are separate checks.

## Harness changes

`capture-layout.py --resize-target` adds `timeline-body`, `timeline-code`,
`timeline-table` and `timeline-nested`. These require the unchanged
`119-rich-timelines.md`, an explicit resize mode and 100/150/200% text. Find
selects the exact authored target; the ordinary native editor receives the
insertion. The whole-file golden permits canonical punctuation escaping only
in the selected paragraph/nested paragraph. Code and the selected plain table
cell stay literal; nested indentation and all unrelated bytes must remain exact.

`resize_layout_check.py` gives this scenario an explicit vertical wide state,
not whichever topology happens to appear. The existing reading setup's later
heading is now scenario data: reveal Compact milestones, then Release notes.
This retains the existing accessibility ScrollIntoView contract and avoids
depending on compositor window coordinates. The existing specification scenario
keeps its own later heading and behavior.

`rich_timeline_check.py::event_support_geometry` uses canonical parentage to
include all paragraph/code leaves belonging to each dated event. It rejects
zero-sized leaves and content overlapping the header or escaping past the next
event. It does not mistake table-row peers for separate chronological events.
Both reading and editing resize checks combine this with complete canonical
identity, rather than testing only the visible date headers.

Three new resize-oracle tests cover supported/rejected targets, source literal
versus escaped contracts, nested indentation, wrong horizontal topology, and
support crossing its date boundaries or losing geometry. A capture-CLI test
rejects wrong/missing fixtures, missing explicit targets and unsupported zoom.
The initial target test failed before implementation. The source/topology
checks initially passed all 20 resize-oracle and 19 capture-harness tests.
Both current Rust rich-event placement and code-growth/source/Undo regressions
also passed, but native edit state exposed the additional defect below.

## Previously visible caret lost on height shrink

The first edit oracle checked stable caret offsets, not viewport containment.
Its native captures exposed a caret below the shortened viewport during nested
editing. Consequently those green reports are not qualification of caret
visibility. The corrected oracle also requires the full caret line to remain
vertically visible, comparing caret and scroll-viewport bounds reported in the
same native window coordinates. Missing bounds, non-finite values and stable
but offscreen offsets fail its new regression.

The production reflow path tested the old visual lines against the new viewport
height when choosing a caret anchor. It could discard a previously visible
caret before anchoring, then preserve the reading position instead. It now tests
against the last committed viewport and, after restoring a caret anchor, reuses
the existing visibility adjustment to fit its full line in the new viewport.
Deliberately reading away from an old edit must not pull that caret back onscreen.

A focused native-editor Rust regression reproduces the failure before the fix:
the caret line was 893.5–917.5px while the new viewport was 393.5–633.5px.
It uses the complete rich-event fixture plus trailing context so the negative
case can genuinely scroll past the edit. It explicitly establishes editor focus;
an earlier draft lacked that precondition and was corrected before the accepted
red run. Both visible-caret and scrolled-away states preserve selection, exact
edited source and Undo. The additional bounds reporter remains feature-gated.

## Native runtime provenance

The initial runs used an isolated copy of validation binary SHA-256
`8db6f11c2380fbdb544804566863727780f6bbbc2a464b3639bec4f763724687`.
Fixture119 SHA-256 remains
`3341916cec6774738cff1db49366eb66ad5221ed251924afcddcd0ad8227bf97`.
The private Weston desktop is 3200×1400 so the complete test window is on-screen.
Window actions are the existing feature-gated native commands, not physical
pointer dragging. Each run uses active accessibility and detailed layout traces.

The first width run passed, but the following height setup failed twice before
resizing: F5 left the document width at 792px. The shared `target/debug` executable
had changed between runs. Its SHA-256 was
`07cb0ac2aab0c5f1fbb11a0048e4c35f0691ffd4489f9af47827bb413819be55`;
Cargo's matching fingerprint recorded `features: []`. The retained dependency
artifact matched the original hash and recorded `layout-validation`. Running an
isolated copy of that exact artifact restored the expected native resize actions.
During that diagnosis the newer shared executable was left untouched. No focus, timeout, layout or
geometry tolerance was changed to hide this build-profile mismatch.

Pre-fix evidence prefixes are `layout-previews/timeline-resize-verified-`.
The six reading runs passed: width and height at every text size, with zero
observed physical-pixel reading displacement. Widths are 1380→650→1380px;
heights are 884→304→884px. Repeated final-size requests retain geometry and
all committed traces match the actual viewport and requested text zoom.
The normal-size narrowed reading screenshot was inspected at original size.
All 24 pre-fix editing reports passed their original source/topology/offset
oracles; they do not establish caret visibility and are retained as diagnostic
evidence rather than silently overwritten.

The corrected runtime is SHA-256
`1f595c23f184e54bd8c8788d7e96cfd21de60c923502852475be0e1b6d044909`,
also captured from an isolated artifact copy. New evidence uses
`layout-previews/timeline-caret-fixed-` and the stricter visibility oracle.
The original nested-edit height case now passes: its caret line moves from
514.5–538.5px in the tall viewport to 355.5–379.5px in the 75–379px short
viewport, within the existing one-physical-pixel rounding tolerance. Selection
remains exactly 812..812. Scroll changes only enough to reveal that line; exact
whole-file autosave and Undo pass. Its original-size screenshot was inspected.

All **18 corrected-runtime native cases pass**: 12 height-edit scenarios
(body, code, table cell and nested text at each of 100/150/200%) and six reading
scenarios (width/height at all three text sizes). Every edit satisfies the new
vertical-caret visibility check, exact whole-file save/Undo, focus/selection,
68-node identity and complete supporting-event geometry. All six reading
displacements are zero; committed viewport and zoom checks pass throughout.
The 150% code and 200% nested-edit short-window screenshots were also inspected.

The corrected-runtime batch ended with SIGTERM after 14 completed cases. Its
handle was terminal and no capture process remained. Only the four unverified
200% cases were resumed: table/nested height editing and width/height reading.
Those accepted reports use the `timeline-caret-final-` prefix; the other 14 use
`timeline-caret-fixed-`. All 18 have the exact same corrected binary hash. No
failure or partial report was substituted for a completed case.

`scripts/check.sh` passes on the fix: formatting, locked all-target check,
strict Clippy, workspace/adapter tests and doctests, totaling **760 passing
Rust tests**, with two existing ignored tests. All **21 resize-oracle and
19 capture-harness Python tests** pass. The earlier timeline pixel verifier also
passes on its original runtime-bound artifacts; that is a verifier regression
check, not new-runtime pixel qualification. Logs include
`/tmp/tachyon-caret-resize-check.log`, `/tmp/tachyon-timeline-resize-unit.log`,
`/tmp/tachyon-timeline-resize-capture.log`, and
`/tmp/tachyon-caret-resize-focused-red.log` / `-green.log`.

## Remaining scope

The app-UX skill guided reflow, full-event grouping and canonical accessibility
checks. The Rust skill guided exact source/Undo regressions. The diagnosis skill
kept the feature-profile mismatch separate from a production resize defect.
L10 and A07 remain partial. Timeline containers nested inside other lists or
quotes, arbitrary reading anchors, complete IME/RTL/copy/selection matrices,
physical resize timing and the wider page/export/media/performance requirements
remain open. Correctness captures are not release performance qualification.
New-runtime width-edit visibility and grow-back-while-editing coverage are not
claimed by this height-edit checkpoint; the earlier width-edit reports checked
offsets only. The full objective is not narrowed to the completed scenarios.

Crusty validation `task_e5dbce277c6d5ddb` for `ctx_c369c0dea386` completed with
75 existing advisory findings and zero new, worsened or resolved findings.
