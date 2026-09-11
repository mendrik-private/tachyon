# `reference.md` scroll-stutter final evidence

Status: complete. The repeatable cold-scroll stall is removed from the exact final release binary while the full native accessibility tree remains active.

## What changed

- Workspace persistence now coalesces scroll-driven `ViewChanged` events behind one debounce wake. The window is notified only when the active outline heading actually changes.
- Local Markdown and inert-HTML image resources are discovered up front and prefetched through the existing bounded image cache, with at most 32 resources and four concurrent loads.
- The Wayland AccessKit publisher retains omitted incremental subtree nodes, evicts explicitly detached subtrees, preserves unchanged nodes moved to a new parent, and republishes reused IDs correctly.
- Large geometry-only reflows below a document still update the exact AccessKit consumer tree, but emit one document bounds notification instead of thousands of redundant per-node AT-SPI events. Text, state, focus, and topology changes keep their normal event paths.

## Protocol

The fixture was the exact 102,156-byte [`example/reference.md`](../example/reference.md), SHA-256 `def4e0898878adf5f62eb0a38827139629af2eb7014028569dc5c601489f41ab`. The final stripped release executable was `target/reference-scroll-prefetch/release/tachyon`, SHA-256 `b49efbde2fcf8613c5b08a4c75b3694fbcbdbb3871524de0e42b3daf101c4284`.

Each final run used an isolated Weston 14 headless GL output at a requested 120 Hz, a private Wayland seat, a private session bus, and an active AT-SPI tree. The driver sent continuous native axis deltas every 12 ms and completed alternating eight-second full-document sweeps for 32 measured seconds. Source-resource hashes and exact post-run source preservation are embedded in every report. The user explicitly waived the physical desktop/input run because it prevents concurrent work; no physical input or desktop session was used for this qualification.

## Results

The original cold build exhibited two application stalls at or above 25 ms, with a 35.62 ms draw maximum and 41.75 ms maximum input latency. Both exact-final cold repeats have zero application stalls:

| Run | Viewport / display scale | Draw p50 | Draw p95 | Draw p99 | Draw max | ≥25 ms app stalls | Input p95 / max | Missed 120 Hz deadlines | Presented FPS |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| [Original cold](layout-previews/reference-current-first.json) | 1440×1000 / 100% | 1.52 ms | 2.31 ms | 2.98 ms | 35.62 ms | 2 | 9.62 / 41.75 ms | 0.34% | 108.5 |
| [Final cold 1](layout-previews/reference-final-verified-cold1.json) | 1440×1000 / 100% | 2.13 ms | 3.11 ms | 4.35 ms | 20.15 ms | 0 | 9.60 / 25.05 ms | 0.60% | 108.7 |
| [Final cold 2](layout-previews/reference-final-verified-cold2.json) | 1440×1000 / 100% | 2.06 ms | 3.12 ms | 4.16 ms | 14.02 ms | 0 | 9.56 / 21.51 ms | 0.26% | 108.8 |
| [Final narrow](layout-previews/reference-final-verified-narrow-100.json) | 720×1000 / 100% | 1.28 ms | 2.35 ms | 3.16 ms | 14.43 ms | 0 | 9.48 / 22.38 ms | 0.23% | 109.0 |
| [Final 125% display scale](layout-previews/reference-final-verified-wide-150.json) | 1440×1000 / 125% scale | 2.06 ms | 3.09 ms | 4.36 ms | 15.93 ms | 0 | 9.58 / 18.69 ms | 0.26% | 108.8 |
| [Final 200% display scale](layout-previews/reference-final-verified-wide-200.json) | 1440×1000 / 200% scale | 1.95 ms | 3.10 ms | 5.41 ms | 15.40 ms | 0 | 9.55 / 21.81 ms | 0.46% | 108.7 |
| [Final 150% document zoom](layout-previews/reference-final-verified-zoom-150.json) | 1440×1000 / 150% zoom | 1.82 ms | 2.68 ms | 3.08 ms | 10.52 ms | 0 | 9.60 / 18.43 ms | 0.14% | 108.5 |
| [Final 200% document zoom](layout-previews/reference-final-verified-zoom-200.json) | 1440×1000 / 200% zoom | 1.73 ms | 2.76 ms | 3.64 ms | 16.19 ms | 0 | 9.61 / 20.79 ms | 0.26% | 108.7 |

All final reports pass the repository gate of more than 60 presented FPS, draw p99 below 16.67 ms, and nonzero native input-latency samples. They preserve the source hash and report an active accessibility tree: 3,060 nodes at the wide viewports and 3,043 at the narrow viewport.

## Diagnosis and validation

The symbolized [`reference-activation-warm-profile.perf`](results/reference-scroll-2026-09-11/reference-activation-warm-profile.perf) localized the repeatable long main-thread interval to `accesskit_consumer` hidden-node filtering and AT-SPI event delivery after a semantic geometry refresh. The aggregate baseline profile is [`reference-current-profile.perf`](results/reference-scroll-2026-09-11/reference-current-profile.perf); the narrow post-fix profile is [`reference-final-narrow-profile.perf`](results/reference-scroll-2026-09-11/reference-final-narrow-profile.perf).

[`scripts/check.sh`](../scripts/check.sh) passes in full; its retained output is [`reference-final-check.log`](layout-previews/reference-final-check.log). This includes formatting, workspace checks, warning-denied Clippy, 596 document-view tests, 48 app tests, 13 AccessKit AT-SPI tests, 10 incremental accessibility-publication tests, and doc tests. Focused regressions cover debounce wake coalescing, large document geometry notification coalescing, retained incremental omissions, subtree removal and ID reuse, and moving an unchanged subtree out of a removed parent.
