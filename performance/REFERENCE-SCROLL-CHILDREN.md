# Linear native child-change notification

2026-09-10. Crusty `work_98682121afe7c2b7` remains active. This is a bounded
algorithmic improvement, not completion of the strict scrolling contract.

## Evidence and change

The fresh `reference-actions-profile` capture of the shared-action baseline
collected 6,078 perf samples, no lost samples, approximately 37.56 billion cycles.
Profiled timings are excluded from comparisons. Self-sample shares include
synthetic semantic publication 9.89%, memmove 8.35%, memcmp 4.77%, and adapter
node_updated 2.53%, alongside consumer text traversal and raster work. These
aggregate samples do not isolate the cause of individual slow frames.
Raw profile: `performance/results/reference-scroll-2026-09-10/reference-actions-profile.perf`;
decoded report: `/tmp/mineral-actions-profile.txt`.

Source inspection found `NodeWrapper::notify_children_changes` doing two nested
linear membership scans over filtered child lists even when only the document's
scroll transform changes. This costs O(n²) for n unchanged direct children.
The candidate first compares the complete filtered vectors, returning on equality.
For changed vectors it uses sets for membership and retains ordered vectors for
notification order and added-child indexes. Expected membership cost is O(n).
It still evaluates filtering on both snapshots, so unchanged raw children with
changed hidden states remain observable. Pure reorders retain the adapter's
existing no-add/remove behavior. Text-change checks remain intact; this does not
reintroduce the previously rejected text-notification shortcut.

## Correctness

`filtered_child_notifications_preserve_order_indexes_and_visibility_changes`
exercises real NodeWrapper notification and adapter callback events: mixed
insertion/removal/reorder, hidden-state changes with unchanged raw child IDs,
and 3,000 unchanged children during parent translation. Existing Unicode text
notification tests also pass. This is an event-correctness test, not a timing
oracle. `scripts/check.sh` passes 787 Rust tests, two existing ignored tests,
formatting, locked checks, strict Clippy, adapter/publication suites and doctests.
Logs: `/tmp/mineral-child-test.log`, `/tmp/mineral-child-check.log`.

Native `children-document` (fixture 27, 1920x1100) passes offscreen reveal,
canonical identity/order, task toggle, exact undo and complete reactivation.
`children-html` (fixture 28, same dimensions) passes native disclosure open/close
and content semantics. Both preserve exact source. Artifacts and AT-SPI/source
sidecars are in `performance/layout-previews/`.

## Full-reference first traversal

Baseline SHA-256 (`/tmp/mineral-before-child-diff`):
`61cef2e4561e086554ecb7a24e8b31b60d757cc4c4f51dd9712b4cd450ba60fe`.
Candidate SHA-256:
`b52b9dee36ead964b2d02b81545b81bb25ae451362852db455cf0b0766c142e2`.
Both are locked Rust 1.98 release, thin LTO, one codegen unit, layout-validation,
`-C strip=none`, 21px body text, retained debug snapshot and shared action map.
Release log: `/tmp/mineral-child-release.log`.

Protocol: full example/reference.md, exact authored relative resources, private
Weston GL 1440x1000 / 100%, requested 120Hz, scale 1, active AT-SPI, continuous
native axis events every 12ms, eight-second directional sweeps, 32 seconds of
measurement after startup. No warm traversal. Document SHA-256:
`bf572f2b6d0226bfa2ee98bca2bf2c63faf8f232c68215937cb5e48deb143cce`.
The exact 102,068-byte source and 3,056 initial native nodes / 327 headings remain
present. No accessibility suppression or viewport-only semantic tree is used.

| Clean run | Draw p50 / p95 / p99 / max ms | Input p95 / p99 / max ms | Missed deadlines | Draw stalls >=25ms |
| --- | --- | --- | --- | --- |
| reference-children-after1 | 1.801 / 3.930 / 7.102 / 19.497 | 9.798 / 11.706 / 25.084 | 1.271% | 0 |
| reference-children-before2 | 1.842 / 4.186 / 7.401 / 20.906 | 9.814 / 11.674 / 26.460 | 1.354% | 0 |

The included pair ran in candidate-then-baseline order, with no compiler activity
in CPU monitoring and no concurrent agent-owned build, capture or architecture
analysis. Average application CPU over the final 32 one-second rows is 31.90%
candidate versus 33.00% baseline (100% = one logical CPU). This single clean pair
suggests modest savings; it does not establish reliable p99 improvement.

`reference-children-before1` and `reference-children-after2` are excluded because
CPU monitoring recorded competing cargo/rustc activity. The repeat had six draw
stalls and 13.738ms p99 under contention; those numbers are not attributed to this
change. Cargo PID 2470655 remained live after the repeat, so no further timing run
was started during that build. Source/semantic checks passed even in excluded
runs. All artifacts, including excluded runs, are retained for inspection.

Both clean runs fail strict draw p99 <=6ms and missed deadlines <0.1%. No warm,
physical-display or broad responsiveness qualification is claimed here. Keep
scrolling open and retain the full `CURRENT-LAYOUT-PERFORMANCE.md` contract.
Further investigation must isolate slow-frame work rather than infer its cause
from aggregate profile shares alone.

Local orchestration: `/tmp/run-reference-scroll-children.py NAME [--baseline]`;
strict assessor: `/tmp/assess-reference-scroll.py NAME`. Native harness flags
above reproduce the protocol; artifacts use `performance/layout-previews/`.
