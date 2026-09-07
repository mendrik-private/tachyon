# Stylo derive compatibility patch

Base: crates.io `stylo_derive` 0.20.0, MPL-2.0. Original Rust source,
normalized Cargo manifest and README are retained. Registry bookkeeping and
the upstream lockfile are omitted. All original source license headers remain.

Only `to_css.rs` is changed: emitted `Ok(())` expressions explicitly name
`std::fmt::Error`. GPUI enables `serde_fmt` through structured logging; its
additional `From` implementation makes Stylo's otherwise inferred error type
ambiguous. The full document-view build reproduced 32 E0282/E0283 errors.

This is the narrow fix described in upstream
[servo/stylo#452](https://github.com/servo/stylo/pull/452), without importing
that branch's unrelated Stylo changes. Remove this patch when a compatible
published Stylo release includes the correction. Verify with
`cargo check -p document-view --locked` in the complete workspace dependency
graph; testing the macro alone does not reproduce the conflicting impl.
