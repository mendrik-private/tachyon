# Justified technical prose: September 14, 2026

Crusty `work_9320e0050d01cf05` is fixed in `editor/typography.rs`.

The observed bullet has no dictionary candidates under the former strict
language detector: the filtered sample scores 0.434 for English, and the full
sentence scores 0.605, both below whatlang's 0.9 reliability threshold. The
wrapper already considers fitting dictionary prefixes before whitespace wraps.
Language selection now accepts supported languages with confidence at least
0.5 and retries with the full sentence when the filtered sample is uncertain.
Protected tokens remain excluded from actual break candidates. No language is
assumed when detection fails.

Justification also rejects an expansion that adds more than half the natural
advance of any word space. The existing total-width guard remains in place.
This retains natural spacing when no suitable prefix fits.

## Verification

- `cargo test -p document-view --locked technical_list_hyphenates_references_before_stretching -- --nocapture`
  failed before the fix: `owned-date ` ended the first line and `references.`
  occupied the next even with room for `ref-`. It now passes at zoom
  0.75/1/1.5/2, with contiguous canonical ranges and exact serialization.
- `justification_does_not_double_sparse_word_spaces` covers a line below the
  former 15% total-width limit that would still double its only word space.
- `scripts/check.sh` passes formatting, locked compiler checks, strict Clippy,
  all workspace/adapter/publication tests and doctests: 907 tests passed,
  two existing document-view tests ignored.
- Crusty `ctx_9dc6afb759c4`, validation `task_a76cda23aa4031bb`: 75 existing
  advisory findings, zero new/worsened findings, no matched blocking constraint.
- Native isolated Weston checks at 1050×850/100% zoom and 1250×850/150% zoom
  enabled both typography controls, compared Select All/Copy before and after,
  verified exact source bytes, captured the enabled state, and closed the app.
  Both pass. The 1050px capture visibly ends the bullet with `ref-`, followed
  by `erences.`; the larger text capture retains restrained spaces.
  Artifacts: `layout-previews/crusty-typography-only-{1050,1250}-enabled.png`,
  corresponding `.titlebar.json` and `.source.json` reports.
  Debug binary SHA-256:
  `21aa77097600dd091dd451df3fedec92d3454f4a62a06ec79ba09b387ced8765`.

## Separate native replay finding

The broader `--titlebar-check` replay passes its typography clipboard check,
but its subsequent keyboard/zoom/search sequence removes the final bullet
marker and regenerates that paragraph. This occurred at both tested widths;
it is not claimed fixed here. A bounded typography-only replay, using the same
private compositor and accessibility probe, passes exact source and clipboard
checks. Broader replay logs remain in `/tmp/crusty-hyphenation-native*.log`.

The pre-existing 71-line change in `editor/prose_flow.rs` was preserved.
