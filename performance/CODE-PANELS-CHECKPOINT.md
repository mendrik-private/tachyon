# Code panes: equal footer insets and selection-safe Copy

This closes the two code-pane defects recorded in CODE-GUTTERS-CHECKPOINT.md.
It does **not** approve Board 04 or the full document design grammar.

Release SHA-256: `e808101b9307bea83ff4701cb23b986b772bac3139e8ebaf275fb4a7e8948586`.
Fixture 61 SHA-256: `cd8661990f047778721b9b9fbbd6d3f4a102879fdd338b096405c0bdd3d169eb`.

## Changes and source contract

- Pointer-down on Copy is a document command, not a document text hit. It no
  longer retargets the caret or destroys a selection before copying.
- The terminal source newline remains in the canonical document and text
  projection. Its empty caret-only row has no idle footprint. Other authored
  blank lines remain visible and numbered.
- The final editing row opens when the active selection endpoint reaches it.
  Right, Down, Shift+Down and Up can visit it. Typing, newline deletion and undo
  keep exact source/selection semantics. Leaving the endpoint closes the row.
- Segment, grouped-measurement and published-geometry caches include this
  view-local state. Background preparation uses the requesting view's state,
  not the selection of another view sharing the document.
- The last visible source row owns the footer padding. Code inside rich table
  cells uses the same measurement and retains its header and cell insets.

The Rust and bug-diagnosis skills guided the failing-then-passing regression
loop and source/cache invariants. The native-desktop design skill guided the
pointer-command behavior and measured screen/zoom validation.

## Native visual and interaction evidence

- [Wide](layout-previews/code-panels-wide.png): dark 12-line and light 10-line
  panes have natural heights, a 24 px card gap/insets, 16 px source footer
  insets, and a compact single-line command strip. Each idle pane loses the
  incorrect extra 21 px row from the preceding release.
- [Narrow](layout-previews/code-panels-narrow.png): 600 × 1100 stacking retains
  the 24 px card inset and 16 px code footer.
- [200%](layout-previews/code-panels-200.png): the light pane's ten 42 px rows
  end at y=1059; its bottom is y=1091, giving a 32 px footer. Card insets are
  48 px. This capture checks footer/zoom geometry, not horizontal overflow.
- [Raster observations](layout-previews/code-panels.pixels.json): sampled
  number rails stop at the final source-line box; footer space matches the
  header's source inset. Fine rules and palette roles remain coherent.
- [Native endpoint edit](layout-previews/code-panels-terminal-exact.edit.json):
  click the last TypeScript line, End, Down, type x; complete autosaved Markdown
  equals the independently specified expected source, and undo restores every
  original byte. [Typed frame](layout-previews/code-panels-terminal-exact-typed.png)
  shows the new thirteenth source row without shifting the peer card.
- [Whole-document clipboard](layout-previews/code-panels-copy.copy.json): exact
  independent plain-text golden, all source blocks/blank lines in order, no
  decorative line numbers. Two [warm replans](layout-previews/code-panels-copy.planning.json)
  pass the cache oracle with zero anchor displacement.
- [Weston MCP observations](layout-previews/code-panels.weston.json): real
  pointer Copy preserves a title-end caret and a full-title drag selection.
  The title highlight/selection toolbar remain while Copied and its checkmark
  appear. Clipboard is the exact TypeScript payload. End/Down visibly opens
  the terminal row while the peer pane stays unchanged. Bounded three-frame
  series were inspected after interactions; provisional zero-refresh startup
  frames were not used for layout approval.

All completed capture source hashes match the original fixture. The initial
`code-panels-terminal-edit` attempt used an unsuitable single-character marker
oracle: serializing an edited unterminated final line correctly adds a newline
before its closing fence. It was replaced with a stricter complete-source
expectation in `code-panels-terminal-exact`, not a relaxed source gate.

## Automated checks

`cargo test --workspace --all-targets --locked`: **483 passed, 2 ignored**.
Five new native-renderer tests cover Copy selection, footer geometry, terminal
editing and exact undo at 100/200% in standalone/card panes, vertical selection
navigation, and cached/background per-view state. The Copy and footer tests
failed before their fixes; the Down test caught and prevented a skipped-row
regression during implementation.

Workspace Clippy with `-D warnings`, formatting, diff whitespace, workspace
doctests (no doctests defined), and the optimized layout-validation build pass.
Crusty validation of prepared context `ctx_54cc0304e16a` completed as
`task_212c6c1bd3518310`: 36 existing advisory findings, none new or worsened.

## Performance and remaining work

Two separate 60-second runs at 1728 × 1080, configured 120 Hz, pass the existing
continuous-input gates without changing thresholds:

| Workload | Average FPS | Draw p99 | Presentation p99 | Input p99 |
| --- | ---: | ---: | ---: | ---: |
| [10 MiB mixed document](layout-previews/code-panels-10mib-perf.json) | 109.60 | 4.407 ms | 10.912 ms | 10.338 ms |
| [Numbered-code specimen, isolated repeat](layout-previews/code-panels-specimen-isolated.json) | 108.79 | 4.665 ms | 12.927 ms | 11.706 ms |

Both have zero application stalls ≥25 ms. The mixed run has two presentation
intervals ≥25 ms (max 31.392 ms); the specimen has none (max 21.021 ms).
These are average/p99 passes, not an every-frame >60 FPS or 120 Hz guarantee.
The mixed generator has one-line code; the small numbered specimen is not a
10 MiB numbered-code-heavy test. Accessibility was inactive during these timing
runs and was checked separately through Weston MCP.

The [first specimen run](layout-previews/code-panels-specimen-perf.json) is
retained: it passed average/p99 gates but had five application/presentation
stalls, including a 75.039 ms draw. A 4.28-second documentation-test compilation
overlapped that run. The repeat above had no concurrent checks; this does not
prove that every original outlier was caused by compilation. No result is
deleted or reclassified as a clean run.

The complete board/state/RTL/extreme-growth,
pagination/continuation, and full-grammar performance matrices remain open.
Definition-list syntax, deep tree fallback, metadata/metrics, table records,
true wrapping, media, page masters and other rows in DESIGN-GRAMMAR-COVERAGE.md
remain part of the unchanged full objective.
