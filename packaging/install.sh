#!/bin/sh
set -eu

if [ "$#" -lt 1 ] || [ "$#" -gt 2 ]; then
    echo "usage: packaging/install.sh PREFIX [RELEASE_BINARY]" >&2
    exit 2
fi

mineral_prefix=$1
mineral_binary=${2:-target/release/mineral-markdown}

case "$mineral_prefix" in
    ""|/)
        echo "refusing an empty or filesystem-root prefix" >&2
        exit 2
        ;;
esac

if [ ! -x "$mineral_binary" ]; then
    echo "release binary is missing or not executable: $mineral_binary" >&2
    exit 1
fi

install -Dm755 "$mineral_binary" "$mineral_prefix/bin/mineral-markdown"
install -Dm644 packaging/dev.mineral.Markdown.desktop \
    "$mineral_prefix/share/applications/dev.mineral.Markdown.desktop"
install -Dm644 packaging/dev.mineral.Markdown.metainfo.xml \
    "$mineral_prefix/share/metainfo/dev.mineral.Markdown.metainfo.xml"
install -Dm644 packaging/icons/hicolor/scalable/apps/dev.mineral.Markdown.png \
    "$mineral_prefix/share/icons/hicolor/scalable/apps/dev.mineral.Markdown.png"
install -Dm644 LICENSE-MIT \
    "$mineral_prefix/share/licenses/mineral-markdown/LICENSE-MIT"
install -Dm644 LICENSE-APACHE \
    "$mineral_prefix/share/licenses/mineral-markdown/LICENSE-APACHE"
