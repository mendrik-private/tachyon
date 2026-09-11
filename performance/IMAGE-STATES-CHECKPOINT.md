# Image loading and failure presentation

September 10, 2026. E01/N06 implementation contract before code changes.

Use the pinned toolkit's actual image loading/failure callbacks, not absence of
dimensions as an error signal. A loading image says “Loading image…” after the
toolkit delay; an actual failed image says “Image unavailable.” Both use the
existing document palette, source-owned alternative description and a compact
destination label. Loaded images retain their ordinary uncropped presentation.

Preserve reserved geometry across loading/failure, captions, source and selection.
Long visible metadata may use bounded preview lines; the canonical image retains
its complete alternative description and destination for accessibility/editing.
Do not expose URL credentials/query tokens in destination previews or insert UI
status text into Markdown/copy. Loading is not failure, and a known image size
does not prove successful decoding.

Use the existing RetryImage command and bounded application cache for a real,
named keyboard/pointer/accessible retry action on failure. Retry must target the
same current canonical image/source, preserve the document and not change the
selection. Do not add a competing loader, state cache, polling or automatic
unbounded retry. Add stable image element identity so the toolkit can track its
loading delay. Validate missing local and existing-but-invalid files, ready
images, controlled loading/retry transitions, narrow/zoom/themed visuals,
accessibility and source/edit/Undo.

## Implementation and evidence

The native baseline (`layout-previews/image-states-before.png`, runtime
`7cfa3f7bab52f84fdf2081bd8a4911c07a65705b21ba803ce62c67e3b8550cd5`)
showed two blank 180 px image reservations. The minimized fixture 103 contains
a missing local file, an existing but undecodable Markdown file and a valid SVG.
The native check `jq -e 'any(.nodes[]; .name == "Image unavailable")'` against
the baseline active-AT-SPI tree returned false/exit 1.

The loader already returned actual errors. The editor supplied neither toolkit
replacement callback, nor the image's own stable ID required for loading delay
state. `editor/image_state.rs` now provides those presentations through the
existing `Img` callbacks. No loader/cache, source-model or layout-planner fork
was added. Retry shares the existing canonical command/app cache eviction path;
a retained control checks its current node and source before emitting it.
The scaled native button aligns with the metadata's leading edge. Visible alt
text is a bounded preview; the full alt remains in its accessible description.
Destination previews omit URL authority, query/fragment tokens and data payloads.

The native wide failure oracle now finds exactly two named failure groups and
two `Retry image` buttons. The same 180 px reservations and captions remain in
place. Final runtime:
`fc4fb7a6bf7aa2b3a92b72215f68c6131d22e5825aee1546e921ee1bf1c1abf0`.
Fixture SHA-256:
`497e003da6a196131c3e907120b29ee6cca0ae15abea91f2fb2995ff393b64a4`.

- `image-states-final-wide`: 1600×1700 light; both failures and loaded SVG.
- `image-states-final-narrow`: 520×1700 light; descriptions, captions and the
  entire loaded figure remain readable within the document canvas.
- `image-states-final-dark-200`: 1280×1700 dark at 200%; state typography and
  button scale together. The viewport shows the first failure and most of the
  second; it is not evidence for the below-fold loaded image at this zoom.
- `image-states-final-caption-edit`: native click/End/type, full edited-file
  equality against an independent expected caption mutation, one-second idle
  autosave and exact-byte Undo. Idle pixels inspected; only the targeted caption
  changed, with image reservation and subsequent content positions retained.
- `image-states-final-copy`: seven unique source markers copied once in source
  order, including image alternatives. This is not full clipboard equality.

Final wide/narrow/dark captures preserve all original source bytes. Their PNGs
and caption-idle PNG were inspected, not just the semantic trees.

Two new regressions cover destination privacy and the actual editor/toolkit
callbacks with a controlled cache. The latter drives pending (before/after
delay), actual failure, pointer Retry with caret/range selections, keyboard
Enter/Space, retry-to-pending, ready-result replacement removal and stale-source
rejection. The ready result in that unit test has no frames; real decoded SVG
pixels are verified separately by the native fixture. This toolkit pin uses
wall-clock `Instant::elapsed` for presentation but a virtual-clock task in tests,
so the test waits a bounded 250 ms for each of its two delayed states.

That keyboard test also caught Enter bubbling to the ancestor editor and
inserting a paragraph. The control now consumes that editor action and issues
exactly one Retry, as does Space; selection and complete source stay unchanged.
Temporary diagnosis probes were removed.

`scripts/check.sh` passes: format, locked dependency checks, workspace/all-target
check, Clippy with warnings denied, 499 document-view tests (2 pre-existing
ignored), 116 core tests, 25 source-fidelity tests, 11 tree-selection tests,
1 public-consumer test, 39 app tests and doc tests. Log:
`/tmp/tachyon-image-states-complete-check.log`.

## Remaining scope

E01/N06 remain partial. Real network permission/transient recovery, native
failed-to-successful retry, very small known-size images, long/multiscript alt
text, linked/nested/inline images, focus restoration after replacement removal,
screen-reader live announcement and the complete state/interaction matrix are
not claimed complete here. The next layout pass should cover cramped/nested
media presentation before expanding the image family or export variants.
