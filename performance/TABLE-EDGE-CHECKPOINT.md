# Automatic table reading edges

2026-09-10. Crusty `work_53a7f65f3a1cb8c3`. User policy confirmed: preserve saved
column widths; apply automatic reading-edge alignment to automatically sized tables.

Automatically measured tables whose preferred width is within 20% of the
loaded reference prose boundary now negotiate that boundary, limited by their
actual container. Column growth is proportional; shrinkage retains the existing
minimum-to-preferred interpolation. Unbreakable minima still win. Compact and
wide tables retain their independent widths. The same projection width resolver
feeds cell wrapping, geometry and hit testing. No source metadata is rewritten.

The reference boundary uses the same calibrated 84-character expansion as
ordinary reference prose. This is a presentation threshold, not a new authored
Markdown setting. Saved column widths and focused table locks keep their
existing authority.

## Verified evidence

- `near_width_tables_share_the_reading_edge_without_squeezing_minima` covers
  growth/shrinkage, constrained containers, compact/wide exclusions and minima.
- GPUI `automatic_table_edges_follow_loaded_font_measure` uses fixture 126 at
  100/150/200% with 400/1000/1600 document-unit canvases. It checks actual loaded
  font measures, table source coverage and unchanged serialized bytes. Existing
  authored-width, rich-cell and geometry tests remain green.
- `scripts/check.sh` passed: 784 Rust tests, two existing ignored tests; formatting,
  locked checks, strict Clippy, adapter suites and doctests passed. Log:
  `/tmp/mineral-table-edge-check-final.log`. `git diff --check` passed.
- Native debug layout-validation binary SHA-256:
  `2fe3a20f99835fa7b277600d1a41a3c5fd88772c22189db51060c141b7ed758f`.
- `table-edge-auto-before` and `table-edge-auto-after` show the second table
  changing from 577px to 639px, matching its preceding prose boundary. The compact
  table stays 128px. Old binary SHA-256:
  `d82e831163d6ecd059fd7febc842894fd51ef8bd34110735c78447a373c6b249`.
- Native 1920/1440/600px and 150/200% captures have exact-source and AT-SPI
  sidecars. At 600px both automatic tables occupy the 576px pane; the lookup
  remains 128px. At 150% the second table is 959px; at 200% it fills the available
  1160px. Regular, narrow and 200% screenshots were inspected. The first table
  participates in the existing supporting-table composition and record fallback;
  its local container can differ from the following full reading track.
- `table-edge-edit-{after,150,200}`: native pointer/Home insertion in Open,
  autosave and two-second focused idle all match the complete expected source
  (only Open -> xOpen). One Undo restores exact original bytes. Four copied
  markers retain source order; this does not qualify complete rich MIME payloads.
  Execution script: `/tmp/verify-table-edge-edits.py`; log:
  `/tmp/mineral-table-edge-edits.log`.

All native artifacts are under `performance/layout-previews/table-edge-*`.
Reproduce a capture with:

```sh
python3 performance/capture-layout.py --fixture 126-automatic-table-edges.md \
  --binary target/debug/tachyon --width 1440 --height 1100 \
  --source-unchanged-check --atspi-active --output /tmp/table-edge.png
```

## Confirmed policy and current-font verification

The user explicitly chose: “Preserve saved widths; align automatically sized
tables.” The reported palette therefore retains its saved column widths
[380.078125, 230.203125]. Fixture 125 preserves the exact excerpt and metadata.
The differing palette edge is intentional under this decision.

Fresh `table-policy-{wide,regular,narrow,150,200}` native captures use the current
21px body font and release binary SHA-256
`41e830f406194ffe1f513be7e878fe44c2e40b0376bf547ba3cad46acc7c6be1`.
The earlier 639px measurements above belong to the previous body font; the
current full reading boundary is about 839px at 100%.

| Window / zoom | Automatic typography table | Saved palette table |
| --- | ---: | ---: |
| 1920px / 100% | 839px | 610px |
| 1440px / 100% | 765px local supporting-table track | 610px |
| 600px / 100% | 576px available pane | 610px |
| 1920px / 150% | 1259px | 915px |
| 1920px / 200% | 1632px available pane | 1220px |

AT-SPI bounds provide the rounded screen-pixel measurements. Saved widths scale
with zoom and may exceed a narrow viewport; automatic alignment does not rewrite
them. Wide, regular, narrow and 200% screenshots were inspected. All five source
checks preserve the exact original SHA-256
`011002539f5e5ff3fe8ab877ffa3d8d203fabee6134733e52b539641dff8e16c`.

The latest full `scripts/check.sh` run passed 786 Rust tests with two existing
ignored tests, including the loaded-font table regressions; see
`/tmp/mineral-snapshot-check.log`. This policy confirmation changes documentation
and work evidence only.

`table-policy-edit-{wide,150,200}` additionally verifies native pointer/Home
insertion in the saved-width palette at 100/150/200%. Autosave and two seconds
of focused idle match the complete original source with only `Primary text`
changed to `xPrimary text`; one Undo restores every original byte, including
column metadata. Four copied markers preserve source order. This is not a full
rich-clipboard qualification. Script: `/tmp/verify-table-policy-edits.py`; log:
`/tmp/mineral-table-policy-edits.log`.
