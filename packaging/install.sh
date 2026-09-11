#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "usage: packaging/install.sh PREFIX [RELEASE_BINARY]" >&2
    exit 2
fi

tachyon_prefix=$1
tachyon_binary=${2:-target/release/tachyon}

case "$tachyon_prefix" in
    ""|/)
        echo "refusing an empty or filesystem-root prefix" >&2
        exit 2
        ;;
esac

if [ ! -x "$tachyon_binary" ]; then
    echo "release binary is missing or not executable: $tachyon_binary" >&2
    exit 1
fi

install -Dm755 "$tachyon_binary" "$tachyon_prefix/bin/tachyon"
install -Dm644 packaging/io.github.mendrik_private.Tachyon.desktop \
    "$tachyon_prefix/share/applications/io.github.mendrik_private.Tachyon.desktop"
install -Dm644 packaging/io.github.mendrik_private.Tachyon.metainfo.xml \
    "$tachyon_prefix/share/metainfo/io.github.mendrik_private.Tachyon.metainfo.xml"
install -Dm644 packaging/icons/hicolor/scalable/apps/io.github.mendrik_private.Tachyon.png \
    "$tachyon_prefix/share/icons/hicolor/scalable/apps/io.github.mendrik_private.Tachyon.png"
install -Dm644 LICENSE-MIT \
    "$tachyon_prefix/share/licenses/tachyon/LICENSE-MIT"
install -Dm644 LICENSE-APACHE \
    "$tachyon_prefix/share/licenses/tachyon/LICENSE-APACHE"
