# Metadata grammar — implementation and native evidence

The complete document grammar remains the objective. This checkpoint advances
C08 and its spacing, typography, editing and accessibility consumers. It does
not approve Board 01, the complete metadata family, or the remaining grammar.

## Implementation

- A labelled unordered list in the opening may represent document properties
  when all labels are grounded property names (Status, Version, Owner, etc.).
  Explicit Metadata / Document metadata / Properties / Document properties
  sections allow domain-specific labels. Ordinary feature lists, tasks,
  numbered instructions and nested items are not reclassified as metadata.
- Native font metrics choose a single compact strip for 2–6 short properties.
  Widths are allocated on the shared twelve-track grid. Both label and value
  must actually fit one line; no character-count estimate approves a strip.
- Longer or more numerous values use aligned key–value rows. Narrow surfaces
  stack each label above its value. Hard breaks stay hard breaks. No value,
  completion state, icon, trend or summary is invented.
- Metadata has shared 14/20 sans-serif type, 8 px inline label/value gaps,
  24 px strip gutters/aligned-row gaps, 12 px between property rows and 16 px
  above/below strip text. Rules are one logical pixel; open rows have no box.
  Native review caught inherited serif feature labels; the shared font-run
  path now uses sans metadata labels while preserving authored mono literals.
- Paragraph IDs, rich runs, selection ranges and source order stay canonical.
  Focus freezes property widths; growing text stays visible in that location.
  Moving the caret outside releases the arrangement and remeasures the rows.
  Projection context and relevant measurement/geometry cache keys include the
  metadata role; scrolling consumes published geometry rather than classifying
  source or measuring candidate strips.

## Authoritative build and specimen

- Binary SHA-256:
  `77b5bd4c6704dc1eee3c1f01e4a4f028990e331438d50a0db5fbe72a624ed759`
- Fixture: `layout-fixtures/64-document-metadata.md`
- Fixture SHA-256:
  `66a1ce991c52340e9768c9c8d8d024aeb71e7f09c979abcfb80e53d8bbc4bbff`
- Reference: written grammar §§1–5 and Board 01 document-metadata specimen.
  The text explicitly governs measurements; board raster lettering is not an
  executable specification.

## Native visual and interaction checks

The final `layout-previews/metadata-verified-*` captures were inspected at
1600×1200, 600×1100, 200% at 1600×1200, and 200% at the native 480 px minimum
width. `metadata-verified-stacked-rows.png` scrolls far enough to expose all
three narrow label/value groups. Wide and 200% captures show compact strips;
narrow captures show readable rows/stacks, without shrinking text to fit.

`python3 performance/metadata_layout_check.py` checks actual final PNGs and
the private-session native AT-SPI tree:

- 1× strip rules at y=232 and 283 occupy exactly one physical row each;
  at 200%, y=410–411 and 512–513 occupy two rows each.
- The text line is 20 logical pixels, with 16 px top and bottom insets.
  The strip's logical width is 548/548.5 px after raster rounding.
- Native property bounds are x=276/466/609 at y=248, height=20, with
  24 px gutters. The next H2 starts 64 px after the strip's lower edge.
- Long property rows have 40 px heights and 12 px inter-row gaps. The next
  section is separated by 64 px. Labels/values remain in source order.

The same app was launched and inspected through Weston MCP. Its zero-refresh
output exposed provisional source-stack geometry; that artifact is retained
as `metadata.weston-provisional.json`, explicitly **not** final layout proof.
Final accessibility bounds come from the periodic native compositor and its
private AT-SPI bus (`metadata-verified-accessible.active-atspi.json`, 117
nodes), not an assumption that the MCP snapshot had caught up.

Native copy (`metadata-verified-wide.copy.json`) retains the nine property
markers in source order. `metadata-edit-verified.edit.json` verifies typing
at the Status value, complete expected autosave and exact byte restoration
on undo. `metadata-growth-verified.edit.json` verifies a 213-byte paste into
Owner and exact undo. Its inspected `-idle.png` keeps the growing value in
its original strip column; `-blurred.png` reflows it into aligned rows with
the full text visible. Reading-only captures retain the complete source hash.

**Serializer limit:** an edited list is currently serialized as a whole.
Unchanged labels inside that list can acquire escaped colons, and pasted
periods are escaped by canonical serialization. The exact expected-source
oracles include this existing behavior. Other roots remain byte-identical,
the rendered text is unchanged, and undo restores every original byte. This
does not prove byte preservation for untouched items inside the edited list;
that stricter fidelity requirement remains open.

Follow-up: SOURCE-FIDELITY-CHECKPOINT.md supersedes this limit for the covered
non-structural list edits on build `305b56af`. Native Status editing and Owner
growth now retain untouched sibling source bytes, with the same measured
layout and exact undo. Arbitrary structural/source-region fidelity remains open.

## Automated evidence

- `cargo test --workspace --all-targets --locked`: 508 passed, 2 ignored.
  `layout-previews/metadata-tests.log` records the complete result.
- `cargo clippy --workspace --all-targets --locked -- -D warnings`: pass.
  `layout-previews/metadata-clippy.log` records the result.
- `cargo fmt --all -- --check` and `git diff --check`: pass.
- Native tests cover measured strips/rows/stacks, real font-run family,
  all source ranges, zoomed rules, focused growth/release, hard breaks,
  8-property fallback, exact undo and semantic exclusion counterexamples.
- Two warm requested replans pass `--cached-geometry-check` in
  `metadata-verified-wide.planning.json`; no repeated text shaping/wrapping
  or segment layout is needed for those cache-resident geometry publications.
- Harness tests: 8 passed, including the positive no-input definition oracle.
- Current-build no-input regression: fixture 63 paints both definition rails
  and its numbered composition at 120 Hz, with zero document-pixel changes
  after pointer movement (`metadata-regression-startup.idle.json`).
- Crusty validation reports no new/worsened architecture findings against
  the prepared 36-finding baseline. No unrelated debt was rewritten.

The desktop-design skill required checking native typography, resizing,
selection, undo and accessible geometry together. That review directly caused
the metadata serif-to-sans correction; static layout tests alone missed it.

## Scrolling check

The final release build was measured with:

```sh
python3 performance/capture-layout.py --generated-bytes 10485760 \
  --width 1600 --height 1200 --perf-seconds 60 --perf-input continuous \
  --source-unchanged-check \
  --output performance/layout-previews/metadata-10mib-perf.json \
  --log-output performance/layout-previews/metadata-10mib-perf.log
```

This 60.02-second, 120 Hz isolated native run averaged **108.17 presented fps**.
Draw p99 was **8.76 ms**; presentation interval p99 was **12.35 ms**. There were
five presentation intervals of at least 25 ms, with a 65.77 ms maximum. The
existing benchmark gate passes, but this is not a promise that every frame
meets 16.7 ms, nor a complete grammar-heavy/accessible/wheel-latency matrix.
The generated 10 MiB source hash is unchanged. No other capture or build was
run concurrently with this measurement. Unrelated host workloads were not
stopped or modified.

## Remaining scope

Complex/nested metadata, definition/table encodings as opening strips, richer
contextual badges, RTL treatment and the complete editing/state matrix remain
open. So do metrics, records, deep trees, true wrapping/prose columns,
footnotes/bibliography, media, page masters/continuations, exports and all
remaining rows in DESIGN-GRAMMAR-COVERAGE.md. This is concrete progress toward
that full scope, not a new definition of completion.
