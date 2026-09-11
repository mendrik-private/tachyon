# Content-fit table widths — September 10

## Contract before implementation

Continue the layout-first audit without replacing readable tables with cards
merely to fill space. The current native 900×1600 light capture of fixture 76
shows `In review` split across two lines and without its pill, while the long
Purpose column receives most of the flexible width. Its source is unchanged;
baseline binary `2ab05dcadef4034961b8b15095dc2d804c0156e3a76354679279d9b9adc11e3f`.
Evidence: `layout-previews/entity-fit-before-intermediate.png` and adjacent
source/AT-SPI records.

Acceptance: measured compact columns reach their natural width when space
permits; long prose uses the remaining space and wraps normally. Never shrink
below measured word/code minima, exceed preferred widths to stretch a small
table, change explicit author widths, reorder cells, or change saved content.
Narrow record fallback remains an independent semantic decision. Measurement,
painting, table controls, selection and local edits must share the same widths.

First reproduce with pure intrinsic constraints and native-shaped fixture 76.
Then verify narrow/intermediate/wide and large text, asymmetric and equally
long columns, exact cell source coverage, focused growth and Undo. Run the
workspace checks and native captures before recording a result. Paired record
grids, generalized candidate scoring and all other ledger gaps remain open.

## Measured choice and final implementation

The previous goal turn made verified progress (supporting image pairs). This
turn first checked the live grammar/coverage and fixture 76. There was no
evidence justifying replacement of its intermediate table with records.

The native-shaper regression reproduced `In review` as two lines at 641
document units. A reduced constraint test reproduced disproportionate column
allocation. Candidate causes were allocation, measure/paint disagreement and
badge padding. Changing only allocation made both tests pass, ruling out a
paint-only defect, but native review rejected that general water-fill solution:
it expanded every short column and made the complete table 63 px taller.

The accepted change instead resolves the existing semantic badge before
intrinsic table fitting. In `editor/measurement.rs`, a recognized short badge
requests its complete native-shaped width plus the existing cell inset, just
as code requests its complete line. Ordinary text still uses word minima.
The existing proportional allocator, explicit-width overrides, record
eligibility, table locks and cache ownership are unchanged. No new heuristic
keyword list, source role, dependency, layout mode or scroll-path work was added.

The existing badge recognizer restricts this to explicitly labeled status,
state, classification and maturity values, including eligible property/value
rows. Long sentences and unsupported rich values remain normal text. Explicit
author widths still allow the source text to wrap and omit an unfittable pill.
Focused growth freezes the original columns, displays the complete new text,
and reconsiders its presentation after blur.

The final native 900×1600 result gives `In review` an 83 px column and one 21 px
text line. The first Purpose cell retains 329 px rather than the general
water-fill candidate's 271 px. Compared with baseline, one additional prose
line is necessary at this width: the following `Comparable capacity` heading
is at y=709, versus baseline 688 and rejected candidate 751. This is a
legibility correction with an explicit 21 px height tradeoff, **not** a claim
of reduced document height. Further measured line-count-aware allocation is
still open. Ordinary unbadged and explicit-width tables are unchanged.

## Verification

Final native binary SHA-256:
`d53f21db34b7d69e5e002f07f8caade15bbbd68a9cd475552964aff5d0047121`.
Fixture 76 SHA-256:
`5c607ea0536a188cd6591478010394890b1cc75612408eb1ace34ab19c7395c9`.

Three new native-font Rust tests cover:

- Fixture 76 source coverage at document widths 260/420/560/641/760/1280 and
  100/150/200% font environments; complete badges and intrinsic fit bounds.
- Status growth at 641/760/1280, unchanged locked column widths, exact
  edited-source equality, row-local versus complete geometry and exact Undo.
- Status/state/classification, bold text, ordinary-label and long-sentence
  negatives, plus an explicitly narrow authored column retaining exact widths
  and readable wrapping rather than clipping or silently widening.

`scripts/check.sh` passes locked metadata, formatting, all-target check,
warnings-denied Clippy, **721 Rust tests**, two existing ignored and doctests.
Final log: `/tmp/tachyon-table-balance-final-check.log`. The focused table run
passed 57 tests before the final semantics/explicit-width test was added; the
full suite includes it. `git diff --check` also passes.

The first generalized-width experiment had a Clippy range-loop failure in
its new test and an overbroad initial 260 px badge expectation. Both belong
to that rejected experiment; its algorithm and two tests were removed, not
promoted as final evidence. Logs and captures retain the diagnostic history.

## Native evidence

All accepted captures use prefix `layout-previews/table-balance-final-` and
the final binary above, with matching `.source.json`, `.appearance.json` and
`.active-atspi.json` records. Personally inspected:

- `intermediate`: 900×1600 light, complete four-row table and one-line status.
- `narrow`: 520×2400 light, complete records, schema header and empty steward.
- `wide`: 1600×1600 light, intrinsic table width and compact comparison tables.
- `dark200`: 1500×2600 dark at 200%, all four statuses and complete directory.
- `technical`: fixture 03 at 1280×1600 light, unbadged property/parameter tables,
  code pane and the opening of the wide comparison table (not its offscreen end).
- `edit-idle`: native status paste with its column held, complete text visible,
  neighboring columns unchanged and following rows moved down without overlap.

`edit.edit.json` proves native paste/autosave with whole edited-file equality,
one second of focused idle, blur and exact original bytes after Undo. It uses
private fixture copies, not a user document. The narrow corridor while focused
is intentional editing stability; it is not the settled presentation for a
long status sentence.

`short` also verifies the 900×480 light opening and contained table viewport;
it does not show the offscreen status rows. `copy.copy.json` verifies eight
unique source-order clipboard markers, including the status between its own
row and the next service. This is not whole-clipboard equality or rich-MIME
interoperability evidence.

Crusty validation `task_82b0a1dabe0841dc` for `ctx_29087bed86a1` completed:
37 existing advisory findings, no new, worsened or resolved findings. The
runtime/test evidence above is separate from that static advisory check.

The captures without `final-` use rejected intermediate binary `e8f7ec60…`.
They are diagnostic comparisons, not current-build approval. No complete
grammar family or performance gate is signed off by this checkpoint.
