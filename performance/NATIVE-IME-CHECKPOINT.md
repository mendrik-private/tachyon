# Native system IME checkpoint — September 11, 2026

## Result

Mineral now has a repeatable native-system-IME qualification path. The harness
nests Sway 1.11 and Fcitx5 5.1.19 with Pinyin inside a private Weston 14 seat.
Mineral connects through Wayland `zwp_text_input_v3`; Fcitx connects to Sway
through `zwp_input_method_v2`. All synthetic keyboard input remains inside the
private Weston seat and never reaches the physical desktop.

The checkpoint edits the first header of a table with explicitly saved
`[160, 320]` logical-pixel widths. Provisional `ni` preedit leaves the saved
source byte-exact. Fcitx's candidate panel begins at `(265, 85)`, exactly below
Mineral's final `(265, 64, 0, 21)` cursor rectangle, and spans
`(265, 85)–(567, 117)`. Space commits one non-ASCII character, `𡬗`, at source
offset 73 while retaining the saved widths. Ctrl+Z restores the exact original
source. A second `ni` followed by Escape also restores the exact source and
creates no committed edit.

This follows the table-width policy: saved widths remain authoritative through
composition, commit and Undo. Only automatically sized tables participate in
automatic reading-edge alignment.

## Cancellation correction

The first native run exposed a source-fidelity defect. Wayland reports Fcitx
cancellation as a text-input Done event with no replacement preedit. GPUI maps
that transition to an empty replacement of the marked range. The editor used to
commit the empty composition, which could normalize untouched punctuation in
the surrounding Markdown.

`RichDocumentEditor::replace_text_in_range` now treats an empty replacement
during an active composition as cancellation and restores the composition
snapshot. The GPUI regression
`platform_ime_empty_marked_replacement_cancels_without_rewriting_source`
reproduces the platform callback directly and checks exact source, selection,
composition state and absence of an Undo entry. No compositor or toolkit vendor
code changed for this correction.

## Reproduction and artifacts

Build the repository's virtual-input helper and debug application, then run:

```sh
performance/wayland-harness/build.sh
cargo build -p markdown-app --bin mineral-markdown
python3 performance/native_ime_check.py
```

The system path needs Weston 14, Sway 1.11 with wlroots 0.19, Fcitx5 5.1.19,
the Pinyin/libime data packages and Pillow. `--runtime-prefix PATH` can point at
a local package extraction containing Sway and Fcitx when they are unavailable
system-wide. The recorded run used `/tmp/mineral-ime-root`; it was neither
committed nor installed into the host system.

The successful run used binary SHA-256
`aa94a032d6f316abb0e756544cd4e36ac9d6e8141d87d43802b03c7cce9dd89f`
and source SHA-256
`53a5a1e60498ab7b13f9ce97cd2802381e7e2138014ec76ff87a57baa02f7464`.
Its retained artifacts are:

- `layout-previews/native-ime-candidate.png`, SHA-256
  `80d7f9ba7f84dd9973297db23d034080ccf51b2e39ee8744c6214ea82e879638`;
- `layout-previews/native-ime-protocol.log`, SHA-256
  `3afe070a5493dfde0fb810759e4e1ae9cc58df3afc711c6c8c3f49e459a2ed37`;
- `layout-previews/native-ime-report.json`, SHA-256
  `51f9f30d631e069b291cfc16d415a6e55603ea844421ec838583442d48b25539`.

The protocol excerpt retains manager discovery/binding, text-input entry and
enablement, cursor rectangles, both preedit sequences, the commit string and
the cancellation Done event. The JSON report is the machine-readable oracle.

## Automated verification

- `python3 -m py_compile performance/native_ime_check.py` passed.
- `python3 performance/native_ime_check.py --runtime-prefix
  /tmp/mineral-ime-root` passed the native protocol, source, width and geometry
  oracles above.
- `scripts/check.sh` passed formatting, locked checks, strict Clippy, all 847
  Rust tests, adapter tests and doctests; two native-font integration tests were
  ignored because their system fonts are not installed.
- `git diff --check` passed.

## Boundary

This qualifies one isolated Sway/Fcitx5/Pinyin table path at 100% scale. It does
not qualify the physical GNOME/Mutter and IBus path, 200% or fractional display
scale, other input methods, RTL or nested/rich table variants, resize or scroll
while the panel is open, candidate paging, or surrounding-text deletion. Those
remain A07 work. This checkpoint changes no performance timing result.
