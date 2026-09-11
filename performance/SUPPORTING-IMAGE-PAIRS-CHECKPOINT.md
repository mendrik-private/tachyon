# Supporting image composition

2026-09-10. A07 / STR-0003. The bounded implementation below is verified;
the full audit remains active.

Use actual native measurement to choose between a supporting image with true
text wrap and a complete figure/explanation pair. Preserve an already useful
wrap, including the full-measure continuation. When the image/caption/credit
lane cannot support wrapping, allow its complete adjacent explanation to
compete in the existing figure-led pair templates. If neither fits, stack.

Keep all existing width, height, balance, intrinsic-size and source-order gates.
Do not crop or upscale images, shorten captions, reduce text size, detach notes,
cross section boundaries, or move a focused group merely to fill space. Pending
resources remain unmeasured. Candidate work stays in requested bounded windows,
not scrolling. No new author syntax or layout preference is introduced.

Verify the three alternatives with complete source fixtures, loaded-font
geometry, resource/focus transitions, native light/dark/large-text views and
whole-file editing/autosave/Undo. Full E03/E04/RTL/nested/HTML/media/page/export
and performance qualification remain in the active audit.

## Implemented

Supporting images now share the existing source-order figure-pair eligibility
with evidence images. Before nominating a pair, the row planner asks the native
figure-flow measurer whether the complete image/labels/prose unit supports true
wrapping. It uses the existing candidate function, not approximate caption
heights or a second wrapping implementation. Successful wrapping takes priority;
otherwise the same measured two-track pair and stack candidates compete.
One result per source range is cached within the bounded planning request.
The sole `measure_rows` caller supplies this native predicate; the row planner
stays independent of fonts and resources. Canvas width comes from its plan.

No new source encoding, dependency, editable model or persistent layout state
was added. The existing wrap/pair width, height, useful-line, balance, source
barrier and intrinsic-image gates are unchanged. Caption/credit ranges remain
part of the image's unit, and all following prose remains in canonical order.

The initial fixture-111 test went red because the long caption made the wrap
lane too tall, but supporting images were unconditionally excluded from pairs.
It now chooses a complete image/caption/credit + three-paragraph pair. Existing
figure/wrap tests remained green, including the ordinary short-caption wrap.

## Transition diagnosis and regression coverage

A new pair → short-caption wrap → narrow stack → restored wrap → original
caption/Undo sequence found a reverse-transition failure. A minimized version
using only the relevant section reproduced it. Candidate traces showed valid
pair footprints (560/440 and 653.5/468px), no edit lock, and a change penalty of
1.0. The previous row-stack was only backing geometry for a rendered wrap, not
a genuine visual preference. A figure pair leaving that wrap no longer pays
the backing-stack change penalty. Actual focus locks still apply. The same
sequence passes with the original source restored exactly; debug output was
removed. Logs: `/tmp/mineral-supporting-pair-{negotiation,return-probe,minimal-red,
negotiation-green}.log`.

Four added native-font tests cover initial pairing, 100/150/200% measured
geometry, 1314/520/657px widths, short-height fallback, return to the pair,
full-image dimensions, readable prose caps, identical measured/realized heights,
nonoverlap and every paragraph's complete source range. They also cover missing
dimensions, resource arrival during prose focus, release after blur, long
focused text growth on a wider canvas, and both directions of wrap/pair changes.

Two first-draft assertions were corrected: supporting prose uses its actual
narrative measure rather than the reference measure, and a held slot's frozen
canvas is compared through its actual width/leading position rather than raw
struct equality. Neither required weakening a production fit constraint.

## Native evidence

Final runtime SHA-256:
`2ab05dcadef4034961b8b15095dc2d804c0156e3a76354679279d9b9adc11e3f`.
Fixture 111 SHA-256:
`d961374a9fb711bc8caa28c8dbf127dd5877a1413effef4ac87b146f5924b29c`.
All source/edit checks use private fixture copies, not user documents.

Final artifacts are `performance/layout-previews/supporting-pair-final-*`:

- `wide`: 1600×1700 light. Image bounds x259/y314/600×600, prose x883/y314 with
  421px text measure: shared top, 24px gutter, full uncropped intrinsic image.
  The next heading is y1080 instead of y1324 in `supporting-pair-before`, on the
  same fixture/window and baseline runtime `0c0b521f…`: **244px reclaimed**.
  No authored content is removed or shortened.
- `narrow`: 520×1900 light; complete image, caption, credit and explanation
  stack in source order.
- `dark200` and `dark-bottom`: 1280×1900 dark, native 200% zoom. Opening shows
  the full image and labels; the scrolled capture shows all three paragraphs
  and the following section. Appearance and source-unchanged checks pass.
- `wrap`: unchanged fixture 69 at 1600×1700 light; small captioned illustration
  retains actual text return to full measure and the following two-image gallery.
- `body-edit`, `caption-edit`: native Home/type x at the first prose/caption
  line, one-second focused idle, entire expected saved file equality, autosave
  and exact Undo. The image/caption/prose geometry remains paired while focused.
- `copy`: seven authored markers occur once and in original source order.
  This is a marker-order check, not a whole-clipboard equality claim.

Earlier `supporting-pair-*` artifacts without `final` used intermediate runtime
`f449f4c3…` and are diagnostic/baseline evidence. Its first dark scrolled capture
correctly showed dark body text, but the appearance oracle failed because no
heading was in the viewport (`heading: 0`). No theme implementation was changed
to satisfy that fixture-dependent oracle. The final top and bottom captures
include headings and pass; the intermediate failure is not claimed a pass.

## Verification and remaining scope

`scripts/check.sh` exits 0: locked metadata/source pins, formatting, workspace
all-target check, strict Clippy, **718 passing Rust tests**, two existing ignored
view tests, and doctests. Log: `/tmp/mineral-supporting-pair-check.log`.
The focused figure run and new negotiation regression also pass separately.
Crusty context `ctx_009c8be1320a`, validation `task_e1534e324376379b`: 37 existing
advisory findings, no new or worsened findings. No existing debt is claimed fixed.

This extends complete top-level image/label units followed by one to four prose
paragraphs. It does not qualify arbitrary nested/HTML/RTL media, new media
controls, every structural/IME/drag/state interaction, or global score balancing
among all possible flow families. Native continuous resize and resource/resize
bursts for this family, broad holdout/board review, paged/export output and
release performance remain in the original goal. E03/E04 remain partial.
