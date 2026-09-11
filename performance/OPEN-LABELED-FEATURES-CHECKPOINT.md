# Open labeled features

September 10, 2026. Contract before implementation.

Bold/code labels identify a title and explanation, not an enclosure. Short
unordered labeled siblings should use the existing measured source-order grid
with open edges, zero card inset and the current label/body typography. Retain
markerless label alignment. Plain open features retain their dots; authored
numbered stages and explicitly classified editorial/resource objects retain
their existing presentations. Four columns remain restricted to short plain
features, not labeled explanations.

The unchanged full-page fixture 53 currently contradicts its own “Short features
stay open” introduction: six filled top-rule cards. Capture before/after native
pixels, actual measured-plan regressions, narrow/zoom fallbacks and native
editing/Undo. Do not rewrite fixture content to improve the screenshot. Preserve
source order, canonical text and focused geometry. This closes one layout defect,
not the entire design-grammar matrix.

## Implemented

`OpenLabeled` replaces the unowned `Top` treatment for unordered labeled
features. It retains markerless anchors and label/body presentation breaks but
owns no enclosure or inset. Painting no longer draws a top-rule card for these
features. Plain open points retain their markers and four-column eligibility;
labeled explanations remain limited to three columns. Numbered stages and
explicit resource/editorial classifications are unchanged.

Candidate measurement now separates marker suppression from enclosure padding.
Both measurement and placement use zero padding for open labels; numbered stages
keep their 24 px insets and numeral allowance. This reuses the existing measured
planner, retained focused slots and canonical ranges, not a parallel layout.

## Diagnosis and automated verification

`cargo test -p document-view --locked labeled_features_use_open_measured_geometry
-- --nocapture` reproduced the actual full-page fixture with six `Top` slots.
A three-feature excerpt reproduced the same defect without surrounding sections.
Logs: `/tmp/tachyon-open-labels-red.log` and
`/tmp/tachyon-open-labels-minimal-red.log`. An initial artificial two-item input
chose label rows instead of a grid; it was not evidence for the card defect.

The first styling change passed the inset assertion, but an expanded measured
height assertion correctly failed: actual 48 px, candidate 96 px. The remaining
48 px was obsolete enclosure padding in list candidate measurement.
`/tmp/tachyon-open-labels-measure-red.log` records that failure. Correcting the
measurement makes both the isolated excerpt and unchanged complete page pass.

Two new native-font tests cover actual plan selection, zero open inset,
candidate/rendered line and height equality, source-complete feature ranges,
width bounds, numbered treatment, and 520/657/1040/1314/1700 logical widths with
100/200% font measurement. A focused body-growth edit retains the original
track width at a wider canvas and exact Undo restores the source. Existing
four-column exclusions and labeled-composition regressions still run.

The initial full suite found one historical regression explicitly requiring
enclosure padding. Its expectation now requires open, markerless alignment;
the underlying layout/identity check remains. No production failure is hidden
by that update. Initial log: `/tmp/tachyon-open-labels-qualified-check.log`.

Final `scripts/check.sh` exits 0: formatting, locked source pins/metadata,
all-target check, Clippy, 116 core tests, 25 source-fidelity tests, 11 tree tests,
**493 document-view tests (2 ignored)**, one external-consumer test, 39 app tests
and doctests. Log: `/tmp/tachyon-open-labels-qualified-check-final.log`.
`git diff --check` passes.

## Native evidence

Runtime SHA-256:
`5b3153b77b9d71711f8fbedfbb18a7a933814bcf2bedb13e543c9ad670d1b118`.
Unchanged fixture 53 SHA-256:
`fde6b28af6e959b6bd73d36be5e7cd34b0cb6b5bf26ae316a789017eb5f14b65`.
Artifacts are under `layout-previews/`.

- `current-grammar-composition`: before, runtime `4e98264c…`, 1600×1700 light.
- `open-labels-wide`: after, same source/window. Six open aligned features in
  two three-column rows. The following section's native heading bounds move
  from y=642 to y=546: **96 px reclaimed**, without smaller text or omissions.
- `open-labels-narrow`: 520×1700 light, readable label/body rows and ordinary
  source-numbered stages. No forced feature grid.
- `open-labels-dark-200`: 1280×1700 dark/200%, readable enlarged label/body rows.
- `open-labels-edit`: native End, type `x` in the first feature body, autosave,
  one-second idle, whole edited-file equality and exact Undo. The complete
  expected file accounts for existing Markdown punctuation escaping. Inspected
  idle pixels retain the open three-column arrangement and body typography.
- `open-labels-copy`: native End/Shift-Home selects and copies exactly
  `Write in plain text with rich results.` from the first feature body.
- `open-labels-editorial-control`: unchanged fixture 59, 1600×1700 light.
  Decision, selected-option, pros/cons, worked-example and validation enclosures
  remain intact. Fixture hash `e67df8f2ca1190f8358beabd936796884d1ddef09637faf248528ed09497c8ca`.

All listed after screenshots were inspected; source checks pass. The preliminary
`open-labels-edit-probe` checks protected fragments and Undo only and is not the
whole-file edit oracle. Baseline relationship fixture 54 was also inspected;
its term rails and ordinary points were already open, not this defect.

UX guidance shaped open hierarchy and available-space use. Rust and debugging
guidance shaped semantic ownership, red-first geometry tests and source-fidelity
verification. The full labeled/nested/RTL/state and paged matrix remains open.
