# Specification/example resize and enlarged text — September 10

## Contract

The preceding goal turn made verified progress on optional guidance and compact
table rows. This turn retains the layout-first priority and verifies the earlier
specification/example composition in the complete, unchanged API reference.
It does not substitute a smaller synthetic document for the mixed fixture.

At normal text size the parameter table and its authored JSON example should
use the existing wide pair, stack under width/height pressure and recover when
space returns. At 150% and 200% text, this specimen must stack even in the
1710px-wide window: the loaded-font widths no longer permit a readable pair.
This is a fixture-specific expected result, not a new application breakpoint
or permission to shrink text. All content, headings and exact code remain.

## Validation changes

`capture-layout.py --resize-target specification-table|specification-code`
selects either edit target in fixture03. Existing default scenarios are retained.
The new modes support 100/150/200% text and reject other specimens or unsupported
zoom combinations before opening the private session. Actual committed traces
must match the requested text zoom as well as the physical viewport dimensions.

The reading oracle asserts either a paired or stacked wide state explicitly;
it cannot accept whichever layout happens to appear. Restoration must return
to that same expected state. The editing oracle uses the complete literal code
as its target for JSON editing, requiring exactly one inserted `x` and exact
surrounding bytes. Unlike prose scenarios with canonical escaping, fenced code may
not be normalized or receive Markdown punctuation escaping. Native Find selects
the intended text; typing, resize, autosave and Undo use the ordinary app paths.
The selected plain table cell also uses an exact literal insertion. Its initial
test expected the hyphen in `Human-readable` to be escaped; the native whole-file
hash instead matched exactly the original bytes plus the intended `x`. The
corrected golden requires that literal result, not either serialization form.
Other existing paragraph scenarios retain their escaping requirements.

The new Python regressions reject wrong wide/restored topology, incorrect code
escaping, unrelated source edits, wrong committed text zoom, unsupported
fixture/zoom requests and missing targets. Existing tests retain their defaults.
The initial scenario test failed before implementation because the alternate
target API did not exist.

## Reading-position setup, not a production scroll fix

The initial native run passed pair/stack/recovery but reported a 32px movement
of Parameters (480→512→480px relative to the editor). The heading started near
the bottom of the viewport. `reveal_semantic_bounds` deliberately returns
without scrolling when a semantic object is already visible; the AT-SPI
TOP_EDGE request reaches this existing ScrollIntoView behavior.

An attempted pointer-wheel setup did not establish the required anchor and
was removed. It depended on compositor coordinates that the semantic bounds
do not establish. The accepted setup reveals the later authored section and
then Parameters through native accessibility actions, preserving selection.
The oracle rejects setup unless the heading is near the viewport top. The
original one-physical-pixel displacement tolerance remains unchanged. No
production renderer, scroll policy, fonts, planner, dependency or source file
was changed to satisfy this test.

## Runtime and evidence

Runtime remains:
`975061b0fee2e48b96d251f8e3e5ad26acf3d8aa38fe1d157860ecd53b42c3e6`.
Original fixture03 remains:
`075a04766b464776cb052a4c077be0e0817cd5b9d2de2520eb0d307777c36205`.

Native runs use a 3200×1400 isolated Weston desktop, the existing validation
width/height actions and active AT-SPI on a private bus. Final prefixes are
`layout-previews/spec-resize-final-`. The desktop is large enough to show the
complete test window; the document still uses its actual 1380/650px widths.
These are burst correctness checks, not physical pointer-drag or release
performance qualification.

All **14 specification scenarios pass**: six reading runs (width and height
at each text size), six width-edit runs (table and code at each text size), and
two normal-size height-edit runs. Reading-heading displacement is zero in all
six runs. Each edit preserves focus/caret, exact whole-file autosave and Undo;
each burst checks the latest committed viewport and requested zoom. Width
changes are 1380→650→1380px; height changes are 884→304→884px. At enlarged
text, the expected wide/restored state is a stack, not a forced pair.

The two initial table width reports named `spec-resize-final-specification-table-
editing-width-zoom0|5` retain their failed escaping expectation as diagnostic
evidence. Accepted reruns are `spec-resize-verified-table-editing-width-zoom0|5`.
All other accepted specification reports retain the `spec-resize-final-` prefix.
Original-size screenshots of the normal wide composition and the visible
200% JSON edit were inspected. This is not an assertion that every offscreen
component or every enlarged glyph was pixel-checked.

Verification: 17 resize-oracle and 18 capture-harness Python tests pass; all
three existing Rust specification relationship/layout/resize/edit regressions
pass. Logs: `/tmp/tachyon-spec-resize-unit.log`,
`/tmp/tachyon-spec-capture-unit.log`, `/tmp/tachyon-spec-resize-rust.log`.
`cargo fmt --all -- --check` and `git diff --check` pass. The previous turn's
756-test workspace run remains historical evidence, not a rerun claimed here.
The default guidance and opening scenarios also pass native width bursts with
zero observed reading displacement (`spec-resize-control-01-field-notes` and
`spec-resize-control-118-opening-overview`), retaining both wide-key contracts.

The app-UX skill guided content-fit adaptation and visual inspection rather
than preservation of columns at all zooms. The Rust skill guided exact source
ownership and negative regression checks. The diagnosing-bugs skill required
distinguishing test setup from an app anchoring defect before changing runtime.

## Remaining scope

The full audit remains active. Broader mixed-document families, arbitrary
reading anchors, Unicode schema recognition, full IME/RTL interaction matrices,
large-text height editing, physical resize timing and page/export/performance
requirements are not completed by this checkpoint. P02 and A07 remain partial.

Crusty validation `task_82cd655009f376f4` for `ctx_6f8c3d510726` completed with
75 existing advisory findings and zero new, worsened or resolved findings.
