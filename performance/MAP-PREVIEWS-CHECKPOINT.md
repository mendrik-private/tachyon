# Authored map previews

September 10, 2026. E12 contract before implementation.

Recognize a canonical linked image whose authored alternative description starts
with `Map:` and contains a nonempty description. This explicit convention is
not an inference from a filename, arbitrary prose, provider URL or geography.
The image remains the same source-owned image/link node. Existing caption and
credit paragraphs remain editable and attached; do not invent locations,
coordinates, route claims, labels, attribution or remote tiles.

Show the complete supplied map with contain geometry, no crop or upscaling,
and a reserved 48 logical px destination strip beneath it. A named `Open map`
control uses the existing canonical link resolver and current live node, with
the complete map description available to accessibility and a tooltip. The
strip is presentation-only and must never enter Markdown or copied text.
Its compact control scales with text zoom, preserves selection, and works by
pointer, keyboard and accessible activation. Invalid destinations are disabled;
deleted/replaced nodes cannot activate a stale destination. Loading/failure
reuse the existing image state/retry presentation without hiding the map link.

Paired map placement requires at least 720 logical px, or the complete intrinsic
width of a smaller source. This is a conservative evidence-layout floor, not an
inference about individual raster label sizes. Zoom reduces available logical
width before candidate fitting; stack the complete figure when a pair would
fall below that floor. A full-width narrow stack may still need the authored
text alternative. Candidate height includes the same 48 px strip as rendering.

Use a deterministic, explicitly fictional site diagram for native checks, with
authored labels and a same-document destination containing its text alternative.
Do not describe this specimen as real geographic or route data. Verify native
wide/narrow/large-text, ready/missing/ordinary-linked-image cases, visible
destination navigation, unchanged source and caption edit/autosave/Undo.
Unit tests additionally check real external URL dispatch through GPUI's test
platform without launching a browser. No new media runtime or dependencies.

E11 playback, the complete nested/RTL/state matrix and static paged/export
translation remain within the full goal, not claimed by this first native map
family. Live map services are not introduced or required by this preview
contract; HTML embed conventions remain unqualified.

## Implementation and evidence

`editor/map_preview.rs` owns the explicit role and native destination strip;
the editor still owns one canonical image and its existing link action.
Loaded and failed image bodies reserve the same geometry. Rendering subtracts
the strip from the actual image height, uses contain fit, and preserves the
source aspect ratio. Candidate measurement includes the strip and rejects
cramped map pairs. Captions/credits keep the existing source-linked geometry.
The compact button uses the existing icon family and scales its text/icon/target;
very narrow hosts show the icon with the complete accessible name and tooltip.

Baseline `map-previews-before` on runtime
`0e3e3df2078581918006c69c6fc077539c933c4192c16884c6f353769ca4aa8a`
has no named Open map control (native AT-SPI query fails/exit 1). Intermediate
`map-previews-final-200` exposed an overly small paired preview at large text.
The final map-specific fit rule changes that to a full-width stack.

Final runtime:
`c5d22822d1a25e72629e57c3486aae51671a80fd88e7be838e863e9ebbbef64f`.
Fixture114 SHA-256:
`adf7ca142305d18d57c6d69cce9fd14e887f355e991c8debf26528a19d781f80`.

- `map-previews-readable-wide`, 1600×1800 light: related introduction/map
  pair; map frame x816/y144, 756×350 including its 48 px strip. Destination
  control is 105×32, aligned with map and caption. Ordinary linked image has
  no map control; missing map has both Retry image and Open map.
- `map-previews-readable-narrow`, 520×2100 light: complete stacked 496 px
  map, captions, credit, text alternative, missing preview and ordinary image.
- `map-previews-readable-200`, 1600×2400 dark at 200%: map grows from the
  intermediate 631 px pair to a complete 1314 px stack. Strip/control height
  doubles to 96/64 px. The below-fold ordinary image is not qualified by this
  top capture; the wide/narrow specimens verify that negative case.
- `map_preview_check.py` independently verifies identical source/runtime,
  two named click actions, 2.5:1 image-body aspect, 8 px caption attachment,
  scaled controls, wide versus stacked alignment, and actual dark glyph pixels
  in each of the three separately labelled map rooms. All checks pass.
- `map-previews-readable-missing-navigation` and
  `map-previews-readable-loaded-navigation`: native clicks on both missing and
  loaded map destinations move the caret to the source heading; typing verifies
  that exact destination by full-file equality, then Undo restores all bytes.
- `map-previews-readable-caption-edit`: real click/End/type, complete expected
  autosave (including existing edited-paragraph punctuation escaping), focused
  idle, blur and exact-byte Undo pass. Caption-idle pixels were inspected.
- `map-previews-readable-copy`: eight unique source markers occur once in
  source order. This is a marker-order check, not full clipboard equality.

Five new Rust tests cover explicit-role negatives, image/strip aspect geometry,
candidate/rendered-height identity and readable-width rejection, blocked URL
activation, and native-toolkit pointer/Enter/Space activation with selection
preservation, live link replacement, role removal, deletion and Undo. The
external URL check uses GPUI's test platform, not a launched system browser.
The test's initial collapsed link selection was a no-op by command contract;
selecting the canonical image range exercises actual link replacement. Keyboard
checks use real key-down/up events, not preferred text-input simulation.

`scripts/check.sh` passes formatting, workspace checks, warning-free Clippy,
734 Rust tests with two existing ignored tests, and doc tests. Log:
`/tmp/tachyon-map-final-check.log`. Crusty preparation `ctx_5b188deebc95` and
validation report 37 existing inferred architecture findings, none new,
worsened or resolved. E12 is partial; the full audit remains active.
