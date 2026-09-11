# Figure-led explanations

September 10, 2026. E03 contract before implementation.

A complete top-level evidence figure, its explicit caption and optional credit,
followed by one to four ordinary prose paragraphs in the same section can
nominate a measured image-first/explanation-second pair. Keep source order and
the full-width heading. Both columns remain open. Never crop, upscale, infer a
caption from alt text, reverse the source, or alter Markdown to fit the screen.

Use the existing twelve-track templates and loaded-font row planner. Keep
prose within its actual reading measure, require at least four useful body lines,
and reject cramped, excessively tall or strongly unbalanced pairs. Narrow/short
windows, unresolved image dimensions, unsupported prose, section boundaries and
focus locks retain a complete stack. Captions/credits remain with the figure.
Do not detach a following margin note or steal prose from a related technical
object. Authored photographic/illustrative supporting roles keep the existing
true-wrap path; this pass fills the evidence-figure-led pairing gap.

Verify eligibility/negative boundaries, measured versus realized geometry,
pending-to-ready resources, focused edits and responsive width/zoom, plus native
wide/narrow/200% pixels and exact source/edit/Undo. Existing assets are reused;
no generated or cropped technical evidence. The complete E03/media/export
matrix remains open beyond this implementation.

## Implemented

`FigureExplanation` is a source-ordered row in the existing bounded planner.
Eligibility consumes exactly two complete units: one figure (with its existing
authored caption/credit association) and one adjacent prose group. No model,
parser, caption syntax, image renderer or editable-tree duplication was added.
Diagnostic traces name the relationship `FIGURE_LED_EXPLANATION`.

The pair uses existing two-track templates, a 900 px canvas floor, the normal
260 px component floor, and a prose floor of 60% of the loaded reference
measure. The prose track is capped at its loaded-font reading limit; the image
track is capped at intrinsic width, preventing a large invisible gutter beside
a non-upscaled image. Actual measured heights must fit 70% of usable viewport
height, provide at least four reference-leading units of explanation, and stay
within a 2:1 height ratio. All actual text survives; these gates choose a stack
when the pair is unsuitable, not smaller type or partial content.

Supporting photographic/illustrative roles keep true wrapping. Galleries,
section changes, nested images, more than four following paragraphs, ambiguous
extra captions, prose already attached to technical content, and an explicit
following margin-note anchor do not nominate this pair. Missing dimensions
cannot yield a measured pair. The shared source-order validator checks both
units and their complete parts before publishing placement.

## Tests and state-transition diagnosis

Four tests exercise semantic boundaries, the actual full-page plan, native-font
geometry, and resource/edit state transitions. The first full-page test went
red before implementation (`/tmp/mineral-figure-led-red.log`): no image-first
pair was chosen. Complete image/caption/credit and all three prose paragraphs
now share one row. Widths 520, 657, 1040, 1314 and 2200, short height 220 and
200% fonts cover pairing/fallback, uncropped aspect ratio, non-upscaled tracks,
readable text caps, shared top anchors, measured/realized height equality and
complete canonical paragraph ranges. Source serialization remains exact.

The state test discovered that an image loaded during prose focus correctly
retained the stack, but the stack stayed after blur. A minimized first-section
reproduction confirmed it. The probe showed focus cleared and valid measured
pair candidates, but a change penalty of 1.0 favored the temporary stacks.
Logs: `/tmp/mineral-figure-led-state-probe.log` and
`/tmp/mineral-figure-led-state-minimal-red.log`.

The focus constraint now marks both stacks of a newly usable, focus-deferred
figure pair as provisional. Focus remains a hard constraint; after blur the
pair competes without treating the forced stacks as settled choices. The test
also grows both caption and body beyond normal candidate bounds, retains their
original widths on a wider canvas, releases to a stack when narrowed, and
checks exact Undo. No debug probes remain in source.

The initial state-test implementation used a nonexistent snapshot accessor;
that compile error was corrected to use the existing projection lookup. Clippy
also caught adjacent identical rejection branches, now combined without a lint
suppression. Neither development failure is presented as a production defect.

`cargo test -p document-view --locked figure_led -- --nocapture` passes all four
tests (`/tmp/mineral-figure-led-state-green.log`). Final `scripts/check.sh`
exits 0: formatting, locked source pins/metadata, all-target check, Clippy,
116 core, 25 source-fidelity, 11 tree, **497 view (2 ignored)**, one external
consumer, 39 app tests and doctests. Log:
`/tmp/mineral-figure-led-complete-check.log`.

## Native evidence

Final runtime SHA-256:
`7cfa3f7bab52f84fdf2081bd8a4911c07a65705b21ba803ce62c67e3b8550cd5`.
Fixture 102 SHA-256:
`a537522e8c7dcccfff17c6c4d65b2c77fd9dce93c881c8777f46ab0267447066`.
All final artifacts are under `layout-previews/figure-led-final-*`; all source
checks pass on isolated copies. Intermediate captures are not final-build proof.

| Artifact | Evidence |
| --- | --- |
| `wide` | 1600×1700 light. Complete evidence and three-paragraph explanation share the row, with attached caption/credit and no box around prose. The following section moves from native y=1146 to y=762: 384 px reclaimed. Before is `figure-led-before`, same source/window on runtime `5b3153b7…`. |
| `narrow` | 520×1700 light. Complete uncropped image, caption, credit and all three paragraphs stack in source order. |
| `dark-200` | 1280×1700 dark/200%. Full figure and caption/credit stack above enlarged readable prose. |
| `body-edit` | Native Home/type `x`, whole edited-file equality (including existing punctuation escaping), autosave, one-second idle and exact Undo. Inspected idle pixels retain the pair. |
| `caption-edit` | Native End/type `x` inside the caption, the same full-file/autosave/idle/Undo oracle; inspected idle pixels retain the caption under its image. |
| `copy` | Seven unique authored caption/credit/prose/heading markers appear once in canonical order. This is not a whole-clipboard equality claim. |
| `wrap-control` | Unchanged fixture 69: small illustration with complete caption/credit lane, true text return to full measure, and the existing two-image gallery remain intact. |

Wide, narrow, dark-200, both edited-idle and wrap-control screenshots were
inspected. Short-window behavior and resource-arrival-during-focus are automated
geometry/state tests, not claims of additional native captures.

UX guidance shaped the full-frame/readable-pair contract and leading/gutter
alignment. Rust guidance shaped shared-plan ownership and layered verification;
debugging guidance exposed and corrected the provisional-state transition.

Still open: general hero/media families, optional-image pair-versus-wrap
negotiation, full nested/HTML/RTL/state coverage and paged/export placement.
The actual unavailable-image example remains an unlabeled blank placeholder;
the new pair correctly rejects it, but its visual error/loading state needs
the next E01/N06 pass. This is an observed open issue, not a completed state.
