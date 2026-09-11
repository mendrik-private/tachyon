#!/bin/sh
set -eu

zed_pin=8b1497dbd22fb06f5838a7c0b84a1e54fafa71bc
component_pin=ff3eb1128ac1058f1bb88e777744ce1237aa3b79

if ! grep -Fq "git+https://github.com/zed-industries/zed#$zed_pin" Cargo.lock; then
    echo "Cargo.lock does not contain the expected Zed revision $zed_pin" >&2
    exit 1
fi

if awk -v expected="$zed_pin" '
    /^source = "git\+https:\/\/github.com\/zed-industries\/zed#/ && index($0, expected) == 0 { bad = 1 }
    END { exit bad }
' Cargo.lock; then
    :
else
    echo "Cargo.lock contains more than one Zed source revision" >&2
    exit 1
fi

if ! grep -Fq "gpui-component?rev=$component_pin#$component_pin" Cargo.lock; then
    echo "Cargo.lock does not contain the expected GPUI Component revision $component_pin" >&2
    exit 1
fi

cargo metadata --locked --format-version 1 --no-deps >/dev/null
cargo fmt --all -- --check
cargo check --workspace --all-targets --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --all-targets --locked
cargo test --locked -p accesskit_atspi_common
cargo test --manifest-path performance/a11y-publication-tests/Cargo.toml --locked
cargo clippy --manifest-path performance/a11y-publication-tests/Cargo.toml --all-targets --locked -- -D warnings
cargo test --workspace --doc --locked
