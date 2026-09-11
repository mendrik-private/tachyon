# Shared accessibility action routing

2026-09-10. Progress on Crusty `work_98682121afe7c2b7`; scrolling remains active.

The previous profile attributed 2.21% self samples to destruction of per-node
semantic action callbacks. The editor registered at least one boxed callback and
weak entity for each semantic target on every frame, including offscreen targets,
and cloned the target vector during publication.

The compiled semantic tree now owns an immutable shared target map. Publication
and paint share that map; one frame-local GPUI handler resolves exact target IDs
and supported actions. Node-specific listeners retain precedence, unowned actions
continue to other handlers and built-ins, and registrations expire at begin_frame.
Source/geometry cache invalidation rebuilds the map. Offscreen targets remain
present; link destinations still resolve from the live canonical document.

## Verification

`scripts/check.sh` passed: 786 Rust tests, two existing ignored tests, formatting,
locked checks, strict Clippy, adapter/publication suites and doctests. Log:
`/tmp/tachyon-action-check.log`. Release build log:
`/tmp/tachyon-action-release.log`.

Real private AT-SPI consumers exercised the release binary:

- `actions-document`: fixture 27, 1920x1100 / 100%; offscreen reveal, task click,
  exact source toggle and undo, stable heading identity/order, full reactivation.
- `actions-math-200`: fixture 40, 1280x1400 / 200%; MathML structure and native
  value action scrolling the overflowing formula from 0 to its maximum 1161px.
- `actions-gallery-small`: fixture 32, 1280x700 / 100%; image-link activation
  navigates to an initially offscreen destination, preserving source.
- `actions-html`: fixture 28, 1920x1100 / 100%; native disclosure open/close,
  content semantics and unchanged source through the existing node listeners.

Initial math and gallery runs at larger available dimensions failed fixture
preconditions (no math overflow and destination already visible), before testing
those actions. The corrected dimensions above exercise and pass both actions.
Every successful run has exact-source evidence beside its native artifact.
Artifacts live under `performance/layout-previews/` with the names above.

## Matched full-reference measurements

Baseline SHA-256, saved as `/tmp/tachyon-before-action-routing`:
`41e830f406194ffe1f513be7e878fe44c2e40b0376bf547ba3cad46acc7c6be1`.
Candidate SHA-256:
`61cef2e4561e086554ecb7a24e8b31b60d757cc4c4f51dd9712b4cd450ba60fe`.
Both use 21px body text and the retained debug-snapshot optimization. Rust 1.98,
locked release, layout-validation, thin LTO, one codegen unit, `-C strip=none`.

Protocol remains full `example/reference.md` (102,068 bytes), private authored
resource paths, Weston GL 1440x1000 / 100%, scale 1, requested 120Hz, active
AT-SPI, continuous native axis input every 12ms, 8-second directional sweeps,
32-second measurements. Warm runs traverse down and back before measurement.
All runs preserve exact source/resources and expose 3,056 initial accessibility
nodes. Source SHA-256:
`bf572f2b6d0226bfa2ee98bca2bf2c63faf8f232c68215937cb5e48deb143cce`.

Artifact prefix: `performance/layout-previews/reference-actions-`.

| Run suffix | Draw p50 / p95 / p99 / max ms | Input p95 / p99 / max ms | Missed deadlines | Draw stalls >=25ms |
| --- | --- | --- | --- | --- |
| before-first | 2.120 / 4.407 / 7.119 / 24.707 | 9.765 / 12.149 / 26.165 | 1.355% | 0 |
| after-first | 1.992 / 4.276 / 7.381 / 25.870 | 9.814 / 11.682 / 29.852 | 1.299% | 1 |
| before-warm | 2.114 / 4.276 / 5.083 / 12.149 | 9.699 / 10.494 / 16.032 | 0.057% | 0 |
| after-warm2 | 2.014 / 4.049 / 4.919 / 9.404 | 9.601 / 10.363 / 14.590 | 0.199% | 0 |

`after-warm` is excluded: CPU monitoring recorded unrelated cargo/rustc activity.
Its terminal run was not restarted until the build had ended; `after-warm2` is
the replacement. All four included runs contain no compiler activity. No builds,
other native captures or Crusty architecture analysis were started by this agent
during their measurement. These are bounded observations, not statistical release
qualification. Average application CPU over the final 32 one-second pidstat rows
is 35.03% -> 34.19% first traversal and 32.22% -> 30.38% warm (100% = one core).
Baseline process name is truncated to `tachyon-before-`; candidate is
`tachyon-markdow`. Do not combine unrelated process rows when computing CPU.

The change removes recurring callback allocations and reduces observed warm draw
work. First-visit tails do not improve in this pair. The candidate still fails the
strict missed-deadline limit even warm, and fails first-visit draw p99/stall limits.
The baseline warm run passing does not qualify either executable for the complete
physical-display protocol. The work item stays active; do not substitute the
capture harness's looser 60fps gate for `CURRENT-LAYOUT-PERFORMANCE.md`.

Local orchestration: `/tmp/run-reference-scroll-actions.py NAME [--baseline]
[--warm]`; strict assessment: `/tmp/assess-reference-scroll.py NAME`.
Existing `capture-layout.py` flags above reproduce the native protocol. The next
investigation should focus on first-visit work and deadline misses, keeping the
complete document and accessibility consumer active.
