# Document design grammar

An extracted visual language and proposed composition system based on the eight supplied screenshots.

## Status and authority

The references establish a visual direction, not an executable specification. Exact fonts, colors, spacing, breakpoints, interaction behavior, and pagination cannot be recovered reliably from raster screenshots. The values below are proposed implementation tokens. This text is the authoritative grammar; the generated image boards are visual specimens, not exact measurements or production-ready text assets.

**Observed:** warm near-white surfaces, dark serif display headings, mixed serif and sans-serif reading modes, compact monospace technical content, muted green accents, light sage states, hairline borders, small radii, open sections, modular columns, margin rails, structured lists, restrained semantic color, and natural editorial photography.

**Proposed extensions:** explicit content-driven layout selection; formal icon meanings; reading-width constraints; genuine text wrapping; bibliography and equation numbering; full page masters; page-break rules; running furniture; narrow-screen behavior; print translations; empty, loading, and error states; metric and timeline specimens.

## 1. Foundation tokens

| Token | Proposed value | Purpose |
|---|---|---|
| Paper | `#FAF9F6` | Main canvas |
| Surface | `#F2F1EC` | Quiet secondary surface |
| Ink | `#152421` | Headings and primary text |
| Body | `#36423E` | Reading text |
| Muted | `#606B65` | Secondary labels; check at final size |
| Rule | `#DADDD5` | Dividers and nonessential boundaries |
| Green | `#3F6247` | Links, active state, selected emphasis |
| Sage | `#E8EEE2` | Positive or editorial tinted surface |
| Blue / pale blue | `#225C83` / `#EDF4F9` | Neutral information |
| Amber / pale amber | `#875500` / `#FFF4DF` | Warning |
| Red / pale red | `#9B302D` / `#FBEDEA` | Error or critical caution |
| Code dark / text | `#202A2B` / `#F0F2ED` | Dark code variant |
| Spacing | 4, 8, 12, 16, 24, 32, 48, 64 CSS px | Screen rhythm |
| Radius | 4 px default; 8 px media; pill only for short statuses | Quiet enclosures |
| Border | 1 CSS px; 0.5–0.75 pt print | Fine boundaries |
| Shadow | None in document content | Preserve the paper-like surface |

Use color roles consistently; do not let green mean both ordinary decoration and confirmed completion in the same context. A thin colored rule may identify editorial emphasis without implying success.

### Typography

Use an editorial serif for display headings; a readable serif for narrative mode; sans-serif for reference mode, tables, metadata, and controls; monospace for code, identifiers, and literal values. Exact source fonts are unconfirmed. Portable starting stacks are `Georgia, serif`, `system-ui, sans-serif`, and `ui-monospace, monospace`.

| Role | Screen starting point | Print starting point | Behavior |
|---|---|---|---|
| Display title | 44–56 px / 1.05–1.12 | 28–36 pt / 1.1 | One document title; adapt to actual width |
| Section heading | 28–32 px / 1.15 | 18–22 pt / 1.2 | Stronger space before than after |
| Subsection heading | 21–24 px / 1.2 | 14–16 pt / 1.2 | Keep with following content |
| Minor heading | 17–18 px / 1.3 | 11–12 pt / 1.3 | Avoid excessive heading levels |
| Narrative body | 18 px / 1.5–1.6 | 10.5–11.5 pt / 1.4–1.5 | Comfortable continuous reading |
| Reference body | 16 px / 1.45–1.55 | 10–11 pt / 1.35–1.45 | Denser but still readable |
| Caption / metadata | 13–14 px / 1.4 | 8.5–9.5 pt / 1.35 | Never the only carrier of a critical instruction |
| Code | 13–14 px / 1.5 | 8.5–9.5 pt / 1.4 | Preserve characters and whitespace |

These are starting values, not universal minimums. Honor reader zoom and larger text preferences. Reflow or paginate before reducing type. Use one coherent body mode within a section. Use bold for importance, italic for linguistic or editorial emphasis, underline for links, and monospace for literal syntax. Do not rely on color alone to identify links or status.

## 2. Semantic selection rules

Choose the component from the relationship between items before choosing its geometry.

| Content meaning | Default form | When it can change |
|---|---|---|
| Explanation or argument | Flowing paragraphs | Add headings when a new topic begins |
| Unordered concise points | Plain bullet list | Borderless grid for independent, similarly short items |
| Unordered points with explanations | Bold lead-in plus hanging body | Cards only if items are self-contained entities |
| Ordered actions | Numbered list | Stepper when steps contain supporting blocks |
| Brief ordered stages | Numbered row or grid | Only if order remains unmistakable; otherwise vertical |
| Rank | Numbered ranked list | Retain rank through every reflow |
| Stable reference items | Explicit identifiers | IDs stay stable across layout changes |
| Tasks with completion state | Checkbox list | Progress summary only from known task states |
| Events tied to dates | Timeline | Horizontal only when labels and dates fit |
| Terms and explanations | Definition list | Two aligned columns when descriptions fit |
| Metadata or properties | Key–value rows | Compact header strip when values are short |
| Entities sharing attributes | Comparison table | Labeled records only when comparisons remain usable |
| Parent–child structure | Nested outline or tree | Collapse optional branches on interactive surfaces |
| Supporting external object | Resource card | Compact linked row when preview adds little |
| Important exception or contextual instruction | Callout | Inline lead-in when a separate block is unnecessary |
| Quotation | Blockquote plus attribution | Pull quote only as a deliberate editorial highlight |

### List composition

1. Use bullets only for unordered siblings. Use a consistent marker size and hanging indentation.
2. Short fragments use tight spacing. Multi-sentence items get paragraph spacing inside the item and larger spacing between items.
3. Preserve parallel wording and punctuation within a list. Do not fabricate content to balance a grid.
4. Grid eligibility: independent items, usually 3–6 entries, each with a short label and at most about three body lines at the chosen width. The tallest item should generally be no more than about 1.5 times the shortest. Measure after actual text rendering.
5. Long, uneven, nested, or dependent items stay vertical. A last grid row may be incomplete; avoid stretching one item into a different visual class solely to fill the row.
6. Grid reading order is left-to-right then top-to-bottom for the supplied English content. Reflow preserves source order. Adapt direction explicitly for other writing systems.
7. Numbers signal order, rank, counts, or reference. Ordinary decision drivers and features receive bullets or labels, even where a reference screenshot uses decorative numbers.
8. An inline numbered sequence is appropriate only for a few very short stages. A step with a paragraph, warning, table, or code sample uses a vertical step layout.
9. A prerequisite list is not a completed checklist unless completion is known. Use bullets for unverified requirements.
10. Nested lists use modest indentation and visible grouping. Beyond three visible levels, prefer named subsections or a tree view rather than squeezing the text.

## 3. Icon and signal grammar

| Meaning | Symbol family | Treatment |
|---|---|---|
| Neutral note | Information circle | Blue or neutral; label “Note” |
| Optional helpful advice | Lightbulb | Green; label “Tip” |
| Important requirement | Flag or prominent text label | Label “Important”; no gear unless it means settings |
| Potential problem | Triangle with exclamation | Amber; label “Warning” |
| Failed validation / critical caution | Error circle or octagon | Red; explicit label and explanation |
| Confirmed completion / valid result | Checkmark | Paired with text or a true checked state |
| Decision | Gavel or decision label | Use only in decision records, not every recommendation |
| External resource | Link or document symbol | Destination title remains readable |
| Copy | Overlapping sheets | Interactive control; display copied feedback when real |
| Expand / collapse | Chevron | Direction matches state; pair with disclosure title |
| Playable media | Play symbol | Interactive player; static print uses poster and destination |
| Settings | Gear | Reserve for settings |

Choose one icon family with consistent stroke weight and optical scale. Align to the first text line in short rows and the title in longer callouts. Hide decorative icons from assistive reading; give interactive icon controls accessible names. Do not replace numbers with category icons in an ordered procedure. Do not communicate severity or validity through color alone.

## 4. Cards, callouts, and open flow

Use a card when a unit has its own identity, boundary, or action and can be understood as a discrete object: a resource, decision, selected option, isolated example, metric with context, or media embed. Normal narrative sections stay open.

Prefer whitespace first, then a divider, then a tinted surface or border when semantic grouping still needs help. A title over prose does not automatically require a box. Avoid routine card-inside-card nesting. A code pane inside a worked example is a justified functional exception.

Cards have a consistent inset, a clear label when needed, and natural height. Do not force equal heights when that creates large empty areas; use a row layout instead. An entire card is clickable only when it represents one destination, with an accessible focus state on interactive surfaces.

Callouts are short and positioned where they matter. Place warnings before the affected action. Split a long explanation into ordinary prose with one short callout rather than boxing an entire page. Pros and cons can share aligned columns, but headings and labels must make the distinction without red/green color. A metric needs a label, unit, period or denominator, and baseline when showing a change; omit trend claims without data.

## 5. Width, flow, and available space

Define usable width as page/container width minus horizontal margins and any navigation or reserved rail. Define usable page height as page height minus vertical margins and running header/footer space. Calculate component fit against these usable dimensions, using loaded fonts and real content.

**A full-width block is allowed.** Separate its outer span from its internal text measure. A section background or card can span the content area while its prose stays at a readable width. Body copy starts with a preferred measure of 55–75 characters; treat 80–85 as a review threshold, not a license to stretch all text. Short headings, metadata strips, tables, figures, code, and rules can span more freely.

### Proposed screen/container thresholds

| Usable width | Default arrangement | Space use |
|---|---|---|
| Below 560 CSS px | Single column | Stack cards, media, and asides; collapse navigation |
| 560–899 CSS px | Main reading column | Pair short independent blocks only if their minimum widths fit |
| 900–1199 CSS px | Main column plus optional rail, or two modules | Choose according to content; not every section needs columns |
| 1200 CSS px and above | Main plus rail, paired sections, or compact feature grid | Cap prose width; avoid adding columns just because width exists |

These thresholds use the document container after subtracting application chrome, not the overall viewport. Container constraints override the suggested width buckets. For `n` columns, every column's minimum width plus all gutters must fit. Reading columns should generally retain at least about 35–40 characters per line; short metadata and compact cards may be narrower.

### True text wrap versus a paired layout

Use a float-like wrap only for a supporting image, portrait, small pull quote, or similarly optional element. Typical float width is 25–35% of the text region with a 16–24 px gap. Require about 35–40 characters of remaining prose width and at least four useful lines beside the object. Keep its caption with it and clear the wrap before the next unrelated heading. A paired figure-and-text module is not the same as wrapping: in true wrap, prose returns to full measure below the figure.

Stack instead when the surface is narrow, the remaining measure is cramped, content order becomes ambiguous, or the element is tall enough to create a narrow text corridor. Tables, code, math requiring alignment, essential diagrams, and critical warnings remain block-level. Margin notes become inline asides near their anchors when the rail disappears.

## 6. Aspect ratio and density

Aspect ratio suggests a page master; actual usable width, height, reading distance, and content determine whether that master works.

| Surface | Suggested master | Overflow behavior |
|---|---|---|
| A4 / Letter portrait | Main column; optional modest margin rail | Continue across pages at semantic boundaries |
| Portrait report with wide evidence | Main column plus occasional spanning table/figure | Isolated landscape evidence page if necessary |
| 3:2 or 4:3 landscape | Paired modules or main plus supporting rail | Add rows only while readable; paginate when finite |
| 16:9 presentation-style page | One focal claim with supporting modules | Split to another page; use larger type for viewing distance |
| Square | Stacked bands or balanced two-part composition | Avoid forcing a three-column layout |
| Tall narrow screen | One continuous column | Scroll; retain source order and document anchors |
| Small landscape screen | Often still one column | Height and navigation constraints can outweigh its shape |

Density presets change spacing and arrangement before typography: **reading** favors open prose and 24–40 px block gaps; **reference** favors compact rows and 12–24 px gaps; **overview** favors short labeled modules and 16–24 px gaps. Never silently summarize, omit, or truncate authored content to satisfy a space budget. Author-approved summaries may link to full detail.

## 7. Technical and evidence components

- **Tables:** use real row/column headers, meaningful units, consistent numeric alignment, and restrained rules. Left-align prose; right-align comparable numbers, ideally with tabular figures. Use an em dash only with an explicit meaning when ambiguity matters. Long cell text may wrap. Repeated header rows belong on printed continuations.
- **Narrow tables:** preserve a scrollable semantic table when side-by-side comparison matters. Convert to labeled records only if every value retains its header and the result supports the task. Do not silently drop columns. For print, split by row, use a wider page, or split columns while repeating the identifier column.
- **Code:** preserve exact source and distinguish optional visual line numbers from copied text. Use language labels and optional copy controls on screen. Short commands may use dark code strips; larger examples can use light or dark panes consistently. Do not replace source with an ellipsis unless it is explicitly an excerpt.
- **Code overflow:** scroll horizontally on screen where necessary. In print, first use wider placement, then syntax-safe visual wrapping without changing the underlying source, then labeled continuation chunks. Avoid shrinking until unreadable.
- **Equations:** keep inline math within prose when compact; use a display equation for substantial expressions. Number only when cross-referenced or required by document convention. Keep symbol definitions nearby. Production equations must use a math renderer, not raster-generated lettering.
- **Diagrams and charts:** use deterministic, accessible rendering for actual topology and data. Give sufficient label size, captions, and prose alternatives. Generated board thumbnails demonstrate styling only and must not serve as authoritative quantitative or technical figures.
- **Trees:** align nesting guides and labels; preserve parent–child order. A tree is for hierarchy, not chronological steps.
- **Request/response and valid/invalid pairs:** align side by side only when both remain legible. Stack with explicit labels on narrow surfaces. Status codes and textual outcomes remain visible in grayscale.

## 8. Media and editorial components

Figures have a deliberate crop, caption, optional credit, and meaningful alternative text on accessible digital output. Do not crop away information from screenshots, maps, charts, or other evidence. Number figures when referenced; otherwise a descriptive caption can suffice.

Galleries use consistent image treatment unless a deliberate editorial composition calls for varied sizes. Use individual captions when each image needs identification and a shared caption only when it describes the whole set. Video and audio need labels, duration when known, and an accessible text alternative. Print uses a still/poster or compact audio row plus a destination; it does not imply playback on paper.

A quotation retains attribution; a pull quote does not replace the source passage. Margin notes stay near their anchor and move inline when space is insufficient. Footnote numbering follows source order, not visual column placement. Bibliography entries use a consistent citation style and hanging indentation. A glossary uses term–definition structure. Thematic breaks signal a real conceptual transition, not empty space filling.

## 9. Pagination grammar

Use stable pages for reports, manuals, proposals, specifications, and exports. Use continuous reading for live reference documentation unless the task calls for page-based navigation. A paged preview and a continuous editor can share one semantic source.

### Page masters

1. **Cover / opening page:** title, short description, essential metadata, optional image. Use a dedicated cover only when the document warrants it; a short memo starts with a compact title block.
2. **Standard reading page:** running document or section title, main content, optional note rail, folio.
3. **Reference page:** compact tables/code/examples, repeated local labels, preserved reading order.
4. **Wide evidence page:** wider or landscape table/figure, caption, source, and ordinary folio continuity.
5. **Appendix / references page:** clear section label, citations, glossary, or technical detail.

### Break priorities and constraints

1. Honor explicit author page breaks and deliberate section starts.
2. Prefer breaks between sections, then paragraphs, then list items or table rows.
3. Keep a heading with at least two following lines of text or the opening part of its component. Keep short lead-in labels with their associated content.
4. Avoid a single stranded paragraph line; aim for at least three lines at either side of a split when feasible. Relax preferred line counts before violating content or creating overflow.
5. Keep short list items intact. Split a long item only between paragraphs while retaining its number and adding “continued” if needed.
6. Keep a warning and the first affected action together. Keep a figure with its caption and a quotation with attribution.
7. Move an atomic component to the next page when it fits there. If it exceeds a whole page, switch to its permitted split mode rather than repeatedly pushing it forward.
8. Repeat table headers on every continued page. Prefer row boundaries; if a single row exceeds a page, split its content with repeated identifying labels and a clear continuation marker.
9. Split long code at logical boundaries with the language, example title, and “continued” label repeated. Retain meaningful line numbering when used.
10. Place footnotes on their anchor page when feasible. If notes cannot fit, use an explicitly marked continuation or a consistent endnote mode; do not silently detach them.
11. Do not force every page to be equally full. Let section endings breathe. As a review heuristic, investigate an accidental final page with less than about one-quarter usable height occupied, but never solve it by silently deleting content.
12. Running headers, footers, and folios occupy reserved space. “Page x of y” is computed only after pagination converges. Generate the contents page references from the final layout.

### Pagination procedure

1. Parse semantic blocks and relationships, including figure–caption, heading–body, warning–action, and footnote anchors.
2. Apply the appropriate surface master, subtract furniture, and load the actual typefaces.
3. Measure preferred, minimum, and splittable sizes using the real content.
4. Choose the richest valid arrangement under the semantic and readability constraints.
5. Place blocks in source order, selecting break candidates by the priorities above.
6. Apply the oversized-component escape path when a block cannot fit an empty page.
7. Recalculate footnotes, contents references, and page totals; repeat until stable with a bounded iteration guard.
8. Inspect overflow, reading order, lonely headings, disconnected captions, and sparse accidental trailing pages. Report unresolved layout constraints rather than hiding content.

Do not paginate by character count alone: font metrics, shaping, images, tables, wrapping, and footnotes affect fit.

## 10. Navigation and static translation

| Interactive surface | Document / print translation |
|---|---|
| Sidebar navigation and outline | Contents, section headings, bookmarks |
| Breadcrumbs | Compact section path when it adds orientation |
| On-page contents rail | Linked section list or compact contents block |
| Search field and application toolbar | Omit from document body |
| Tabs | Show the selected scope explicitly or render necessary variants as labeled sections |
| Disclosure | Expand essential content; place optional detail in an appendix if appropriate |
| Copy control | Omit; retain language and example labels |
| Previous / next controls | Page sequence and explicit cross-references |
| Media player | Poster / label, caption, and destination |
| Hover tooltip | Inline explanation, note, or footnote |
| Loading / empty / error state | Include only if documenting the state itself; do not leave transient UI in an export |

Keep application chrome distinct from document content. Persistent side navigation consumes width on screen; it should not be baked into every exported page. Active links need a shape or rule as well as a color. A missing result and a failed load are different states: explain each accurately and expose a relevant recovery action only on interactive surfaces.

## 11. Component board coverage

“Extension” identifies a proposed addition or a formalized variant that the screenshots do not clearly demonstrate.

| Board | Observed component families | Extensions |
|---|---|---|
| 01 · Typography and foundations | Serif hierarchy, sans metadata, mono literals, links, inline emphasis, quotes, rules, title metadata, footnote anchor | Explicit token scale, measure specimen, complete footnote/bibliography styling, print roles |
| 02 · Lists and relationships | Bullet features, numbered grid, vertical steps, nested outline, tasks, long lists, prerequisites | Definition list, dated timeline, semantic number rules, unverified prerequisite treatment |
| 03 · Cards and signals | Note/tip/important/warning/caution, decision, pros/cons, resource card, badges, valid/invalid, color swatch | Formal icon mapping, metric with context, open-flow versus card specimen |
| 04 · Data and technical content | Property and comparison tables, light/dark code, language labels, copy affordance, line numbers, request/response, status codes, schema tree, math, diagram | Continued table and code treatments, equation references, narrow record fallback |
| 05 · Media and editorial flow | Hero figure, gallery, media player, paired figure/text, pull quote, margin note, caption, map preview | True wrap returning to full width, narrow stacked alternative, audio row, credits, full-span block with constrained prose |
| 06 · Page masters and pagination | Broad modular reference layouts and narrow preview | Portrait report, square brief, 16:9 overview, continuous mobile page, cover/contents/body/appendix rhythm, running furniture, explicit continuations |
| 07 · Navigation and states | Sidebar, search, top toolbar, tabs, breadcrumbs, outline, on-page contents, disclosure, previous/next, copy, issue/person chips | Focus state, empty/error/loading states, static print translations |

The boards cover the observed component families and the named additions. They are a finite core vocabulary; specialized future content can extend the grammar without inventing a new visual language.

## 12. Acceptance checks

- The chosen form conveys the content's actual relationship, especially order and completion.
- Open prose is not needlessly enclosed; cards and signals have semantic reasons.
- Full-width placement does not produce unreadably long body lines.
- Wrapped text returns to full measure below the element; narrow layouts stack coherently.
- All content survives reflow and pagination in a stable reading order.
- Labels, units, captions, attributions, and note anchors stay attached.
- No component becomes illegible to fit the available area.
- Important states are understandable without color; interactive controls have names and visible focus.
- Production exports use actual selectable text, semantic tables, real math, and accurate diagrams.
- Generated board lettering, sample data, and geometry are checked against this specification before implementation.
