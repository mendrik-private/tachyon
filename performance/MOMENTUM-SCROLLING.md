# Momentum scrolling verification — 2026-09-06

The old renderer only animated line-based wheel events, using an 85 ms decay
constant. Pixel events bypassed momentum on the assumption that they contained
platform inertia. The pinned GPUI Wayland backend instead emits raw pixel
events with `TouchPhase::Moved`; it does not synthesize a release coast.

The document now follows continuous input directly and starts a velocity-based
coast after 50 ms without input. Wheel impulses ease out too. Both use a 500 ms
exponential decay constant, integrated by elapsed time, with bounded residual
distance and cancellation on edits, clicks, direction changes and boundaries.
Reduced motion preserves immediate scrolling. Small horizontal noise no longer
diverts predominantly vertical gestures.

## Longer fade-out follow-up

The current release's native wheel and continuous-input checks both showed a
coast before this refinement; a completely absent coast was not reproduced.
The 320 ms decay did, however, leave a single wheel notch moving only 0.65 logical
pixels between 700 and 800 ms after release. A new single-notch regression failed
on that behavior. The 500 ms decay keeps that interval above one logical pixel,
without increasing the wheel notch's total travel. Continuous input still tracks
the fingers directly, with bounded release velocity and the same cancellation
and reduced-motion behavior.

All 116 document-view tests, formatting, and warning-denied document-view Clippy
pass. The native coast harness retains its deceleration assertions and now checks
the final settled position at 2.8 and 3.2 seconds to accommodate the longer tail.

Release `94ae12275b2c7910fde0c67acf4059d0b11000c3ca2c97303c70252c0b704302`
passes both native checks: [wheel](layout-previews/momentum-long-fade-wheel.json)
and [continuous](layout-previews/momentum-long-fade-continuous.json). Wheel thumb
positions progress through 125, 151, 168, 184, 195, 203, 203 after release;
continuous positions progress through 129, 197, 243, 285, 312, 334, 335.
These verify actual on-screen coasting, deceleration and settling independently
of the deterministic physics tests. Restart the rebuilt
`target/release/tachyon` to use this refinement; an already running
process or separately installed copy is not updated by building.

The same release passes the isolated 10 MiB scroll gate at 1728×1080, 166.7%
scale and 120 Hz: **109.1 FPS, 6.55 ms draw p99** over a 10-second measured run
([report](layout-previews/momentum-long-fade-scroll-10m.json)). This is an
isolated-compositor result, not a measurement on the user's physical display.

## Correctness

- The pixel-input regression failed before the fix and passes afterwards.
- All 168 workspace tests, formatting and warning-denied Clippy pass.
- Tests cover 60/120/144 Hz integration, bounds, reversal, cancellation,
  zero-delta release events, diagonal input, reduced motion and edit interruption.
- Native Wayland input is checked independently of FPS by recording the actual
  document scrollbar after input stops. Its position must continue changing,
  slow down, then settle. Both [continuous input](layout-previews/momentum-continuous-after.json)
  and [wheel input](layout-previews/momentum-wheel-after.json) pass. The old
  [continuous-input capture](layout-previews/momentum-continuous-before.json) fails.

Reproduce with `performance/wayland-harness/build.sh`, then:

```sh
python3 performance/capture-layout.py --fixture 02-list-arrangements.md --coast-check continuous --output /tmp/mineral-continuous.png
python3 performance/capture-layout.py --fixture 02-list-arrangements.md --coast-check wheel --output /tmp/mineral-wheel.png
```

## Performance

Two release runs on an isolated Weston 14 headless GL display at 1728×1080,
166.7% scale and 120 Hz, using the existing 10 MiB stress generator, measured
100.2 / 99.0 FPS and 14.17 / 15.34 ms draw p99. Both pass the >60 FPS / <16.67 ms
draw-p99 gate: [run 1](momentum-isolated-10m-1.json), [run 2](momentum-isolated-10m-2.json).
These are isolated-compositor measurements, not physical-display measurements.

The [physical-display retest](adaptive-scroll-2026-09-06-momentum.json) received
zero input and presentation samples because GNOME reported the session locked.
Those zero-FPS results are invalid as scrolling measurements. The locked session
was not changed; physical-display performance remains unverified for this binary.
