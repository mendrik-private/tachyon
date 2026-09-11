# Optional guidance rows — September 10

## Contract and implementation

The previous goal turn made verified progress on compact table tracks. This
turn keeps the layout-first priority and inspects current whole reading and
product-specification specimens. Fixture01 exposes two short optional callouts
stacked in half a wide page. Preserve their source, ordinary presentation and
reading width, but let complete adjacent guidance use the available second track.

`adaptive/rows.rs` adds a measured Guidance row for two source-adjacent Note/Tip
alerts in the same section. Each contains one or two bounded paragraphs. A
shared prefix heading remains full-width. Intervening prose, different sections,
nested lists, oversized content and Important/Warning/Caution notices do not
qualify. Critical conditions and their attached actions keep their existing flow.

The row uses equal tracks capped at twice the existing standalone reference
reading measure plus the standard24px track gutter. Both tracks must retain
that complete measure. Existing loaded-font measurement, overflow, viewport-height
and1.5 height-balance guards still apply. Each callout owns its existing panel,
label, icon, color and padding; there is no duplicate outer card. Native
measurement and final geometry share the same slots. The new diagnostic role
is `guidance_row`, reason `ADJACENT_OPTIONAL_CALLOUTS`.

Focus locks retain the pair through typing growth, including when bounded
measurement can no longer nominate it. Blur releases the stale arrangement;
width/height reduction stacks it and restoration reconsiders the pair. The
existing full-width heading-focus exception remains topology-based. No new
renderer, dependency, source conversion or user-facing layout selector.

The app-UX skill drove the choice to preserve normal typography and callout
presentation. Native review rejected the first implementation's narrower
preferred-width tracks: they stacked labels above the body and left excess outer
space. The accepted version uses the same maximum reading measure as standalone
callouts. The Rust skill guided bounded fit, source ownership and regression tests.

## Native evidence

Before: `layout-previews/layout-review-reading`,1600×1600 light, binary
`565b0ecc9667d6fd932ab5ee749c21815762303bd27a58d8e085f66f8887c094`.
Final binary:
`2b6ea049ebea8b4409bccaf74032cfda57b5e02665ba804d67f7f7c938b23432`.
Fixture01 source remains unchanged:
`a0f5b3458c558ff21d5fc8267e5747c8383c073de8e0f49a8760eafd27491834`.

- `guidance-verified-wide`: the Note's semantic rectangle is exactly unchanged
  at x259,y1073,width639,height48. The Tip retains its639×48 rectangle and moves
  from x259,y1181 to x922,y1073. Both visible panels remain631×84 with leading
  icon/label rails, not stacked labels. Their row spans99.09% of the1314px canvas.
  The next section moves up **108px**. Earlier title/prose/quote geometry stays
  unchanged. The inherited8px trailing inset makes the painted panel gap32px;
  semantic track gutter remains24px.
- `guidance-verified-regular`1280×1600, `-narrow`768×1600 and `-200`1600×1800
  dark at200% text correctly stack. All **29 canonical document nodes**, their
  IDs, parents, names, roles, descriptions and actions match the baseline.
- `guidance-verified-short-settled`,1600×480 light, scroll11steps: both complete
  callouts and their shared heading remain visible and aligned. The initial
  `-short` sample was taken during remaining scroll coast; its sequential AT-SPI
  queries differed by2px. Repeating with the existing3-second scroll-settle option
  gives matching geometry and pixels without weakening the alignment oracle.
- `guidance-verified-product`: the complete unchanged product specification
  preserves all canonical semantics and heading/prose/table/code/notification
  geometry compared with `layout-review-product`. Its critical notice is unchanged.
- `guidance-verified-edit`: native pointer/Home placement, typing `x`, one second
  of focused idle, blur and autosave match a complete-file golden. Only the Note
  paragraph changes, including required literal punctuation escaping. First changed
  byte1163. Undo restores the exact original source bytes.

Original-size wide, first-candidate and settled-short screenshots were inspected.
The narrow/large-text fallback checks include complete semantic bounds; this is
not a claim that every offscreen glyph in those screenshots was inspected.
All interactions use disposable fixture copies on the private Weston seat.

## Verification and remaining gaps

Two Rust regressions cover full fixture placement and independent cases at
100/150/200% measurement scales: both source orders, shared heading, critical
types, section/prose barriers, nested content, oversized and uneven paragraphs,
width/height fallback and recovery, large focused growth with complete painted
source-range coverage, blur release and exact Undo. GPUI mock shaping is topology
evidence, not native-font proof. The initial fixture regression failed before
implementation; its first root-ID lookup was subsequently corrected to address
the callout's projected paragraph, and its canvas now derives from mock metrics.

`scripts/check.sh` passes locked metadata, formatting, workspace/all-target check,
strict Clippy, **756 Rust tests**, two existing ignored tests and doctests. Log:
`/tmp/tachyon-guidance-check.log`. The additional within-bound uneven-height case
passes in `/tmp/tachyon-guidance-boundaries-final.log`. `git diff --check` passes.

`guidance_row_check.py` checks all29 canonical nodes, unchanged individual
dimensions, source identity,108px saving,24px tracks,99% utilization, actual
semantic-color fills and every painted body line in wide/short states. The
baseline-as-after negative control fails. Shared canonical-tree extraction now
accepts an explicit specimen minimum, retaining41 as the default for existing
large fixtures; the prior compact-table verifier still passes with that helper.

`guidance-row-wide` is the rejected preferred-width candidate, not final evidence.
Full C01 nesting/title/rich-content coverage, multi-script direction, native
continuous resize/edit bursts, wider multi-notice compositions and release
performance remain open. The full audit goal is not complete.

Crusty validation `task_a35d2918000acbbc` for `ctx_d652e3039f34` completed with
75 existing advisory findings and zero new, worsened or resolved findings.
A07 remains active with this checkpoint appended as evidence.
