# Code line-number gutters — checkpoint

Board 04's code-number variant is implemented and natively inspected. This
checkpoint does not approve the complete technical board or document grammar.

Release SHA-256:
`35792f9b206c518d79340aa1213c912bfdf5e35beb834dec81d0417129726e5f`.
Fixture: `layout-fixtures/61-code-gutters.md`, SHA-256
`cd8661990f047778721b9b9fbbd6d3f4a102879fdd338b096405c0bdd3d169eb`.

## Behavior and geometry

- Four or more physical source lines nominate automatic numbering. Short
  commands remain unnumbered; math source uses its existing separate treatment.
  No Markdown annotation or user-facing layout preference is added.
- Blank lines count. The caret-only line after the final source newline has
  no invented number. Fenced and indented code use the same rule.
- Native mono digit measurements reserve the rail before painting. One spare
  digit keeps ordinary 9→10 and 99→100 transitions stable. Labels are right
  aligned at the code baseline, with a 16 logical px number-to-source gap and
  a fine separator halfway across that gap.
- Source, hit testing, selection and horizontal-scroll extents use the reduced
  text viewport. The number rail and language/Copy header stay fixed while
  source scrolls. The trailing source allowance remains 16 logical px.
- Typing retains the active gutter through local row refresh, full refresh and
  asynchronous reflow commits. Zoom scales the retained geometry; leaving the
  active block allows the automatic choice to be measured again.
- Numbers are paint-only decorations, not document nodes, source edits,
  clipboard prefixes or extra assistive-reading items. Table measurement also
  includes the gutter, without giving code inside a table a second scroll owner.

The native-desktop design skill guided baseline alignment, restrained rails,
contrast and narrow/zoom checks. The Rust skill guided source ownership,
shared measured geometry, regression tests and release validation.

## Native visual and interaction evidence

- [Wide](layout-previews/gutters-wide.png): measured unequal-width example
  cards, natural heights, light/dark panes, two-digit alignment and compact
  unnumbered command. No forced equal-height card bottoms.
- [Narrow](layout-previews/gutters-narrow.png): 600 × 1100 stacking; title,
  numbers and source retain their hierarchy and insets.
- [Request/response](layout-previews/gutters-exchange.png): explicit authored
  labels and methods remain attached to four-line numbered JSON payloads.
- [200% before](layout-previews/gutters-200-code-before.png) and
  [after horizontal scrolling](layout-previews/gutters-200-code-after.png):
  fixed rail, reachable line endings and unscaled one-pixel separator.
- [Raster checks](layout-previews/gutters.pixels.json): rail crop is identical
  before/after horizontal scrolling while source pixels change. Wide cards
  have 24 px inter-card gaps and code insets, Paper/Surface/Code-dark tokens,
  sampled one-pixel rules. Number contrast is 4.90:1 light and 7.60:1 dark.
- [Native edit/undo](layout-previews/gutters-edit.edit.json) and
  [border probes](layout-previews/gutters-edit.layout.json): pointer typing
  edits the intended TypeScript field, autosaves, and exact undo restores all
  bytes. Both measured card leading edges stay unchanged.
- [Whole-document copy](layout-previews/gutters-copy.copy.json): exact match
  against the independently authored plain-text golden, including blank code
  lines, in source order, without visual numbering.
- [Weston MCP evidence](layout-previews/gutters.atspi.json): exact first-pane
  clipboard payload, canonical source-only accessible code nodes and named
  Copied feedback. Real pointer hover exposed the Copy tooltip; a bounded frame
  series showed the Copied checkmark. Initial zero-refresh provisional frames
  were allowed to settle before judging arrangement.
- [Warm geometry](layout-previews/gutters-copy.planning.json): two requested
  warm replans pass the cache/anchor oracle, without new segment layout,
  wrapping, shaping or anchor displacement.

All linked capture `.source.json` checks pass. `gutters-200.png` and
`gutters-200-pane.png` are positioning captures, not full code-pane evidence;
the earlier `gutters-200-scrolled.png` targeted prose and does not prove code
scrolling. The corrected `gutters-200-code-*` pair is authoritative.

## Tests and performance

`cargo test --workspace --all-targets --locked`: **478 passed, 2 ignored**.
Four new tests cover eligibility/blank lines, the previously missing rail,
native 100/130/200% scroll geometry and exact copy, and typing across 3→4 and
9→10 in both standalone and card-contained code. The initial rail regression
failed before implementation. Workspace Clippy with `-D warnings`, formatting,
diff whitespace and the release build passed. Crusty validation reports no new
or worsened architecture findings (36 existing findings remain advisory).

[10 MiB continuous-scroll profile](layout-previews/gutters-perf-continuous.json):
60 seconds, 1728 × 1080, configured 120 Hz, **109.55 FPS average**, p99 draw
**5.52 ms**, p99 presentation **10.50 ms**, p99 input **10.07 ms**. Gate passes;
zero application stalls ≥25 ms. Three presentation intervals exceed 25 ms,
so this is not an every-frame >60 FPS or 120 Hz stretch-target claim.

That stable mixed fixture contains one-line code, not numbered panes. The
[numbered-specimen profile](layout-previews/gutters-specimen-perf.json) independently
passes a 60-second run at **109.80 FPS**, p99 draw **3.54 ms**, p99 presentation
**10.47 ms**, p99 input **9.83 ms**, with no application or presentation stalls
≥25 ms. It is not a substitute for a 10 MiB numbered-code-heavy or full-grammar
performance matrix. The known wheel-profiler input-attribution limitation
remains; no gate is weakened.

## Remaining work

The full board/state/RTL/table-cell and extreme line-count-growth matrix is
not approved. The extra terminal caret-row footer and pointer Copy retargeting
found in this release are fixed in the later
[code-pane checkpoint](CODE-PANELS-CHECKPOINT.md), with native visual,
selection, keyboard endpoint, exact copy/undo and cache regressions.
Print numbering and continuation labels
belong to the still-open pagination work. See DESIGN-GRAMMAR-COVERAGE.md for
the unchanged full objective and remaining component families.
