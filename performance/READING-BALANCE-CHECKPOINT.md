# Reading-column balance — September 10

## Contract before implementation

Native unchanged `audit.md` at 1600×1800 shows a large accidental void below
the first Verdict and scope reading column. Fixture113 extracts the three
original paragraphs and heading, with a following-section boundary. No text
is shortened to force a preferred arrangement.

Reproduce the measured imbalance before choosing a fix. If a legal paragraph
split can substantially balance the columns, paragraph-boundary preference
must not prevent it. Preserve at least three paragraph lines on each side,
source order/coverage, readable width, native fonts, paragraph gaps and
complete following-section separation. A focused flow retains its current
source anchors while editing; improved balancing applies only when legal
reconsideration is allowed. Narrow and short surfaces remain single-column.

Acceptance includes actual-font geometry, same-source real-document native
before/after, width/zoom fallback, cross-column selection/edit/save/Undo and
the existing flow/figure/full workspace regressions. Full grammar and audit
scope remain active.

## Diagnosis and bounded change

GPUI's `TestPlatform::new` in the pinned dependency installs `NoopTextSystem`.
Registering bundled fonts in a `#[gpui::test]` does not make its wrapping native.
The initial mock test passed while the actual app remained visibly uneven;
it is retained only as a source/geometry regression, not font qualification.

Native shaping produces paragraph line counts 4, 5, 3 at the original measure.
With three-line guards, the middle paragraph cannot split. The legal split is
therefore 112 px left versus 248 px right, including the paragraph gap. This
is not an excessive paragraph-boundary scoring penalty.

Only when the original worst within-band height ratio exceeds 1.5, prose
measurement now tries 96%, 92%, then 88% of the original column width. The
first acceptable alternative must retain the existing measured 40-character
floor, reach a ratio at most 1.5, add neither occupied height nor band count,
and preserve all existing shaped-line/widow limits. Otherwise the original
flow is retained. Font size, global canvas, other block families, source
anchors and focused-flow retention are unchanged. Search adds at most three
bounded candidates and unchanged flows still reuse published measurements.

## Native evidence

- Baseline runtime: `cc1a4414c43ab89de9f237f8dd1e989a1ecd4db8e1e116b2931b6b6b31b1a1f5`.
- Verified runtime: `0e3e3df2078581918006c69c6fc077539c933c4192c16884c6f353769ca4aa8a`.
- Fixture113 SHA-256: `082549bfcc2cdf23abff2abcbc51878ad7400b14af93fc192c3a70b0b603043b`.
- `reading-balance-before` / `reading-balance-after`, 1600×1000 light:
  paragraph counts become 4, 6, 3; the middle paragraph splits 3+3. Column
  boxes become 220/192 px. The following heading moves from y408 to y380,
  reclaiming 28 px. The approximately 1109 px reading band uses 84% of the
  1314 px available document canvas, with a 24 px gutter and unchanged type.
- `reading_balance_check.py` independently counts actual screenshot glyph
  rows, not the union rectangle of the split paragraph. It fails on baseline
  (ink extents 116/252 px) and passes after (220/196 px). Ink ascenders account
  for the small difference from line-box heights.
- Exact original `audit.md` holdout SHA-256
  `cb4d2b22b05517c8283488fdf3fc72f52e7030f6dd1520f3db3172b611340345`:
  `audit-holdout-current` / `audit-holdout-balanced`, 1600×1800 light, reproduce
  the same improvement. Constraints heading moves y798→770. Private copies
  only; the original file is checked unchanged.
- `reading-balance-narrow` (520×1200), `reading-balance-short` (1600×480),
  and `reading-balance-200` (1600×1800, dark 200%) retain single-column flow.
- `reading-balance-three-control`, fixture65 at 2000×1800 light, retains
  three aligned reading columns spanning the 1712 px document canvas; the
  shorter next section still uses two columns and explicit hard breaks stay
  stacked. Native source/appearance checks and visual review pass.
- `reading-balance-cross-selection` verifies exact native drag + keyboard
  extension/copy across the newly introduced column break.
- `reading-balance-right-verified` types at the continuation's start, checks
  complete autosaved Markdown, focused idle and blur, then exact byte Undo.
  The first exploratory expectation omitted the serializer's existing
  punctuation escaping in the edited paragraph; the corrected golden includes
  those escapes and keeps all unrelated source bytes exact. No serializer
  behavior was changed to pass this layout check.

All listed native source and appearance checks pass; screenshots were reviewed.
Three additional Rust tests cover captured native line-count constraints,
multi-band height scoring and the exact real-text fixture's source geometry.
The existing focused editing, warm reuse, figure, width and zoom tests remain
required. This is one continuous-reading improvement, not qualification of
the full grammar, native font/state matrix, paged layout or export.

`scripts/check.sh` passes: formatting, workspace check, warning-free Clippy,
729 Rust tests with two existing ignored tests, and doc tests. Final log:
`/tmp/mineral-reading-balance-final-check.log`. Crusty validation against
`ctx_83866e87fe70` reports 37 existing architecture findings with none new,
worsened or resolved. A07 and the wider audit remain active.
