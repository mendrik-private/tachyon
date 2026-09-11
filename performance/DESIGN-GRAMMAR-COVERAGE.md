# Complete document grammar — implementation and evidence ledger

Authority: `designs/document-design-grammar.md` and boards 01–07. The full
grammar, including its named extensions, is the active implementation goal.
This ledger is not a redefinition of completion around the existing renderer.
The September 8 native audit is progress: it supplies measured counterexamples
and determines the next changes. It does not approve the implementation.

September 9 clarification: `NATIVE-VALIDATION-CHECKPOINT.md` resolves the earlier
"non-periodic startup" concern: Weston 14's zero-refresh backend intentionally
repaints only on capture. Properly driven MCP frames publish measured layouts,
images and footnote actions; normal 60/120 Hz idle startup is checked separately.
Do not add an application redraw timer to compensate for this capture mode.

September 10 resize follow-up: `RECENT-PAIR-RESIZE-CHECKPOINT.md` covers native
width/height bursts and exact edit/autosave/Undo for the recent C01 optional
guidance row and P02 compact table tracks. It retains the original fixtures,
asserts content-dependent short-window behavior and checks current committed
geometry. Neither family is complete; other pair types and arbitrary reading
anchors remain separate coverage obligations.

`SPECIFICATION-RESIZE-CHECKPOINT.md` extends P02 evidence to the complete
parameter-table/JSON-example pair in fixture03: native width/height reading
bursts at 100/150/200% text, normal-size height editing, enlarged-text width
editing in both table and code, exact literal autosave/Undo and committed
zoom checks. It does not establish full mixed-document or RTL coverage.

`RICH-TIMELINES-CHECKPOINT.md` extends L10 to explicit dated events containing
paragraphs, code, tables, callouts and nested children. Rich events stay vertical
with full-width technical support; prepared event ownership keeps connectors
visible when the date header is offscreen. Native size/zoom, exact edit/Undo,
68-node semantic identity and connector pixel checks pass. Nested timeline
containers and the full interaction matrix remain open.

`RICH-TIMELINE-RESIZE-CHECKPOINT.md` strengthens native resize checks from stable
caret offsets to actual vertical visibility. It fixes height-shrink reflow
discarding a previously visible editing caret by comparing against the old
viewport and fitting the restored caret line into the new one. A deliberately
scrolled-away edit is not revealed. Exact source/Undo and complete rich-event
semantic/geometry checks accompany the native resize scenarios.
The corrected runtime passes 12 native height-edit and six width/height reading
scenarios at 100/150/200%, with visible carets and zero reading displacement.
`EDIT-RESIZE-ROUNDTRIP-CHECKPOINT.md` adds 24 complete native edit cycles for
the same rich-event fixture: body/code/table/nested text × width/height ×
100/150/200%. Full horizontal/vertical caret visibility, restored dimensions
and committed geometry, complete semantic/event ownership, unchanged saved bytes
through both resizes and exact Undo pass. Other families, post-restore typing,
IME/RTL and component-local overflow interactions remain separate work.

`ENCLOSED-TIMELINES-CHECKPOINT.md` adds vertical dated lists inside quotes and
callouts. Date rails and container padding no longer consume the summary's
readable measure; published component bounds follow the same measurement.
Canonical list ownership stops connectors at the correct boundary. Fixture120
covers wide/narrow/short/200% layouts and native exact body/code save/Undo.
The following checkpoint extends list nesting; the full enclosed resize/IME/RTL
matrix remains open.

`NESTED-TIMELINES-CHECKPOINT.md` extends dated layouts inside list items and
inside other events. Prepared nearest-event/ancestor links retain both rails
while their headers are offscreen; redundant ordinary-tree rules are removed.
Deep cramped hierarchies retain the complete readable-tree fallback. Fixture121
has native wide/narrow/200% geometry, 38-node identity, exact body/code edit/Undo,
source-order copy and double offscreen-rail evidence. Full nested interaction,
structural-edit, RTL/IME and resize coverage remains open.

`PAIRED-RECORDS-CHECKPOINT.md` adds measured two-record bands for overflowing
independent entity directories. Fixture122 uses the full document canvas and
saves 333 px of vertical space, with native wide/narrow/200% layouts, all 88
canonical nodes, unchanged comparison columns and exact body/header save/Undo.
The old fixture76 narrow-record oracle also fails on the pre-change runtime;
its identical 104-node/cell-geometry control is recorded, not requalified.
`RECORD-RESIZE-CHECKPOINT.md` resolves that mismatch as changed canvas width
after margin reduction, with current narrow record pixels and boundary tests.
It separately fixes active trailing-cell reveal and premature scroll clamping
during resize. Eighteen native reading/body/header cycles at 100/150/200% pass
width/height bursts, full caret visibility, exact save/Undo and release of the
focused measure. The complete record interaction matrix remains open.
`TABLE-NAVIGATION-CHECKPOINT.md` adds post-restoration typing, previous/next
cell navigation and exact stepwise Undo. It fixes a separately reproduced
Tab/Shift+Tab caret-reveal omission; the native pre-change runtime fails the
same 200% overflowing-header check that passes on the corrected runtime.
All 88 canonical nodes and unrelated source bytes remain checked at each
intermediate edit. Structural row operations and the full matrix remain open.

`PAGED-HTML-EXPORT-CHECKPOINT.md` adds the first production static/paged output
path. The app exports an immutable snapshot atomically to standalone inert HTML;
the core projection supplies A4/Letter orientation options, semantic tables and
repeated headers, GFM heading anchors, static tasks, expanded disclosures,
running furniture and print break hints. A real four-page A4 render retains
selectable source-order text and explicitly saved table widths. The receiving
browser still owns pagination; finite-page preview, PDF UI, complete masters,
footnote/code/oversized-row continuations, project-font embedding, tagged PDF,
real print equations and bounded Tachyon convergence remain open.

`NATIVE-IME-CHECKPOINT.md` adds one real cross-process Pinyin path with Tachyon
on Wayland text-input-v3 and Fcitx5 on Sway input-method-v2, isolated inside a
private Weston seat. The candidate panel aligns exactly below the published
caret rectangle. Preedit leaves source untouched; commit retains explicitly
saved `160/320` table widths; Undo and Escape restore exact bytes. The run also
found and fixed empty marked-range cancellation normalizing adjacent Markdown.
Physical GNOME/IBus, scale, script, transformed-family and interaction matrices
remain open.

## Contract and ownership

This is a native Linux/Wayland GPUI document editor with an editorial reading
surface and compact reference components. Canonical content, source positions,
commands and undo belong to `document-core`. Semantic grouping and candidate
selection belong to `document-view::adaptive`; native measurement, visual
placement, hit testing and accessibility consume that same plan. Application
navigation, file recovery and export commands belong to `markdown-app`.

New presentation families must remain mapped to canonical nodes/ranges. Do not
create a second editable document or insert invented headings, numbers, checks,
captions, metrics, dates, summaries, or identities to fill a layout. Reuse the
source tree for continuous editing and paged output. New types/modules should
own a specific grammar capability, not become generic UI helpers.

The explicit product decisions still apply: light first, Files and Outline,
no minimap, no source pane/tabs, automatic content layout without per-block
layout selectors, stable selection/scroll/caret during editing. Paged preview
and export are document surfaces, not manual per-block layout preferences.

Performance: release native build, 10 MiB mixed Markdown, sustained scrolling
above 60 fps, with 120 Hz as a stretch target. Measure end-to-end presentation
and draw tails. Startup may prepare more geometry; scrolling must not parse,
reshape or search candidates. All source and accessible reading order must
survive reflow, edits and pagination. Approximate layout measurements cannot
count as evidence that a candidate actually fits.

## Delivery sequence

1. Shared geometry: font-aware measures, open/card insets, rhythm, thin rules,
   caption baselines, zoom, and independent pixel/geometry regressions.
2. Semantic list/relationship vocabulary: label rows, definition lists,
   timelines, nested trees, prerequisites, steps and numbered/grid alternatives.
3. Named editorial objects: metadata, resource/decision/example/selected-option
   cards, pros/cons, contextual metrics, literal badges, validation and swatches.
4. Technical evidence: table variants/record fallback, code gutters and
   overflow, request/response pairs, clean math/references and accurate diagrams.
5. Editorial flow: source-linked figures/captions/credits, true wrapping,
   bounded flowing prose columns, margin notes, footnotes and bibliography.
6. Media: image/hero/gallery families, labeled playable audio/video, map
   previews and their accessible/static representations.
7. Page masters/export: all masters, finite-page placement, splits,
   continuations, cross references, contents, running furniture and print roles.
8. Navigation/state vocabulary: contextual navigation and chips when grounded
   in known context, focus/hover/disabled, empty/loading/error/recovery states.
9. Full board/content/width/zoom/state matrix, editing/accessibility tests,
   source-preservation tests, cold-start and sustained scrolling measurements.

## Requirement inventory

Status meanings: **open** = incomplete or missing; **partial** = implementation
exists but not the full rule/variant; **verified** requires current-build native
and automated evidence recorded here. Old passes are baselines, not sign-off.

| ID | Grammar requirement | Current status / evidence needed |
| --- | --- | --- |
| F01 | Every foundation foreground/background/rule color | Partial; core palette present, validate every surface pairing and contrast |
| F02 | 4/8/12/16/24/32/48/64 spacing roles, coherent gaps/insets | Partial; current relationships specimen proves 24/64 section gaps, open x=0 and 13 raster rows below task meter; complete family matrix remains open |
| F03 | 4 px component/8 px media corners; status-only pills | Partial; inspect all enclosed variants and nested corner relationships |
| F04 | 1 logical px screen rules; 0.5–0.75 pt print rules | Partial; shared-edge screen-table rule fix supersedes the historical 2 px audit failure; current timeline rail has one physical row at 1×. Print rules remain open |
| F05 | No document-content shadows | Partial; headers/core surfaces checked; new families must retain this |
| F06 | Display/section/subsection/minor type roles and leading | Partial; enforce ratios, source levels/anchors and zoom |
| F07 | Coherent narrative/reference modes, code/caption/metadata roles | Partial; add all new consumers and verify frozen editing geometry |
| F08 | Preferred 55–75 character prose, review at 80–85 | Partial; font-aware current English specimen has 56–70 characters per line. PARAGRAPH-ENDINGS-CHECKPOINT.md adds bounded native-measured isolated-final-word refinement, unchanged source/height, cache and zoom tests, native cross-wrap selection and edit/undo. Global balancing and the full script/font/zoom matrix remain open |
| F09 | Emphasis/strong/strike/links/mono, hard/soft breaks | Partial; PUNCTUATION-WRAPPING-CHECKPOINT.md closes the holdout's slash-leading wrap with Unicode legal opportunities, measured correction and an emoji overflow regression. Native same-source README wide/narrow/200%, exact cross-wrap copy, whole-file edit/autosave and exact undo pass. Unicode 15 opportunities are not full current Unicode/complex-script qualification; complete inline/script/source-selection coverage remains open |
| L01 | Short unordered siblings, consistent dots/hanging bodies | Partial; SHORT-LIST-COUNT-CHECKPOINT.md implements the 2–12 work bound and preserves authored term rails. LIST-PARTITIONS-CHECKPOINT.md adds mixed source-order rows. FOUR-COLUMN-FEATURES-CHECKPOINT.md adds single-line plain open features in four columns. INLINE-ENUMERATIONS-CHECKPOINT.md adds measured source-backed introductory paragraph clauses, width/zoom fallback, rich punctuation and focused editing retention. OPEN-LABELED-FEATURES-CHECKPOINT.md removes unjustified labeled-feature enclosures, unifies zero-inset measured/rendered geometry, preserves numbered/editorial cards and verifies the unchanged complete page at wide/narrow/200% with native edit/autosave/exact Undo. Additional labeled/presentation variants and the complete state matrix remain open |
| L02 | Labeled explanations, aligned term–description rows | Partial; bold-prefix rows (fixture 56) and first-class Markdown/HTML definition containers (fixture 62) now exist. Loaded-font open term rails, narrow stacks, rich descriptions, empty/split-term serialization and focused-rail inheritance are covered in DEFINITION-LISTS-CHECKPOINT.md. TREE-SELECTION-CHECKPOINT.md adds core tree-aware rich copy/replacement, nested block paste, native exact selection/save/undo and retained term rails (fixture 63). Nested rail selection, native rich-MIME interoperability, startup publication and the full family/state matrix remain open |
| L03 | Ordered actions, rank, stable authored reference identifiers | Partial; verify each role and source order across every reflow |
| L04 | Brief numbered rows/grids; supporting blocks use steppers | Partial; grid and stepper exist. INLINE-ENUMERATIONS-CHECKPOINT.md adds source-backed colon-introduced, semicolon-delimited paragraph clauses, including authored (a)/(b)/(c) markers, measured columns/fallback and focused geometry. Further inline-stage conventions and supporting-block variants remain open |
| L05 | Known tasks/check state/progress; unverified prerequisites stay bullets | Partial; CHECKLIST-STRIPS-CHECKPOINT.md adds compact measured rows for 2–12 short tasks. COMPACT-TREE-CHECKPOINT.md adds narrow deep-prose task context and fixes summaries that ignored unfinished descendants: explicit states count throughout the canonical subtree, with native exact marker-only toggle/undo and 2-of-4 → 3-of-4 evidence. Mixed deep branches and the complete state/keyboard/RTL/accessibility matrix remain open |
| L06 | Nested hierarchy, aligned guides, deep readable tree fallback | Partial; NESTED-READING-MEASURES-CHECKPOINT.md separates nesting gutters from readable width; PARAGRAPH-ENDINGS-CHECKPOINT.md refines endings. COMPACT-TREE-CHECKPOINT.md adds pressure-triggered trees and parent context; MIXED-TREE-CHECKPOINT.md adds supporting components; ENCLOSED-TREES-CHECKPOINT.md adds quoted/callout roots. SOURCE-LED-TREES-CHECKPOINT.md adds code/heading/image-first items, component-aware captions, outside code markers and a discovered fenced-code descendant-source save fix with exact native edit evidence. PARENT-SOURCE-FIDELITY-CHECKPOINT.md extends source-local saving to ordinary paragraph/heading/image parents and compound task edits. CONTAINER-LED-TREES-CHECKPOINT.md adds quote/note/table-leading parents, outside captions/markers/guides and native edit/copy evidence. UNLABELED-TREES-CHECKPOINT.md adds canonical blank caret rows for Markdown empty/list-first items, recovered deep width, marker-local introduction saves and native parent/child/empty-row edit evidence. Remaining outer contexts, richer nesting and the full interaction/source-fidelity matrix remain open |
| L07 | Long/uneven/dependent lists remain vertical | Partial; SHORT-LIST-COUNT-CHECKPOINT.md adds measured 2/10/12 negative cases for long, uneven, dependent, nested and task lists, plus the 13-item work bound. Full family/RTL/structural-edit state coverage remains open |
| L08 | Grid actual-fit: roughly 3–6, ≤3 body lines, height ratio ≤1.5 | Partial; measure and paint must use identical insets and font roles |
| L09 | Row-major order, incomplete rows, explicit RTL adaptation | Partial; LIST-PARTITIONS-CHECKPOINT.md and FOUR-COLUMN-FEATURES-CHECKPOINT.md add native-measured complete 2/3/4-column rows, explicit shared row identity, stable focused geometry, source-order copy, nonoverlap and 100/150/200% local-edit comparisons. Uniform partial rows remain legal when necessary. The full RTL/keyboard/structural-edit/accessibility matrix remains open |
| L10 | Dated vertical/horizontal timelines without invented dates | Partial; explicit-date flat lists choose measured horizontal, aligned vertical or narrow stacked timelines (TIMELINE-GRAMMAR-CHECKPOINT.md). Rich dated events retain full supporting blocks and nested children in vertical source order, with offscreen connector ownership, native edit/Undo and 68-node semantic/pixel checks (RICH-TIMELINES-CHECKPOINT.md, fixture119). ENCLOSED-TIMELINES-CHECKPOINT.md adds quotes/callouts, full readable summary measures, matching published bounds, canonical list-boundary ownership and fixture120 native evidence. NESTED-TIMELINES-CHECKPOINT.md adds lists inside lists/events, prepared ancestor rails, deep readable-tree fallback and fixture121 native evidence. Full nested resize/structural-edit/interaction/RTL coverage remains open |
| C01 | Five GFM callouts, proper icons/labels, compact and structured variants | Partial; GUIDANCE-ROWS-CHECKPOINT.md adds source-adjacent Note/Tip rows at unchanged standalone reading measures, retaining independent panels and full-width headings; fixture01 saves108px and uses99% of the canvas, with native narrow/large-text stacks, short-window pixels, complete canonical semantics and exact edit/autosave/Undo. Critical/structured/oversized/uneven notices remain in flow. Complete nesting/long/title/multi-script and interaction coverage remains open |
| C02 | One coherent icon family, optical alignment, names and semantics | Partial; new symbol families and control states need coverage |
| C03 | Resource card/compact linked row with grounded destination | Partial; source-owned linked titles/descriptions now use measured natural-height cards, link-only objects use compact rows, and resource links publish live-target activation. Fixture 58 covers wide/narrow/200% placement, exact undo and copy order. Authored thumbnails, complex/nested objects and the full interaction/RTL matrix remain open; see RESOURCE-GRAMMAR-CHECKPOINT.md |
| C04 | Decision record, selected option and isolated example cards | Partial; explicit bounded heading/body objects use native measured cards with reference typography, natural heights and stable editing. Wide/narrow/200% fixture 59 and code/prose edit/undo evidence are recorded in EDITORIAL-OBJECTS-CHECKPOINT.md; complex object families remain open |
| C05 | Labeled pros/cons and valid/invalid pairs, grayscale meaning | Partial; explicit labels, true unordered bullets and equal-track measured pairs preserve meaning without color. Fixture 59 checks wide/narrow/200%, source order and exposed semantics; the complete state/structural-editing matrix remains open |
| C06 | Metric with label/unit/period/denominator/baseline when supplied | Partial; explicit named quantitative objects now use 18/24 labels, 40/44 values, 14/20 context, native-measured peer cards or stacks, 24 px insets/gutters and 8 px internal gaps. METRIC-GRAMMAR-CHECKPOINT.md records fixture 73, wide/narrow/short/200% native pixels, exact source/undo, focused editing and Weston outline navigation on build 774f5e58. No metric/trend/completion is invented. Additional list/table encodings, complex objects, exhaustive locale/RTL/extreme-value and paged/print families remain open |
| C07 | Status/classification badges, literal color swatches | Partial; explicit status/classification/maturity fields and property/value tables now have source-backed, fully rounded badges. Named hexadecimal color objects have measured natural-height cards, square opaque/alpha swatches and exact editable literals. SIGNALS-GRAMMAR-CHECKPOINT.md records final build e8110201, wide/narrow/short/200% native pixels, source-order AT-SPI navigation, exact native edited-file/undo evidence and nine clipboard markers. It also fixes stale in-progress color previews, badge clipping and unnecessary metadata stacking. Arbitrary inline/CSS-functional colors, remaining HTML/nested/strip/card variants, full RTL/state and print coverage remain open |
| C08 | Metadata key/value rows and compact opening strips | Partial; explicit property lists now choose native-measured twelve-track strips, aligned rows or narrow stacks. METADATA-GRAMMAR-CHECKPOINT.md records 14/20 sans typography, 1 px rules, 16 px insets, 24 px gutters, 12/64 px row/section gaps, native copy/edit/growth/blur/undo and 100/200% evidence. Complex metadata, additional source encodings, RTL and the full state matrix remain open |
| C09 | Semantic enclosure, natural height, readable full-span containers | Partial; resource/editorial cards have 24 px insets, 8 px title/body gaps, natural column heights and constrained prose; compact resource rows have 12 px insets and a fine separator. Native editorial review corrected embedded code's right inset and Copy/overflow bounds; 1× pixels prove equal 24 px code insets. Complete family matrix remains open |
| C10 | Short warnings remain before/attached to affected action | Partial; WARNING-ACTIONS-CHECKPOINT.md adds one source-order group for short critical alerts and immediately following code/ordered procedures, with an optional explicit lead-in and 12px internal gaps. Wide/narrow/200% native layouts, exact warning edit/undo, boundary negatives and measured nonoverlap are checked. No relocation/rewriting; nested/rich/chained warnings, full state matrix and paged keep-with-next remain open |
| T01 | Property/comparison tables; headers, units, numeric alignment | Partial; authored alignment works and property/entity keys retain semibold reference typography. PROPERTY-RECORDS-CHECKPOINT.md and ENTITY-RECORDS-CHECKPOINT.md record current table/record semantics, wide/narrow/200% placement and measured labels. Full family coverage remains open |
| T02 | Intrinsic fitting, legible wide overflow, explicit widths | Partial; TABLE-WIDTH-BALANCE-CHECKPOINT.md reserves native-shaped widths for recognized short status badges before allocating remaining prose space. Fixture76 narrow/intermediate/wide/dark200, focused growth, exact native edit/Undo and explicit-width/ordinary-text negatives pass. NATIVE-IME-CHECKPOINT.md retains explicitly saved `160/320` widths through a real Fcitx5 preedit, commit, Undo and cancellation path; automatic alignment remains limited to automatically sized tables. General line-count-aware column scoring, complete width/scale/nesting and paged matrices remain open |
| T03 | Labeled record fallback retains every header/value relationship | Partial; explicit property/value and multi-attribute entity directories retain canonical schema headers/rows/cells in measured natural-height records. ENTITY-RECORDS-CHECKPOINT.md records historical fixtures 76/77. PAIRED-RECORDS-CHECKPOINT.md adds measured two-record bands, narrow/200% stacking, all 88 canonical nodes and exact native save/Undo. RECORD-RESIZE-CHECKPOINT.md resolves the historical narrow mismatch as changed content width and fixes active trailing-header reveal and premature horizontal-scroll clamping. TABLE-NAVIGATION-CHECKPOINT.md adds continued typing, visible Tab/Shift+Tab and stepwise Undo after restoration. NATIVE-IME-CHECKPOINT.md covers one fixed-width table path; the full structural/IME matrix, transposed comparisons, complex/nested/RTL and print variants remain open |
| T04 | Light/dark code, exact whitespace, language, Copy/Copied | Partial; COMMAND-STRIPS-CHECKPOINT.md adds native-measured single-row shell commands, stable language/Copy tracks, 32px less header chrome, narrow/full-pane fallback and active-edit retention through background publication. Wide/narrow/dark-200% pixels and full-file-equal native paste/autosave/undo are recorded. CODE-PANELS-CHECKPOINT.md covers terminal-row spacing and pointer Copy; CODE-GUTTERS-CHECKPOINT.md covers overflow/copy/hover. Nested/table strips, the full state/RTL/IME matrix and print remain open |
| T05 | Optional line numbers distinct from copied source | Partial; automatic measured gutters for ≥4 source lines, right-aligned numbers/blank lines, fixed rail during horizontal scrolling, typing stability, exact native copy/undo and 100/130/200% checks now pass. CODE-GUTTERS-CHECKPOINT.md records wide/narrow/exchange pixels and source-only AT-SPI evidence; full state/table/RTL/extreme-growth and print-continuation coverage remain open |
| T06 | Request/response/status-code pairs with explicit labels | Partial; explicitly labeled H2/H3 exchange objects with code now use measured equal-track pairs and narrow stacks. Direction, topic barriers, exact clipboard, native payload edit/undo and 200% overflow are checked in REQUEST-RESPONSE-CHECKPOINT.md. Complex objects, contextual badges and the complete state/RTL matrix remain open |
| T07 | Inline/display real math, references only when authored/conventional | Partial; supported display equations now use source-owned atomic rows, actual glyph dimensions, centered prose measures or wider overflow, 24 px equation gaps and open paper styling. DISPLAY-EQUATIONS-CHECKPOINT.md records fixture 72, wide/narrow/short/200% native pixels, native keyboard/source editing, autosave/exact undo and a native-shaper regression. Equation references/numbering placement, complete TeX and nested/state/RTL matrices, and semantic exports remain open |
| T08 | Accurate deterministic diagrams/charts and prose alternatives | Partial; bounded inert Mermaid flowcharts already provide bundled-font figures and source-order topology alternatives. DIAGRAM-COMPOSITION-CHECKPOINT.md adds authoritative figure-based candidate measurements, native wide explanation/diagram pairs, narrow overflow/panning, 200% stacking, valid/invalid source editing and exact undo. Other diagram/chart families, complete graph/state/RTL/accessibility matrices, whole-page composition and static exports remain open |
| T09 | Schema tree with readable nesting and original labels | Partial; SCHEMA-TREE-CHECKPOINT.md adds bounded source-backed structural JSON Schema trees with original field order, raw value spelling, measured labels, nesting guides, source editing and inert reference values. Complete 1–4 paragraph introductions now compete beside their technical content while preserving compact peers and code-label attachment. Wide/narrow/dark-200% native views, exact edited-file/undo checks and source-order clipboard markers are recorded. Semantic condensed schemas, per-field interaction, arbitrary dialects, full RTL/state/accessibility and exports remain open |
| E01 | Deliberate hero/inline/wide images; preserve evidence/full image | Partial; intrinsic non-upscaled image bounds, shared caption leading edges and rounded underlays are verified in FIGURE-CAPTIONS-CHECKPOINT.md. IMAGE-STATES-CHECKPOINT.md adds actual delayed loading/failure callbacks, readable alternative descriptions, safe destination previews and source-preserving Retry. Full hero/inline/wide selection, inline/nested media and the full state matrix remain open |
| E02 | Galleries with individual/shared captions and stable order | Partial; FIGURE-CAPTIONS-CHECKPOINT.md covers individual captions and 24px gallery gutters. SHARED-GALLERY-CAPTIONS-CHECKPOINT.md adds explicit shared Gallery:/Gallery credit: bands after complete bounded galleries, 13/18 typography, 8/4px attachment gaps, all-member accessible descriptions and source-order narrow stacking. Native edit/undo and exact End/Shift-Home copy verify the discovered focused-worker width-cap fix. Nested/HTML conventions, larger/uneven sets, full state/RTL and paged variants remain open |
| E03 | Paired figure/text with narrow stack | Partial; FIGURE-LED-LAYOUT-CHECKPOINT.md adds complete evidence-image/caption/credit + explanation pairs with intrinsic/reading-width caps and focus-aware resource fallback. SUPPORTING-IMAGE-PAIRS-CHECKPOINT.md adds native-measured wrap eligibility before supporting-image pair selection, whole-unit fallback and reversible wrap/pair transitions. Fixtures102/111 native wide/narrow/dark200, whole-file body/caption edit/autosave/exact Undo and source-order copy pass; fixture69 true-wrap/gallery control remains intact. General cross-family scoring, nested/HTML/RTL/state, native continuous resize and paged matrices remain open |
| E04 | True 25–35% float, 16–24 gap, ≥35–40ch/4 useful lines | Partial; FIGURE-FLOW-CHECKPOINT.md covers the original measured wrap and source-boundary editing. FIGURE-CAPTIONS-CHECKPOINT.md adds image/caption/credit lane measurement, nine useful adjacent lines, full-measure return below the complete lane, 100/200% glyph/gap checks and narrow/short stacking. Right-side, multi-script and comprehensive structural edits remain open |
| E05 | Tables/code/aligned math/essential diagrams/critical warnings never float | Partial; float candidates accept supporting images with optional explicit captions/credits followed by prose. Evidence/technical roles, ambiguous editorial asides, hard breaks in body prose, inline media/math, unknown dimensions and tall/cramped geometry reject wrapping. Named diagram/table/warning exclusions have regressions; full mixed-container and technical-content matrix remains open |
| E06 | Bounded flowing prose columns, not just adjacent section columns | Partial; source-linked paragraph fragments now flow across native-measured paired bands with height/widow constraints, 24 px gutters/gaps, focused anchors and narrow/short/200% fallback. PROSE-FLOW-CHECKPOINT.md records fixture 65, independent native geometry/pixels, column-crossing keyboard selection, exact save/paste/undo and rebalancing after blur. Multi-script, large-flow and complete structural/accessible interaction coverage remain open |
| E07 | Quote/attribution, deliberate pull quotes and full-span readable quote | Partial; fixture 70 now verifies 18/28 reading quotes, explicit 14/20 attribution with 8 px gap, centered 24/32 pull-quote text inside a full-span Sage surface, 24/16 px insets/padding and distinct nested rails. QUOTATION-GRAMMAR-CHECKPOINT.md records wide/narrow/200% native pixels, focus-stable width and exact save/undo evidence. HTML citations, mixed/nested attribution structures, floats and the complete script/state/print matrix remain open |
| E08 | Anchored margin notes, inline fallback | Partial; MARGIN-NOTES-CHECKPOINT.md implements explicit top-level `Margin note:` blockquotes anchored to the preceding paragraph, measured main/rail placement, quiet 14/20 typography, semantic descriptions and inline fallback. MARGIN-NOTE-CLUSTERS-CHECKPOINT.md adds complete adjacent-note clusters with one shared anchor, collision-free measured rails, bounded whole-cluster fallback and focus-stable neighboring notes. Fixtures 101/110 native wide/narrow/dark200, source-order copy and whole-file note edits/autosave/exact Undo pass. Distant/nested/HTML notes, complex multi-paragraph interactions, RTL rail placement, full resize/IME/performance and paged variants remain open |
| E09 | Source-order superscript footnotes, notes/backlinks, anchor attachment | Partial; native-measured superscripts, atomic source references and editable 14/20 note rows with 40 px rails now have pointer/keyboard/AT-SPI navigation. FOOTNOTES-GRAMMAR-CHECKPOINT.md records wide/narrow/200% pixels, 12/16/24 px gaps, exact note save/undo and source-local sibling preservation. NATIVE-VALIDATION-CHECKPOINT.md resolves capture-only startup and verifies forward navigation in Weston MCP. Large/bidi/complex notes and structural ownership remain open |
| E10 | Bibliography hanging indentation and glossary term structure | Partial; glossary semantics and measured open term rails exist (DEFINITION-LISTS-CHECKPOINT.md). Bibliography paragraphs and flat reference lists now have source-owned citation roles, serif 18/28 typography, 24 px hanging continuations, quiet authored numbers and 12 px entry gaps. BIBLIOGRAPHY-GRAMMAR-CHECKPOINT.md records fixture 71, wide/narrow/short/200% native pixels, focus-stable measures, source-order copy, edit/autosave/exact undo and semantic entry hierarchy. Complex/nested citation schemas, cross references, full interaction/RTL and paged/print families remain open |
| E11 | Video/audio with real controls, labels/duration/transcript when known | Open |
| E12 | Map preview with meaningful labels and destination | Partial; MAP-PREVIEWS-CHECKPOINT.md adds explicit source-owned linked `Map:` images, uncropped evidence geometry, a named scaled destination control, native-readable pair/stack selection and preserved captions/credits. Fixture114 wide/narrow/dark200 geometry/pixels, missing-preview navigation, whole-file caption edit/autosave/exact Undo and source-order copy pass; toolkit tests cover external URL dispatch, Enter/Space, invalid destinations and stale-role/link/deletion safety. Complete nested/HTML/RTL/state and static paged/export translations remain unqualified |
| E13 | Figure numbering only when referenced; captions/credits not alt-text guesses | Partial; explicit authored individual and shared caption/credit labels attach without alt/title fabrication or automatic numbering. Native caption roles and all-member shared descriptions are verified in SHARED-GALLERY-CAPTIONS-CHECKPOINT.md. Nested/HTML associations, additional encodings and figure-reference resolution remain open |
| P01 | Usable width/height subtract margins/navigation/rails/furniture | Partial; continuous width exists, finite page accounting open |
| P02 | <560, 560–899, 900–1199, ≥1200 content-fit behaviors | Partial; single-column floor exists. EXPLANATION-RESIZE-CHECKPOINT.md adds measured table/code explanation recovery after narrow/short stacking, independent full-width heading focus, prose-lock release after blur, 100/150/200% geometry cycles and native width/height burst/edit/source verification. TECHNICAL-CONTINUATIONS-CHECKPOINT.md adds complete technical sibling pairs retaining trailing explanations, 385px native fixture height savings, narrow/short/large-text fallback and exact native edit/Undo verification. TECHNICAL-PARTITIONS-CHECKPOINT.md permits measured technical sibling partitions without breaking matched nontechnical card trios; fixture76 saves 213px with unchanged table sizes, verified recovery and native source/edit/Undo. CONTENT-LED-LAYOUT-CHECKPOINT.md adds table/code-first explanation pairs with actual-footprint 24px gutters, 336px fixture115 savings, unchanged full README verification, native narrow/short/dark200 stacks and whole-file prose/code autosave/exact Undo. CONTENT-RESIZE-CHECKPOINT.md adds topology-based full-width-heading focus recovery, native table/code width/height bursts with stable heading anchors and current committed geometry, and exact whole-file edit/Undo with soft source breaks. SPECIFICATION-EXAMPLE-CHECKPOINT.md adds field-linked specification/example technical pairs while preserving explicit exchange counterparts and editorial identity; fixture03 saves 295px with unchanged table dimensions, aligned component borders, full canonical accessibility, fit-based stacks and exact table/code edit/Undo. COMPACT-TABLE-TRACKS-CHECKPOINT.md adds naturally fitting table-only 3:9/9:3 candidates, reclaiming 112px of comparison-table width and saving 42px without shrinking the compact table; native source/semantics/edit/Undo and content-driven short-height fit/stack are verified. Full rail/master selection and mixed-document/state coverage remain incomplete |
| P03 | Reading/reference/overview densities change arrangement before type | Partial; REFERENCE-RHYTHM-CHECKPOINT.md adds 32px stacked technical-sibling separation with stable editing, unchanged chapter gaps/type and native narrow/wide/200% verification. READING-BALANCE-CHECKPOINT.md adds bounded native-measured width alternatives for otherwise uneven prose bands, retaining type, widow guards, focused anchors and source order; fixture113 and exact audit.md save 28px, with narrow/short/dark200, native cross-column copy and edit/Undo checks. REFERENCE-PROSE-FLOW-CHECKPOINT.md extends bounded reading bands to sustained reference-font passages without changing typography; fixture27 uses 99% of its wide canvas and saves 192px, with complete canonical accessibility, regular/narrow/short/dark200, cross-column selection and whole-file autosave/Undo verification. Repeated short instructions remain stacked. OPENING-LAYOUT-CHECKPOINT.md adds authored lead/overview pairs below the full-width title, native font/height guards, narrow/short/200% fallback, resize/edit/Undo and canonical accessibility evidence; the whole README saves 120px and the regular window uses 99.9% of its opening canvas. Unfocused cached prose flow now respects newly measured row ownership. Full density modes and composition policies remain open |
| P04 | Portrait, wide/landscape, square, 16:9, tall/short screen masters | Partial; PAGED-HTML-EXPORT-CHECKPOINT.md adds core A4/Letter portrait/landscape page options and the app's A4 portrait export. Square, 16:9, tall/short screen masters and UI selection remain open |
| P05 | Cover/opening, standard, reference, wide evidence, appendix masters | Partial; the standalone export supplies one standard reading page with reserved running furniture. Distinct cover/opening, reference, wide-evidence and appendix masters remain open |
| P06 | Explicit breaks; section/paragraph/item/row break priorities | Partial; the paged stylesheet prefers section, paragraph, short-item and table-row boundaries. Explicit authored page breaks and a complete ordered break solver remain open |
| P07 | Heading +2 lines; paragraph preferred ≥3 lines each side | Partial; headings avoid trailing breaks and paragraphs request three-line widows/orphans in the receiving renderer. Exact heading-plus-two-lines measurement and fallback priority remain open |
| P08 | Keep short items, warnings/actions, figures/captions, quotes/attribution | Partial; short items, callouts, figures, footnote sections and quotations request atomic placement. Relationship-aware warning/action, figure/caption and quotation/attribution pagination remains open |
| P09 | Oversized atomic escape, permitted splitting, guaranteed progress | Open |
| P10 | Repeated table headers, row splits/identifiers/continuation | Partial; static tables emit semantic `thead`/`tbody`, repeated headers and preferred row-boundary splits. Oversized-row splitting, repeated identifiers and continuation labels remain open |
| P11 | Code logical splits, repeated label/language/continued/gutter | Open |
| P12 | Anchor-page footnotes or explicit continuation/endnotes | Open |
| P13 | Sparse endings reviewed, never delete/truncate to fill pages | Partial; deterministic four-page output retains all source content and its sparse endings were visually inspected. Automated sparse-ending review and layout alternatives remain open |
| P14 | Reserved running header/footer/folio; converged totals/contents | Partial; browser/paged-renderer output reserves margins and computes a running title plus `page / pages` folios. Generated contents and Tachyon-owned convergence remain open |
| P15 | Bounded pagination convergence and explicit unresolved constraints | Open |
| P16 | Print type/borders, selectable text, semantic tables, real equations | Partial; paged HTML supplies print typography/borders, selectable source-order text, semantic tables, embedded PDF fallback fonts and visible lossless math source. Project-font embedding, tagged PDF and real typeset equations remain open |
| N01 | Files, outline, active shape/rule, native navigation/focus | Partial; preserve product shell and test all states |
| N02 | Grounded breadcrumb, on-page contents, previous/next context | Open |
| N03 | Disclosures retain state, reveal anchored content, named controls | Partial; utility strip/marker spacing differs from board |
| N04 | Formatting/table icons, hover, focus, disabled and copy feedback | Partial; formatting hover verified, full command-state matrix open |
| N05 | Person/issue chips only with real identity/repository context | Open; plain text must remain when context is absent |
| N06 | Distinct empty/no-result/loading/error/permission/conflict and recovery | Partial; IMAGE-STATES-CHECKPOINT.md covers document image loading versus actual failure, accessible recovery controls, preserved captions/geometry and pointer/Enter/Space Retry without document edits. Broader application state treatments, permission/conflict, media retry success under real network conditions and the complete state matrix remain open |
| N07 | Static translation of every interactive surface; no transient export UI | Open |
| X01 | Canonical selection/edit/undo/autosave for every transformed family | Partial; TREE-SELECTION-CHECKPOINT.md closes the top-level-only range restriction for existing semantic containers, with rich copy, nested block paste, composition, table identity and exact native save/undo evidence. METADATA-GRAMMAR-CHECKPOINT.md adds property editing, frozen growth and exact undo, and recorded an intra-list fidelity limit. SOURCE-FIDELITY-CHECKPOINT.md fixes covered non-structural sibling preservation and a nested-list indentation round-trip defect. PROPERTY-RECORDS-CHECKPOINT.md adds source-local GFM cell patches, retained untouched delimiters/punctuation, semantic cell-line-break reopening and whole-file-equal native edit/autosave/undo. ENTITY-RECORDS-CHECKPOINT.md adds complete-source native value/header edits and worker-publication typography stability for multi-attribute records. NATIVE-IME-CHECKPOINT.md fixes platform empty-marked-range cancellation and proves exact preedit, commit/Undo and Escape behavior in one system-IME table path. Arbitrary structural-region fidelity, missing families, native rich-MIME and the full interaction matrix remain open |
| X02 | Stable layout while typing; responsive reflow, caret and scroll anchors | Partial; ENTITY-RECORDS-CHECKPOINT.md verifies entity value/header typing, measured label growth and shared frozen-role restoration through immediate refresh and background publication, including native unchanged-title glyph checks after focused idle. Revalidate every geometry primitive and remaining cross-family interactions |
| X03 | Source order/content/labels survive every reflow and page split | Partial; exhaustive family acceptance matrix needed |
| X04 | Accessibility, contrast, names, focus, zoom/RTL/large/missing content | Partial; NATIVE-VALIDATION-CHECKPOINT.md adds native named title-bar/formatting buttons and zoom activation, without visual changes. Application/Text dropdowns still lack advertised accessible actions; complete focus, contrast and variant coverage remain open |
| X05 | Current-build native board specimens with measured geometry/pixels | Partial; HOLDOUT-LAYOUT-CHECKPOINT.md adds actual plan/README wide/narrow/short/200% native review, command edit/autosave/exact undo, source-order copy and origin-hash provenance. Plan revisions changed externally and are not a controlled same-source comparison. The full board/content/width/state matrix remains unqualified; historical captures are not current goldens |
| X06 | Release 10 MiB >60 fps, timing tails, graceful startup/recovery | Partial; RETAINED-ACCESSIBILITY-CHECKPOINT.md closes the isolated active-AT-SPI size boundary: exact 1 MiB/10 MiB sources expose 3,337/33,268 direct roots, match 23 representative semantics across each document, and sustain 107.5/108.2 fps with 9.99/9.76 ms draw p99. Accessibility readiness is measured separately at 5.32/39.52 s. The stricter physical-Mutter repetitions, draw p99 ≤6 ms, missed deadlines <0.1%, full latency attribution and startup/recovery matrix remain open |

## Evidence protocol

For each completed row record source fixture, exact binary hash, widths/zoom/
display scale, native screenshot, geometry/pixel expectations and test command.
Check wide, narrow, short and large text; add state/overflow/nesting cases where
the component supports them. Source checks run on isolated copies. Compare
actual output to the written grammar and reviewed board—not a golden generated
from the candidate alone. Mark uncertain, indirect, partial or missing evidence
as not complete. The goal remains active until every applicable row passes.

The original audit artifacts are `layout-previews/audit-*` and
`layout-previews/grammar-*` (baseline binary `65f6b09b…cc120`). Do not overwrite
those baseline captures with changed output.

The [earlier visual audit](DESIGN-GRAMMAR-VISUAL-AUDIT.md) records native
captures from binary `1287a7e6…dc885`, including measured improvements and
remaining failures. None of the complete board families is signed off.

The later [timeline checkpoint](TIMELINE-GRAMMAR-CHECKPOINT.md) records current
`e7741202…aae3b5` evidence for aligned labels, hierarchy guides and dated
timelines, including native interactions and scrolling. It also distinguishes
Weston MCP's zero-refresh capture mode from the periodic compositor required
for startup and frame-timing validation. The full objective remains unchanged.

The [resource checkpoint](RESOURCE-GRAMMAR-CHECKPOINT.md) records the next
Board 03 implementation and current-build native evidence. It does not approve
the whole cards/signals board or reduce the remaining grammar scope.

The [editorial-object checkpoint](EDITORIAL-OBJECTS-CHECKPOINT.md) records
decision/selected/example and labeled comparison objects on build `ffcb4983`.
It includes the corrected nested code inset, native Copy hover, exact undo,
200% complete-card evidence and separate continuous-performance/wheel-coast
checks, without claiming that the failed wheel-latency run passed.

The [request/response checkpoint](REQUEST-RESPONSE-CHECKPOINT.md) records
explicit technical pair direction and heading boundaries, the native 200%
code-overflow defect and its fix, exact clipboard/source/undo checks, final
`e42a16b4` screenshots and native performance. The complete technical board
and the rest of the grammar are still not signed off.

The [code-pane checkpoint](CODE-PANELS-CHECKPOINT.md) closes the two defects
found during gutter review without dropping the final source newline or its
editing endpoint. Build `e808101b` has current wide/narrow/200% footer evidence,
real pointer Copy selection preservation, exact terminal edit/undo and
source-only clipboard checks. No complete board family is signed off.

The [metadata checkpoint](METADATA-GRAMMAR-CHECKPOINT.md) adds measured property
strips, aligned rows and narrow stacks on build `77b5bd4c`, with native spacing,
type, edit/growth/undo and accessible bounds. A 60-second 10 MiB native run
averages 108.17 fps (presentation p99 12.35 ms, draw p99 8.76 ms), retaining
five ≥25 ms presentation intervals as an explicit tail limit. The checkpoint
also records untouched-item punctuation normalization inside edited lists.
The [source-fidelity checkpoint](SOURCE-FIDELITY-CHECKPOINT.md) fixes that
normalization for covered non-structural list edits on build `305b56af`, retaining
the same independently checked strip/row spacing, focused growth and exact undo.
All remaining grammar families and the full matrix stay in scope.

The [property-record checkpoint](PROPERTY-RECORDS-CHECKPOINT.md) records the
first measured narrow property/value fallback and source-local GFM cell edits
on `a33c9286`, including wide/narrow/short/200% pixels, real Weston MCP focus,
exact native autosave/undo, clipboard order and current 10 MiB frame timings.
It also retains two under-resolved wheel-coast failures alongside a passing
shorter-document decay run. Multi-attribute entity records and the remaining
grammar families are still required; none is replaced by this subset.
