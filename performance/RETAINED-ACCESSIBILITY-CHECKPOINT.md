# Retained accessibility checkpoint — September 11, 2026

## Result

The complete semantic document is retained across scroll frames. The release
binary publishes no unchanged descendants, keeps the full offscreen tree
available after activation and reconnect, and avoids reconstructing the native
document text during a pure scroll. Final large-document binary SHA-256:
`1e76cb31f3bb225ecead48a981eca91c37ba870f731eb8fb29b7a3c658512017`.

This closes the isolated 100 KiB regression and the bounded 1 MiB and 10 MiB
active-client gates demonstrated by the September 10 baseline. Full release
qualification still requires the physical Mutter repetition matrix and its
stricter frame-tail limits.

## Ownership and update contract

- `RetainedA11ySubtree` validates one immutable, complete hierarchy once.
  Initial publication, replacement, reattachment, and every inactive-to-active
  transition emit the full baseline. An unchanged frame emits no descendants;
  property-only overrides emit only their nodes.
- The document view retains all canonical offscreen nodes, action identities,
  labels, native table/list/math structure, and local bounds. Dynamic math
  scroll properties are the only descendant overrides.
- A canonical full-document `TextRun` remains first in the retained document,
  with an empty terminal run that bounds both ends of AccessKit's text iterator.
  The AT-SPI adapter detects text capability with a forward traversal, skips
  unchanged retained text, and suppresses only wholesale text events larger
  than 64 KiB; normal Unicode insertion/removal events remain exact.
- A transparent `GenericContainer` owns the live scroll/scale transform. Its
  retained `Document` child owns text ranges. AT-SPI forwards one bounds
  invalidation to the exposed document when that transform changes, without
  concatenating descendant text.
- The debug accessibility snapshot merges deltas into a complete retained
  snapshot through a persistent ID index. An unchanged transform delta scans
  zero retained nodes and reuses unchanged property storage.

## Native correctness

Fixture 27 at 1600×1200 and scale 120 exposes 243 native nodes and all ten
headings. It retains exact heading IDs and source order after scrolling, table
names and header/cell hierarchy, side-by-side table semantics, list order,
formula source, task activation and exact Undo, and a complete tree after
reactivation. The source hash remains
`a6a2ca1d038460fac618de55484181e7688ede02c013f155e1345adfb0b08f9a`.
The screenshot `layout-previews/retained-a11y-wrapper-smoke.png` was inspected.

## Matched isolated timing

All runs used the exact 100 KiB generated source hash
`4ec95d32761f4c47cde245d8d98418e4512d7d15a5cfe9d370604fb1c54ce00c`,
1728×1080 at scale 120 and 120 Hz, 15-second configured warmup, continuous
private-seat input, active private AT-SPI, 8,286 accessible nodes, and no
concurrent build or capture.

| Run | Duration | Presented FPS | Draw p99 | Input p95 | Missed deadlines | App stalls ≥25 ms |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `wrapper-100k-1` | 10 s | 99.3 | 14.57 ms | 14.82 ms | 8.79% | 0 |
| `wrapper-100k-2` | 10 s | 103.2 | 12.43 ms | 11.81 ms | 3.00% | 0 |
| `wrapper-100k-3` | 10 s | 99.3 | 13.28 ms | 12.49 ms | 6.22% | 0 |
| `wrapper-100k-60` | 60 s | 101.6 | 12.39 ms | 12.05 ms | 3.11% | 0 |

All four pass the isolated diagnostic gate of more than 60 FPS and draw p99
below 16.67 ms. The prior locked baseline measured 37–50 FPS and roughly
30–37 ms draw p99. The retained implementation before separating geometry from
text measured 78.9–92.6 FPS and 14.83–17.76 ms draw p99; the latter missed the
tail gate in two of three runs. The final structural separation passes in every
matched run, including 60 seconds.

The stricter release criteria remain draw p99 at most 6 ms, input p95 at most
16.7 ms, missed deadlines below 0.1%, and five 60-second physical-session runs
per size. These isolated results do not meet the draw or deadline limits and do
not replace that matrix.

## Large-document active-client timing

Both final runs used 1728×1080 at requested scale 120, a private Weston seat
and private AT-SPI bus, continuous input, exact-source verification, and a
10-second steady measurement after the complete generated layout was committed.
The activation probe checks the exact direct-root count and hashes 23
representative roots across the beginning, middle and end.

| Source | Direct roots | Accessibility ready | Presented FPS | Draw p99 | Source |
| --- | ---: | ---: | ---: | ---: | --- |
| 1 MiB | 3,337 / 3,337 | 5.32 s | 107.5 | 9.99 ms | exact |
| 10 MiB | 33,268 / 33,268 | 39.52 s | 108.2 | 9.76 ms | exact |

Artifacts are `layout-previews/retained-a11y-1m-fixed.*` and
`layout-previews/retained-a11y-10m-fixed.*`. Both pass the isolated gate of
more than 60 FPS and draw p99 below 16.67 ms. Readiness includes initial parse,
layout, retained-tree construction, transport and bounded client activation;
it is kept separate from steady scrolling.

## Automated evidence

- GPUI retained-subtree tests: 3 passed, including zero unchanged clone/scan
  work, override deltas, detach/reattach/replacement/activation, and invalid
  hierarchy rejection.
- Accessibility publication tests: 6 passed against the real AccessKit
  consumer, including complete retained debug snapshots and zero delta index
  scans.
- Document-view accessibility tests: 13 passed.
- Vendored AT-SPI adapter tests: 8 passed. The 3,000-run transparent geometry
  regression performs zero document comparisons and emits one document bounds
  invalidation; simultaneous Unicode edits and scroll still emit exact removal
  and insertion notifications.

## Remaining qualification boundary

The bounded activation sample proves the direct-root cardinality and
representative semantics without making exhaustive client traversal part of
the frame measurement. It does not replace audible Orca/MathCAT, braille,
high-contrast, reduced-motion, multi-scale, or repeated physical-Mutter runs.
The isolated draw p99 also remains above the stricter 6 ms release target.
