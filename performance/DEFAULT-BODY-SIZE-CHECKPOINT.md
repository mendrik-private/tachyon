# Default body size matches the reference opening

2026-09-10. Crusty `work_3fe38fbe1f09732a`.

The opening of example/reference.md was captured before changing tokens. Its
lead used 21px text and 32px line spacing (four native lines, 128px paragraph
height). Ordinary reference text used 16/24, narrative text 18/28. The requested
reference is size; existing serif/sans reading roles remain intact.

DocumentStyle now defines shared BODY_SIZE=21 and BODY_LEADING=32. Ordinary
reference/narrative prose and list/card bodies use those values. Feature labels
move from 19px to 24px to retain their title/body hierarchy. Lead, document
headings, code, table, caption and metadata tokens retain their explicit values.
No zoom preference or zoom-control code changed. The existing bounded/resettable
zoom regression and native 100/150/200% controls continue to pass.

The existing loaded-font measurement path recalibrates reading widths and all
component heights. Tests now exercise wider fitting canvases at the new body
size while retaining narrow/short fallback, source coverage, image aspect ratio,
focus retention, editing and exact Undo assertions. The peer-column specimen
uses equally long journeys so it tests a legal balanced row; the original uneven
journeys no longer necessarily qualify at this size. Table-edge tests use a
longer cell specimen because tables intentionally retain their smaller text.
A plain image test incorrectly assumed a 16px inset that the old intrinsic-size
cap masked; it now checks the actual plain image width and aspect ratio.

Larger wrapping exposed a real label-composition problem: a short colon-ended
label could inflate a table explanation enough to qualify for a parallel column.
The existing measured short-code-label exclusion now also covers tables. The
original recorded typography/table-leadin regression passes unchanged.

## Verification

- `ordinary_and_card_bodies_match_the_reference_opening_size` checks 21px body
  text, at least 32px line spacing, complete paragraph source coverage and
  retained code sizing across 400/1000/1800 document-unit canvases and 100/150/200%
  zoom. Other layout/geometry/source tests remain green.
- `scripts/check.sh` passes: 785 Rust tests, two existing ignored tests, locked
  checks, formatting, strict Clippy, adapter suites and doctests. Final log:
  `/tmp/mineral-body-size-check-final.log`. `git diff --check` passes.
- Native final debug layout-validation binary SHA-256:
  `400aa162d222e55726ebe7a4441df926af4230bf6be50746fde92bf4f74a1bc5`.
  Before binary: `2fe3a20f99835fa7b277600d1a41a3c5fd88772c22189db51060c141b7ed758f`.
- `body-size-before` captures the actual reference opening. The initial attempt
  to copy example/ as a resource directory collided with the staged document
  directory and stopped before launch. The successful before capture stages
  only the Markdown; the visible opening has no images. Final reference captures
  explicitly stage performance/layout-fixtures at its authored relative path.
- `body-size-reference-{wide,narrow,150,200}` captures the actual reference at
  1920/600px and 150/200% zoom, with unchanged-source and active AT-SPI sidecars.
  Wide, narrow and 200% screenshots inspected. At 200% the opening stacks while
  maintaining the readable enlarged text; the toolbar remains at its UI size.
- `body-size-cards-{wide,150,200}` uses fixture 127 to show ordinary list text,
  independent labeled cards and compact code. Three cards fit at 100%; at 200%
  the shared-width grid becomes two columns plus a final left-aligned card.
  Wide and 200% screenshots inspected.

All artifacts are under performance/layout-previews/body-size-*.

Reproduce:

```sh
python3 performance/capture-layout.py --source-document example/reference.md \
  --source-resource-dir performance/layout-fixtures \
  --binary target/debug/tachyon --width 1920 --height 1400 \
  --source-unchanged-check --atspi-active --output /tmp/body-size.png
```

This qualifies the requested font-size and bounded reflow change, not every
reference chapter, IME/RTL behavior or scrolling performance. The broader queue
and serializer work remain open.

Native `body-size-edit-{wide,150,200}` checks pass: pointer placement into the
card body, visual Home, type x, autosave and two seconds of focused idle match
complete expected source. One Undo restores every original byte. The existing
serializer escapes the edited bold label's colon (`Document view\:`); the full
expected source explicitly includes only that escape and Measure -> xMeasure.
An initial stricter expectation recorded this known serializer behavior rather
than a misplaced edit. Six copied markers retain source order; this is not a
complete rich MIME qualification. Script: `/tmp/verify-body-size-edits.py`;
final log: `/tmp/mineral-body-size-edits-final.log`.
