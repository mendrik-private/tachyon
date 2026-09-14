# Cold typography opening, September 14, 2026

Opening with justification and hyphenation enabled shaped distant prose in the
supervisor's recovery stack before starting the scoped adaptive planner. On the
181,913-byte Nudge implementation specification, this redundant pass accounted
for most worker time. Recovery now measures only the requested planning windows,
retains complete estimated source elsewhere, and is reused on planner failure.
The adaptive planner and its four-window scroll-ahead batch are unchanged.

## Native release comparison

Baseline: `6873ea1`. Same release profile, default features, rustc 1.98.0,
x86_64 Linux/Wayland, AMD Ryzen AI Max+ Pro 395. Three alternating launches of
each binary, no concurrent build, new private workspace state for every launch,
both typography settings enabled, zoom 1.0, final canvas width 1440 px. The
selected Markdown was copied; the original was never opened for editing.
These are process-cold measurements with warm filesystem/font caches.

Source SHA-256:
`26f624b89caac3ffbff43d4a6a64a564995853c6e90a037391366a2c212a11dc`.

| Measurement, milliseconds | Before (three runs) | After (three runs) |
| --- | --- | --- |
| First committed final-width worker | 321.4, 311.5, 224.3 | 71.5, 72.6, 71.5 |
| Last startup layout publication after process launch | 801.2, 761.7, 672.5 | 405.8, 354.0, 374.2 |
| Total startup worker time, including discarded initial-width work | 574.3, 561.3, 462.2 | 162.8, 150.3, 160.3 |

Median final-width worker time fell 77%; median time to the last startup layout
publication fell 51%. The recovery pass dropped from 1,979 wrap requests to 131,
and from a median 252.8 ms to 18.3 ms. Final native planner choices and measured
window coverage agreed in every before/after pair.

Timing uses `TACHYON_LAYOUT_TRACE=summary`: worker duration is reported by the
worker itself; launch-to-publication time is observed when its committed report
arrives on stderr. This measures geometry publication, not compositor presentation
latency. Raw local traces and the launch driver are in
`/tmp/tachyon-cold-benchmark.json` and `/tmp/tachyon-cold-benchmark.py`.

## Regression coverage

`cold_recovery_measures_requested_windows_without_shaping_distant_prose` exercises
the real supervisor path with recovery enabled, a 40-chapter document, two
width/zoom combinations, first and later chapter requests, and a forced planner
failure. It failed on the baseline's 80 wrap requests. It now verifies bounded
wrap requests, complete deferred text, identical visible source ranges and
hyphen placement against full native measurement, exact fallback reuse, and
unchanged serialized source. Existing recovery tests cover timeout, cancellation,
retained HTML/math, and environment invalidation.

The original measurement run found twelve stale layout expectations. They were
subsequently updated to use the shared gutter and typography tokens, with a
tolerance for sub-pixel timeline geometry. The complete repository check now
passes, including 628 document-view tests and two ignored tests.
