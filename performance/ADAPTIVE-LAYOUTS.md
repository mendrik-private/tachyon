# Adaptive document layout

Current style authority: [Document design grammar](../designs/document-design-grammar.md).
The [September 8 native implementation and visual evidence](DESIGN-GRAMMAR-IMPLEMENTATION.md)
supersede conflicting historical palette, spacing, card and shadow descriptions below.

Next milestone: [Editorial layout grammar](LAYOUT-GRAMMAR-PLAN.md) (planned September 8, 2026) expands the vocabulary to measured section composition, bounded prose columns, and text flow around components. It supersedes conflicting layout scope and spacing rules here. The implementation notes and measurements below remain historical evidence, not validation of that new milestone.

## Implementation plan

1. Translate all eight `designs/` references into shared warm-paper, ink, green,
   serif-heading and fine-border tokens. Keep native commands, Files and Outline;
   remove the minimap from the application, including its frame-time work.
2. Add a small, deterministic layout vocabulary: vertical lists, row-major grids,
   instruction steps, checklists and adjacent image pairs. Nominate candidates
   using count, reject them using nesting, task state, length, imbalance and
   available width, then score readable candidates with resize hysteresis.
3. Compose source-ordered geometry separately from Markdown. Keep prose within
   a readable measure; let tables, code, figures and grids use the wider canvas. Never
   crop figures, manufacture captions, hide code or infer unrelated comparisons.
4. Select layouts automatically, with no user-facing overrides or presentation
   records in workspace state. Preserve node identities, selection, source bytes,
   undo and scroll anchors. Keep an existing arrangement while typing; a settled
   resize can reconsider it.
5. Precompute geometry and component bounds on load/reflow. Scroll frames query
   the visible range and reuse shaped text; no layout search or whole-document
   component scan belongs in that path.
6. Validate synthetic reading, list, technical and image documents at narrow
   and wide sizes in the native Wayland app. Run correctness and release
   interaction checks and record actual results below.

## Contract and scope

The current user request supersedes the old dark/minimap direction in `plan.md`
and Crusty's STR-0001/STR-0002. Cold preparation may take longer. On this machine,
the release app should sustain more than 60 presented frames per second during
scrolling; report draw and presentation distributions separately. Use the same
100 KiB, 1 MiB and 10 MiB workloads before/after, plus a mixed adaptive fixture.
The hypothesis is that cached component bounds and removal of minimap work keep
the larger layouts within a 16.67 ms frame budget; draw/presentation regressions
on those workloads falsify it.

Markdown parsing and source-preserving serialization remain the semantic oracle.
Layouts must retain row-major reading order, hard breaks, exact code, table cell
relationships, image proportions and selection offsets. The parser continues to
handle [GFM](https://github.github.com/gfm/); GitHub-only extensions remain separate
capabilities. This iteration does not add a new HTML execution path, mathematical
renderer, diagram engine, inferred tabs/comparisons, or long scrolling columns.

## Refinement pass: specification composition

The follow-up reference prompted a broader, denser document canvas and automatic
composition of three or four bounded sibling sections with matching heading
levels and body structure. Long chapters, nested outlines, code, and tables do
not become speculative comparison cards. Explicit short arrow sequences can
stack inside list cards without deleting their arrows or changing source offsets.

- Prose measure: up to 960 logical pixels; full canvas: up to 1280.
- Markdown soft breaks render as spaces; authored hard breaks and original
  source serialization remain intact.
- Automatic table columns distribute available space above a 96-pixel minimum.
  Explicit column widths remain authoritative; unreadably narrow tables scroll.
  Shared borders are painted once, snapped to two physical pixels.
- Quote panels include inset padding; chapter prefixes use distinct green sans
  typography; H1/H2 have cached, low-opacity gray shadows.
- Native SVG formatting/table buttons have distinct hover/pressed colors and
  tooltips. Layout selectors and saved overrides were removed.
- Wheel deltas ease toward their destination with time-based exponential decay;
  precise pixel input retains platform inertia. Reduced motion is direct.
  Editing, navigation, pointer interaction, and document replacement cancel
  added momentum. GPUI implicit wheel integration is disabled to avoid applying
  the same tick twice. The outer scrollbar shares the editor scroll handle.

The native-design and performance skills guided interaction states, conservative
composition, and bounded frame-time work. The fifth synthetic fixture is
`05-product-specification.md`; it covers the supplied reference’s major patterns.
`scripts/check.sh` passes formatting, locked dependency checks, all-target checks,
warning-free Clippy, 159 unit/integration tests, and documentation tests. New
regressions verify soft/hard breaks, fitted columns, source-preserving arrow cards,
sibling-card growth/undo, chapter prefixes, and momentum without double scrolling.
Crusty's advisory validation reports no new or worsened architecture findings;
its unpublished index remains a limitation, not a substitute for runtime checks.
Screenshots: [wide specification](layout-previews/specification-wide.png),
[narrow specification](layout-previews/specification-narrow.png), and
[table toolbar](layout-previews/table-toolbar.png),
[single-column sections](layout-previews/specification-stacked.png),
[padded quotes and arrow cards](layout-previews/specification-details.png), and
[outer scrollbar dragging](layout-previews/scrollbar-drag.png).

The [refinement measurement](adaptive-scroll-2026-09-06-refinement.json) passed
all eight 15-second samples (two per workload), on the same Mutter display and
balanced power profile. No build or capture ran concurrently with the samples;
the shared machine's recorded load average was approximately 32–33.

| Document | Average presented FPS | Worst draw p99 |
| --- | ---: | ---: |
| 100 KiB | 113.0–115.4 | 6.97 ms |
| 1 MiB | 114.6–115.1 | 6.78 ms |
| 10 MiB | 112.9–114.3 | 7.11 ms |
| Mixed adaptive content, including specification | 116.7–116.8 | 5.71 ms |

The 100 KiB/1 MiB/10 MiB source fixtures are identical to the first-pass
baseline. The mixed workload now includes the fifth fixture and is larger;
it is not a like-for-like comparison. Every sample exceeds the requested 60 FPS
average and stays below 16.67 ms draw p99; this is not a guarantee of zero missed
presentation deadlines. Raw input and presentation distributions and binary
hashes are retained in the JSON report.

A [follow-up smoke](adaptive-scroll-2026-09-06-refinement-final.json), after aligning
completion summaries within sibling cards, passed all four 15-second workloads:
117.9–119.1 FPS, worst draw p99 5.75 ms. These later samples had less competing
load; they do not establish a causal speedup over the repeated measurement.

After the final checkbox-centering correction, the exact release binary passed
two further 10-second mixed-layout samples at 116.7–118.8 FPS, draw p99 4.40 ms:
[final binary verification](adaptive-scroll-2026-09-06-checkbox-final.json).
The [checklist capture](layout-previews/checklist-refined.png) confirms centered
ticks, number badges, completion text, and the outer-edge scrollbar.

The original measurements below are historical evidence for the first pass, not
measurements of this refinement.

## Original validation results

Implemented the six steps above. The layout vocabulary deliberately stops short
of speculative semantic transformations such as inferred comparison columns,
FAQ collapsing, code tabs, and timelines.

### Correctness and architecture

- `scripts/check.sh`: formatting, locked dependency checks, all-target check,
  warning-free Clippy, 152 unit/integration tests and documentation tests pass.
- Added tests cover candidate suitability, width hysteresis, row-major source
  order, byte-identical Markdown, label wrapping without missing text, tall
  paired-image visibility, and typing/selection/undo inside a pinned grid.
- Crusty preparation and advisory validation report no new or worsened
  architectural findings. Its index is not published; compiler/runtime tests
  remain the authoritative evidence.
- Existing in-progress document-core, session and minimap utility edits were
  preserved. The application no longer builds or displays a minimap.

### Reproduction

```sh
cargo build --release --locked --bins
python3 performance/capture-layout.py --fixture 02-list-arrangements.md \
  --width 1440 --height 1000 --output performance/results/lists-wide.png
python3 performance/run-layout-scroll.py --seconds 30 --runs 2
```

Native captures use an isolated headless Weston compositor and temporary copies
of fixtures/state. `--width 760` exercises narrow layouts; `--scale 150` means
125% display scaling. Capture and performance harnesses require the existing
`performance/wayland-harness/build.sh` tools; capture also needs Weston and
`weston-screenshooter`.

Scrolling measurements use the optimized app, the active Mutter Wayland display
and a temporary kernel uinput device. Do not interact with other windows while
running this driver. It focuses the test window, repeatedly scrolls in both
directions, does not type, and isolates application state and generated files.
One warmup is excluded; two 30-second samples per document follow, with a
2.5-second per-process profiler warmup. The gate requires more than 60 presented
frames/second on average, draw p99 below 16.67 ms, and observed input samples.
Full presentation distributions are retained: average FPS is not a claim that
every frame meets its deadline. The older 120 Hz mixed-editing qualification is
a separate, stricter contract, not silently redefined as passing.

### Native visual review

All eight supplied design references informed the warm-paper palette, serif
heading hierarchy, quiet green/gray surfaces, restrained rules and content-led
arrangements. The four synthetic documents cover prose and hard breaks; grids,
steps, tasks and nested outlines; compact and wide tables/code; and paired
figures, authored captions and callouts.

Native screenshots are in [layout-previews](layout-previews/):
[wide lists](layout-previews/lists-wide.png),
[narrow lists](layout-previews/lists-narrow.png),
[reading](layout-previews/reading-wide.png),
[technical reference](layout-previews/technical-wide.png),
[paired figures](layout-previews/figures-wide.png) and
[stacked figures](layout-previews/figures-narrow.png),
[steps, tasks and nested lists](layout-previews/lists-details.png),
[wide table and exact code](layout-previews/technical-details.png), and
[callouts below a figure](layout-previews/callouts.png).
Review exposed and corrected cramped three-column layouts at narrow widths,
awkward bold-prefix wrapping, and long image alt text becoming visible or
reserving multiple image-height lines. Image alt text is not a generated caption.
The scrolled callout fixture also exposed a GPUI image percentage-height/aspect
ratio interaction: a standalone SVG could exceed its reserved height. Images
now receive that exact pixel height and retain `Contain` painting, keeping the
entire figure visible without overlapping following blocks.

### Sustained scrolling

The [repeat measurement](adaptive-scroll-2026-09-06-repeat.json) passed all eight
30-second samples on the active 2880×1800, 120 Hz Mutter display at 166.67% scale,
using the balanced power profile. The machine was shared with a CPU-heavy
unrelated workload (recorded load averages approximately 34–35); no competing
build or capture was started by this task during those samples.

| Document | Average FPS, two runs | Worst draw p99 | Worst input p95 |
| --- | ---: | ---: | ---: |
| 100 KiB | 115.1–115.6 | 9.10 ms | 12.59 ms |
| 1 MiB | 114.0–114.4 | 9.50 ms | 12.92 ms |
| 10 MiB | 114.5–115.7 | 9.31 ms | 12.48 ms |
| Mixed adaptive content | 118.0–118.6 | 5.99 ms | 10.93 ms |

The [initial diagnostic](adaptive-scroll-2026-09-06.json) is retained, including
its failed 58.6 fps sample. That driver's first direction was upward at the
document's top and it could spend time at a scroll boundary. The revised driver
first moves down, then oscillates between interior positions and isolates each
sample's workspace state. This corrects the workload rather than relaxing the
gate. Shared-machine variation remains a limitation.

Presentation p99 in the repeat was 14.39–16.97 ms, with occasional longer
intervals. These results establish sustained throughput above 60 fps on this
machine, not a guarantee of zero stutter, universal hardware performance or the
older strict 120 Hz release qualification.

The [rebuilt-binary smoke check](adaptive-scroll-2026-09-06-final.json), after
badge-spacing and preference-persistence refinements, also passed all four
15-second samples: 112.3–119.1 fps, draw p99 at most 10.28 ms and input p95 at
most 13.37 ms. Both smoke reports retain the executable SHA-256. The image-height
fix receives a separate targeted adaptive-content rerun because it changes
image painting, not the scrolling or layout-query algorithms.
That [final image-content rerun](adaptive-scroll-2026-09-06-image-fix.json)
passed both 15-second samples at 118.5 and 119.5 fps, with draw p99 at most
5.33 ms. The wide, narrow and scrolled callout screenshots were recaptured and
visually checked against this final binary: figures stay uncropped and the
following heading/callouts remain unobstructed.
