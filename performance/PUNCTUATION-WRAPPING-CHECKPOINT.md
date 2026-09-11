# Punctuation-aware document wrapping

2026-09-09. Layout-first A07 continuation. This closes the measured README
slash-leading-line counterexample, not every F09 or document-grammar requirement.

## Diagnosis and correction

The native holdout in HOLDOUT-LAYOUT-CHECKPOINT.md split `undo/redo` immediately
before `/`. The actual `FontMeasurement::wrap` regression reproduces this with
only `A undo/redo` at width 90: `["A undo", "/redo"]`.

Command: `cargo test -p document-view --locked punctuation_wrap_does_not_start -- --nocapture`.
The original and minimized runs fail deterministically in milliseconds after
compilation (`/tmp/mineral-punctuation-red.log` and
`/tmp/mineral-punctuation-minimal-red.log`). Targeted probes show the same
boundary with final-line refinement disabled; raw GPUI reports byte offset 6.
Changing only `/` to `_` removes that boundary. The root cause is GPUI's native
word-wrap policy, not a source rewrite, source-offset translation, or our
paragraph-ending refinement. Diagnostic probes were removed after verification.

The editor now validates native boundaries against `unicode-linebreak` 0.1.5,
already present in Cargo.lock. Its Unicode 15 implementation supplies legal
line-break opportunities; it is not a claim of complete Unicode 17 conformance
or dictionary-based complex-script breaking. The direct dependency adds no
new package/version and leaves the pinned GPUI revision unchanged.

`editor/line_breaks.rs` keeps legal, fitting native lines. When correction is
needed, it uses the existing shaped glyph advances to nominate legal positions
and checks changed lines with the standalone styled shaping used for painting.
A monotonic glyph cursor avoids repeatedly scanning the entire glyph layout.
Emergency breaks are permitted when no legal unit fits, but never divide an
extended grapheme; a single oversized grapheme remains intact. Input and output
are source ranges only. Existing measurement-cache identity and warm reuse are
retained. RTL retains its logical-source shaping path, now using the same legal
break opportunities instead of word boundaries.

The expanded test caught a second defect: snapping a native wrap backward to
preserve an emoji cluster could leave the following line too wide. Validation
now checks the resulting line advances as well as boundary legality before
keeping a native wrap. The strict width/grapheme test went red before this
correction (`/tmp/mineral-punctuation-emoji-red.log`) and green afterward.

Unicode permits appropriate breaks **after** a slash; this patch does not promise
to keep every slash-separated expression on one line. See the primary
[Unicode line-breaking rules](https://www.unicode.org/reports/tr14/) and the
versioned implementation in the local `unicode-linebreak-0.1.5/src/lib.rs`.

## Regression coverage

Three new tests exercise actual FontMeasurement behavior:

- Minimal slash-leading-line reproduction across adjacent widths.
- Legal boundaries, measured fit, exact canonical projection, graphemes and
  warm cache reuse at 100/150/200%, with bold/link/inline-code runs, paths,
  numeric dates, NBSP/word joiners, combining accents, emoji sequences, CJK
  punctuation and mixed Arabic/Latin text. Widths are derived so each legal
  unit fits; emergency wrapping cannot excuse an illegal boundary in this test.
- Extremely narrow measures, repeated oversized emoji compounds, nonzero
  projection offsets, forward progress and exact source preservation. Only a
  single indivisible grapheme may exceed the measure.

The focused `punctuation_` run passes four tests (three new and the existing
footnote punctuation test): `/tmp/mineral-punctuation-final-focused.log`.
The final `scripts/check.sh` exits 0: formatting, pinned/locked metadata,
all-target check and Clippy, 116 core tests, 25 source-fidelity tests, 11 tree
tests, 485 document-view tests (2 ignored), 1 external-consumer test, 39 app
tests and doctests. Log: `/tmp/mineral-punctuation-qualified-check.log`.
`git diff --check` passes. Crusty validation of `ctx_68bc929cb76b` reports 36
existing advisory findings and no new or worsened findings.

## Native document evidence

Runtime SHA-256:
`01facd3ec77c08b363555e5469ef2321178ebdde09b8f9840916bbade18ee6e8`.
README SHA-256:
`abca9115a24f8b4293074b510011acfc5d11a036f910ba68d77e6f0054527541`.
The source matches the previous counterexample, so this is a controlled
before/after comparison. All captures stage a private exact Markdown copy;
the original is never an editable target. Source-origin records remain unchanged.

Artifacts are under `layout-previews/`:

| Prefix | Native observation |
| --- | --- |
| `punctuation-readme-dark-200` | 1280×1400 dark/200%; `undo/` ends the preceding line and `redo,` starts the next. Enlarged text uses the available canvas without shrinking. |
| `punctuation-readme-narrow` | 520×1400 light; corrected punctuation, readable stacked prose, preserved code overflow and command strip. |
| `punctuation-readme-wide` | 1600×1400 light; readable narrative opening, complete explanation/code pair, full-width examples and compact command strip remain intact. |
| `punctuation-readme-cross-wrap-copy` | Native Home/arrow/Shift-arrow selection across the corrected 200% wrap copies exactly `undo/redo`, with unchanged source. |
| `punctuation-readme-boundary-edit` | Native Home/type at the start of `redo` changes it to `xredo`. Entire autosaved file equals the expected serializer output; idle settlement and byte-exact Undo pass. |

The edit oracle accounts for the existing paragraph serializer's punctuation
escaping and soft-line normalization; it checks the entire file, not merely an
inserted marker. The untouched file is restored exactly by Undo.

## Scope retained

The debugging skill required a red-capable minimized reproduction and direct
boundary probes. UX guidance shaped the native width/zoom review; Rust guidance
shaped dependency reuse, source-range invariants and layered regression checks.
Context7 tooling was unavailable; pinned source and primary Unicode guidance
were used instead.

F09 remains partial for the full inline/script/interaction matrix. This pass
does not qualify every board, anchored margin notes, real playable media,
map views, paged masters/exports, or controlled release performance. Cache
counter evidence is not a throughput or scroll-latency benchmark. The complete
audit goal and A07 remain active.
