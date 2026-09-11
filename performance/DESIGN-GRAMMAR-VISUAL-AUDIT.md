# Document grammar — native visual audit

September 8, 2026. **Verdict: needs changes; not full grammar compliance.**

All seven design boards were inspected against the authoritative
[`document-design-grammar.md`](../designs/document-design-grammar.md). Their
generated lettering and geometry are illustrative; the written tokens and
semantic rules are the acceptance criteria. The renderer resembles the
references in its foundations and some components, not in its full vocabulary
or editorial/page composition capabilities.

This review uses actual native GPUI frames from an isolated Wayland compositor,
not a browser recreation or a generated mockup. Fresh captures use release
binary SHA-256
`1287a7e6f77fbbbdd246fb98c4ff606b98b1ea9b6dc077abb1b57df9082dc885`.
The working tree includes ongoing foundation changes. The earlier `audit-*`
captures use `65f6b09b…cc120` and are baseline evidence, not current-build passes.
An unqualified `font` call in the pending measurement work was corrected to
`gpui::font` so the current tree could build. No missing rendering family was
implemented merely to make this audit pass.

## Board-by-board verdict

| Board | Present / directly observed | Incomplete, missing, or not yet verified |
| --- | --- | --- |
| 01 · Foundations | Warm paper, serif hierarchy, sans reference text, serif narrative, mono literals; improved measured prose width and section rhythm | Metadata strips, superscript footnotes, bibliography and full reference styling; comprehensive inline/scale coverage |
| 02 · Lists | Open feature grids, numbered tiles, hanging bullet bodies, task icons and known-state progress; baseline vertical steppers | Aligned term–description rows, timelines, nesting guides/tree treatment; all rank/reference/stage variants and adversarial eligibility cases |
| 03 · Cards/signals | Five labeled callouts with appropriate icons and semantic tints; open vs enclosed layout; limited literal table statuses | Resource, decision, selected-option, contextual metric, valid/invalid and pros/cons families; general classification badges and swatches |
| 04 · Technical | Semantic tables, measured columns, light/dark code and Copy, real math rendering | Thin table rules fail; clean math presentation, code gutters, labeled request/response pairs, schema trees, narrow record variants, deterministic diagram/chart rendering and continuations |
| 05 · Editorial/media | Baseline full-image containment, paired figures and ordered galleries; native quote with authored attribution | True wrapping that returns below an object, flowing prose columns, anchored margin notes, caption/credit associations, bibliography, playable audio/video and map components |
| 06 · Page masters | Continuous native document with narrow reflow | Finite page masters, semantic page breaking, running furniture, contents/folios, repeated headers, oversized-component splitting and print translation |
| 07 · Navigation/states | Files/Outline, outer-edge scrollbar, native controls, disclosures, baseline formatting hover; current accessibility probe passes | On-page rail, contextual previous/next and breadcrumb grammar, contextual person/issue chips, complete focus/disabled/recovery-state matrix and static translations |

Tabs and a source pane are not missing product requirements: the user's
explicit no-tabs/no-source-pane decisions override those illustrative board
specimens. Likewise, completion checks, metadata, figures and dates must not
be invented merely to populate a template.

The missing-family conclusions are supported by the renderer inventory, not
just by an empty fixture. The adaptive row vocabulary is currently Stack,
Peer, IntroList, Explanation, Tables and Gallery. Its relationship model has
heading/content, prose continuation, adjacent explanation and consecutive
images. These primitives do not implement true wrap, footnote placement or
finite-page layout. The complete requirement inventory remains in
[`DESIGN-GRAMMAR-COVERAGE.md`](DESIGN-GRAMMAR-COVERAGE.md).

## Current measured results

### Improvements that pass these specimens

- [Relationships](layout-previews/review-current-relationships.png), 1600 × 1200,
  100% text and display scale: the current committed geometry has section
  heading bottom `252.5` and open row top `276.5`: **24 logical px**. The row
  ends at `517.3`; the following heading starts at `581.3`: **64 px**. The next
  section repeats 24 px after and 64 px before. Open row x is now **0**, matching
  the section edge, instead of the baseline's unwanted 24 px card inset.
  See the [native geometry](layout-previews/review-current-relationships.planning.json).
- The same frame's progress bar occupies y **425–432**; the first checkbox
  begins at **446**, leaving **13 clear raster rows**. The baseline left only
  two. Count and progress still derive from the actual two checked tasks out
  of four.
- [Reading](layout-previews/review-current-reading.png), 1600 × 1200: the five
  rendered lines of the second narrative paragraph contain **64, 64, 68, 70,
  56 characters**. This sample is within the grammar's preferred 55–75 measure.
  The baseline's 99–101-character lines do not describe the current output.
  This is an English specimen, not proof for every font, script and content.
- [Overview](layout-previews/review-current-overview.png): the six open features
  retain row-major order and no longer carry enclosed-card padding. Ordered
  stages retain authored numbers and a distinct enclosed treatment. Gutter
  and card-inset tokens are 24 logical px; labels and bodies share a hanging
  text edge.
- [Narrow signals](layout-previews/review-current-signals-narrow.png), 600 × 1100:
  all five labels, icons and full messages remain readable in stacked panels.
  Direct flat-pixel samples match paper `#FAF9F6`, Note/Important `#EDF4F9`, Tip
  `#E8EEE2`, Warning `#FFF4DF` and Caution `#FBEDEA` exactly. The visible panels
  retain small, consistent corners without stretched left-border joins.
- [200% text](layout-previews/review-current-200pct.png), 600 × 1000: the title
  and lead reflow without visible horizontal clipping; navigation is collapsed
  and the scrollbar stays at the outside edge. This frame does not verify
  every downstream component at 200%.

### Current failures and gaps

1. **Table rules are twice the specified screen thickness.** In the current
   reading frame, the `#DADDD5` horizontal rules at x=500 and x=600 occupy
   y=621–622, 660–661, 701–702, 742–743, 783–784 and 824–825: **2 physical px at
   1× display scale**, where the guide requires 1 logical px. The painter also
   handles every non-None border through one solid-edge path, so authored
   dotted/physical border semantics need review, not just a global color tweak.
2. **Footnote presentation is not implemented to the guide.** The fresh
   [footnote capture](layout-previews/review-current-footnotes.png) displays
   literal `[^measure]` on its own line, with an unlabelled definition later.
   It does not provide the required superscript reference, associated note
   treatment and return navigation.
3. **A labeled list is not yet an aligned definition list.** The relationships
   frame shows bold lead-ins above their explanations, with bullets. Board 02
   also calls for a genuine term–description alignment variant; this existing
   presentation is not evidence that the separate variant exists.
4. **Nested outlines lack the board's grouping guides.** Indentation is visible
   in the current relationships frame, but there are no aligned tree rails or
   deep-hierarchy presentation.
5. **The editorial and page grammar is substantially incomplete.** Adjacent
   section columns do not flow one prose section across columns. A paired
   image does not wrap text back beneath it. Neither implements margin notes,
   page masters, captions/notes attached across page breaks, or print output.
6. **Math/disclosure presentation is still utility-heavy.** The current native
   component frame shows the formula alongside a permanent “Math · source”
   panel and raw TeX; disclosures include an HTML utility strip. These do not
   reproduce the boards' restrained document treatments. The current component
   capture and accessible probe are linked below for review.
7. **Two layout regression assertions fail in the current tree.** These require
   investigation before sign-off; a successful screenshot or accessibility
   test does not supersede them.

## Verification scope and reproducibility

Fresh screenshot files are accompanied by `.source.json` records containing
the exact binary/fixture hashes and byte-unchanged checks. Fixtures were opened
from private copies; the user documents were not used as editable test targets.

The current [native component capture](layout-previews/review-current-components.png)
and [AT-SPI record](layout-previews/review-current-components.atspi.json) verify
165 accessible nodes, source-order/stable heading identities, table headers
and cells, list reading order, offscreen heading reveal, task activation and
byte-exact undo for fixture 27. They cover existing primitives only.

The [native feature-edit record](layout-previews/review-current-editing-inside.edit.json)
confirms typing inside a feature body, autosave and byte-exact undo. The
[copy-order record](layout-previews/review-current-editing-inside.copy.json)
confirms the tested markers retain canonical order across grid rows and the
ordered stages; this is a marker-order check, not full clipboard equality.
An initial probe clicked exactly before the target phrase, leaving that phrase
unchanged and failing the probe's interior-insertion precondition. Clicking
inside the phrase passed; the initial result is not counted as an editor defect
or as a successful edit/undo check.

Commands used include:

```sh
cargo check -p document-view --locked
cargo fmt --all --check
cargo test -p document-view --locked
cargo build --release --locked --features layout-validation --bin mineral-markdown
python performance/capture-layout.py --fixture 54-grammar-relationships.md --width 1600 --height 1200 --layout-trace details --source-unchanged-check --output performance/layout-previews/review-current-relationships.png
```

Check, formatting and release build pass. View tests: **302 passed, 2 failed,
2 ignored**. Failing assertions:

- `editor::arrangement::tests::product_plan_opens_with_a_labelled_editorial_split`
  expects an open labeled-list group in `plan.md`; none was selected.
- `editor::arrangement::tests::growing_a_sibling_card_keeps_its_column_until_reconsideration`
  expects the grown section to leave the peer arrangement after reconsideration;
  it retained a slot.

The advisory Crusty validation reports no new/worsened architecture findings,
but its index is stale and it did not run these checks. It is not a substitute
for the compiler, native screenshots or failed assertions. The older 108–109 fps
scrolling records describe the previous binary, not a fresh performance result
for this one.

**Sign-off requires** completing the missing primitives and their semantics,
resolving current regressions, then repeating the native content × width × zoom
× state matrix with spacing/geometry assertions, source-preserving edits and
sustained scrolling measurements. No board is signed off as fully implemented.
