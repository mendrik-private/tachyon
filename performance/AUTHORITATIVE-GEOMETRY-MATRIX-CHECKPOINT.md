# Authoritative geometry matrix checkpoint

Date: 2026-09-11

This checkpoint closes A07's native fractional-display-scale geometry and
editing matrix. The human explicitly bypassed the physical presentation run
because input injection prevents concurrent desktop work.

## Defect found by the matrix

The first 480 px / 125% probe found an AT-SPI lifecycle mismatch. The editor
could enter the adapter's initial object-server snapshot before measured
semantic descendants were ready. Its later descendants made
`Accessible.GetInterfaces` report `Text`, although the object server had never
registered that interface. Making all text inputs advertise `Text` immediately
was invalid: `accesskit_consumer::Node::document_range()` panicked when the
input had no `TextRun`.

The editor now publishes one valid empty `TextRun` until authoritative measured
semantics are available. The retained canonical document replaces it after the
first committed layout. The adapter continues to expose `Text` only when a
text-range descendant exists. The formerly failing probe now reads the editor's
caret, 467-character range, and zero selection count without an adapter error or
application panic.

## Native matrix

`performance/a07_geometry_matrix.py` ran all 28 combinations:

- widths: 480, 640, 799, 800, 999, 1000, and 1440 logical px;
- compositor scales: 100%, 125%, 150%, and 200%;
- viewport height: 800 logical px;
- fixture: `41-find-document.md`;
- release binary SHA-256:
  `9fa0500d37d3df43293858b35dbf27e853d24e1a32635375e4b4077f8160eb6c`;
- fixture SHA-256:
  `3bf716a15c2d1898475ec9e96c61ace8d3a605d1ac726a94f387e70928fb5530`.

Every case uses the harness-owned Weston fractional-scale protocol module and
private AT-SPI bus. The oracle requires:

- the native 28 px Close control to scale to the expected physical size;
- editor bounds to remain inside the physical output;
- the committed logical canvas and viewport to match native physical editor
  bounds within one physical pixel;
- no planner failure or unresolved image dimensions;
- every document semantic component to remain inside the document horizontally;
- every committed scroll-anchor displacement to stay within one physical pixel;
- the editor's AT-SPI Text interface and basic range state to be readable;
- Files and Outline to be absent by default below 800 px and present by default
  at 800 px and above;
- Ctrl+Alt+N to reveal or hide both navigation panes, then restore the default
  state;
- Outline item names and order to equal the document's semantic headings;
- exact source bytes to remain unchanged.

All 28 cases pass. Observed maximum anchor displacement is 0 physical px. The
799/800 boundary changes exactly from an overlay-hidden editor to the persistent
224 px navigation pane. Fractional native widths agree with logical publication,
including 614.4 logical px -> 768 physical px at 640 px / 125% and 766.6667 ->
1150 at 799 px / 150%.

The aggregate report is
`layout-previews/a07-native-geometry-matrix.json`. Representative screenshots
cover 480/125%, 799/150%, 800/125%, and 1440/200%. All four were inspected; they
show complete title-bar controls, unclipped document content, the correct
responsive navigation state, and matching Outline headings.

## Isolated native editing matrix

The same 28 cells were repeated with a deterministic native edit inside the
private Weston session. Each case derives the pointer position from the fresh
AT-SPI bounds of the unique opening prose paragraph, converts physical bounds
back to the compositor's logical input coordinates, inserts one `x`, waits for
autosave, and requires the resulting text to equal exactly that one semantic
insertion. The serializer adds one lossless escape before the paragraph's final
period. No other semantic source change is accepted. One native Undo must then
restore every original byte before geometry and Outline are probed again.

All 28 edit cases pass. Every final source hash equals the original fixture,
every post-Undo geometry/Outline state passes, and the observed maximum anchor
displacement remains 0 physical px. The aggregate report is
`layout-previews/a07-native-edit-geometry-matrix.json` (SHA-256
`ef93bdb81dc49157e786baeaa9f9b832423956476788083c10d7faa1541c5b05`).
It records layout-validation release binary SHA-256
`40a65ac2cb3049f0a394187c57ab85ffa9cb1804b086d292351a8b85de86b503`.
The four retained post-Undo screenshots cover 480/125%, 799/150%, 800/125%,
and 1440/200%; all were inspected and remain unclipped with the expected
Files/Outline state.

## Reproduction and tests

```sh
cargo test --manifest-path vendor/accesskit_atspi_common/Cargo.toml \
  pending_text_input_with_empty_run_has_a_safe_native_text_range
python3 -m unittest performance/test_a07_geometry_check.py \
  performance/test_capture_layout.py
cargo build --release -p markdown-app --bin tachyon
python3 performance/a07_geometry_matrix.py \
  --binary target/release/tachyon \
  --output performance/layout-previews/a07-native-edit-geometry-matrix.json
```

The pure Python oracles reject independently wrong compositor scale, horizontal
clipping, Outline drift, excess anchor displacement, edits outside the target,
additional semantic source changes, and an incomplete capture contract. Saved
table widths remain authoritative, and automatic reading-edge alignment remains
limited to automatically sized tables. Files and Outline are retained. The
current product plan excludes the minimap.

Final validation passes: `scripts/check.sh` (129 core units, 54 source-fidelity,
18 structural/tree-selection, 596 document-view with two documented native-font
ignores, 44 app, 12 vendored AT-SPI, and eight retained-publication tests, plus
locked checks, strict workspace Clippy and doctests); strict standalone adapter
Clippy; 29 Python capture/oracle tests; formatting; and `git diff --check`.
Crusty validation `ctx_a8af8be39cc4` / `task_17b4436da94b522e`
reports 75 existing advisory findings, zero new, worsened, or resolved findings,
and no blocking constraint. The editing-matrix follow-up adds two pure oracle
tests; the combined geometry/capture suite now passes 31 tests, and the full
`scripts/check.sh` result remains unchanged at 596 document-view tests with two
documented native-font ignores.

## Physical qualification bypass

The requested five-run physical GNOME/Mutter qualification was stopped on the
human's instruction because its input injection prevented normal desktop work.
Five 100 KiB samples and one 1 MiB sample had completed when it was interrupted;
the next 1 MiB sample and the remaining matrix never completed. No aggregate
qualification report was produced, and these partial samples are not a pass.
Their draw p99 values were 3.56–4.08 ms, input p95 values were 8.77–9.69 ms,
and presentation misses were 0.67–1.03%, still above the 0.1% gate.

The physical gate is intentionally bypassed for queue progression. It must not
be run again or inject input into the physical session without a later explicit
human request. A07's acceptance evidence instead rests on the complete logical
and isolated native matrices, focused physical diagnostics already recorded in
`LARGE-DOCUMENT-EDIT-CHECKPOINT.md`, exact source restoration, and the full
repository validation.
