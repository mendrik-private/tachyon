# Editorial layout grammar: implementation plan

Status: **planned, not implemented or visually qualified**. Prepared September 8, 2026 from the current engine and the eight images in `designs/`.

This is the next layout milestone. It supersedes the small-vocabulary restriction, blanket exclusion of prose columns and text wrapping, and conflicting layout/spacing prescriptions in the earlier plans. The application remains a native Rust/GPUI editor with a light theme, Files and Outline, automatic layout, and no minimap or user-facing template controls. Markdown, editing commands, and autosave remain authoritative.

## 1. Intended result

The engine should compose an appealing document, not independently decorate each block. A section can open with a heading and lead, continue with a restrained feature strip, place a compact specification beside its explanation, then return to a generous reading column. Another section can use numbered tiles or a short two-column passage. Those choices must follow the material, not a rotation through decorative presets.

Interpret “visual pages” as **bounded composition bands in one continuous scrolling document**, not fixed paper pages, extra blank screens, or a new paginated editing mode. Bands provide alignment and visual rhythm; their height follows content. Long documents repeat that rhythm without repeating the same card treatment everywhere.

Success requires all of the following:

- Measured selection from genuinely different template families, including ordinary short lists without bold labels.
- Joint composition of related blocks and their internal presentations.
- Comfortable spacing, hanging numbers, consistent text anchors, and measured icons/chips.
- Bounded multi-column prose and actual line flow around small images/tables.
- Identical source content and reliable editing in every arrangement.
- Native visual confirmation and sustained scrolling above 60 fps on the agreed performance environment. Slower preparation is acceptable; scroll-time layout search is not.

Matching a reference means matching its composition and craft with the supplied content. The engine must not invent subtitles, metadata, diagrams, icons implying approval, or summaries simply because a mockup contains them.

## 2. What currently limits the result

The existing implementation has useful foundations: native text measurements, source-preserving projections, bounded row search, cached geometry, and editing locks. Extend these rather than replace the editor or introduce a browser-based rendering surface.

| Current evidence | Consequence | Planned correction |
| --- | --- | --- |
| `adaptive/candidates.rs::choose_list` considers stack and equal two/three-column grids. The labelled-list path nominates mainly by count and retains a legal previous grid independently of the normal score. | Measured fit does not amount to measured design quality; many lists receive the same appearance. | Typed template candidates with shared scoring, typography-aware measurements, and finite hysteresis. |
| `adaptive.rs::compact_section` and list eligibility have restrictive heading, byte-count, shape, and item-count gates. | Similar content receives different treatment because of small syntax differences; plain lists are frequently missed. | Broad structural recognition, with measured size constraints replacing character-count proxies after cheap safety screening. |
| `editor/arrangement.rs` measures outer rows before internal lists. `RowCandidate.parts` describes contiguous whole-root ranges. | Outer and inner layouts cannot fully negotiate. The representation is insufficient for a paragraph split across regions or flowing below a figure. | Hierarchical candidates and source-addressed flow fragments. |
| `adaptive/rows.rs` has seven useful track patterns, but a small set of row relationships and scalar height/preferred-width measurements. | Geometry alone cannot express an opening, specification sidebar, numbered feature band, or balanced reading passage. | Semantic band templates with richer footprints, anchors, break opportunities, and decoration budgets. |
| `adaptive.rs::gap_between` chooses flat pairwise gaps, with a special earlier branch for a preceding heading. | Section boundaries, consecutive headings, and headings inside compositions do not share a consistent spacing model. | Hierarchical spacing and keep-with-next rules resolved into geometry. |
| `editor.rs::styled_runs` adds an editorial card-label font after calling `styled_projection_runs`, while measurement also calls the latter directly. | Presentation decisions are spread between planning and painting; exact font/geometry agreement needs a dedicated invariant. | One resolved presentation consumed by measurement, painting, hit testing, and accessibility. |
| Existing composition assertions include checking that several top-accent groups appeared. | Passing tests can still produce a dull page. | Full-page visual benchmarks and ranked candidate comparisons, alongside correctness tests. |

These observations are from the current dirty worktree, not a claim that all previous work is absent. Prior performance and screenshot results remain historical baselines; they do not qualify this proposed grammar.

## 3. Extract the grammar from the north star

All reference names below are in `../designs/`, with the common prefix `ChatGPT Image Sep 6, 2026, `.

| Reference suffix | Composition to reproduce | Reusable rules |
| --- | --- | --- |
| `02_02_56 PM (1).png` — API reference | Endpoint opening, wide parameters, paired request/response, compact status results. | Authored method/status chips; aligned code headers; short result rows; tables negotiate width separately. |
| `02_02_56 PM (2).png` — decision record | Title and metadata, context beside decision, three numbered drivers, wider alternatives beside narrower consequences. | Metadata strip; prose/callout pair; numbered feature cards; asymmetric sibling composition. |
| `02_02_56 PM (3).png` — configuration | Explanations, warning, configuration code, properties, and short valid/invalid examples. | Related technical groups; label/value rows; unequal widths; shared heading and component anchors. |
| `02_02_56 PM (4).png` — getting started | Prerequisites band followed by a numbered instruction rail containing rich examples. | Compact checklist strip; stepper; keep each explanation, code block, and table with its step. |
| `02_03_04 PM (1).png` — reading | Strong opening, quote/notes, bounded multi-column prose. | Lead typography; restrained quotation rail; balanced reading regions; authored notes in a margin where they fit. |
| `02_03_04 PM (2).png` — lists | Number tiles, step rails, open features, nested outline, checklist, long vertical items. | Multiple list families, not multiple skins for the same grid. Preserve sequence and hierarchy. |
| `02_03_05 PM (3).png` — media | Hero and paired images, small media beside text, badges, compact alerts and resources. | Aspect-ratio-aware figures; wrapping strip; figure/text flow; quiet semantic decorations. |
| `02_03_05 PM (4).png` — technical content | Prose beside code/diagrams, compact properties and wide comparison tables, equations. | Independent component widths; technical sidecars; wide interruptions; narrow-screen stacking. |

The visual system is warm paper, dark Fraunces headings, Spline Sans body, Spline Sans Mono technical text, muted green accents, fine rules, and selective pale surfaces. Keep most prose and many feature groups unboxed. A card is a grouping decision, not the default container for every item. Do not copy the reference images' layout selectors or extra navigation panels into the app.

## 4. A hierarchical, source-preserving grammar

Use one canonical document and a derived presentation tree:

```text
Canonical Markdown tree + source identities
    → structural roles and relationship evidence
    → styled atoms and candidate group presentations
    → measured section/band compositions
    → immutable flow fragments + spatial/source indexes
    → paint, caret, selection, navigation, accessibility
```

Proposed grammar:

```text
Document  := Opening? Section*
Section   := Heading Band+ Subsection*
Band      := Reading | FeatureBand | CardRows | Steps | Checklist
           | SideBySide | TechnicalPair | Metadata | Gallery
           | BoundedColumns | WrappedComponent | WideComponent
Group     := source-contiguous blocks or source-addressed inline fragments
Atom      := Text | Label | Number | TaskMarker | Link | Code
           | AuthoredStatus | LiteralTag | Figure | Table | Decoration
```

The grammar describes legal compositions, not a second editable document. Every content atom references canonical node IDs and source/projection ranges. Decorative atoms are explicitly non-content. Every template declares its reading traversal; the compiler proves it matches the logical source sequence.

### Recognition

- Inspect every section, every list, repeated sibling units, and explicit inline enumerations. Do not special-case `plan.md`, heading titles such as “Product contract,” or only the first eligible paragraph.
- Separate evidence from conclusions: ordered/unordered, task state, nesting, authored prefixes, punctuation, short repeated structures, explicit dates, links, table alignment, image dimensions, section membership, and reference relationships.
- Recognize list labels from bold/code prefixes, plain `Label: description`, and unambiguous short title/description structures. Ordinary independent points can use open feature columns without acquiring invented titles.
- An introductory clause followed by clearly separated short clauses can nominate an inline-list presentation. Start with explicit semicolon-separated or repeated labelled clauses; comma-only prose is not enough. Keep the introduction and exact delimiter ranges in the source mapping. Do not extract a few attractive words from an otherwise ordinary sentence.
- Preserve nested children within their parent group. Do not flatten an outline to fill a grid.
- Recognize sibling sections by their structure and common parent, including compact H2 sections when appropriate; adjacency alone does not establish a comparison or a shared topic.
- Record evidence and rejection reasons. Low confidence selects a well-typeset normal flow, not an error or missing content.

Deterministic rules and native measurements are sufficient for this milestone. No runtime LLM, network classification, or rewriting step is required. English keyword lists may provide supporting hints but cannot be the sole basis for extracting content or changing its meaning.

## 5. Template vocabulary

Every template definition contains: accepted roles and relationships, forbidden structures, slot proportions, minimum usable measures, resolved typography, marker geometry, insets/gaps, allowed breaks, reading traversal, edit-lock behavior, and a simple-flow fallback. Skins such as a top rule versus a thin outline are variants within a family, not counted as separate layout capabilities.

### Short lists and repeated units

| Family | Suitable material | Arrangement and guardrails |
| --- | --- | --- |
| Open feature columns | Short independent bullets, with or without labels. | Two to four columns where measured text fits; generous gutters; no enclosing surface. Four columns reserved for very short content. |
| Label/description rows | Repeated terms, settings, responsibilities. | Shared label track and aligned descriptions; shrink to stacked label/body when labels consume too much width. |
| Feature cards | Independent titled items with moderate explanations. | Thin outline, quiet surface, or top rule according to surrounding composition. Two/three columns normally; align labels and first body baselines. |
| Number tiles | Short ordered independent points. | Large green number in a separate gutter; row-by-row order; usually two/three columns. No forced circles on every numbered item. |
| Compact numbered cards | Short labelled principles or decision drivers. | Smaller circular marker beside title/description, as in the decision-record reference. |
| Horizontal steps | A short complete sequence. | Two to four stages in one band only if all stages fit without cramped explanations. Otherwise a vertical stepper. |
| Vertical stepper | Instructions with details, code, tables, or media. | Persistent number rail; rich children remain inside their step; clear spacing between stages. |
| Checklist strip/panel | Authored tasks or prerequisites. | Compact wrapping strip for short tasks; vertical panel for richer tasks; real task state and optional computed completion count. |
| Tag/chip strip | Explicit short tags, enumerated literal values, or badge links. | Intrinsic-width wrapping chips; retain original labels, links, and order. Sentences are not tags. |
| Metric strip | Repeated authored value/unit/label structures. | Aligned value emphasis and supporting labels. Never derive or invent a metric. |
| Resource rows | Repeated short links with optional descriptions. | Consistent link icon/gutter, title, description; no speculative network preview or fabricated favicon. |
| Outline / spacious list | Nested, uneven, long, ambiguous, or strongly dependent items. | Subtle hierarchy guides and deliberate spacing; normal flow is a designed outcome too. |

Evaluate flat lists of two through twelve items initially, with explicit complexity budgets rather than a universal three-to-nine gate. Larger lists may use compact vertical/outline treatments; later grid extension must be justified by measurements. A single item does not become a lonely card by default.

Consider row partitions as well as column counts: four items can be `2+2`, five `3+2` or `2+3`, seven `3+2+2`, subject to measured size and consistent row reading. Penalize orphan cells and large accidental voids. Do not reorder items by height, use masonry for ordered material, or enlarge the first card solely to create a fashionable asymmetric shape. Unequal emphasis needs evidence in the authored content.

### Section compositions

- **Opening:** actual title, genuinely introductory paragraph, and nearby authored metadata/badges. Missing roles are omitted. Do not steal the first paragraph after an H2 to manufacture a document subtitle.
- **Prose and supporting panel:** related explanation beside a quote, alert, decision, checklist, or compact specification. Try `7:5`, `8:4`, and equal splits where appropriate.
- **Repeated sibling features:** two/three related heading/body units, each internally measured. Keep heading hierarchy and anchors; do not add a card inside another card.
- **Technical pair:** explanation beside its code/table/diagram; or explicitly paired examples with aligned headers. Preserve code exactly and stack when its minimum width wins.
- **Specification/comparison band:** compact properties beside prose or a wider comparison table. A wide comparison stays a table; do not silently turn it into records.
- **Gallery:** source-adjacent images measured by aspect ratio, with full-image containment and no auto-caption from alt text. A small image remains small.
- **Question/answer and dated entries:** open FAQ rows and timelines only for repeated explicit questions or dates. No automatic collapsing, sorting, or timeline inference from ordinary prose.

The current 12-track system is a useful common alignment lattice. Retain it, but let templates span multiple rows and negotiate internal variants. Content width is canvas width minus actual shell, margin, and gutter space, not window width or a fixed character-count breakpoint.

## 6. Typography, spacing, numbers, and small details

Resolve a semantic spacing tree before layout. A section requests space around itself; a heading requests a keep-with-next relationship; a card supplies its own internal rhythm. When adjacent boundaries meet, resolve compatible requests using the larger required gap rather than summing unrelated margins. Consecutive headings receive an explicit heading-to-heading rule.

Starting tokens below are proposals for native visual tuning, in logical pixels at 100% zoom. Scale typography and spacing together. Keep the existing font families and calibrated Fraunces instances.

| Relationship | Starting range |
| --- | --- |
| Body text | 18 px / 29 px leading; prose normally about 65–80 characters per line, measured with the actual font. |
| Introductory lead | 21 px / 32 px; only at a legitimate opening. |
| H1 / H2 | Approximately 44–48/50–56 and 28–32/36–40; tune line breaks against reference pages. |
| Paragraph to paragraph | 18–24 px. |
| Before / after major section H2 | 64–80 / 20–24 px, except at a composition's beginning. |
| Before / after H3 | 36–48 / 12–16 px. |
| Title/lead block to first section | 40–56 px. |
| Consecutive headings | 24–32 px for H1→H2; 16–24 px for subordinate pairs; never collapse to a tiny body gap. |
| Between composition bands | 32–48 px, distinct from a major section boundary. |
| Card insets / label to description | 20–28 / 8–12 px. |
| Card gutters / reading-column gutters | 20–32 / 32–48 px. |
| Component to related explanation | 12–20 px. |

Measure prose independently from wide components, but share a deliberate leading edge. Avoid the previous combination of narrow text marooned beside unused space. On wide canvases, the compositor can assign that space to a related element or a second reading column. Where no such relationship exists, whitespace remains legitimate.

### Hanging geometry

- Split an authored chapter prefix into a separate visual number gutter, not just a differently colored run in the title line. Preserve its source range, punctuation, and heading anchor.
- Measure the widest sibling number, including `9.5`, `10.12`, and large numbers; reserve a shared gutter plus 12–16 px separation. Wrapped heading lines align to the title text, not the numeral.
- Align heading titles, paragraph text, and relevant component content through shared anchors. Nested headings should gain hierarchy from typography and spacing, not unlimited accumulating indentation.
- Give list markers their own measured track. Center circles and numerals using font ascent/descent and optical checks; never approximate vertical centering with a text baseline offset alone.
- Use one coherent existing vector-icon family. Task controls remain identifiable checkboxes with checked, unchecked, hover, focus, and disabled states; a plain bullet must not acquire a “completed” checkmark.
- Borders are painted once, snapped to device pixels. Use fine solid table rules, not overlapping cell edges or dotted default borders. A thick callout rail and its rounded outline share one path and equal corner radii. Insets must include rail width.
- Use heading typography and spacing for hierarchy; no heading shadows or decorative heading badges in this direction.

### Chips and semantic icons

Resolve eligible chips before measuring text: authored HTTP methods/status codes in an endpoint context, explicit status fields, versions/tags, language headers, and literal color values. Keep the original text; frame and icon are decoration. Status color requires explicit evidence, not keyword guessing in ordinary prose. Retain a readable label so color is not the only signal.

Each chip has intrinsic width, baseline, padding, selection mapping, and wrapping rules. Long values revert to a readable inline treatment rather than overflowing. Copy and autosave contain only canonical content; generated completion counts and decorative icons must not leak into copied Markdown. Passive chips must not look like buttons. Actual interactive controls use native hover/focus states and accessible names.

## 7. Multi-column prose and wrapping components

These features are in scope for this milestone, not left indefinitely behind an “editorial mode” switch.

### Bounded reading columns

Column layout fragments a continuous text flow, whereas a card grid places independent items. Treat them as different primitives. The W3C multi-column model is a useful reference for balancing, breaks, and spanning headings; this is a native implementation, not a CSS migration. [Multi-column model](https://www.w3.org/TR/css-multicol-1/#the-multi-column-model)

- Nominate two columns for sufficiently long, ordinary prose within one section. Both columns need a useful reading measure: initially at least about 42 characters, preferably 45–60, measured for the actual language/font. Do not shrink type to qualify.
- Bound each reading band to approximately `min(560 px, 0.65 × usable viewport height)` at 100% zoom. This is a proposed ergonomic limit, not a standards requirement. Require enough content for roughly eight body lines in each column; otherwise use one column.
- Balance at paragraph boundaries first. Where necessary, split paragraphs only at shaped line boundaries with widow/orphan guards, initially three lines each side. Respect explicit hard breaks and keep headings with at least the first three body lines.
- Continue additional content in the next band below, never in more columns offscreen to the right. Wide headings, tables, equations, or figures interrupt the column flow and span a new band.
- Reading traversal is first column top-to-bottom, then the second, then the next band; mirror column progression for RTL context. Card grids remain row-major. Accessibility and copying follow canonical source order in both cases.
- Do not automatically fragment code, tables, task groups, poetry/hard-break-heavy passages, or unknown HTML structures. At high zoom or in short windows, choose a readable single column.

### Text around a small table or image

Implement an exclusion region: lines alongside the component receive a narrower available interval; lines below it recover the full reading width. This is actual text flow, not two rectangles with an empty gap beneath the smaller one. The W3C Shapes definition provides a useful model of shortening line boxes beside a float. [Wrapping model](https://www.w3.org/TR/css-shapes/#terminology)

- Start with one rectangular float per band, on the logical leading or trailing side. No contour tracing, overlapping floats, or floats nested inside multicolumn bands in this milestone.
- Require a source-adjacent, related component within the same section; anchor it at its original position in logical flow. Do not move a figure up across preceding paragraphs or pull prose across a heading to fill space.
- Measure a compact table's minimum width, row heights, header, and cell content. Initially nominate small property tables, typically two/three columns and a handful of rows. Reject comparisons or rich cells needing more width; never scale text down to make a table float.
- Measure images at preserved aspect ratio, considering intrinsic resolution and authored size. Keep screenshots/diagrams uncropped and avoid enlarging badges.
- Reserve 20–28 px between text and component and a comfortable remaining text measure, initially about 38 characters minimum. Below that, or when wrapping creates only a few stunted lines, choose a normal side-by-side band or stack.
- Preserve captions only when authored. Preserve the full table structure, selectable cells, image alt text, and local link behavior.

Both primitives require a node to own multiple visual fragments. This is the primary editing/geometry risk and must be implemented as a first-class capability before either feature ships.

## 8. Measurement and composition algorithm

Adaptive grid-based document design is an established research direction; the relevant idea here is an explicit design vocabulary that adapts to content and available space, not arbitrary free-form rectangle packing. This plan is a project-specific proposal, not a claim that the current engine is research-leading. [Adaptive Grid-Based Document Layout](https://www.microsoft.com/en-us/research/publication/adaptive-grid-based-document-layout/)

### A. Resolve presentation before measuring

Create a `ResolvedPresentation` for a candidate: font faces/weights/sizes, inline runs, label breaks, spacing, markers, chips, borders, and component constraints. The measurement and paint paths consume the same result. No paint-only font substitution or padding change is permitted.

Measure at the actual candidate widths and retain a footprint: min/preferred widths, height-by-width samples, line counts, legal breaks, heading/body baselines, marker/label anchors, overflow, indivisible children, image ratio, and source coverage. Intrinsic measurements nominate widths; final native shaping decides fit. Character counts are only bounded-work prefilters.

Cache by immutable content identity/revision, presentation/template version, resolved font environment, width, zoom, direction, resources, and edit state. A resource completion must invalidate the affected measurements, not silently reuse an old footprint.

### B. Generate candidates hierarchically

For each group, generate a bounded set of eligible presentations. Remove dominated alternatives with equivalent content/traversal and worse fit/readability. An outer band asks its children for their best candidates at its proposed slot widths. Do not commit the outer row first and then prevent a child list from considering its own grammar.

For example, compare `prose + vertical feature panel` against `full-width prose + three-column open features` as complete alternatives consuming the same source. Compare `small table with wrapped prose` against both a plain pair and a stack. Internal whitespace and total section rhythm become visible to the decision.

### C. Search bounded section windows

Reuse the current bounded row-search foundation. Extend its state to band family, alignment anchors, preceding rhythm, and child candidate IDs. Exact dynamic programming remains useful for small row partitions; use a bounded beam when fragment/band combinations add dimensions. Start with a maximum 40-group lookahead window, up to eight retained candidates per group, and beam width 16; these are tunable work limits, not visual rules. Carry limited rhythm state across windows.

All paths must have a valid simple-flow solution. Search cannot cross section barriers merely to fill space. Bound recursion, shaping work, and retained geometry; reject an expensive optional transformation rather than truncating content.

### D. Reject first, then score

Hard constraints: exact source coverage; legal reading order; preserved hierarchy/relationships; no overlap or overflow; readable minimum measures; bounded columns; heading keeps; valid fragment maps; available resource measurements; active edit locks.

Then score comparable alternatives using:

- Reading comfort: line lengths, line counts, widows/orphans, cramped labels and code.
- Grouping quality: related material nearby, clear markers, consistent heading/content anchors.
- Space quality: accidental holes, orphan cards, extreme peer-height imbalance; intentional margins are not “waste.”
- Hierarchy and rhythm: adequate section breathing room, coherent emphasis, restrained panel density, excessive repetition of the same family where equally suitable alternatives exist.
- Visual complexity: too many borders, chips, nested containers, or competing alignments.
- Stability: change in topology, fragment assignments, and scroll-anchor displacement.
- Recognition confidence: uncertainty costs more for semantic regrouping than for spacing.

Normalize scores over the same consumed source/band area. Summing per-row costs without normalization can favor fewer rows for mathematical reasons rather than better design. Remove count-based winner overrides. Count nominates; actual presentation quality selects.

Variety is a small tie-breaker between suitable choices, not a requirement to alternate styles. Identical content and environment produce identical plans. Initially tune weights through hand-ranked candidate pairs and held-out documents; do not add online learning or randomness.

### E. Publish a stable plan

Publish immutable geometry atomically with its source/environment revision. Require a meaningful measured improvement, initially 10–15%, before switching a still-legal topology after editing or resize settles. Unlike permanent retention of a previous grid, hysteresis must eventually allow a materially better layout.

Capture the source position and screen offset of the scroll anchor before publication, then restore them through the new fragment map. Viewport dimensions may affect planning; scroll position must not trigger template reconsideration.

Provide developer-only traces listing every eligible group, candidates, measurements, rejection reasons, score terms, and selected template. This is essential for diagnosing “why was this left as a list?” without adding user-facing layout controls.

## 9. Editing and accessibility invariants

Introduce explicit flow regions and fragments containing source ranges, shaped-line references, bounds, local transforms, and traversal links. Maintain both source-to-fragment and point-to-source indexes. Preserve grapheme boundaries and caret affinity at column and wrapped-line boundaries.

- One source node may have several rectangles; navigation, search, selection, and accessibility must not assume one bounding box per paragraph.
- Pointer hit testing uses fragment geometry. Keyboard movement follows native editing expectations through shaped lines; logical selection/copy still traverses the source exactly once.
- Freeze the active group's structural template, slot widths, and column break assignments during typing, IME composition, or drag selection. Let active content grow and move following content down; temporarily exceeding a reading-band height target is better than relocating the caret to another column.
- Reconsider structure after a suitable idle pause and completion of composition/selection. Necessary narrow-window fallback preserves the source caret and scroll anchor; it is not deferred if the retained layout would clip content.
- Images loading, font changes, and table edits invalidate only affected groups plus their dependent composition window. Publish the newest valid revision; discard stale background results.
- Visual labels, number gutters, and chips do not alter Markdown. Formatting, undo/redo, hard breaks, nested lists, table commands, and byte-preserving save must work across every template.
- Decorative icons are not announced redundantly. Semantic task controls, links, headings, tables, and disclosures keep their roles and names. Text remains readable without relying on color.
- A11y traversal uses the canonical logical sequence, while screen extents come from the fragment map. Test RTL, mixed-direction text, zoom, and keyboard-only operation.

No source rewrite, portable annotation, persisted layout choice, or separate reading/editing mode is needed.

## 10. Runtime ownership and scrolling budget

Keep document semantics/transactions in `document-core`. Keep role evidence, templates, measurements, composition, and derived geometry in `document-view`. `markdown-app` supplies shell dimensions, environment, resources, and scheduling; it does not acquire parallel layout heuristics.

| Existing area | Planned responsibility |
| --- | --- |
| `adaptive/groups.rs`, `adaptive.rs` | Structural evidence, section boundaries, template eligibility; migrate simple layout flags toward typed presentations. |
| `adaptive/candidates.rs`, `adaptive/rows.rs` | Measured template candidates and hierarchical composition; remove the competing greedy/count-driven decisions when replaced. |
| New focused modules under `adaptive/` as needed | Grammar/template definitions, semantic spacing, fragment-flow algorithms; split by ownership rather than create a module per visual variant. |
| `editor/measurement.rs`, `editor/arrangement.rs` | Shared resolved styles, native measurement, region/fragment realization. |
| `editor/published_geometry.rs`, `editor/geometry_cache.rs`, spatial/height indexes | Immutable plans, incremental invalidation, visible-region queries, bidirectional source geometry. |
| `editor.rs`, selection/navigation/search/accessibility modules | Consumers of published geometry; no independent font, layout, or source-order decisions. |
| `theme.rs` and existing icon assets | Shared typography, spacing, surface, rule, marker, and chip tokens. |

Do preparation on the existing safe worker/scheduling path, respecting the text backend's threading contract. Coalesce work, cancel superseded plans, and avoid holding cache locks during shaping. Never replace a full-document blocking operation with an unbounded task per block.

On scrolling, the path is: query visible bands/fragments, reuse shaped text and component geometry, paint. Target `O(log n + visible fragments)` with **zero template searches, native remeasurements, or whole-document scans caused by scroll motion**. Existing momentum scrolling and the outer document scrollbar remain unaffected.

Measure cold and warm preparation time, peak/cache memory, invalidation size, and editing publication latency separately. Slower startup is allowed, not an unresponsive window or unbounded memory growth. Cheap ordinary-flow candidates remain available for pathological inputs.

Performance qualification must use release builds and the same recorded machine/compositor/refresh rate: 100 KiB, 1 MiB, and 10 MiB documents plus composition-heavy fixtures. Use sustained repeated scroll runs, initially five runs of at least 60 seconds, reporting draw p50/p95/p99, presentation intervals/missed frames, long stalls, and memory. Above 60 presented fps requires a display/compositor capable of it; on a 60 Hz display report refresh-rate saturation and frame misses instead of an impossible claim. Retain 120 Hz as a stretch goal. No new performance claim follows from this planning document.

## 11. Delivery sequence and gates

Each phase must retain a working editor. Public layout controls are not a migration mechanism; developer-only candidate forcing is acceptable for visual tests.

| Phase | Deliverable | Exit gate |
| --- | --- | --- |
| 0 — Reference contract | Eight source-backed fixture pages corresponding to the eight designs; current native captures; hand-composed target placements using the same complete text. Include ordinary real `plan.md`, README, a technical specification, and prose-heavy material as holdouts. | Review the target composition, spacing, and content coverage before tuning heuristics. No synthetic summaries inserted to make a screenshot work. |
| 1 — Shared geometry and rhythm | Resolved presentation shared by measure/paint; semantic spacing; hanging headings/numbers; measured icon/chip primitives; region/fragment model first exercised in ordinary single-column flow. | Font metrics agree; source↔geometry round trips, caret, selection, scaling, borders, and heading spacing pass native checks. |
| 2 — List grammar | All listed core short-list families, ordinary/plain-label recognition, explicit inline enumerations, row partitions, measured widths, diagnostics. | Positive and negative fixtures for every family; automatic selection across whole documents, not only the opening panel. |
| 3 — Section compositor | Hierarchical candidate negotiation, asymmetric prose/panel and technical compositions, openings/metadata, repeated sections and galleries, normalized scoring and hysteresis. | Full-page variety and alignment match reference families without invented semantics or decorative overuse. |
| 4 — Continuous text flow | Bounded balanced prose columns and rectangular text wrapping around eligible images/tables, on the common fragment model. | Cross-column editing/copy/accessibility and resize behavior work; no clipped content, giant reading columns, or empty rectangular gaps below a float. |
| 5 — Qualification | Held-out visual review, score calibration, removal of replaced heuristic paths, release performance and regression tests, updated evidence ledger. | All acceptance criteria below pass. Phase 2 alone is not completion of this request. |

Treat phases 1 and 4 as the highest correctness risk. If fragment-aware editing fails its gate, repair that foundation; do not ship a visually convincing but uneditable screenshot path. If phase 3 still favors stacks, use candidate traces and pairwise review to fix recognition/scoring rather than hardcode more document titles.

## 12. Visual and functional acceptance

### Native visual evidence

Extend `performance/capture-layout.py` and the existing fixture/preview workflow. Record build identity, source hash, viewport, scale, fonts, chosen templates, and measured geometry beside each capture. A web mockup or generated image is not confirmation of the running GPUI layout engine.

1. Capture all eight reference families at regular and wide widths, including lower sections and multiple adjacent bands. Also capture real documents without editing their content to help the engine.
2. Maintain both developer-forced template fixtures, to validate the renderer, and automatic-selection fixtures, to validate recognition and ranking. Passing the former does not prove the latter.
3. Include narrow 520 px, intermediate 768 px, regular 1280 px, wide 1600/1920 px windows, a short 480 px-high window, and 100/125/150/200% scaling. Compute fit after sidebar/padding deductions. Test reduced-motion and high-contrast/focus visibility where supported.
4. Inspect at native scale: heading air and line breaks; shared anchors; number/checkbox centering; card widths/heights; inset equality; sharp borders; table readability; chip baselines; image preservation; transitions between bands; and overall panel density.
5. Compare candidate alternatives with the same text. Establish reviewed reference pages and a small visual rubric for hierarchy, rhythm, balance, reading clarity, and restraint. Pixel diffs catch regressions; human visual review judges composition quality.

### Correctness and selection coverage

- For each template family, test a positive witness, near-threshold width/height case, and a negative case where simple flow is correct. Test counts 2–12, long labels, uneven content, nesting, task state, and inline punctuation.
- Every eligible group anywhere in a fixture must appear in a decision trace with measured candidates or explicit rejection reasons. A test asserting “at least three grids” is insufficient.
- Test equal content with different harmless Markdown syntax: plain/bold/code labels, setext/ATX headings, links and inline emphasis. Do not require an author to rewrite everything into bold prefixes.
- Assert exact canonical source coverage once in logical traversal; no dropped, duplicated, reordered, or manufactured text. Verify save and copy against exact expected content, including delimiters and hard breaks.
- Exercise caret/selection across every marker, chip, column boundary, and wrapped component; IME; undo/redo; adding enough text to outgrow a card; table row/column edits; changing image dimensions; zoom and rapid resize; font/resource completion during editing.
- Assert headings remain attached to following content, required lines remain visible, no illegal overlaps occur, and every chosen plan has a readable fallback. Include malformed/huge source and unavailable resources.
- Run workspace formatting, checks, Clippy, Rust tests, and relevant capture/interaction Python tests when runtime implementation occurs. Use the existing installed-font/native environment for geometry tests rather than substituting guessed font metrics.

### Completion definition

The milestone is complete only when the native app demonstrates the eight reference composition families, selects multiple appropriate treatments across complete real documents, supports both bounded columns and component wrapping with intact editing, and meets the performance gate. The final evidence must include actual screenshots and measurements, with remaining mismatches called out explicitly.

This planning change does not modify the running engine. The next implementation step is phase 0's reference contract together with phase 1's shared presentation/spacing foundation.
