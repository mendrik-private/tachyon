# Performance qualification

Measured 2026-09-06 with the optimized `mineral-perf` binary. Each row contains
30 paired samples. `Open + prepare` times `Document::from_markdown` plus
`PreparedDocumentView::prepare` in the same iteration, including the fixture
clone that hands ownership to the document. It is the CPU-side gate for a fully
editable projection and visual geometry; filesystem read, process launch, GPU
initialization, and compositor presentation are deliberately reported
separately.

Host: AMD Ryzen AI Max+ PRO 395, Linux 7.2.3, GNOME Wayland, built-in
2880x1800 display at 120.001 Hz and 166.67% scale, Radeon 8060S using RADV/Mesa
26.0.8, balanced power profile, Rust 1.98.0 (LLVM 22.1.8). Measurements were
made on the real Wayland desktop rather than a nested compositor.

| Fixture | Samples | Open + prepare p95 | Parse p95 | Prepared view p95 | Local edit p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 100 KiB | 30 | 3.54 ms | 2.99 ms | 0.62 ms | 0.0065 ms |
| 1 MiB | 30 | 72.14 ms | 53.81 ms | 21.35 ms | 0.0352 ms |
| 10 MiB | 30 | **961.14 ms** | 668.86 ms | 285.27 ms | 0.0569 ms |

The 10 MiB p95 satisfies the plan's one-second open-and-prepare gate. The
maximum paired sample was 1,024.32 ms and the median was 839.74 ms. The
ordinary edit measurement includes the persistent model transaction and the
localized prepared-view refresh; it no longer serializes, reparses, scans, or
copies the rest of the document.

Supporting metrics:

| Fixture | Projection p95 | Cold estimated layout p95 | Shared-cache layout p95 | Viewport lookup p95 |
| --- | ---: | ---: | ---: | ---: | ---: |
| 100 KiB | 0.30 ms | 0.77 ms | 0.51 ms | 0.0278 us |
| 1 MiB | 13.18 ms | 19.61 ms | 10.96 ms | 0.0556 us |
| 10 MiB | 149.76 ms | 264.31 ms | 183.57 ms | 0.0767 us |

Raw reports:

- [`core-2026-09-06-100k.json`](core-2026-09-06-100k.json)
- [`core-2026-09-06-1m.json`](core-2026-09-06-1m.json)
- [`core-2026-09-06-10m.json`](core-2026-09-06-10m.json)

Reproduce the CPU scenarios:

```sh
cargo build --release --bin mineral-perf
target/release/mineral-perf --bytes 102400 --runs 30
target/release/mineral-perf --bytes 1048576 --runs 30
target/release/mineral-perf --bytes 10485760 --runs 30
```

The separate end-to-end startup and 120 Hz compositor interaction matrix has
now been executed. See [RELEASE-QUALIFICATION.md](RELEASE-QUALIFICATION.md) and
the linked raw report. Startup and 10 MiB open/prepare pass, while the complete
sustained-interaction release gate remains failed; this CPU table does not
override that result.
