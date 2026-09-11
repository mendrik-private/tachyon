# Tachyon renderer patches

Base: crates.io `latex-rust` 1.0.2, package checksum
`002bcaf7505b3516103424d8211b44cc4eb401cec17d35283402882b403c11fc`.
Upstream: https://github.com/jscarr64/LaTeX-Rust

The source, required data, font and licenses are retained here so correctness
fixes apply before parent formulas are laid out. Original benchmark/test/gold
files are not shipped in this copy; the unused benchmark manifest entry is
removed. Tachyon's document-view tests exercise the integrated renderer.

Local changes:

- Center matrix row stacks and delimiters on the math axis before computing
  enclosing formula geometry, including nested fractions and scripts.
- Apply the measured glyph scale in the SVG emitter. Upstream 1.0.2 measures
  scripts at reduced size but renders every outline at the root em size.

No network provider, external TeX command or executable document content is
enabled. The embedded font remains covered by its original SIL OFL license.
