# Definition lists — semantic import and measured term rails

September 8, 2026. This is progress on L02/E10, not full grammar sign-off.
Authority: `designs/document-design-grammar.md`, especially sections 2, 4, 5
and 8, and board 02's term-list specimen.

## Implemented

- Markdown `Term / : description` syntax now imports as explicit list, term
  and description containers. Terms are not headings, bullets or table cells.
  Multiple descriptions and supporting paragraphs, nested lists, code and
  nested definitions retain their source-order relationships.
- Canonical editable descendants participate in indexing, lookup, revisions,
  split, deletion, cloning, table traversal, plain text, outline and semantic
  accessibility. Untouched source remains byte-exact.
- Normal definitions serialize as Markdown. Empty or split terms, empty
  description paragraphs and multiple consecutive HTML terms use semantic
  `<dl>/<dt>/<dd>` serialization instead of dropping text or merging blocks.
  Plain standalone HTML definition lists import natively. Unknown wrappers
  and attributed list/group elements retain the existing inert HTML path.
- Loaded-font term widths choose a shared open rail on the twelve-track
  grammar. The explanation keeps a readable measure; the term rail occupies
  additional width. Below 560 usable pixels, for overlong labels, or for RTL
  labels without a mirrored-rail implementation, the list stays stacked.
- Native rows preserve baseline, source order, a 24 px gutter and 24 px row
  gap, with no enclosing cards or invented markers. Term/body stacked gaps
  are 8 px. Supporting blocks retain their own padding and controls.
- Focused rows retain their published tracks. A newly split text leaf inherits
  its original term/description rail rather than jumping to full width.

## Evidence collected during implementation

The initial [wide capture](layout-previews/definitions-wide.png) is retained
as a counterexample: the glossary incorrectly stacked even on a wide canvas.
Both columns had been constrained to one prose measure. The corrected
[width-policy capture](layout-previews/definitions-width-policy.png) shows
the measured term rail beside a separately constrained description column.
The first capture's successful source/cache gates did **not** prove its visual
layout correct; direct screenshot inspection found the missing arrangement.

Core tests cover import, rich nested round-trip, edits to every editable leaf,
unchanged neighboring source, exact undo/selection restoration, empty and
split terms, HTML synonyms and unsupported wrapper preservation. Planner
tests cover fit thresholds, multiple descriptions, source-order open slots,
and inheriting a focused rail after Enter. A native-measurement test checks
equal first baselines and separate leading positions.

## Release validation

Release binary SHA-256:
`c6b64527e4935a1a91371bdef4909f1977a62eec82d1a9bbf10e6ca586c62f1c`.
Fixture source SHA-256:
`f638592d8048071ade5e28e0bc2fdd65d23cd004e4c6d967314b1b7fb8228681`.

- [Wide](layout-previews/definitions-verified-wide.png),
  [600 px narrow](layout-previews/definitions-verified-narrow.png), and
  [200%](layout-previews/definitions-verified-200.png) captures were inspected.
  The narrow window stacks labels and bodies without clipping; 200% retains
  aligned rows where their measured minimum widths still fit.
- [Native edit](layout-previews/definitions-verified-wide.edit.json) inserted
  `x` inside `Canvas`, autosaved, retained the rail through an idle interval,
  and undid back to the exact original bytes. The targeted code and following
  heading remained byte-exact during the edit. This is not a full-document
  byte oracle for the edited serialization; canonical escaping may change.
- [Plain-text copy](layout-previews/definitions-verified-wide.copy.json)
  contains the five tested unique markers once and in source order. It is
  not an exact rich-clipboard or full-text-content gate.
- [Warm replan trace](layout-previews/definitions-verified-wide.planning.json)
  passes the existing cache gate: two requested warm replans reuse geometry
  without reshaping or anchor displacement. No cache thresholds were weakened.
- Weston MCP app 19 independently exposes description-list, description-term
  and description-value roles. The [recorded tree](layout-previews/definitions-verified.weston.json)
  has the first term at `(276,362,159,24)` and its description at
  `(459,362,527,48)`: equal tops, 24 px rail gutter. Subsequent row tops are
  434 and 506: two 24 px body lines plus 24 px inter-row gap. The second
  description for Stable identity remains a separate value at y=570.
  Terms do not create additional outline entries.
  Independent raster inspection of the second row puts both label and body
  ink at y=439–455, confirming their visible baseline alignment. The
  caret affects the first label's ink bounds, so that row is not used as a
  glyph-height oracle. The first section heading ends at y=338 and its list
  starts at y=362, preserving the 24 px heading/component gap.
- The initial AT-SPI query exposed provisional stacked bounds. Pointer movement
  and committed frame capture updated both pixels and accessible bounds to
  the aligned arrangement. That capture alone does not approve no-input
  startup. The controlled fixture-63 follow-up in TREE-SELECTION-CHECKPOINT.md
  now distinguishes successful periodic no-input startup from Weston's
  zero-refresh stall; it does not claim the complete startup matrix passes.
- A real Weston pointer click on the description's Copy button returned exactly
  the two Rust source lines including the final newline; the private fixture
  hash stayed unchanged.
- `cargo test --workspace --all-targets --locked`: **493 passed, 2 ignored**.
  Workspace Clippy with `-D warnings`, formatting and diff whitespace checks
  pass. Crusty validation `task_2ef97eb016053f85` of `ctx_584fdf313d87`
  completed with 36 existing advisory findings, none new or worsened.

The isolated [60-second definition-specimen scroll run](layout-previews/definitions-specimen-perf.json)
at 1728 × 1080, configured 120 Hz, passes the unchanged gate: **109.10 FPS**
average, **5.325 ms** draw p99, **11.141 ms** presentation p99 and
**10.584 ms** input p99. No application or presentation interval reaches
25 ms; maximum draw/presentation/input values are 7.344/13.402/12.370 ms.
No compiler/check job ran concurrently. Accessibility was inactive for the
timing run and inspected separately via Weston MCP. This small specimen is
not a 10 MiB definition-heavy stress document or a full-grammar benchmark.

The separate [10 MiB mixed-document run](layout-previews/definitions-10mib-perf.json)
also passes the existing average/p99 gate: **108.30 FPS**, draw p99
**8.528 ms**, presentation p99 **12.698 ms**, input p99 **11.772 ms**.
It has one application stall ≥25 ms (max draw 27.181 ms) and three
presentation intervals ≥25 ms (max 58.163 ms); maximum input latency is
54.690 ms. Its draw tail is higher than the preceding code-pane checkpoint,
so this is not a no-regression or every-frame >60 FPS claim. No concurrent
compiler/check job was running. The unchanged-source gate passed. The mixed
generator does not specifically stress definition lists; the definition-heavy
large-document and performance-tail audits remain open.

## Still required for this family

The complete family remains partial. Nested glossaries keep their semantic
containers but do not independently choose nested aligned rails. Complex
attributed HTML remains inert, rather than being silently simplified. The
former core restriction on cross-container rich copy/replacement is superseded
by [TREE-SELECTION-CHECKPOINT.md](TREE-SELECTION-CHECKPOINT.md), which records
tree-aware commands, rich extraction, native exact copy/edit/block-paste and
undo evidence. Native cross-application rich clipboard interoperability is
still unverified. Full table/image-in-description,
RTL, empty-state, accessibility interaction, structural edit and large
definition-heavy performance matrices remain to be completed.

The full grammar objective also still includes the other open rows in
`DESIGN-GRAMMAR-COVERAGE.md`: deep trees, metadata, metrics, record tables,
true text wrapping, prose columns, source-linked editorial/media objects,
footnotes, references, page masters and production exports.
