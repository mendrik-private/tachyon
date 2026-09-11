# Relationship grammar checkpoint — September 8

This is implementation progress, not approval of the complete document grammar.
Authority remains `designs/document-design-grammar.md` and boards 01–07.
Board 02 was visually reviewed for this relationship-focused checkpoint.

## Build and behavior

Release binary SHA-256:
`e7741202b23b910776d0b5fd1d76f5bc99425f6a88f34c26283e2a7f9daae3b5`.

Explicit dates now nominate timelines before ordinary list-grid selection.
Supported labels include years, ISO year/month/day, English month dates and
authored bold/code dates. Versions, ambiguous slash dates, impossible calendar
dates, durations, ranked lists and task states are not converted to timelines.
Source order is retained, including reverse chronology; no dates are invented.

Two to four short events may occupy one horizontal row only after loaded-font
measurement proves one date line, at most three body lines per event, no
overflow, useful widths, and a tallest/shortest height ratio at most 1.5.
Other supported events use aligned date/body columns; narrow ones put the date
above its body. Labels and descriptions remain ranges of one canonical node.
During text editing, columns remain fixed; after blur, an overlong event causes
vertical reconsideration. Undo restores the original Markdown bytes exactly.

Earlier aligned label rows and nested guide work is retained. All three use
prepared geometry for painting; the scroll path neither parses dates nor
measures the text. The automatic-only interface is unchanged.

## Native and geometry evidence

- [Wide timeline](layout-previews/timeline-verified-wide.png): horizontal short
  milestones, a vertical reverse-chronological history, and authored month dates.
- [Narrow timeline](layout-previews/timeline-verified-narrow.png),
  [200% text](layout-previews/timeline-verified-200-detail.png), and
  [stacked long dates](layout-previews/timeline-verified-200-stacked.png).
- [Aligned labels](layout-previews/timeline-verified-labels.png) and
  [nested outline](layout-previews/timeline-verified-outline.png), fixture 56.
- [Native edit/undo](layout-previews/timeline-verified-edit.edit.json): actual
  pointer placement inside the second event, typing, autosave and exact undo.
- [Native copy](layout-previews/timeline-verified-edit.copy.json): all four
  milestone markers are present once and in order. This is a marker-order
  assertion, not an exact full-clipboard golden.
- [Weston MCP AT-SPI](layout-previews/timeline-verified.atspi.json): four named
  list items remain in canonical order, with increasing x and shared y=346.
- All `timeline-verified-*.source.json` checks compare the private fixture
  bytes before/after viewing or after undo; source is unchanged.

Measured screen details at 1600×1200, 100% text/display scale:

- Horizontal event x positions 276, 602, 928, 1254; text slots separated by
  24 logical px. Dots are 8 px in diameter; connecting rails are 1 logical px.
- Pixel (606,334) is exactly Green `(63,98,71)`. At x=700, only y=334 is Rule
  `(218,221,213)`; y=332–333 and 335–336 are Paper `(250,249,246)`.
- Published milestone bottom 348.5 and following heading top 412.5 establish
  exactly 64 px of section separation. The detailed history bottom 855 and
  following heading top 919 repeat that gap.
- Tests cover a 24 px aligned label/body gutter, 8 px stacked date/body gap,
  12 px between vertical events, zoom-scaled markers and connected rail ends.
- A failing regression exposed text extending beyond its published outer span;
  timeline measurement now subtracts the same list insets as final layout.
- Three requested warm replans pass the existing cached-geometry gate: no
  reshaping or anchor displacement in geometry reuse.

## Weston MCP versus timing evidence

Weston MCP was used for actual native screenshots, pointer input and AT-SPI.
Its installed headless configuration uses `--refresh-rate=0`. A controlled
run of the regular harness with only refresh changed to zero reproduces stalled
startup reflow (`timeline-zero-refresh.log`). This configuration must not be
used to judge idle startup or momentum timing. Pointer input can expose a newer
prepared frame, which initially looked like an application repaint failure.
The early hypothesis that diagnostic output caused the issue was rejected.
No speculative repaint workaround or debug logging was retained in the app.

At the normal 120 Hz harness setting, the
[idle startup gate](layout-previews/timeline-verified-wide.idle.json) proves the
timeline dot is already visible before input, with zero changed document
pixels after moving the pointer into the sidebar. The oracle rejects blank or
untransformed equal frames and excludes only the known pointer-sprite area.

## Performance and verification

Both 60-second runs use the same release binary and 10,485,760-byte mixed fixture,
1728×1080 isolated headless GL output with a 120 Hz refresh clock. No build or
other capture ran concurrently with either measured scroll run.

| Input | Average FPS | Draw p99 | Presentation p99 | ≥25 ms presentation intervals |
| --- | ---: | ---: | ---: | ---: |
| [Wheel](layout-previews/timeline-verified-perf.json) | 107.7 | 6.34 ms | 12.25 ms | 4 |
| [Continuous](layout-previews/timeline-verified-continuous.json) | 108.2 | 6.93 ms | 12.30 ms | 4 |

These pass the existing >60 FPS gate, not a claim of a perfect frame deadline
on every frame. Maximum presentation intervals were 33.26 and 71.50 ms.
The mixed generator is not exhaustive coverage of every future grammar family.

[Momentum evidence](layout-previews/timeline-verified-coast.json) shows
wheel-release thumb speed decaying from 185.4 to 70.8 px/s and settling at
the same position at 2.86 and 3.27 seconds. These are scrollbar-thumb speeds,
not a fabricated estimate of content velocity.

Commands actually run:

```sh
cargo test --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked
cargo fmt --all -- --check
python -m unittest discover -s performance -p test_capture_layout.py
cargo build --release --locked --features layout-validation --bin mineral-markdown
```

Results: 461 Rust tests passed, 2 ignored; Clippy and formatting clean; seven
Python harness-oracle tests passed. Capture parameters and the running binary
hash are stored in the adjacent JSON reports.

## Remaining scope

The coverage ledger still contains incomplete definition-list forms, deep-tree
fallbacks, supporting-block/nested timelines, editorial cards, real text wrap,
flowing prose columns, footnotes, media, diagrams, pagination and export, plus
the complete accessibility/state/width matrix. This checkpoint does not mark
any complete board or the overall goal finished.
