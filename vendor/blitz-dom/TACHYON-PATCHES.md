# Blitz DOM text hit-testing patch

Base: crates.io `blitz-dom` **0.3.0-beta.2**, upstream commit
`67edf2061121382b3e43af19977fc3472aa7e069` (`packages/blitz-dom`),
MIT OR Apache-2.0. The published Rust sources, assets and normalized Cargo
manifest are retained; registry bookkeeping and the upstream lockfile are not.

Only `src/node/node.rs` is changed. `Node::hit_inner` uses Parley's
`Cluster::first_style()` to identify the text owner instead of requiring the
cluster to have a glyph. A ligature continuation has text and advance but may
have no glyph of its own. Previously, clicking it returned `None` early and
incorrectly hit an ancestor; one combining accent could therefore disable
Tachyon's verified direct editing for an entire HTML fragment.

This leaves paint-order traversal, clipping, pointer-events handling and
per-cluster geometry unchanged. It does not accept ancestor hits in Tachyon
or normalize canonical text. Tests exercise the complete dependency graph:

```sh
cargo test --locked -p document-view combining_accent_retains_verified_html_text_hits
cargo test --locked -p document-view html_edit_hits_never_guess
cargo test --locked -p document-view installed_ -- --ignored
```

The installed-font integration tests require Noto CJK, Arabic and Color Emoji
coverage. The combining-accent regression uses Tachyon's bundled Latin font
and does not require those installed fonts. Remove this local patch when a
compatible published Blitz version uses cluster-owned hit-test styles; rerun
the regressions and native HTML edit/undo checks before doing so.
