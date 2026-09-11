# List source fidelity — grammar editing checkpoint

The complete document grammar remains in scope. This checkpoint addresses an
editing defect discovered while visually verifying document metadata; it is
not approval of the complete grammar or its interaction matrix.

## Reproduction and cause

`cargo test -p document-core --test source_fidelity --locked` reproduced the
failure before implementation, first with a rich labelled list and then with
the minimal `- a\n- b:\n` source. Inserting `x` in the first item serialized the
untouched second item as `- b\:`. The minimized test failed deterministically
in under a second including the incremental build.

Dirty tracking did not mark the sibling text as changed. The source spine
recorded only root spans, and any dirty descendant caused the complete list
to be canonically serialized. Removing colon escaping would not preserve
authored marker dialects, whitespace, links, entities or line endings.

## Implementation

- Import records original list-item spans and retains shared original root
  trees in the source spine. Synthetic Markdown converted from HTML has a
  separate coordinate space and cannot contribute original-file spans.
- Saving compares stable identities, structural properties and shared child
  allocations. It patches the smallest changed list items and copies the
  untouched spans and separators from the original source. Nested lists,
  quotations, alerts and definition contexts can retain unchanged siblings.
- Marker dialects (`+`, `*`, `-`, `.`/`)`), authored numbering, task marker case,
  indentation and newline style are retained where source reuse is valid.
  A plain item in a mixed task list does not acquire an invented checkbox.
- Root bounds, item order, list kind/tightness and container metadata guard
  reuse. Unsupported or structurally changed regions serialize the edited
  tree; original source must never hide a real edit.
- An adversarial structural test also found double-applied indentation in
  canonical nested-list serialization. An indented item could reopen as code.
  Continuation indentation now has one owner, and tight nested lists do not
  gain a blank separator that changes their grouping.
- Source preparation and saving own this work. Projection, layout selection,
  drawing and scrolling paths are unchanged.

## Core verification

Six integration tests cover the minimal regression, rich/reference syntax,
all bullet dialects, parenthesized and zero-prefixed ordered markers, tabs,
CRLF, no final newline, nested and quoted lists, alerts, definitions, mixed
and ordered tasks, checkbox toggling, sequential edits, authored hard breaks,
structural edits, semantic hierarchy on reopen, undo and redo.

`source-fidelity-tests.log` records 514 passing workspace tests and two ignored
tests. `source-fidelity-clippy.log` records warning-free workspace Clippy.
The document-core doctest target completes with zero doctests.

## Native visual and editing evidence

Release binary SHA-256:
`305b56af2031b60a8dbb3a3bd7b6eba2f2fd03b57107c41d33135158034b6bbf`.
Specimen `layout-fixtures/64-document-metadata.md` retains SHA-256
`66a1ce991c52340e9768c9c8d8d024aeb71e7f09c979abcfb80e53d8bbc4bbff`.

- `layout-previews/source-fidelity-edit.edit.json`: native Status typing,
  complete expected autosave, byte-exact Version and Owner sibling fragments,
  and exact undo pass. The changed Status label still uses canonical escaping;
  the untouched sibling labels no longer do.
- `source-fidelity-growth.edit.json`: a 213-byte native paste into Owner keeps
  Status and Version byte-exact, saves the complete expected document and
  restores every byte on undo. Inspected `-idle.png` retains the focused strip
  column while it grows. Inspected `-blurred.png` reflows into aligned rows,
  keeping the entire value visible and preserving authored order.
- Wide, 600 px narrow and 200% captures were inspected. Current native
  metadata spacing matches the written Board-01 role tokens. The 117-node
  periodic-session AT-SPI tree has 20 px text rows, 24 px strip gutters,
  12 px between long property rows and 64 px section gaps.
- `source-fidelity.pixels.json` passes the existing independent rule/spacing
  oracle: 16 px strip insets, exactly 1 logical px rules at 100/200%, and
  consistent 548/548.5 logical px strip widths after raster rounding.
  The checker now accepts explicit capture prefixes and verifies that both
  scales and accessible evidence come from the same binary. Its original
  metadata-build evidence also still passes unchanged.
- Weston MCP launched this build as app 24, captured the UI, clicked into the
  document and read its accessibility tree. It still exposed provisional
  source-stack geometry; `source-fidelity.weston-provisional.json` is retained
  as a counterexample, not final-layout approval. Final geometry comes from
  the periodic native compositor. The MCP app was stopped and its private
  specimen remained byte-identical. No user document was edited.

The native-desktop design skill required checking visible placement and source
integrity together. That prevented treating either a correct save or a stale
MCP snapshot alone as proof of a correct editable layout.

Crusty validation against context `ctx_646b41dce2f4` reports 36 existing
architecture findings, with none new or worsened. No unrelated debt was changed.

## Current-build scrolling check

```sh
python3 performance/capture-layout.py --generated-bytes 10485760 \
  --width 1600 --height 1200 --perf-seconds 60 --perf-input continuous \
  --source-unchanged-check \
  --output performance/layout-previews/source-fidelity-10mib-perf.json \
  --log-output performance/layout-previews/source-fidelity-10mib-perf.log
```

The 120 Hz isolated native run averages **107.37 presented fps**. Draw p99 is
**9.21 ms**, presentation interval p99 **12.37 ms**. Four intervals are at least
25 ms, with a 56.43 ms maximum and 1.06% missed-deadline opportunities. The
existing benchmark gate passes and the generated 10 MiB source stays byte-exact.
This does not prove that every frame meets 16.7 ms, or cover the complete
grammar-heavy, accessibility, wheel-latency and memory/startup matrix.

The prior metadata checkpoint used the same workload and averaged 108.17 fps.
These individual runs do not establish a performance improvement or a causal
regression. No build or other capture from this task ran concurrently with
the current measurement; unrelated host processes were left alone.

## Remaining fidelity scope

This is not byte-preservation proof for arbitrary structural insertion,
deletion, reordering, changed container kinds, edits in multiple unrelated
roles of one root, converted HTML, or every footnote context. Those operations
may still canonically regenerate a larger region. An edited item's own inline
punctuation can still be escaped; untouched siblings in the covered cases
remain byte-exact. Ordered-task structure changes also need further coverage.

A separate probe found that inserting a bare newline into an inline text node
can reopen as a soft space. Authored hard breaks use the existing two-space
newline representation and are covered here. That broader newline/model
contract remains open; its oracle was not claimed green by these tests.

All remaining visual families, page masters, genuine prose flow, exports,
accessibility states and performance matrices in DESIGN-GRAMMAR-COVERAGE.md
remain part of the original objective.
