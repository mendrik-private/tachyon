# Large-document edit checkpoint — September 11, 2026

## Result

The physical Mutter release run no longer rebuilds the whole measured layout
for ordinary image-alt typing, and a no-wrap edit no longer rebuilds the
complete component geometry index. On the 1 MiB qualification source this
removes the 180–195 ms edit-specific tail: a final 10-second scroll/edit run
measured 3.61 ms draw p99, 10.63 ms input p95, 12.29 ms input max, and no
application draw stalls at or above 25 ms.

The follow-up local-range representation removes the ordinary-edit visual-line
suffix rewrite. Three matched 10 MiB runs now measure 12.39/12.36/13.16 ms
input p95 and 2.59/2.45/2.20 ms draw p99, with no application stall. This
checkpoint's bounded-coordinate follow-up also removes the remaining linear
projection byte/UTF-16 suffix update. Its clean matched series measures
8.27–8.86 ms input p95 and 2.31–2.39 ms draw p99 with no application stall.
It still does not qualify A07: presentation misses remain 1.75–2.24% and the
complete release matrix remains open.

## Production changes

- `document-core` classifies image-alt text commands as one localized text-node
  change. Whole image attribute replacement remains structural.
- Local measured-layout refresh compares only fields consumed by component
  geometry. If line count, vertical position, fractions, inset, line height,
  slot, and table-row geometry are identical, it retains the existing
  component index. Geometry-changing edits still rebuild it.
- Component-index construction advances a monotonic projection-segment cursor
  for source-ordered visual lines and falls back to the existing binary lookup
  when paint order moves backward.
- Projection segments box their immutable context and keep their coordinate
  record at 64 bytes.
- Visual lines split into a 64-byte hot coordinate record and a shared,
  immutable cold payload. Ordinary prose shares the empty payload; sparse
  table, slot, math, HTML, diagram, and record data uses copy-on-write only when
  it changes. Inline-math wrapper ranges are stored node-locally so an ordinary
  suffix edit does not copy that payload solely to shift an absolute range.
- Zoom and vertical suffix rebasing consult hot flags before touching cold flow
  geometry. This keeps the current linear suffix walk cache-local without
  changing its exact geometry or source-coordinate semantics.
- Projection segments share byte-bounded, 256-segment coordinate chunks. An
  edit rebases only the remainder of its chunk and records one byte/UTF-16
  suffix delta in a sparse balanced index for later chunks. Versioned chunk
  caches resolve repeated reads without walking the suffix. Visual lines store
  chunk-local byte ranges, cached geometry is normalized to its segment before
  reuse, and cloned projections rebuild independent coordinate state.
- Editable-segment lookup is source ordered and binary. The accessibility
  bounds pass advances one monotonic segment cursor for source-ordered lines;
  it no longer performs one binary segment lookup per line after an edit.
- The benchmark-facing prepared view now applies the same bounded
  `ComponentRefresh` result as the live editor. A no-wrap update retains the
  exact component index instead of rebuilding it after the localized geometry
  path already proved it unchanged.
- Local refresh applies its exact height delta directly. If a simple top-level
  component loses wrapped lines while its suffix moves upward, the compact
  component bounds, heading positions, line indexes, and paint-bottom prefix
  array are rebased in place. Other geometry changes retain the full rebuild.
  Identity paint order is regenerated directly after a local line-count change.
- Semantic accessibility revisions share one native retained-tree owner.
  Property-only revisions preserve native identity and publish sparse updates;
  topology changes replace the retained subtree.
- The Unix accessibility adapter computes incremental text notifications
  directly from Mineral's marked canonical text run, preserving exact Unicode
  offsets without concatenating the semantic document tree.

No diagnostic timing environment variables or debug prints remain in the
production source.

## Physical-session evidence

The runs used the active GNOME/Mutter session at 2880×1800, 120 Hz, scale
1.6666666, a 1100×720 logical app viewport, and the temporary `/dev/uinput`
device already used by the release qualification harness. Each timed run had a
2.5-second warmup. Scroll/edit injected ordinary scrolling plus two single-key
edits. These bounded diagnostics do not replace the required five 60-second
runs per size or the complete source/editing oracle matrix.

| Source / interaction | Duration | Draw p99 | Input p95 | Input max | Present max | Missed | App stalls |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 MiB scroll/edit, before localization fix | 5 s | 3.52 ms | 39.32 ms | 194.51 ms | 195.43 ms | 6.07% | 0 |
| 1 MiB scroll/edit, clean compact-record source | 10 s | 3.61 ms | 10.63 ms | 12.29 ms | 59.18 ms | 1.66% | 0 |
| 1 MiB scroll-only control | 10 s | 3.24 ms | 9.99 ms | 16.29 ms | 64.91 ms | 1.24% | 0 |
| 1 MiB continuous frames, no input | 10 s | 2.39 ms | — | — | 55.38 ms | 0.75% | 0 |
| 10 MiB scroll/edit, before compact rebase | 5 s | 3.68 ms | 274.46 ms | 274.46 ms | 274.46 ms | 7.08% | 1 |
| 10 MiB scroll/edit, before compact visual-line record | 5 s | 3.56 ms | 36.34 ms | 96.73 ms | 100.66 ms | 4.92% | 1 |
| 10 MiB compact-record repetition 1 | 5 s | 4.02 ms | 31.78 ms | 43.22 ms | 58.62 ms | 3.61% | 0 |
| 10 MiB compact-record repetition 2 | 5 s | 3.68 ms | 16.49 ms | 89.46 ms | 94.31 ms | 2.31% | 0 |
| 10 MiB compact-record repetition 3 | 5 s | 3.64 ms | 14.52 ms | 66.75 ms | 66.81 ms | 2.79% | 0 |
| 10 MiB scroll/edit, final compact-record source | 10 s | 3.37 ms | 32.70 ms | 49.02 ms | 99.09 ms | 3.30% | 0 |
| 10 MiB local ranges, repetition 1 | 10 s | 2.59 ms | 12.39 ms | 42.60 ms | 129.89 ms | 3.61% | 0 |
| 10 MiB local ranges, repetition 2 | 10 s | 2.45 ms | 12.36 ms | 42.83 ms | 120.06 ms | 2.57% | 0 |
| 10 MiB local ranges, repetition 3 | 10 s | 2.20 ms | 13.16 ms | 43.68 ms | 119.93 ms | 2.82% | 0 |
| 10 MiB bounded coordinates, repetition 1 | 10 s | 2.36 ms | 8.27 ms | 17.65 ms | 76.22 ms | 1.83% | 0 |
| 10 MiB bounded coordinates, repetition 2 | 10 s | 2.39 ms | 8.75 ms | 18.42 ms | 76.09 ms | 2.24% | 0 |
| 10 MiB bounded coordinates, repetition 3 | 10 s | 2.31 ms | 8.86 ms | 15.23 ms | 74.71 ms | 1.75% | 0 |

The scroll-only and no-input controls show that presentation misses are not
specific to edit processing. GPUI records the
interval between Wayland compositor frame callbacks around submitted frames;
the 0.1% gate remains authoritative, so the compositor-independent diagnosis
does not turn these failures into passes.

Fixture SHA-256 values:

- 1 MiB: `e658053ad13e1e2424888818b2bd1685cf821a50ad05e6202df48a1bc91c37da`
- 10 MiB: `16834e22b862cffa330391dc2564890634d9e719a8d368446c6ff1161ba437d9`

The rebuilt clean release binary used for the final compact-record rows was
`0d51ea2d34f7ddb966d9915eb8ef7e98310b89c8741ac05a30089d4b0629658d`.
The local-range follow-up binary was
`006d8422d309a409249153d5f683dc43b093c10a9741c4f412a2ded6dc119b54`.
The bounded-coordinate and monotonic-accessibility follow-up binary was
`8ff6358a6d2fa4eeceaf60b3e1209762ae9f76b35c4c2ca14eac425487fdce8a`.
The before and physical controls used the otherwise equivalent instrumented
candidate `0b288fa454cf13ce0ecae3377ae5a1d49bd94ab74155b916369e273d0f90587d`;
its timing logs were disabled during those runs.

Retained reports are `layout-previews/large-edit-before-1m.json`,
`large-edit-final-1m.json`, `large-edit-before-compact-rebase-10m.json`,
`large-edit-final-10m.json`, `large-edit-compact-record-10m-01.json`,
`large-edit-compact-record-10m-02.json`,
`large-edit-compact-record-10m-03.json`,
`large-edit-local-ranges-physical-1x.json`,
`large-edit-local-ranges-physical-3x.json`,
`large-edit-chunk-coordinates-core-10m.json`,
`large-edit-chunk-coordinates-physical-3x.json`,
`large-edit-scroll-control-1m.json`, and `large-edit-no-input-control-1m.json`.

## Regression boundary

`image_alt_typing_is_localized_but_attribute_replacement_is_structural`
checks the core transaction classification.
`no_wrap_image_alt_typing_reuses_exact_component_geometry` edits an image alt
ahead of 300 paragraphs, requires exact component-index identity, compares its
geometry and visible ranges with an independent rebuild, preserves exact
serialized source, and checks Undo. The retained semantic-tree and adapter
tests cover property revisions, topology fallback, direct 1 MiB Unicode edits,
and later full publication from the revised baseline.
`retained_prose_ranges_follow_projection_without_suffix_line_rewrite` checks
that a middle edit updates a retained tail line through the shared projection
start, keeps cold payload identity, preserves the 64-byte hot record, and only
copies sparse vertical geometry when its y position changes. Projection tests
also verify that a live segment handle follows refresh while a cloned
projection remains independent.
`chunk_boundary_edits_rebase_local_and_indexed_coordinates_exactly` checks both
sides of a coordinate-chunk boundary. The large suffix regression performs six
distinct updates, bounds balanced-index visits, and compares every byte and
UTF-16 range with a complete rebuild. Geometry-cache coverage moves an
unchanged segment within its chunk and requires a real cache hit at the new
absolute range. The saved-width table history regression continues to protect
explicit column widths; reading-edge alignment remains limited to automatic
tables.

`authoritative_geometry_and_outline_agree_across_required_width_scale_matrix`
now exercises 480/640/799/800/999/1000/1440 logical-pixel canvases at
100/125/150/200% text scale. All 28 cases assert that the prepared view and its
published geometry share the same line, paint-order, component, and height
records; ordinary measured text fits its realized slot; caret X-to-byte
round trips hold; outline headings agree with the component index; and a
semantic scroll anchor survives every transition within one pixel. The current
product plan explicitly excludes the retained legacy minimap from the
application, so this matrix qualifies Files/Outline rather than restoring or
claiming the old minimap surface. It does not replace the full release
qualification. The native fractional-display-scale portion is now covered by
`AUTHORITATIVE-GEOMETRY-MATRIX-CHECKPOINT.md`.

## Remaining work

The compact component rebuild, full document-height scan, large inline
visual-line payloads, visual-line suffix rewrite, and projection-coordinate
suffix rewrite are removed from the ordinary 10 MiB edit tail. All three clean
bounded-coordinate runs pass the draw-p99 and input-p95 limits with no
application stall. The historical September 6 core open/prepare report is no
longer an attributable comparison for the much larger current renderer; a
temporary direct-coordinate control on the same source measured in the same
roughly 0.39 s projection and 1.7 s prepared-view band as this implementation.
The retained core report records the current result without claiming the old
one-second open gate. The complete native width/display-scale geometry,
source-preservation, and native edit/autosave/exact-Undo matrix now passes in
the private Weston harness; see
`AUTHORITATIVE-GEOMETRY-MATRIX-CHECKPOINT.md`. On 2026-09-11 the human
explicitly stopped and bypassed the remaining physical qualification because
its input injection prevented normal desktop work. The interrupted run produced
only five 100 KiB samples and one 1 MiB sample, no aggregate report, and no
qualification claim. Physical input injection must not run again without a
later explicit human request. The queue therefore advances using the complete
isolated matrix and repository validation while retaining the partial physical
reports only as non-qualifying diagnostics.
