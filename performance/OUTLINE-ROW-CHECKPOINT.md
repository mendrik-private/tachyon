# Outline agreement with shared heading rows — 2026-09-09

Continues A07 and the current Files/Outline navigation contract. The current
`plan.md` explicitly excludes the minimap for now; this slice does not restore
it or qualify the retained legacy minimap module. Older audit/minimap steering
must not be mistaken for the currently implemented navigation surface.

## Defect and changes

The active-heading lookup selected the last heading whose vertical position had
passed the viewport's leading threshold. For side-by-side sections at the same
height this selected the last source sibling, rather than the beginning of the
shared reading row. The actual-font regression failed with Configuration file
instead of Configuration options before the lookup fix.

Active scroll position now resolves equal-position headings to the first source
entry. The geometry-publication pass gives the first heading of each column in
a shared row the same navigation anchor, tolerating small per-column glyph
offsets without changing painted geometry or hit testing. Later headings within
a column and later grid rows retain their own anchors. Lookup remains two
binary searches; the grouping work runs at geometry publication, not per scroll
frame.

Outline previously set `aria-selected` without painting a corresponding selected
row. It now uses shared palette roles for a tinted surface and a leading 2px
rule. Inactive rows reserve the same border width, so selection does not shift
the label. Keyboard focus and existing navigation actions are unchanged.

This defines the representative heading for an ambiguous shared scroll row;
it does not infer which sibling the reader is looking at or implement a
caret-following outline policy.

## Evidence

- `outline_uses_the_first_source_heading_in_a_shared_row` fails before the fix
  and passes after. It uses measured fixture-79 geometry, tests a synthetic
  half-pixel peer offset, verifies advancement to the next section, and checks
  that later headings in the same column or another grid row stay independent.
- `outline-peer-target-before.active-atspi.json` selects Configuration file
  after native scrolling on baseline binary `ae97523a…`. The earlier
  `outline-peer-before` and `outline-peer-scrolled-before` captures did not scroll
  far enough to reach the row and are not reproductions of this defect.
- `outline-peer-after.active-atspi.json` selects Configuration options after
  the same four native wheel inputs; the screenshot was inspected and shows
  its visible leading rule and selected surface. Source remains byte-identical.
- Final native binary SHA-256:
  `5b395270d56fe8c7a2f866095721ffaa7ac83414fa5ca2e6d9c760d14f8a23ad`.
  These are isolated Weston/D-Bus/AT-SPI sessions with a disposable fixture,
  1440 × 700 output and 100% text/display scale. Final captures wait two seconds
  after wheel input before reading geometry; intermediate AT-SPI reads during
  momentum are not evidence of exact raster alignment.

```sh
python3 performance/capture-layout.py --fixture 79-technical-sections.md \
  --binary target/debug/mineral-markdown --width 1440 --height 700 \
  --zoom-steps 0 --scroll 300 --scroll-steps 4 --scroll-settle-seconds 2 \
  --source-unchanged-check --atspi-active \
  --output performance/layout-previews/outline-peer-after.png
```

The plan's confirmed always-light decision conflicts with earlier dark/system
guidance and the recent theme implementation. A user choice has been requested;
this slice leaves theme behavior unchanged. That ambiguity does not block the
outline correction or close other audit work.

Full navigation keyboard/focus, caret-following behavior, fractional-scale
qualification, continuous resize/scroll and the remaining source/save and design
grammar requirements remain open. No full A07 or audit sign-off is implied.
