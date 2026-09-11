# Current layout performance — September 11

## September 11 physical edit result

The 1 MiB physical Mutter scroll/edit path stays within the draw and input
limits after localizing image-alt typing, retaining unchanged component
geometry, and splitting visual lines into compact hot records with shared cold
payloads: 3.61 ms draw p99, 10.63 ms input p95, 12.29 ms input max, and no
application draw stalls in a 10-second diagnostic. The corresponding
scroll-only and no-input controls still miss the strict presentation threshold.
The follow-up segment-local range representation removes the retained
visual-line suffix rewrite. Three matched 10 MiB runs measure 2.20–2.59 ms draw
p99 and 12.36–13.16 ms input p95 with no application stall, passing those two
limits. The bounded-coordinate follow-up also removes linear projection
byte/UTF-16 suffix maintenance. Its three clean matched runs measure 2.31–2.39
ms draw p99 and 8.27–8.86 ms input p95 with no application stall. Physical
presentation still misses at 1.75–2.24%. See
`LARGE-DOCUMENT-EDIT-CHECKPOINT.md` for the implementation boundary, matched
physical results, regression coverage, and remaining work. A07 remains open.

## September 11 retained-accessibility result

The retained/delta implementation and transparent geometry owner pass the
matched isolated 100 KiB series plus final 1 MiB and 10 MiB active-AT-SPI
runs. Final release binary SHA-256:
`1e76cb31f3bb225ecead48a981eca91c37ba870f731eb8fb29b7a3c658512017`.
The large runs expose exactly 3,337 and 33,268 direct document roots, match 23
representative roles across each document, and preserve exact source bytes.
They measure 107.5/108.2 FPS and 9.99/9.76 ms draw p99 respectively.

See `RETAINED-ACCESSIBILITY-CHECKPOINT.md` for the implementation boundary,
native correctness evidence, exact run table, and remaining qualification work.
The full release remains unqualified because the physical-session matrix is
still open and the isolated runs do not meet the 6 ms draw-p99 or 0.1%
missed-deadline release limits.

## September 11 static-export result

`PAGED-HTML-EXPORT-CHECKPOINT.md` records the first production paged output:
an atomic app command backed by a source-preserving `document-core` projection.
A deterministic mixed document renders without warnings as four A4 pages with
selectable text, repeated table headers, working internal links, running folios
and preserved authored table widths. This does not change the continuous-editor
timings above or qualify finite-page preview, PDF UI, complete page masters,
tagged PDF, real print equations or Mineral-owned pagination convergence.

## September 11 native-system-IME result

`NATIVE-IME-CHECKPOINT.md` records a real isolated Sway/Fcitx5 Pinyin path over
Wayland text-input-v3. Provisional input leaves source exact, the candidate
panel begins at the published caret rectangle's lower edge, one CJK character
commits while saved `160/320` table widths remain intact, and both Undo and
Escape restore the exact original bytes. The run also found and fixed empty
marked-range cancellation committing serialization normalization. This does
not change the timing results below. Physical-session, compositor/input-method,
scale, script and transformed-family coverage remains open, so A07 stays active.

## Measurement contract

Continue the full audit with layout and available-space use prioritized.
Measure the current locked release runtime after the new content-first and
heading-focus recovery work. Use the existing deterministic rich 10 MiB
generator and isolated Weston 14 GL output with a private Wayland input seat.
Do not inject input into the user's physical desktop or change its session,
power profile or monitor settings to obtain a result.

Start with a bounded scrolling diagnostic, then repeat matched runs to inspect
median/tail behavior. Keep native rendering and accessibility enabled, verify
the exact source hash, and record runtime/fixture identity, output dimensions,
display/text scale, warmup, duration, event samples, presented FPS, draw-work
tails and input latency. A passing average with a long stall is not release
qualification. Trace/profiling runs must be labeled separately from timing runs.

The isolated harness gate (>60 presented FPS, draw p99 <16.67ms and real input
samples) is a diagnostic subset. The original physical-Mutter 120Hz protocol,
30 warm/cold startup samples, five 60-second mixed interaction runs per size,
draw p99 ≤6ms, input p95 ≤16.7ms, missed deadlines <0.1%, no application draw
stalls ≥25ms, and complete visual/source/editing oracles remain the authority.
Do not replace those requirements with this smaller isolated test.

Environment at start: AMD Ryzen AI Max+ PRO 395 / Radeon 8060S, 32 logical CPUs,
Linux 7.2.3-070203-generic, Rust 1.98.0 (88d9e12ae, LLVM 22.1.8), Weston 14.0.2,
unchanged `balanced` power profile. Release uses the workspace's thin LTO,
one codegen unit and locked dependencies. The `layout-validation` feature
enables the existing private diagnostics; it does not disable rendering work.

Build: `cargo build --release --locked -p markdown-app --bin mineral-markdown
--bin mineral-fixture --features layout-validation` (one shell command).

## September 10 baseline: not qualified

Runtime SHA-256:
`586b55338f6301016ee4d4f5d9d7ef6a2c9ac44e930e35438c3f2a3daf93ed2e`.
No renderer optimization was applied in this checkpoint. It establishes a
current failure that must be resolved without reducing accessible content.

Reproduce the scale-120 large-document failure:

```sh
python3 performance/capture-layout.py --binary target/release/mineral-markdown \
  --generated-bytes 10485760 --width 1728 --height 1080 --scale 120 \
  --refresh-rate 120000 --perf-seconds 10 --perf-input continuous \
  --atspi-active --source-unchanged-check \
  --log-output performance/layout-previews/current-layout-scroll-10m-repeat.log \
  --output performance/layout-previews/current-layout-scroll-10m-repeat.png
```

The 10 MiB run with active AT-SPI exited without a performance report twice at
requested scale 200 and again at scale 120. The retained compositor logs report
`Data too big for buffer (4092 + 20 > 4096)` followed by a client communication
error and broken pipe. The harness then fails reading the absent report.
Only 136 accessible nodes were observed before the disconnect: that snapshot
does **not** establish complete document readiness or accessible coverage.

Reducing only generated source size produced a report, but missed the layout
scrolling target. The following are diagnostics, not a matched speedup series:

| Run suffix | Source / AT-SPI | Seconds | FPS | Draw p99 | Input p95 | Draw stalls ≥25ms |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `100k-repro` | 100 KiB / active, 8,271 nodes | 10 | 42.1 | 34.57ms | 33.28ms | 157 |
| `100k-scale120` | 100 KiB / active, 8,271 nodes | 10 | 50.2 | 30.69ms | 30.13ms | 59 |
| `100k-profile` | 100 KiB / active, profiled | 30 | 37.1 | 33.24ms | 33.54ms | 533 |
| `10m-noatspi` | 10 MiB / no active client | 10 | 107.8 | 9.54ms | 10.00ms | 0 |

Artifacts are `layout-previews/current-layout-scroll-<suffix>.json` and `.log`.
The 10 MiB failures have retained logs with suffixes `10m-repro` and
`10m-scale120`; the first unlogged attempt has no retained artifact.
The profiled run is intentionally excluded from timing comparisons.
The control without an active accessibility client is **not** an accepted
configuration or proof of improvement. Its warmup was 5 seconds versus 15 for
the accessibility runs; future comparisons must match both warmup and input
start. It only demonstrates that this runtime can finish the generated large
document with the same isolated input mechanism in that reduced configuration.

All completed reports record actual scale factor **1.0**. In particular,
`100k-repro` requested 200/120 but did not report that effective scale. Do not
cite it as fractional-scale performance evidence. The scale-120 runs avoid
this requested-versus-effective mismatch; its cause remains uninvestigated.

Generator source hashes:

- 100 KiB: `4ec95d32761f4c47cde245d8d98418e4512d7d15a5cfe9d370604fb1c54ce00c`.
- 10 MiB: `16834e22b862cffa330391dc2564890634d9e719a8d368446c6ff1161ba437d9`.

Only the passing no-client control reached the harness's post-run exact source
check, which passed. Existing harness failure paths raise before that check;
do not claim failed runs verified unchanged bytes or complete reading order.
No physical desktop input, session, monitor or power settings were changed.

## Profile and next layout work

A separate 15-second CPU profile attached only to the isolated 100 KiB app
process, using `perf record -e cpu-clock -F 99 --call-graph dwarf -p PID`.
It captured 1,436 samples without lost samples. Raw data remains at
`/tmp/mineral-current-layout-accessibility-profile.perf` (temporary evidence).
The release binary is stripped: dominant app addresses are not symbolized,
so this profile cannot yet assign cost to a Rust function. The named
`memmove` sample share is 6.89%, insufficient by itself to identify a cause.
The initial report reader produced no output for 39 seconds; it was stopped.
The retry read successfully with symbol downloading disabled via
`DEBUGINFOD_URLS=`.

Source inspection identifies a candidate, not a proven root cause:
`SemanticTree::publish` caches compilation but clones and republishes every
canonical node on each synthetic subtree publication, then clones action
targets. The pinned GPUI builder accepts owned nodes through `push_child`.
Do not remove offscreen semantic nodes or actions as a shortcut.

Next, within the layout-first priority:

1. Obtain a symbolized optimized profile and isolate publication versus adapter
   update cost, using matched timings and complete document readiness.
2. Preserve the entire canonical tree and stable IDs while avoiding needless
   repeated work during scrolling; invalidate correctly for reflow, zoom,
   source edits, selection/focus, task state and geometry-dependent actions.
3. Check the actual scaled viewport and retain source/failure diagnostics even
   when timing gates fail. Keep cold-open and steady scrolling separate.
4. Re-run active-AT-SPI 100 KiB/1 MiB/10 MiB, source/order/action tests and the
   native width/height recovery cases before expanding layout families.

The previous content-first width gains and resize fixes remain implemented.
This checkpoint does not qualify their large-document performance and does
not close A07 or the full audit.

## Follow-up: symbolized profile and rejected local optimization

A same-optimization build retaining symbols (`cargo rustc --release --locked
-p markdown-app --bin mineral-markdown --features layout-validation -- -C
strip=none`) produced baseline SHA
`18c3874058ea06fead8b2d61b734ebac0b2d40ce537a43ae88253017e162659a`.
It is retained temporarily at
`/tmp/mineral-a11y-baseline-4Duop9/mineral-markdown`.
The native profiling run is `layout-previews/a11y-symbolized-before.json`;
raw 15-second, 199Hz CPU samples are `/tmp/mineral-a11y-symbolized-before.perf`.
It includes warmup/activation, not just steady scrolling. There were 2,825
samples and no lost samples. Function symbols resolve, but call-stack unwinding
is incomplete; do not infer exclusive caller costs from this profile.

Sampled self costs include consumer `State::update` (9.31%),
`first_filtered_child` for preceding text runs (8.00%), `Node::child_ids`
(7.65%), synthetic child publication (3.58%) and `TreeUpdate::clone` (2.83%).
Allocation and copies are also prominent. Source inspection confirms GPUI
reconstructs and submits a complete tree each frame and its debug snapshot
clones it even in release builds. The application geometry cache does not
eliminate those costs.

A candidate guard skipped unchanged role/value/children/graft identity before
text-range checking. Its scan-count regression failed before and passed after;
actual Unicode text changes and inserted/removed text runs remained announced.
Nevertheless, the end-to-end result did not justify keeping the shortcut.
Candidate SHA:
`59d91906cec2822591f48822f25b7bfba8a27374f1cd8225d75a22f6ad2ce37c`.

Alternating unprofiled runs used the exact saved baseline and candidate,
the same 100 KiB source hash, 1728×1080 output, requested/actual scale 1.0,
active AT-SPI (8,271 nodes), 120Hz, 15-second configured warmup and continuous
private-seat input. No build or other native capture ran concurrently.

| Duration / pair | Baseline FPS | Candidate FPS | Baseline draw p99 | Candidate draw p99 |
| --- | ---: | ---: | ---: | ---: |
| 10 seconds / 1 | 46.2 | 51.9 | 31.00ms | 30.49ms |
| 10 seconds / 2 | 37.6 | 34.7 | 37.39ms | 46.14ms |
| 10 seconds / 3 | 36.0 | 43.9 | 36.90ms | 29.62ms |
| 60 seconds / 1 | 43.6 | 40.8 | 31.80ms | 31.54ms |

All eight runs failed the scrolling gate. Short-run median improvement did not
persist in the longer pair, and tail behavior was inconsistent. **The candidate
guard was removed**, not relabeled as a performance fix. No release speedup is
claimed. Timing artifacts use `a11y-text-before/after-1/2/3/60` prefixes.

The harness now runs the exact source check before reading/gating a completed
performance report. All eight failed timing runs therefore retain passing
`.source.json` evidence; previously that evidence was lost on a failed FPS
gate. It still does not claim full readiness, editing or spoken output.

Three notification-correctness tests remain in the adapter: geometry-only
updates produce no text-change events, simultaneous parent scrolling does not
hide Unicode edits, and text-run insertion/removal remains announced. The
normal check script now includes the previously excluded adapter tests.
The rejected scan-count assertion is not presented as a passing performance
regression: the final tests intentionally verify notifications, not scan cost.

The next structural step is specified in `RETAINED-ACCESSIBILITY-PLAN.md`:
validated retained subtrees, framework-owned liveness/focus/activation/deltas,
current action resolution, and on-demand debug snapshots. No offscreen semantic
content may be removed to meet the target. A07/A16 and full qualification remain
open.

Final verification after removing the experiment:

- Locked release rebuild SHA returned exactly to
  `586b55338f6301016ee4d4f5d9d7ef6a2c9ac44e930e35438c3f2a3daf93ed2e`.
- `scripts/check.sh`: 738 workspace tests plus 5 adapter tests pass;
  2 existing ignored tests. Formatting, all-target check, strict workspace
  Clippy and doctests pass. Log: `/tmp/mineral-a11y-final-check.log`.
- Separate strict all-target Clippy for `accesskit_atspi_common` passes;
  17 capture-harness Python tests pass; `git diff --check` is clean.
- `a11y-restored-actions.atspi.json`: native fixture 27 at 1600×1200/100%
  exposes 231 nodes and all 10 headings, reveals the final offscreen heading,
  retains heading IDs/order, contextual table headers/cells and side-by-side
  tables, list order and formula source. Task activation changes exact source;
  Undo restores source and task state. Final source SHA remains
  `a6a2ca1d038460fac618de55484181e7688ede02c013f155e1345adfb0b08f9a`.
  The original-size native screenshot was inspected. This bounded fixture is
  not a substitute for complete large-document performance/interaction coverage.
