#!/bin/sh
# Build this checkout and replace the per-user installation.
set -eu

if [ "$#" -gt 1 ]; then
    echo "usage: ./install.sh [PREFIX] (default: \$HOME/.local)" >&2
    exit 2
fi
case "${1-}" in
    -h|--help)
        echo "usage: ./install.sh [PREFIX] (default: \$HOME/.local)"
        echo "Builds the current checkout, including local edits; does not fetch Git updates."
        exit 0
        ;;
esac

tachyon_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
tachyon_prefix=${1-"$HOME/.local"}
case "$tachyon_prefix" in
    ""|/)
        echo "refusing an empty or filesystem-root prefix" >&2
        exit 2
        ;;
    /*) ;;
    *) tachyon_prefix="$PWD/$tachyon_prefix" ;;
esac

cd "$tachyon_root"
echo "Building Tachyon from $tachyon_root ..."
cargo build --release --locked --bin tachyon --target-dir "$tachyon_root/target"

mkdir -p "$tachyon_prefix"
tachyon_prefix=$(CDPATH= cd -- "$tachyon_prefix" && pwd -P)
if [ "$tachyon_prefix" = / ]; then
    echo "refusing a filesystem-root prefix" >&2
    exit 2
fi
tachyon_stage=$(mktemp -d "$tachyon_prefix/.tachyon-install.XXXXXX")
trap 'rm -rf -- "$tachyon_stage"' EXIT
trap 'exit 1' HUP INT TERM

packaging/install.sh "$tachyon_stage" "$tachyon_root/target/release/tachyon"
mkdir -p "$tachyon_prefix/bin" "$tachyon_prefix/share"
cp -R "$tachyon_stage/share/." "$tachyon_prefix/share/"
# Rename on the same filesystem: running instances keep their existing binary.
mv -f "$tachyon_stage/bin/tachyon" "$tachyon_prefix/bin/tachyon"

if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$tachyon_prefix/share/applications" ||
        echo "Desktop cache refresh failed; application files are installed." >&2
fi
if command -v gtk-update-icon-cache >/dev/null 2>&1; then
    gtk-update-icon-cache -f -t "$tachyon_prefix/share/icons/hicolor" ||
        echo "Icon cache refresh failed; application files are installed." >&2
fi

echo "Installed Tachyon to $tachyon_prefix/bin/tachyon"
echo "Restart Tachyon to use this build. Ensure $tachyon_prefix/bin is in your desktop session's PATH."
