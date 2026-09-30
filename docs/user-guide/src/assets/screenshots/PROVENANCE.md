# Screenshot provenance

These screenshots were captured on 2026-09-30 from the existing GitHub draft
release asset `tachyon-v0.1.14-linux-x86_64.tar.gz`. They document the v0.1.14
application baseline, whose source revision is
`9977ce86cf2451f53b97aee65089674c53240d78`.

The release manifest verified both downloaded assets with `sha256sum -c
SHA256SUMS`:

- Package: `11bccae6e8af6eda50658fa664f66dca8116ef5f602da3eaf8391656c91ec878`
- Runtime inventory: `7dd3485a3cda5c0c1eeaae7eb9e775ab1f8b2be8c9343807a2b88a4121785334`
- Extracted `bin/tachyon`: `19452c60cca14146dbc957cec55f8eae5b6288a4833d91bcf0e9ec09787bd2e3`

The captures use the repository's `performance/capture-layout.py` harness on a
private headless Weston compositor. The harness copied `project-note.md` into a
private state directory before opening it. Every run used
`--source-unchanged-check`; the adjacent `.source.json` reports record a passing
byte-for-byte check. The source SHA-256 is
`1c91a3f8cac4218fb2bf3132183e7fd67020db8057919a04702f223d2a8fb228`.

## Captures and suggested captions

- `tachyon-project-note-wide.png` — 1440×1000, light system appearance, 100%
  text. Tachyon shows the real **Outline** and **Browser** tabs; automatic layout
  places the preview command and review table side by side.
- `tachyon-project-note-narrow.png` — 560×1200, light system appearance, 100%
  text. The document fills the narrow window without the sidebar, and the
  preview command stacks above the next review section.
- `tachyon-project-note-dark.png` — 1280×900, dark system appearance, 100% text,
  scrolled within the document. The open **Outline** sidebar remains visible
  beside the checklist, preview command, and review table.

Image SHA-256 values:

- Wide: `8aeca4a331f23a6e3f4be0a238561e396aadec8ba7b0893f2f9d90fc5b0fdf90`
- Narrow: `302ceb4379521d3a9f82a3d5a0c12638ab024afb096299a855ea54eb80e3b9fe`
- Dark: `3769702e06a282cc52e00be3e04c94006cabd2ebe90c54c680b72a2d2796bb37`

## Capture options

All runs used:

```text
--binary /tmp/tachyon-guide-capture/extracted/tachyon-v0.1.14-linux-x86_64/bin/tachyon
--source-document /tmp/tachyon-release-pages/docs/user-guide/src/assets/screenshots/project-note.md
--scale 120
--source-unchanged-check
```

The per-image options were:

```text
wide:   --width 1440 --height 1000 --appearance light
narrow: --width 560  --height 1200 --appearance light
dark:   --width 1280 --height 900  --appearance dark --scroll 900 --scroll-settle-seconds 1
```
