# Shared gallery captions and credits

2026-09-09. Layout-first continuation of Audit A07, grammar E02/E13.
The complete grammar remains active and is not signed off by this checkpoint.

Later ownership review: WARNING-ACTIONS-CHECKPOINT.md records a missed validity
gate. The new shared-caption edge crossed from the preceding gallery group, but
the group validator rejected all external endpoints. The precise adjacent
GalleryCaption exception now validates while arbitrary external edges remain
invalid. Runtime `70e174df` restores complete plan initialization and rechecks
native gallery layout/End selection. Earlier pixels and tests did not prove this
partition-validity invariant; they must not be treated as that evidence.

## Layout

An explicitly authored `Gallery:` paragraph immediately after two through nine
source-adjacent top-level images identifies their shared caption. An immediately
following `Gallery credit:` identifies shared credit. Each image may still have
its existing individual caption and credit. Ordinary intervening prose, headings,
an empty label, a single image or an oversized run do not establish this new
relationship. The labels are ordinary Markdown paragraph content; no generated
caption, numbering, alt-text inference or new stored syntax is introduced.

Shared text forms a full-span band below the complete gallery, not a caption
inside the final image's column or another image tile. It uses the available
document measure, 13/18 caption typography, an 8px attachment gap and a 4px credit
gap. Images retain their intrinsic-size limits; a short shared caption need not
fill every pixel of its available band. Individual captions retain their own
image widths. Narrow layouts stack images in source order, with shared text
after the final image and any individual labels. Normal prose then resumes its
ordinary reading measure.

Canonical paragraph IDs/ranges own the text. The shared role retains the first
and final image identities; accessibility describes every member with the same
authored shared text while publishing the caption paragraph only once. Image
lookup uses a source-order index during semantic preparation, not a repeated
whole-document scan or work during scrolling. Shared text is excluded from an
individual image's float candidate.

## Focus-reflow defect found and fixed

The first native edit placed `x` before the final word after pressing End. An
End/Shift-Home copy check also failed. The captured focused frame showed the
caption had unexpectedly wrapped, although the idle frame was one line.

A direct End-handler test passed, including synchronous refresh; it did not
exercise the asynchronous focus transition. Adding a prepared-worker transition
to the measured regression reproduced the changed width (0.8959992 rather than
1.0 in its first failing case). The stack edit-lock path treated shared text as
ordinary prose and applied the prose-width cap. That path now preserves its
full-span role, matching the renderer. The complete native End/edit sequence and
exact selection-copy check pass after the fix.

No saving code was changed. The complete edited-file oracle includes the existing
serializer's escaping of colon, comma and period in the edited paragraph. All
other source bytes remain unchanged, and Undo restores the original file exactly.
The initial edit expectation omitted some punctuation escapes as well as expecting
the caret at the final character; those are distinct from the actual focus bug.

The UX skill informed attachment, shared alignment and responsive stacking. The
Rust skill informed canonical identity and worker regression coverage. The
diagnosis skill helped distinguish native focus reflow from End handling and
serialization, with a red-capable worker test and native replay.

## Verification

- Association test fails on the original implementation (one individual label
  rather than three individual/shared labels) and passes after implementation.
- Positive/negative source tests cover explicit ownership, interruption, empty
  labels, single images, oversized sets and unchanged Markdown.
- Native-font geometry tests cover logical widths 360/900/1280 and 100/150/200%
  font environments, with/without individual labels. Assertions cover full-span
  caption width, 8/4px gaps below every image row, typography, source text once
  in canonical order, and unchanged source. Focused workers preserve the visible
  line range/measure for each shared caption and credit.
- Semantic tests cover shared descriptions on every member, preservation of
  individual descriptions, one caption node, and unrelated-image exclusion.
- `scripts/check.sh` exits 0: formatting, locked all-target checks, strict Clippy,
  470 view tests passing (two ignored), 116 core, 25 source-fidelity, 11
  tree-selection, one external-link and 39 app tests, plus doctests.
  Log: `/tmp/tachyon-shared-gallery-qualified-check.log`. `git diff --check` passes.
- Crusty preparation `task_844f851f2d3509e2`, context `ctx_6c68b9302ee7`,
  implementation validation `task_3aaf978f70b50209`: completed, 36 existing advisory
  findings, none new or worsened. No dependencies were added.

## Native artifacts

Final runtime SHA-256:
`e82c02c00055aef6290e80962539bc7a9eb4556b8652db108841673157a1bead`.
Fixture `98-shared-gallery-captions.md` SHA-256:
`c24f6219c99e896fc08330793a2779e93630ee709b292a51c95c6b4bcfe2590b`.
Artifacts in `layout-previews/` retain source/runtime/input/appearance sidecars.
Native sessions use isolated fixture copies, not user files.

| Prefix | Evidence |
| --- | --- |
| `shared-gallery-qualified-edit` | 1600×1700 light, native click/End/type/autosave/idle/undo. Complete expected edited-file equality and exact original-file undo. Inspected typed pixels retain the one-line shared caption below both images. |
| `shared-gallery-qualified-end` | Same surface, native End/Shift-Home/Copy equals the complete shared caption exactly. This failed before the focus fix. |
| `shared-gallery-qualified-narrow` | 400×1800 light. Inspected stacked figures, shared caption/credit and following prose. Source/appearance pass. |
| `shared-gallery-qualified-dark-200` | 1000×1600 dark, 200%, 25 wheel steps. Inspected enlarged shared text, credit, normal prose, next heading and individual-caption treatment. Source/appearance pass. |
| `shared-gallery-qualified-individual` | 1600×1700 light, 12 wheel steps. Inspected individual labels beneath their respective images, shared text below both, and following ordinary content. Eleven clipboard markers occur once in canonical order; this is a marker-order check, not whole-clipboard equality. Source/appearance pass. |

The final native accessibility dump is also checked: both first-gallery images
have the shared caption and credit in their descriptions, with a source caption
role. Earlier `shared-gallery-wide`, `-narrow`, `-individual`, `-dark-200` and
`-copy` use intermediate runtime `eb1870d0`; they are supplemental layout/order
evidence, not final focus-state qualification. `shared-gallery-before` is the
previous implementation. `shared-gallery-edit` and `-end-selection` are retained
failing native repros, not successful evidence.

## Remaining

Nested/HTML/shared-label encodings beyond the explicit convention, mixed media,
larger galleries and uneven multi-row variants, complete structural/RTL/IME/
resize/missing-image/accessibility-state coverage, caption references, paged/static
output and controlled sustained release-performance qualification remain open.
Other grammar families and the full audit keep their original scope.
