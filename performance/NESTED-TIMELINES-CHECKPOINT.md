# Nested dated lists — September 10

## Layout and ownership

The layout-first implementation now measures dated lists inside list items,
including a dated event containing its own dated milestones. Every candidate
still requires explicit accepted dates and nonempty summaries. Nested lists
remain vertical, keep their source indentation and complete supporting blocks,
and use the readable summary measure established by the enclosed-timeline pass.
Code retains the available technical width. No dates, ordering or content are
invented and no second editable document is introduced.

The planner discovers all canonical list ancestors, deduplicated by list ID.
Each projected leaf points to its nearest event; nested event headers separately
point to their enclosing event. These acyclic ancestor links participate in the
geometry key. Painting follows the prepared chain and deduplicates event rails,
so an offscreen parent is not lost when only a child's code is visible. Leaving
a child list resumes the enclosing event; leaving that list removes ownership.

An ordinary hierarchy guide is suppressed when the same list already owns a
timeline rail. This avoids two parallel rules for one relationship. Under deep
hierarchy pressure, the existing compact tree keeps explicit parent captions and
readable reanchored text; date rails are not layered over those reanchored
branches. This is a content-preserving geometric fallback, not flattened data.

## Rust verification

Three new regressions cover:

- All eight dated headers in fixture121, nested/outer ownership, parent-link
  geometry identity, complete projected text and measured bounds at mock
  100/150/200% text and wide/narrow widths.
- Depth-four dates on a wide canvas, switching to the complete readable tree
  under width pressure, with exact source preserved.
- A nested code block growing by 60 lines, retained focused columns and both
  ancestor rails spanning the support, and byte-exact Undo.

The first regression failed before implementation: only the two top-level dates
were measured instead of all eight. Mock fonts establish layout invariants, not
native glyph fit. `scripts/check.sh` passes formatting, locked all-target
checking, strict Clippy, workspace tests, adapter tests and doctests:
**765 passing Rust tests**, two existing ignored tests.
Log: `/tmp/mineral-nested-timeline-check.log`.
All 23 resize-harness and 19 capture-harness Python tests also pass. Final
formatting and `git diff --check` pass.

## Native evidence

Fixture `layout-fixtures/121-nested-timelines.md` SHA-256:
`8e8eab7e72edb964a3f9e0e0e5ec4c2bb43512ebb4cff54e1dbe7d7d4e9ca20d`.
Immutable final layout-validation binary:
`/tmp/mineral-nested-timeline.B3CYKl/mineral-final`, SHA-256
`fb0b13812d5c4a752b1b6b28be70dc2b605750097fbc05547375919ebc8a2b0e`.

All captures run in isolated Weston with a private active AT-SPI session, not
on the physical desktop. Prefixes under `layout-previews/`:

- `nested-timeline-before`: pre-change 1600×2000 light baseline.
- `nested-timeline-final-wide`: 1600×2000 light, 100%.
- `nested-timeline-final-narrow`: 768×2000 light, 100%.
- `nested-timeline-final-enlarged`: 768×2000 dark, 200%.
- `nested-timeline-offscreen`: 1600×240, 60 additional nested code lines and
  settled scrolling; no fixed appearance oracle is claimed for this stress view.
- `nested-timeline-deep-fallback`: a separate depth-four source document,
  768×1200 light, 200%, with readable parent captions and preserved source.

`nested_timeline_check.py` compares all **38 canonical nodes** for fixture121,
including identities, parents, names, roles and actions. Wide/narrow/enlarged
captures retain exact source and event order. Both original technical blocks
retain native x/width/height. All four timeline connectors have 100% one-pixel
rule coverage, without a duplicate ordinary-tree rule. The pre-change baseline
used as the final capture fails the native marker oracle.

The offscreen stress source SHA-256 is
`91e0825cdbdb9cce64b46bdfb4fe1337c65ac6d6aa2ddb64ba9705984f712d4e`.
Its editor viewport is y=54, height166; the enclosing header is y=-952 and the
inner header y=-917, each height24, both outside the full overscan viewport.
Both distinct ancestor rails retain 100% coverage across all 166 visible pixels.
The 60 added comments and original command remain exact. Original-size enlarged,
offscreen and deep-fallback screenshots were inspected. This does not claim that
every offscreen glyph or all nested configurations were visually reviewed.

`nested-timeline-body-edit` and `nested-timeline-code-growth` exercise native
typing/paste, whole-file exact autosave, idle reflow, blur and exact Undo. The
code case pastes all 60 comments into the inner event. The body golden explicitly
includes normal serializer escaping only in the edited paragraph; all unrelated
source bytes remain exact. `nested-timeline-copy` confirms all ten supplied
markers once in source order through native clipboard copy; it is an order check,
not a full rich-MIME interoperability qualification.
Log: `/tmp/mineral-nested-timeline-edits.log`.

Intermediate `nested-timeline-wide` uses b4d1cb5f and still has duplicate ordinary
tree rules; it is not final visual evidence. The final fb0b1381 build removes
those rules and supplies the native pixel evidence above.

`nested-timeline-enclosed-control` reruns fixture120 on the final binary. The
enclosed-timeline checker passes its full 40-node identity, unchanged technical
geometry, readable summary and both rail pixel checks.

## Remaining scope

The app-UX skill guided hierarchy, readable width and restrained guides; the Rust
skill guided canonical ownership, retained geometry and regression coverage.
L10/A07 and the full audit remain active. Nested continuous resize, structural
edits changing hierarchy during focus, outer-context combinations, RTL/IME and
the full state matrix remain unqualified. Export/page families, playable media,
retained accessibility and release performance remain separate open work.

Crusty validation `task_46b53dc3d292bdb6` for `ctx_b7b167b71220` completed with
75 existing advisory findings and zero new, worsened or resolved findings.
