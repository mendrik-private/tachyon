# Release qualification protocol

`run-release-qualification.py` is the authoritative end-to-end performance protocol. Its defaults execute the release plan verbatim: 30 warm and 30 cold 100 KiB launches, one separately labelled empty-shader-cache launch, and five 60-second interaction runs for each 100 KiB, 1 MiB, and 10 MiB fixture.

The startup clock begins at the first instruction in the invoking process's `main` and stops in the frame callback after the loaded, editable document scene has completed its first paint. Warm samples exercise Mineral's local single-instance activation path: an uncounted launch initializes and retains the GPUI render process, then each measured CLI process sends one document-open request over an isolated Unix-domain socket and waits for that request's first paint. Reports explicitly identify this as `resident render process reused`. The timestamp crossing that process boundary uses `SystemTime` nanoseconds since the Unix epoch; each sub-100 ms sample is short enough that wall-clock adjustment is not expected, and all raw samples remain in the report for audit. This measures the warm user-visible activation path without omitting invoking-process or IPC time.

Cold samples create a fresh render process and fresh XDG state/cache directory, and apply `POSIX_FADV_DONTNEED` to the document while retaining the warmed Mesa shader cache. The separately labelled first-GPU sample also creates a fresh process and uses an empty Mesa shader cache. Network image completion is not part of startup readiness. The runner owns the exact temporary socket path, requests orderly server shutdown in `finally`, and terminates only that child if shutdown cannot complete.

Interaction runs use the active physical Mutter Wayland display directly. A temporary, narrowly configured `/dev/uinput` device supplies genuine libinput pointer, discrete wheel, selection-drag, and keyboard events; the account running qualification must have write access to `/dev/uinput`. The device is destroyed when the runner exits. Every run also inserts text, permits the 750 ms autosave to fire during continued scrolling, and scrolls wide content horizontally. The first of five runs in each size class resizes twice, so resize is represented without measuring the same compositor reconfiguration ten times. A 2.5-second continuously rendered warmup is subtracted before the 60-second capture. One uncounted interaction warmup populates GPU pipeline caches.

Run the complete protocol from the repository root while the 120 Hz output is active and the system power profile is unchanged:

```sh
performance/run-release-qualification.py
```

For a plumbing smoke test only:

```sh
performance/run-release-qualification.py --startup-runs 2 --interaction-runs 1 --seconds 3 --warmup-ms 500 --output /tmp/mineral-qualification-smoke.json
```

The JSON contains every raw startup and interaction report plus aggregate gates. A successful full run requires warm startup p95 ≤100 ms, cold startup p95 ≤250 ms, 10 MiB open-and-prepare p95 ≤1 second, every run's draw-work p99 ≤6 ms, every run's input-to-present p95 ≤16.7 ms, combined missed presentation deadlines below 0.1% for each size, and zero application draw stalls of at least 25 ms. Presentation intervals of at least 25 ms are reported separately rather than being misattributed to application work. The environment record includes Mutter's live display state, compiler, session, kernel, and power profile.

## Recorded run: 2026-09-06

The full default protocol was rerun from the final optimized binary on the
specified 2880×1800, 120.001 Hz Mutter Wayland session at 166.67% scale. Its
authoritative raw and aggregate artifact is
[`release-qualification-2026-09-06.json`](release-qualification-2026-09-06.json).
The overall release gate **did not pass**; the failed samples are retained.

| Gate | Recorded result | Outcome |
| --- | ---: | --- |
| Warm 100 KiB startup p95 | 42.95 ms | Pass |
| Cold 100 KiB startup p95 | 155.23 ms | Pass |
| First-GPU startup (separate sample) | 175.91 ms | Informational |
| 10 MiB open + prepare p95 | 961.14 ms | Pass |
| 100 KiB worst draw p99 / input p95 | 4.57 / 9.54 ms | Pass / pass |
| 1 MiB worst draw p99 / input p95 | 10.74 / 13.58 ms | **Fail / pass** |
| 10 MiB worst draw p99 / input p95 | 4.34 / 11.43 ms | Pass / pass |
| Missed presentation deadlines | 1.05% / 1.78% / 3.62% | **Fail** |
| Application draw stalls ≥25 ms | 0 / 1 / 0 | **Fail** |

Fourteen of fifteen sustained runs met both foreground draw and input budgets.
The one failing 1 MiB run recorded draw p99 10.74 ms and one 29.79 ms draw;
the other four 1 MiB runs recorded draw p99 between 2.95 and 3.51 ms. All
three size classes exceeded the strict presentation-miss budget even when draw
work remained short. These facts distinguish the generally fast interaction
path from the still-unmet release contract; they are not grounds to relabel the
run as passing.
