# Retaining the complete accessibility debug snapshot

2026-09-10. Follow-up on Crusty `work_98682121afe7c2b7`. The work remains open
until the strict complete-reference scrolling and physical-display qualification
requirements are met. This checkpoint uses the new 21px default body text; do
not treat comparisons with the earlier smaller-font binary as matched timings.

## Current baseline

Full example/reference.md is 102,068 bytes, SHA-256
`bf572f2b6d0226bfa2ee98bca2bf2c63faf8f232c68215937cb5e48deb143cce`.
Relative performance/layout-fixtures and performance/visual-assets are copied
privately at authored paths, with source/resource hashes in the sidecars.

Baseline executable SHA-256:
`353df09a36146fa79235342f42aa7cf6403887e00e7a0d5c8c4d8828bbdf5865`.
Saved as performance/results/reference-scroll-2026-09-10/current-baseline.
Rust 1.98.0 locked release, thin LTO, one codegen unit, layout-validation,
`-C strip=none`. Dirty revision 3edaf925194624496c656552f2055a11f4e42c94.

The existing isolated native protocol uses Weston GL, 1440x1000, 100% text,
120Hz requested refresh, scale 1, active private AT-SPI consumer (3,056 initial
nodes), continuous native axis input every 12ms, eight-second directional sweeps
and 32 seconds of measurement. A warm run adds a down-and-back traversal first.
Original/private source hashes are unchanged in all recorded runs. One-second
pidstat CPU logs contain no cargo/rustc activity during these baseline runs.
The second first-traversal run is EXCLUDED from timing comparisons: Crusty
architecture preparation completed at 17:21:02 local time, overlapping its
measurement (CPU log starts 17:20:37, configured warmup 15 seconds). No compiler
ran, but this agent's own analysis workload was not part of the scrolling test.

| Baseline run | Draw p50 / p95 / p99 / max ms | Input p95 / p99 / max ms | Missed deadlines | Draw stalls >=25ms |
| --- | --- | --- | --- | --- |
| First 1 | 2.157 / 4.399 / 7.291 / 18.547 | 9.781 / 11.256 / 25.100 | 1.242% | 0 |
| First 2 (excluded) | 2.292 / 4.727 / 7.754 / 21.152 | 10.076 / 12.009 / 25.182 | 1.385% | 0 |
| Warm | 2.273 / 4.395 / 5.640 / 10.322 | 9.798 / 10.658 / 14.729 | 0.370% | 0 |

`python3 /tmp/assess-reference-scroll.py reference-current-first1` exits 1:
draw p99 exceeds 6ms and missed deadlines exceed 0.1%. The excluded First 2 also failed, but is not controlled repeat evidence. Warm passes draw/input/stall limits but fails missed deadlines.
This is the strict-budget diagnostic result, distinct from the harness's smaller
60fps/16.67ms gate. The full document was retained rather than minimized away
because complete-reference scrolling is the reported workload.

## Profile and candidate

A separate first-traversal perf capture, delayed by 15 seconds, collected 6,578
samples / 53MB with no lost samples. One short-lived thread failed attachment;
optimized dependency unwinding is incomplete. Profiled timings are excluded
from uninstrumented comparisons. Self-sample shares include synthetic tree
construction 6.82%, memmove 6.73%, TreeUpdate::clone 4.92%, text-tree traversal,
property destruction and raster bounds. These do not identify a single exclusive
cause. The debug capture call clones a complete TreeUpdate unconditionally on
every frame, including unchanged text/property buffers.

The candidate retains unchanged entries in the complete debug snapshot, comparing
both ID and Node value. Size changes replace the snapshot; equal-length edits and
reordering replace changed entries. Tree metadata, identity and focus are refreshed.
Live platform publication, complete offscreen semantics, action dispatch and the
public debug JSON interface remain intact. This is not a delta snapshot.

The production helper is compiled directly by the existing standalone publication
suite. Its regression fails before the fix because an unchanged paragraph gets
a new text allocation on scroll, then passes with retained storage. It also
checks the exact complete snapshot and AccessKit consumer focus through editing,
reordering, insertion and deletion. All five standalone publisher tests pass.

Artifacts: performance/layout-previews/reference-current-{first1,first2,warm,profile}
with JSON, source/resource evidence, CPU logs and application/compositor logs.
Raw profile: performance/results/reference-scroll-2026-09-10/current.perf.
Decoded report: /tmp/tachyon-scroll-current-profile.txt. Local orchestration:
/tmp/run-reference-scroll-current.py. Reproduction flags match
performance/REFERENCE-SCROLL-FIX.md; `--perf-warm-sweep` selects the warm run.

## Candidate timing comparison

Candidate executable SHA-256:
`41e830f406194ffe1f513be7e878fe44c2e40b0376bf547ba3cad46acc7c6be1`.
The final optimized build and full checks completed before the timing runs.
The clean alternating first-traversal sequence is baseline First 1, candidate
First 1, baseline First 3, candidate First 2. The earlier First 2 is excluded
as described above. All listed runs retain source/resources and active AT;
CPU logs contain no compiler activity.

| Run | Draw p50 / p95 / p99 / max ms | Input p95 / p99 / max ms | Missed deadlines | Draw stalls >=25ms |
| --- | --- | --- | --- | --- |
| Baseline First 3 | 2.232 / 4.776 / 7.651 / 24.871 | 9.921 / 12.567 / 28.672 | 1.720% | 0 |
| Candidate First 1 | 2.130 / 4.440 / 8.045 / 29.999 | 9.830 / 12.313 / 25.281 | 1.385% | 1 |
| Candidate First 2 | 2.099 / 4.403 / 7.471 / 25.002 | 9.830 / 11.952 / 31.539 | 1.356% | 1 |
| Candidate warm | 1.994 / 4.141 / 5.116 / 9.732 | 9.716 / 10.527 / 13.361 | 0.256% | 0 |

First-traversal tails overlap and do not establish a reliable tail improvement.
Both candidate first traversals still exceed 6ms p99, have one draw stall, and
miss more than 0.1% of deadlines. Warm p99 improves from 5.640 to 5.116ms and
median draw from 2.273 to 1.994ms in this matched pair. Average process CPU over
the final 32 one-second pidstat samples drops from 34.59% to 31.03% (100% is one
logical CPU). These are observed bounded-run results, not statistical release
qualification or a universal speedup. The candidate retains measurable warm-work
savings without claiming to fix first-visit stutter.

`scripts/check.sh` passes: 786 Rust tests, two existing ignored tests, locked
checks, formatting, strict Clippy, adapter/publication suites and doctests.
Log: /tmp/tachyon-snapshot-check.log. The existing publication suite now has
five tests. `git diff --check` passes. Both candidate first-traversal native trees
contain 3,056 nodes and the same 327 headings in canonical order as the baseline.
The unchanged full-source checks pass independently of performance gates.

The separate candidate profile has no lost samples. TreeUpdate::clone is no
longer among symbols with at least 1% self samples (baseline 4.92%); memcmp grows
from 2.45% to 4.45%, reflecting the retained-node comparisons. Synthetic frontend
construction remains prominent at 7.65%, and memmove at 8.09%. Approximate total
sampled cycles are 39.79 billion versus 41.06 billion before; these profiled runs
are diagnostic, not timing qualification. Remaining allocation/frontend tree
construction and first-visit layout work need further investigation rather than
another claim that debug snapshot copying was the sole cause.

Raw candidate profile:
performance/results/reference-scroll-2026-09-10/reference-snapshot-profile.perf.
Decoded report: /tmp/tachyon-scroll-snapshot-profile.txt.

`reference-snapshot-visual-{before,after}` opening screenshots at 1440x1000 are
pixel-identical (zero changed pixels); the after screenshot was inspected.
Native after/150/200 snapshots each retain 3,056 nodes and all 327 headings in
identical canonical order. All original/private source hashes remain unchanged.
These are bounded visual/semantic checks, not width/zoom timing qualification.
The full queue item remains open: first-visit draw p99, missed deadlines, draw
stalls, representative-width performance and physical-Mutter qualification remain.
