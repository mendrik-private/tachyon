# Reference-document scroll investigation — 2026-09-10

Crusty work: `work_98682121afe7c2b7`. This concerns the complete
`example/reference.md`, not a generated or shortened substitute. The physical
Mutter release protocol remains distinct from these isolated Weston checks;
do not close the full qualification on the diagnostic harness gate alone.

## Workload and environment

- Source: 102,068 bytes, SHA-256
  `bf572f2b6d0226bfa2ee98bca2bf2c63faf8f232c68215937cb5e48deb143cce`.
- Working revision: dirty `3edaf925194624496c656552f2055a11f4e42c94`.
  Earlier layout and table-history edits are preserved. Binary hashes identify
  the actual tested snapshots; the Git revision alone is insufficient.
- Rust 1.98.0; locked Zed `8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc`;
  release thin LTO/one codegen unit, `layout-validation`, `-C strip=none`.
- Ryzen AI Max+ PRO 395/Radeon 8060S; isolated Weston 14.0.2 GL renderer,
  requested 120 Hz, scale 1, 1440×1000 unless stated. Balanced power policy.
- Private native AT-SPI consumer active, initial complete reference tree 3,055
  nodes at 100% zoom. Markdown and explicitly named relative resource
  directories are copied into the private session with authored paths intact.
  Original/private source hashes and resource hashes are retained in JSON.
- Native continuous axis input every 12 ms, eight-second directional sweeps
  sized from the complete accessible geometry. First-pass measurement begins
  after a 15-second startup/AT activation allowance; warmed measurement starts
  after an additional down-and-back traversal. This is not OS-cold startup.
  F8 state samples record actual offsets at each reversal. The document shrinks
  from provisional geometry to approximately 91,497 px maximum scroll as
  measured regions replace estimates. Timed runs must have no concurrent builds
  or benchmarks; profiled runs are not timing qualification.

## Evidence and hypotheses

The baseline `reference-sweep-before.json` failed even the smaller isolated
diagnostic gate: draw p99 17.22 ms, worst draw 26.51 ms, input p99 21.18 ms.
Its pointer was centered; subsequent `reference-delta-*` comparisons use the
document's leading inset to avoid nested scroll handlers. Preserve that
distinction when comparing artifacts.

Baseline CPU samples: AccessKit consumer `State::update` 10.65%, allocation
7.79%, memory copy 6.42%, synthetic accessibility publication 3.63%, with
additional node traversal, destruction and tree cloning. Shaping was not a
leading sampled function. Stack unwinding is incomplete for optimized
dependencies; these are self-sample shares, not a complete causal call graph.
Raw profiles are retained locally under
`performance/results/reference-scroll-2026-09-10/` (Git-ignored).

A first adapter-boundary delta experiment preserved semantics but moved much
of the consumer's hot loop into two per-frame hash-map passes. Its matched
first-traversal draw p99 was 16.91 ms versus 15.63 ms baseline: no reliable tail
improvement. Warm p99 was 6.08 versus 6.80 ms. Do not present this first
experiment as the completed fix. `reference-delta-profile.log` then exposed
272 layout reports / 265 commits during first traversal, each publishing new
whole-document geometry. The profile also put the first delta loop at 11.33%
of CPU samples. The first candidate profile is retained beside the baseline.

## Implemented changes

1. Compare stable complete-tree traversal directly at the Wayland adapter
   boundary; build an ID lookup only for reordered/topology-changing entries.
   Publish only changed nodes, preserving all retained descendants and actions.
   Activation/reconnection forces a complete baseline under AccessKit's shared
   adapter lock. See `vendor/gpui_linux/README.tachyon.md` for the dependency
   contract and actual-consumer regression suite. This is not the broader
   retained-GPUI-subtree API: frontend assembly/debug copies remain.
2. Coalesce viewport planning requests into aligned batches of 16 existing
   semantic planning windows. At most 15 windows are added per edge. The
   per-window layout algorithm, source, first-paint path, cancellation,
   generation guards and scroll-anchor restoration are unchanged. Large
   documents remain scoped; there is no fixture recognition or hidden content.
   Batching is exclusively in asynchronous reflow. The first implementation
   also expanded synchronous structural-edit work; three existing tests caught
   that regression. Moving batching to the asynchronous call site restores all
   three original assertions without weakening them.
3. Extend the native harness with explicit private resource copying, full
   directional sweeps and separately warmed sweeps. Preserve timing failures
   and source-fidelity evidence independently.

## Reproduction

```sh
cargo rustc --release --locked -p markdown-app --bin tachyon \
  --features layout-validation -- -C strip=none
python3 performance/capture-layout.py --source-document example/reference.md \
  --source-resource-dir performance/layout-fixtures \
  --source-resource-dir performance/visual-assets \
  --binary target/release/tachyon --width 1440 --height 1000 \
  --perf-seconds 32 --perf-sweep-seconds 8 --perf-input continuous \
  --atspi-active --source-unchanged-check \
  --output performance/layout-previews/reference-scroll.png \
  --log-output performance/layout-previews/reference-scroll.log
```

Add `--perf-warm-sweep` for a separately warmed measurement, or
`--layout-trace details` for a diagnostic reflow-count run. Do not use the
instrumented run as the uninstrumented timing comparison.

## Verification

`scripts/check.sh` passes: formatting, locked workspace/all-target checks,
warning-free strict Clippy, 774 existing/workspace/adapter tests (two existing
ignored tests), four production-publisher consumer tests, strict Clippy for that
standalone suite, and doctests. The batching test covers all subranges of 101
planning windows and proves seven publications across a complete down-and-back
walk. The three synchronous edit-work-bound tests remain unchanged and pass.
The native harness unit suite passes 23 tests.

Crusty validation completed for `ctx_99c05ce1b589`, `ctx_13c06076b6a8` and
`ctx_36cf6c06ce11`: no new/worsened advisory architecture findings. Its published
semantic index remains unbuilt/stale; compiler/runtime evidence is authoritative.

Native fixture-27 checks at 100%, 150% and 200% verify complete headings,
canonical IDs/order, offscreen reveal, task action, exact saved source, one-step
Undo, restored task state and full accessibility tree after disabling/re-enabling
the private screen reader. `reference-final-atspi{100,150,200}.atspi.json` records
the final binary at 1600×1200, 2400×1200 and 3200×1400 respectively. Those widths
preserve this oracle's explicit side-by-side table assertion; they do not stand
in for narrow-window performance. The earlier `reference-fix-atspi*` files are
intermediate-binary evidence.

`reference-visual-{baseline,final}.png`: 1440×1000 opening viewport, both with
active AT and exact source/resources, is pixel-identical (zero changed pixels).
This is an opening-scene check, not a claim that every viewport was compared.

Final optimized executable SHA-256:
`8ce361c011b1ea8d10308f3e3c1c98b2c6e457417e36aab47a73c146d0f47ed2`.
Baseline executable SHA-256:
`3846748105f0def0f0441b7dc2fd46192cc66deddd4f5097b815838f3353cf9f`.

Final timing and representative-width evidence follow below. The work remains
open: the strict draw-p99/missed-deadline/zero-stall and physical-display
qualification have not been met by the measured first-pass runs.

## Measured result at 1440×1000 / 100%

All rows use complete reference bytes/resources and active AT. Times are ms.
`reference-delta-before1` and `reference-delta-warm-before1` are the matched
pre-change input-driver runs; `reference-final-first` and `reference-final-warm`
identify the final binary. Each measures 32 seconds. Later monitored final
runs had no compiler samples. The extra `reference-final-baseline` repeat is
**excluded** because its CPU log records another project's Rust compilation.

| Run | Draw p50 / p95 / p99 / max | Input p95 / p99 / max | Missed deadlines | Draw stalls ≥25 ms |
| --- | --- | --- | --- | --- |
| Before, first traversal | 2.91 / 8.40 / 15.63 / 33.88 | 12.44 / 18.63 / 35.52 | 171/3585 = 4.770% | 2 |
| Final, first traversal | 2.41 / 5.08 / 9.92 / 26.38 | 9.96 / 13.67 / 30.15 | 67/3550 = 1.887% | 2 |
| Before, warmed | 2.88 / 5.20 / 6.80 / 11.21 | 10.10 / 11.26 / 15.26 | 12/3527 = 0.340% | 0 |
| Final, warmed | 2.29 / 4.33 / 5.64 / 9.94 | 9.76 / 10.78 / 13.57 | 10/3523 = 0.284% | 0 |

These are observed comparisons, not statistical release qualification. The
earlier batched reading path measured first-pass p99 7.81 ms; the final repeat
was 9.92 ms, so do not claim a precise universal speedup. Final warm draw p99
meets 6 ms, but missed deadlines still exceed 0.1%, and first-pass draw stalls
remain. Average presentation rates are around 107–110 FPS on this isolated
Weston setup, not proof of physical 120 Hz presentation.

The final **profiled** 32-second traversal (`reference-final-profile.log`) has
47 layout reports / 45 commits versus the first experiment's 272 / 265.
Every committed anchor displacement is zero; largest measured commit is
0.521 ms. The largest background job is 163.68 ms; it is not a UI draw duration.
The final delta publisher falls from 11.33% to 2.68% of CPU self samples.
Allocation (7.86%), memory copy (7.13%), frontend synthetic publication (5.09%),
full debug-tree cloning (3.83%), and native text traversal remain visible costs.
Profiles have no lost samples, but optimized dependency unwinding is incomplete.

The next optimization should address retained frontend semantic/action/debug
publication and correlate the remaining first-visit stalls with per-frame
work. Do not disable accessibility, remove offscreen nodes, change layout
quality, weaken the thresholds, or report this issue closed.

## Interrupted quiet-repeat attempt

After the user agreed to a quiet build window, the planned stopping rule was
four alternating first-traversal runs (baseline, final, final, baseline), with
the same immutable executable hashes, full source/resources, active AT and
32-second input protocol above. The preflight process check found no builds.
However, both attempted runs recorded `rustc` CPU activity in their one-second
`pidstat` monitors: `reference-quiet-before1.cpu.log` at 13:26:06–13:26:21 and
`reference-quiet-after1.cpu.log` at 13:27:15–13:27:19 (Europe/Helsinki).
A subsequent live process check found renewed `nudge` builds under the
`target/agents/session-resume` and `target/agents/people-ui` directories.

Both `reference-quiet-{before1,after1}` timings are **excluded**, irrespective
of their results. The first failed the smaller diagnostic gate; the second
passed it, but neither is a controlled comparison. Original/private source
hash checks passed for both. The remaining two runs were not started because
competing compilation continued. No implementation was changed, no unrelated
process was suspended or terminated, and no new speedup or qualification is
claimed. The earlier results and open qualification status remain unchanged;
repeat only after the other build-producing sessions are actually paused.

## Representative width and zoom smoke measurements

Final binary, 16-second down-and-back traversals, 1000 px output height,
continuous native input, complete reference/resources and active AT. CPU logs
show no compiler activity. All private source checks pass and reversal samples
return to zero scroll. All 327 canonical reference headings retain exactly the
baseline order in every width/zoom tree snapshot. These runs pass the isolated diagnostic gate, **not**
the strict performance contract. No before/after speedup is claimed for this
shorter width/zoom matrix.

| Output width / zoom | Draw p95 / p99 / max ms | Input p95 / p99 / max ms | Missed deadlines | Draw stalls ≥25 ms |
| --- | --- | --- | --- | --- |
| 720 / 100% | 5.75 / 13.02 / 38.50 | 10.26 / 15.56 / 32.15 | 3.365% | 4 |
| 1440 / 150% | 4.98 / 11.30 / 28.44 | 10.12 / 15.75 / 33.65 | 2.913% | 2 |
| 1440 / 200% | 5.39 / 10.85 / 22.59 | 10.41 / 13.68 / 24.99 | 2.525% | 0 |

Artifacts: `reference-final-{narrow,zoom150,zoom200}` JSON, source evidence,
complete native tree snapshots, CPU logs and compositor logs under
`performance/layout-previews/`. First-traversal tails, particularly narrow
layouts, remain the primary unfinished result of this work.
