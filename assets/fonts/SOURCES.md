# Bundled fonts

The font binaries in this directory are generated from Google Fonts commit
`5e35378e6bda803962ee6fd257e444a7d459660d` with FontTools `4.64.0` by
`scripts/prepare-fonts.sh`. Re-run that script to reproduce the static assets;
`SHA256SUMS` records the expected result.

- The single Fraunces headline family pins ExtraBold `wght=800`, maximum
  `SOFT=100`, `WONK=1`, and `opsz=72`. Upright and italic inputs are
  instantiated separately.
- Public Sans is the proportional body family. ExtraLight 200 is the prose
  weight; 400, 600, and 700 faces retain native UI and authored emphasis.
  Each weight has a true italic companion.
- Spline Sans Mono includes upright 400/600 and the upstream italic 400 face.
- Noto Sans upright/italic faces provide bundled Latin, Greek, Cyrillic, and
  Vietnamese fallback coverage. GPUI's platform fallback remains available for
  scripts outside those bundled ranges.

Each family is distributed under the SIL Open Font License 1.1. The relevant
license text is included alongside the generated files.
