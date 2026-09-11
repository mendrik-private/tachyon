#!/usr/bin/env bash
set -euo pipefail

google_fonts_commit=5e35378e6bda803962ee6fd257e444a7d459660d
fonttools_version=4.64.0
repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
output_directory="$repository_root/assets/fonts"
work_directory=$(mktemp -d /tmp/tachyon-font-preparation.XXXXXX)
trap 'rm -rf "$work_directory"' EXIT

python3 -m venv "$work_directory/venv"
"$work_directory/venv/bin/pip" --quiet install "fonttools==$fonttools_version"
python="$work_directory/venv/bin/python"
base="https://raw.githubusercontent.com/google/fonts/$google_fonts_commit/ofl"

download() {
    curl --fail --location --silent --show-error "$1" --output "$2"
}

download "$base/fraunces/Fraunces%5BSOFT%2CWONK%2Copsz%2Cwght%5D.ttf" "$work_directory/fraunces.ttf"
download "$base/fraunces/Fraunces-Italic%5BSOFT%2CWONK%2Copsz%2Cwght%5D.ttf" "$work_directory/fraunces-italic.ttf"
download "$base/publicsans/PublicSans%5Bwght%5D.ttf" "$work_directory/public-sans.ttf"
download "$base/publicsans/PublicSans-Italic%5Bwght%5D.ttf" "$work_directory/public-sans-italic.ttf"
download "$base/splinesansmono/SplineSansMono%5Bwght%5D.ttf" "$work_directory/spline-mono.ttf"
download "$base/splinesansmono/SplineSansMono-Italic%5Bwght%5D.ttf" "$work_directory/spline-mono-italic.ttf"
download "$base/notosans/NotoSans%5Bwdth%2Cwght%5D.ttf" "$work_directory/noto-sans.ttf"
download "$base/notosans/NotoSans-Italic%5Bwdth%2Cwght%5D.ttf" "$work_directory/noto-sans-italic.ttf"
download "$base/fraunces/OFL.txt" "$work_directory/OFL-Fraunces.txt"
download "$base/publicsans/OFL.txt" "$work_directory/OFL-Public-Sans.txt"
download "$base/splinesansmono/OFL.txt" "$work_directory/OFL-Spline-Sans-Mono.txt"
download "$base/notosans/OFL.txt" "$work_directory/OFL-Noto-Sans.txt"

instance() {
    input=$1
    output=$2
    shift 2
    "$python" -m fontTools.varLib.instancer \
        --quiet --static --no-recalc-timestamp \
        --output "$work_directory/$output" "$work_directory/$input" "$@"
}

instance fraunces.ttf Fraunces-Tachyon-ExtraBold.ttf wght=800 SOFT=100 WONK=1 opsz=72
instance fraunces-italic.ttf Fraunces-Tachyon-ExtraBoldItalic.ttf wght=800 SOFT=100 WONK=1 opsz=72
instance public-sans.ttf PublicSans-Tachyon-ExtraLight.ttf wght=200
instance public-sans.ttf PublicSans-Tachyon-Regular.ttf wght=400
instance public-sans.ttf PublicSans-Tachyon-Semibold.ttf wght=600
instance public-sans.ttf PublicSans-Tachyon-Bold.ttf wght=700
instance public-sans-italic.ttf PublicSans-Tachyon-ExtraLightItalic.ttf wght=200
instance public-sans-italic.ttf PublicSans-Tachyon-Italic.ttf wght=400
instance public-sans-italic.ttf PublicSans-Tachyon-SemiboldItalic.ttf wght=600
instance public-sans-italic.ttf PublicSans-Tachyon-BoldItalic.ttf wght=700
instance spline-mono.ttf SplineSansMono-Tachyon-Regular.ttf wght=400
instance spline-mono.ttf SplineSansMono-Tachyon-Semibold.ttf wght=600
instance spline-mono-italic.ttf SplineSansMono-Tachyon-Italic.ttf wght=400
instance noto-sans.ttf NotoSans-Tachyon-Regular.ttf wght=400 wdth=100
instance noto-sans-italic.ttf NotoSans-Tachyon-Italic.ttf wght=400 wdth=100

"$python" - "$work_directory" <<'PY'
from pathlib import Path
import sys
from fontTools.ttLib import TTFont

root = Path(sys.argv[1])

def rename(path, family, subfamily):
    font = TTFont(path, recalcTimestamp=False)
    full = f"{family} {subfamily}"
    postscript = full.replace(" ", "-")
    replacements = {1: family, 2: subfamily, 4: full, 6: postscript, 16: family, 17: subfamily}
    for record in font["name"].names:
        if record.nameID in replacements:
            record.string = replacements[record.nameID].encode(record.getEncoding())
    font.save(path, reorderTables=False)

rename(root / "Fraunces-Tachyon-ExtraBold.ttf", "Fraunces Tachyon", "ExtraBold")
rename(root / "Fraunces-Tachyon-ExtraBoldItalic.ttf", "Fraunces Tachyon", "ExtraBold Italic")
rename(root / "PublicSans-Tachyon-ExtraLight.ttf", "Public Sans Tachyon", "ExtraLight")
rename(root / "PublicSans-Tachyon-Regular.ttf", "Public Sans Tachyon", "Regular")
rename(root / "PublicSans-Tachyon-Semibold.ttf", "Public Sans Tachyon", "Semibold")
rename(root / "PublicSans-Tachyon-Bold.ttf", "Public Sans Tachyon", "Bold")
rename(root / "PublicSans-Tachyon-ExtraLightItalic.ttf", "Public Sans Tachyon", "ExtraLight Italic")
rename(root / "PublicSans-Tachyon-Italic.ttf", "Public Sans Tachyon", "Italic")
rename(root / "PublicSans-Tachyon-SemiboldItalic.ttf", "Public Sans Tachyon", "Semibold Italic")
rename(root / "PublicSans-Tachyon-BoldItalic.ttf", "Public Sans Tachyon", "Bold Italic")
rename(root / "SplineSansMono-Tachyon-Regular.ttf", "Spline Sans Mono Tachyon", "Regular")
rename(root / "SplineSansMono-Tachyon-Semibold.ttf", "Spline Sans Mono Tachyon", "Semibold")
rename(root / "SplineSansMono-Tachyon-Italic.ttf", "Spline Sans Mono Tachyon", "Italic")
rename(root / "NotoSans-Tachyon-Regular.ttf", "Noto Sans Tachyon", "Regular")
rename(root / "NotoSans-Tachyon-Italic.ttf", "Noto Sans Tachyon", "Italic")
PY

mkdir -p "$output_directory"
find "$output_directory" -maxdepth 1 -type f \( -name '*.ttf' -o -name 'OFL-*.txt' \) -delete
generated=(
    Fraunces-Tachyon-ExtraBold.ttf
    Fraunces-Tachyon-ExtraBoldItalic.ttf
    PublicSans-Tachyon-ExtraLight.ttf
    PublicSans-Tachyon-Regular.ttf
    PublicSans-Tachyon-Semibold.ttf
    PublicSans-Tachyon-Bold.ttf
    PublicSans-Tachyon-ExtraLightItalic.ttf
    PublicSans-Tachyon-Italic.ttf
    PublicSans-Tachyon-SemiboldItalic.ttf
    PublicSans-Tachyon-BoldItalic.ttf
    SplineSansMono-Tachyon-Regular.ttf
    SplineSansMono-Tachyon-Semibold.ttf
    SplineSansMono-Tachyon-Italic.ttf
    NotoSans-Tachyon-Regular.ttf
    NotoSans-Tachyon-Italic.ttf
)
for font in "${generated[@]}"; do
    install -m 0644 "$work_directory/$font" "$output_directory/$font"
done
install -m 0644 "$work_directory"/OFL-*.txt "$output_directory/"
(
    cd "$output_directory"
    sha256sum "${generated[@]}" > SHA256SUMS
    chmod 0644 SHA256SUMS SOURCES.md
)
