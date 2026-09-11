# Enclosed timelines and readable summary measures — September 10

## Layout change

The layout-first pass now recognizes dated lists inside quotes and callouts,
not just top-level lists. These enclosed timelines stay vertical; they do not
acquire horizontal peer slots or leave their source container. Their code and
nested supporting bullets remain in authored order. Existing flat top-level
milestones can still form measured horizontal timelines.

Date labels and container padding are now outside the summary's reading-width
budget. A vertical event can use the full native-measured reference prose
measure when the canvas permits, rather than wrapping prematurely because of
its label. Narrow events still stack their date above the summary. This width
correction also applies to existing top-level vertical timelines.

`nested_measures::reading_width` publishes the same outer extent as the measured
columns, including container insets. Container chrome, accessibility, selection
and painting therefore receive matching bounds instead of the old narrower
prose rectangle. No new typography, palette, borders or dependencies were added.

The planner discovers each outermost list through canonical projection ancestry.
Quoted lists retain incremental-measurement and edit locks through their real
top-level container. Prepared event ownership is keyed by the actual list, not
the enclosing quote root: a following paragraph or unrelated list must never
inherit the last event's connector. Supporting nested leaves remain owned by
their event. Dated lists inside another list are deliberately not enabled here.

## Regression evidence

Fixture `layout-fixtures/120-enclosed-timelines.md` has SHA-256
`6cea77a62ea39cc93151a39ce902e3f67840f3453731729295908ea0d9927f89`.
The immutable final layout-validation binary is
`/tmp/tachyon-enclosed.UXxF6s/tachyon-published`, SHA-256
`88379c6be9d7b5b2feff8a38ad91c7fce3111da83976a8a049d0ff12fec18a7e`.

Two new Rust regressions cover enclosed recognition, wide/narrow geometry at
mock 100/150/200% text, complete projected text, event/list boundaries, unchanged
source, focused body/code/date edits, narrow recomposition and exact Undo.
The recognition test first failed because enclosed dates had no timeline.
The published-bounds assertion separately failed before matching outer geometry
was added. Mock shaping does not establish native glyph fit.

`scripts/check.sh` passes formatting, locked all-target checking, strict Clippy,
workspace tests, adapter tests and doctests: **762 passing Rust tests**, two
existing ignored tests. Log: `/tmp/tachyon-enclosed-check-final.log`.
All 23 resize-harness and 19 capture-harness Python tests also pass; final
`cargo fmt --all -- --check` and `git diff --check` pass.

## Native evidence

All captures use isolated Weston and an active private AT-SPI session, never the
physical desktop. Final fixture120 prefixes under `layout-previews/`:

- `enclosed-timeline-final-wide`: 1600×1800, light, 100%.
- `enclosed-timeline-final-narrow`: 768×1800, light, 100%.
- `enclosed-timeline-final-short`: 1600×480, light, 100%.
- `enclosed-timeline-final-enlarged`: 1600×1800, dark, 200%.
- `enclosed-timeline-final-cramped`: 768×1800, dark, 200%.

`enclosed_timeline_check.py` compares all **40 canonical semantic nodes**,
including identities, parent relationships, names, roles and actions. All five
captures preserve source bytes and document order. Native wide code position,
width and height remain unchanged from `enclosed-timeline-before`. The quoted
date header now owns a 700px rectangle instead of 616px, and its short summary
fits one 24px line. Both quoted and callout connectors have 100% native one-pixel
rule coverage. Existing top-level milestones retain horizontal placement.
The pre-change baseline used as the final capture fails the width oracle.

`enclosed-timeline-final-stacked` adds a settled, scrolled 768×1800 capture at
200% text with default light appearance. Its callout visibly stacks the complete
date above each summary and retains container insets. It passes the same complete
semantic/source comparison, but does not claim a separate appearance-oracle
check. This original-size screenshot was also inspected.

Original-size wide, narrow and cramped screenshots were inspected. This is not
a claim that every offscreen glyph has been visually reviewed. Intermediate
`enclosed-timeline-wide` uses the earlier c00425c6 build and is not final evidence:
its short quoted summary wrapped to two lines, prompting the readable-measure
correction and the published-bounds regression.

`enclosed-timeline-edit-verified-body` and `-code` exercise native typing,
whole-file exact autosave, idle reflow, blur and byte-exact Undo. The first body
attempt expected an unescaped terminal period; the source serializer correctly
escaped that punctuation only in the edited paragraph. The final whole-file
golden explicitly includes that escape and still requires every unrelated byte
to remain untouched. The failed initial attempt is not counted as a pass.
Log: `/tmp/tachyon-enclosed-native-edits-verified.log`.

`enclosed-timeline-rich-resize-control` exercises the existing full fixture119
body at 200%, shrinking and restoring window width during editing. All checks
pass: complete 68-node semantics, source event support, focused visible caret,
restored dimensions/committed geometry, unchanged edited bytes across both
resizes and exact Undo. This is one current-build control, not a rerun of the
earlier 24-case matrix or evidence for enclosed-container resize interactions.

## Remaining work

The app-UX skill informed shared alignment, readable measures and native review;
the Rust skill informed canonical ownership, retained geometry and discriminating
regressions. L10/A07 and the full audit remain active. Dated lists nested in other
lists, outer-context combinations, enclosed continuous resize, RTL/IME and the
complete state/interaction matrix remain open. Page/export families, playable
media, retained accessibility and release performance qualification are not
closed by these correctness captures.

Crusty validation `task_b6b2d7fefb39e3dc` for `ctx_edcfcb565309` completed with
75 existing advisory findings and zero new, worsened or resolved findings.
