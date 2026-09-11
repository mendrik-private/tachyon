# Resize recovery for compact layouts — 2026-09-09

Continues STR-0003 and [compact technical sections](TECHNICAL-SPACE-CHECKPOINT.md).
This closes two reproduced layout defects, not A07 or the complete audit.

## Defects and fixes

The native fixture-79 reading sequence failed after narrowing and widening the
window: configuration sections remained stacked at the restored width.

1. The presentation-change penalty suppressed technical-row recovery from a
   narrower stack. The existing exception only handled matched three-card rows.
   Technical rows now compete without this penalty when all their members were
   stacked at a smaller canvas. Measurement, height, overflow and editing locks
   remain authoritative.
2. Even with that planner fix, native traces showed no reflow after the last
   widening. Paint published the actual new bounds but only called `notify`;
   the resize could finish with the narrow geometry until another event arrived.
   `publish_painted_bounds` now schedules reflow from the painted width directly.
   Initial Ready handling and duplicate-width suppression are retained.

Pre-fix evidence remains in `technical-resize-reading.resize.json` and
`technical-resize-trace.planning.json` under `layout-previews/`. The latter ends
with a 650px committed canvas despite the window restoring a 1040px editor.
Final traces include the missing 1040px commit.

The native oracle now supports both fixture 47 and fixture 79, rejecting unknown
scenarios. Reading drift is limited to one physical pixel (logical displacement
multiplied by display scale), replacing the previous four-logical-pixel allowance.
Negative tests reject two-pixel drift at 100%, one-logical-pixel drift at higher
scales, missing peers and reversed stacked order.

## Verification

- `scripts/check.sh` and `git diff --check` pass. The view suite has 413 passing
  tests and two intentionally ignored native-font tests.
- Seven resize-oracle tests and eleven capture-harness tests pass.
- The measured-row regression failed before removing the technical recovery
  penalty. The paint-publication regression fails with notification-only
  behavior and passes with immediate dispatch, over repeated shrink/expand.
  It invokes the production paint-publication boundary directly because GPUI's
  test scheduler can deliver extra renders that hide the native scheduling bug.

Final native captures use debug `layout-validation` binary SHA-256
`2ea2b3959e91dbb84e9fdb9171d7ff5bcc7b7e55d6c01b54f601b514de13912c`,
private Weston/AT-SPI sessions, light appearance, 100% display and text scale,
and a 1920 × 1000 output. Window widths change through native F9/F10 commands;
the measured document canvas goes 1040 → 650 → 1040px.

| Capture prefix in `layout-previews/` | Result |
| --- | --- |
| `technical-resize-final-reading` | Paired → stacked → paired; duplicate narrow resize stable; heading identities and source unchanged |
| `technical-resize-final-editing` | Caret/focus retained during narrowing; insertion in the right explanatory paragraph autosaved; exact-byte undo |
| `editorial-resize-final-reading` | Three-card row → stack → row; deep-document reading heading stays 11px below viewport top |

Both reading runs report zero observed anchor displacement. The technical
heading stays 250px below viewport top; that case verifies layout recovery near
the document start, not arbitrary mid-paragraph anchoring. The older editorial
fixture supplies the deep-document reading-position check. These are discrete
native resize tests, not continuous-resize latency or full fractional-DPI
qualification. The desktop-shell captures can place the enlarged window partly
outside the output; AT-SPI local bounds and committed layout traces are the
width/reflow oracle, not an assertion that these captures prove window placement.

Reproduce (change fixture/mode/output for the other entries):

```sh
cargo build --locked --features layout-validation --bin mineral-markdown
python3 performance/capture-layout.py --fixture 79-technical-sections.md \
  --binary target/debug/mineral-markdown --width 1920 --height 1000 \
  --zoom-steps 0 --resize-check reading --layout-trace details \
  --output performance/layout-previews/technical-resize-final-reading.png
python3 -m unittest discover -s performance -p test_resize_layout_check.py
```

## Still open

Follow-up: bounded height-only reading/editing recovery is now covered by
[the height-resize checkpoint](HEIGHT-RESIZE-CHECKPOINT.md), and system appearance
by [the theme checkpoint](SYSTEM-APPEARANCE-CHECKPOINT.md). The list below records
the remaining scope at the time of this width-resize slice.

Height-only resizing, continuous resize/scroll interaction, arbitrary prose
anchors, fractional-scale native qualification, minimap agreement and dark/system
theme matching still need work. Save/source dependencies and other audit items
remain open. No release-performance claim follows from this debug build.
