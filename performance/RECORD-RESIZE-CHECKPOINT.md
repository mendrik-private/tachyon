# Record thresholds and active-cell resize — September 10

## Narrow-layout finding resolved

The previous checkpoint's fixture76 mismatch was an obsolete window-size
expectation, not a broken record classifier. The old 600 px specimen had a
544 px content canvas, below the 560 px record threshold. Reduced outer margins
now provide 576 px in the same window; the complete compact table fits there.
The current 550 px window has 526 px for content and correctly uses records.
The exact source and all 104 canonical nodes are unchanged between those modes.

`entity_records_check.py` adds the separately reviewed 550×2400 surface with
the current 32 px following-section gap. It retains every historical surface's
original expectations. Native `entity-threshold-narrow` passes all four record
border/inset checks, twelve repeated-label ink checks, empty Steward retention
and comparison-column checks. The source-boundary Rust test now explicitly
covers 526, 544, 559, 560 and 576 logical pixels, alongside wider controls.
There is no filename/window-size exception in the renderer and no forced record
mode on the better-fitting 576 px canvas.

## Actual resize defect and correction

The new native record matrix exposed a different defect: after switching to a
scrollable table, an active trailing header could disappear horizontally even
though its source offset and vertical anchor survived. A reduced three-column
table with explicit 400 px columns reproduces this without any record grammar.
Its regression test failed because the active header was not painted within
the resized viewport (`/tmp/tachyon-header-reveal-red.log`).

Caret restoration now derives the horizontal target from the newly prepared
cell bounds, native shaped caret position, alignment and table viewport. It
adjusts only that component's horizontal scroll. The same calculation checks
whether the old caret was horizontally visible: intentionally scrolling away
from an edit does not pull it back into view on resize.

A rapid 200% resize revealed a second contributing cause. While replacement
geometry was still preparing, painting the previous fractions at the new
element width could prematurely clamp the retained horizontal offset. Clamping
now waits until painted element width matches committed layout width. This
also fixes the separate negative regression for a deliberately scrolled-away
first header. Debug probes demonstrated the intermediate offset being reduced
from 2255 px to 735 px; all `[DEBUG-table-reveal]` source instrumentation was
removed before the final build.

The validation-only caret reporter now rejects the unrelated last painted line
when the actual selected range is absent. A missing caret cannot supply plausible
but false coordinates to the native visibility oracle.

## Focus and verification contracts

An edited record that stacks after shrinking intentionally keeps that narrower
measure when the window grows. The existing focus lock prevents text moving
under the caret. The native oracle checks retained width, relative leading edge,
complete source geometry and full caret visibility, then leaves the edited
table through native Find and verifies recomposition and unchanged saved bytes.
At 100% this restores the full-width paired layout; at 150/200% the complete
single-record/table arrangement remains appropriate to the measured content.

The initial new oracle incorrectly required immediate pairing while focused;
it was corrected to the existing focus-lock contract, with a separate mandatory
release/recomposition check. A first release attempt used unbound Ctrl+Home;
a pointer attempt exposed Wayland's unavailable global window origin. Both
failed attempts are retained, not accepted. Native Find now supplies an exact
source-owned keyboard target. General document-start/end shortcuts remain
outside this qualification.

Resize polling now waits, within its existing deadline, when a replacement
AT-SPI tree briefly lacks the editor. It records the incomplete snapshot count;
duplicate editors, malformed extents, probe errors and failure to reach the
requested size remain failures. Entire canonical snapshots and committed
geometry are still required after settling.

`record_geometry` checks complete fields, source-ordered bands, non-overlap and
retained empty values in both labeled records and ordinary table rows. Native
resize checks additionally compare all 88 canonical nodes and their identities,
parents, roles, names and actions. Exact whole-file edit/autosave/Undo checks
reject changes to unrelated fields; restoration checks include both axes of
the native caret rectangle. Reading checks retain their visible heading anchor.

## Build and evidence

Final immutable layout-validation runtime:
`/tmp/tachyon-header-reveal.IynIRn/tachyon-final`, SHA-256
`462dc128dd3c7e6b1c2a6e5e595a90eec572f1c52a697661b68107cba2b25f71`.
Fixture122 source SHA-256:
`c3ae9753a2fdec364181e33a95d8364cf8311ec09deeb22ea3c86de8a13d0e35`.
Fixture76 source SHA-256:
`5c607ea0536a188cd6591478010394890b1cc75612408eb1ace34ab19c7395c9`.

`scripts/check.sh` passes formatting, locked all-target checks, strict Clippy,
workspace/adapter tests and doctests: **770 passing Rust tests**, two existing
ignored tests. Log: `/tmp/tachyon-header-reveal-final-check.log`.
Both new resize regressions pass, including exact source/Undo and intentional
horizontal-scroll preservation. All 27 resize-harness, 27 record-checker and
19 capture-harness Python tests pass. `git diff --check` passes.

All **18 native scenarios** pass on the final SHA: six reading cycles and twelve
body/header editing cycles, each across width/height and 100/150/200% text.
The isolated output is 3200×1400; feature-gated commands produce the wide/narrow
and tall/short app windows. Each resize sends a rapid alternating burst, then
requires the requested committed geometry. All six reading displacements are
0 px. Every editing case retains exact autosave through shrinking and restoring,
full caret visibility, source identity and exact Undo; leaving the table releases
the focused layout and passes the appropriate recomposition check.

Accepted prefixes under `layout-previews/`:

- `record-verified-z{0,5,10}-reading-{width,height}`: six reading cycles.
- `record-verified-z{0,5,10}-record-{body,header}-{width,height}`: eleven edit
  cycles; the 200% header/width case is `header-reveal-final-native` instead.
- `entity-threshold-verified`: final-runtime 550×2400 light fixture76, all four
  record borders/insets, twelve labels, empty value and comparison checks pass.
- `paired-records-reveal-control`: final-runtime 1600×2200 light fixture122,
  all 88 canonical nodes, 100% canvas utilization, retained comparison geometry,
  exact source and 333 px height saving still pass.

The accepted matrix contains exactly one instance of every required case, all
with the same source/runtime hashes. No incomplete accessibility snapshot was
observed in those final 18 cases; earlier attempts established that the polling
race can occur. The final 200% narrowed header and normal-size released paired
layout were inspected at original size, as was the narrow threshold specimen.
No claim is made that every offscreen glyph in every capture was inspected.

All native work uses isolated Weston and a private active AT-SPI session, not
the physical desktop. These are correctness captures, not release-performance
qualification. Intermediate `record-final-*` captures use ff2e8c04 and do not
contain the final temporary-metric correction. `header-reveal-debug` additionally
contains temporary diagnostics and is not a release candidate.

The UX skill guided content-width decisions and focus stability; the debugging
and Rust skills guided minimized failing cases, source ownership and negative
regressions. T03/A07 and the full audit remain active. Structural table edits,
post-restoration typing, arbitrary selection/IME/RTL, nested records, transposed
comparisons, print/export, media and release performance are not qualified here.

Crusty validation `task_587250739ab339ef` for `ctx_36fa9c0ecdf4` completed with
75 existing advisory findings and zero new, worsened or resolved findings.
