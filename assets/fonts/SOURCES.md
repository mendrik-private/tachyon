# Bundled fonts

The document grammar's narrative role uses the unmodified Liberation Serif
2.1 family (regular, italic, bold, and bold italic), imported from the local
`fonts-liberation` distribution. Unlike a system-font fallback, these embedded
faces give reading sections identical metrics on every installation.
Upstream: <https://github.com/liberationfonts/liberation-fonts>.
`LIBERATION-SHA256SUMS` pins these binaries; `OFL-Liberation-Serif.txt` retains
their copyright and SIL Open Font License. `prepare-fonts.sh` does not regenerate
these unmodified upstream assets.

The other font binaries in this directory are generated from Google Fonts commit
`5e35378e6bda803962ee6fd257e444a7d459660d` with FontTools `4.64.0` by
`scripts/prepare-fonts.sh`. Re-run that script to reproduce the static assets;
`SHA256SUMS` records the expected result.

- Fraunces heading faces pin `wght=600`, `WONK=1`, with H1 at
  `SOFT=30,opsz=120`, H2 at `SOFT=40,opsz=72`, and H3-H6 at
  `SOFT=40,opsz=20`. Upright and italic inputs are instantiated separately.
- Spline Sans body faces pin weights 400 and 600. Upstream does not contain an
  italic master, so the two oblique faces apply a deterministic 12 degree
  outline transform and correct the OpenType italic flags. The generated body
  faces set U+0020 to a 0.30 em advance so prose remains legible at the 15 px UI
  size; this changes metrics only, not glyph outlines.
- Spline Sans Mono includes upright 400/600 and the upstream italic 400 face.
- Noto Sans upright/italic faces provide bundled Latin, Greek, Cyrillic, and
  Vietnamese fallback coverage. GPUI's platform fallback remains available for
  scripts outside those bundled ranges.

Each family is distributed under the SIL Open Font License 1.1; the relevant
license text is included alongside the font files.
