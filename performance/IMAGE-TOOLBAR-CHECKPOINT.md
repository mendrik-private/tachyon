# Image formatting toolbar labels

2026-09-10. Completes Crusty `work_ffe90c381be297c7`.

The Image… and Retry actions were plain divs inheriting document typography,
while the adjacent Text action was a small GPUI Component button. Both image
actions now use that same small ghost button, retaining their IDs and click
callbacks and adding explicit tooltips. This shares font size, line height,
control height, padding and baseline behavior with Text, including document
zoom. The enclosing toolbar still prevents pointer-down from changing the
editor selection. The image panel's separate Retry image action is unchanged.

Native fixture103 captures under `performance/layout-previews/`:

- `image-toolbar-before.png`: reproduced oversized labels at 100%.
- `image-toolbar-final.png`: inspected light100% with matching compact labels.
- `image-toolbar-{light,dark}-{150,200}.png` and `image-toolbar-dark-100.png`:
  all inspected; toolbar remains 44px tall and labels retain their UI scale.
- `.source.json` sidecars verify exact unchanged fixture bytes in every run.
- `image-toolbar-label-measurements.json`: thresholded label-ink bounds are
  unchanged across all three zooms for each appearance. Different words have
  different ink heights (capitals/descenders); this is not a font-metric oracle.
  Shared small-button typography in source establishes the common text style.

Runtime SHA-256:
`97b1f414bd0e9913d7e70bfde279b3432e89fbd4354b8c3e73ad070c8293674c`.
`scripts/check.sh` passes (781 Rust tests, 2 existing ignored, locked checks,
formatting, strict Clippy, adapters, doctests); `/tmp/tachyon-toolbar-check.log`.
`git diff --check` passes. Crusty `ctx_9ebd1dd1353c` /
`task_d2d0b1d2872e4d8b`: 75 existing advisory findings, none new/worsened/resolved.

Reproduce the light100% capture:

```sh
python3 performance/capture-layout.py --fixture 103-image-states.md \
  --binary target/debug/tachyon --width 1440 --height 1100 \
  --select 380 370 700 370 --source-unchanged-check \
  --output /tmp/image-toolbar.png
```

For 150% use `--zoom-steps 5 --select 380 530 700 530`; for 200% use
`--zoom-steps 10 --select 380 720 700 720`. Add `--appearance dark` for dark
captures. This verifies the reported typography defect, not full command,
popover placement or keyboard-accessibility qualification (Audit A14).
