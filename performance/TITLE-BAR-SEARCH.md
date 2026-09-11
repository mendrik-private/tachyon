# Title-bar document search

2026-09-10. Completes Crusty `work_e723e35b4b3caf38`.

The title bar now has a discoverable Search icon beside the zoom controls. Its
28px square target does not shrink with the filename; the existing filename
ellipsis leaves room at narrow widths. The same toolkit button supplies pointer,
Tab focus and focus-ring behavior. It exposes the native name `Search document`,
shortcut `Control+f`, and tooltip `Find in document — Ctrl+F`.

The button dispatches the existing FindDocumentAction, sharing Ctrl+F/menu entry
behavior. Existing canonical indexing, asynchronous query cancellation, match
navigation, count, selection highlighting, disclosure/overflow reveal, Escape
focus restoration and document undo semantics remain the search implementation.
No additional search index or persistent state is introduced.

## Acceptance evidence

- `canonical_find_preserves_source_order_formatting_and_unicode_offsets` checks
  canonical match order, empty/missing queries, Unicode offsets and case folding,
  HTML mapping, out-of-range navigation and exact source preservation. Existing
  editor tests cover late-query cancellation, closing find, edits with find open,
  selected HTML conversion/undo and horizontal table/HTML reveal.
- The extended native `performance/find_check.py` first opens search by clicking
  the title-bar control, pastes into its focused input without Ctrl+F, navigates
  next/previous with pointer buttons, closes and copies the exact selected match.
  It then exercises Ctrl+F, Enter/Shift+Enter and forward/backward wraparound,
  Escape/document copy, nested disclosure reveal, selected HTML editing and one
  exact undo, followed by no-results and a fresh successful query.
- The native check fails on pre-change binary b52b9dee with `Expected one
  actionable button: Search document`; log `/tmp/tachyon-title-search-before.log`.
- Current release `title-search-regular`, `title-search-narrow` and
  `title-search-small-200` pass at 1440x1100 / 100%, 600x1100 / 100%, and
  480x1100 / 200% document zoom. Every find report checks unchanged source through
  navigation and exact restoration after the editing exercise. All three
  `*-title-search.png` screenshots were inspected: title icon, input, result
  count and match controls fit; the active match is visibly highlighted.
- `title-search-focus` adds native Tab from the query to Previous result, checks
  its AT-SPI focused state, and captures the visible focus ring. The screenshot
  was inspected with the pointer moved away. Pointer close still restores the
  document's selected match and exact native Copy afterward.
- `title-search-overflow-200` passes fixture 42 at 600x1100 / 200%, finding and
  copying exact matches in both directions across a wide table, wide code and
  HTML. All four target screenshots were inspected; active text is visible.
  Selected HTML editing and exact source undo pass.

The frame accessibility node reports unknown (-1) extents. The pointer harness
therefore validates bounds against its known private kiosk output, then sends
real pointer events on that seat. It does not infer a general window-frame
geometry contract from those missing adapter bounds. Teardown's occasional
`wl_display_dispatch: Broken pipe` is emitted after the checks pass when the
private compositor closes; successful harness processes exit zero.

## Build and reproduction

Release SHA-256:
`e04ba1394e7e71b236a5259a0032f7e5c8d89fe25cd33e3ef075951743894ead`.
`scripts/check.sh` passes 787 Rust tests (two existing ignored), formatting,
locked checks, strict Clippy, adapter/publication suites and doctests. Logs:
`/tmp/tachyon-title-search-check.log`, `/tmp/tachyon-title-search-release.log`.
Python find-oracle tests: 3 passed; capture harness tests: 23 passed.
`git diff --check` passes.

Fixture 41 SHA-256:
`3bf716a15c2d1898475ec9e96c61ace8d3a605d1ac726a94f387e70928fb5530`.
Fixture 42 SHA-256:
`4d0b219fa0184fd3abcedab8c13391a63f7997ca43f0259aa4a3558e77859876`.
Artifacts and `.find.json` evidence: `performance/layout-previews/title-search-*`.

```sh
python3 performance/capture-layout.py --fixture 41-find-document.md \
  --find-check --binary target/release/tachyon \
  --width 600 --height 1100 --output /tmp/title-search.png
```

Use fixture 42 with `--zoom-steps 10` for the overflow check. The separate title-bar
restyling item and complete accessibility/scrolling audits remain open; this
checkpoint closes the document-search entry and behavior requirement only.
