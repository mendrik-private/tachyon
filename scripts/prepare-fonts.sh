#!/usr/bin/env bash
set -euo pipefail

google_fonts_commit=5e35378e6bda803962ee6fd257e444a7d459660d
fonttools_version=4.64.0
repository_root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
output_directory="$repository_root/assets/fonts"
work_directory=$(mktemp -d /tmp/mineral-font-preparation.XXXXXX)
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
download "$base/splinesans/SplineSans%5Bwght%5D.ttf" "$work_directory/spline-sans.ttf"
download "$base/splinesansmono/SplineSansMono%5Bwght%5D.ttf" "$work_directory/spline-mono.ttf"
download "$base/splinesansmono/SplineSansMono-Italic%5Bwght%5D.ttf" "$work_directory/spline-mono-italic.ttf"
download "$base/notosans/NotoSans%5Bwdth%2Cwght%5D.ttf" "$work_directory/noto-sans.ttf"
download "$base/notosans/NotoSans-Italic%5Bwdth%2Cwght%5D.ttf" "$work_directory/noto-sans-italic.ttf"
download "$base/fraunces/OFL.txt" "$work_directory/OFL-Fraunces.txt"
download "$base/splinesans/OFL.txt" "$work_directory/OFL-Spline-Sans.txt"
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

instance fraunces.ttf Fraunces-Mineral-H1-Semibold.ttf wght=600 SOFT=30 WONK=1 opsz=120
instance fraunces-italic.ttf Fraunces-Mineral-H1-SemiboldItalic.ttf wght=600 SOFT=30 WONK=1 opsz=120
instance fraunces.ttf Fraunces-Mineral-H2-Semibold.ttf wght=600 SOFT=40 WONK=1 opsz=72
instance fraunces-italic.ttf Fraunces-Mineral-H2-SemiboldItalic.ttf wght=600 SOFT=40 WONK=1 opsz=72
instance fraunces.ttf Fraunces-Mineral-H3-Semibold.ttf wght=600 SOFT=40 WONK=1 opsz=20
instance fraunces-italic.ttf Fraunces-Mineral-H3-SemiboldItalic.ttf wght=600 SOFT=40 WONK=1 opsz=20
instance spline-sans.ttf SplineSans-Mineral-Regular.ttf wght=400
instance spline-sans.ttf SplineSans-Mineral-Semibold.ttf wght=600
instance spline-mono.ttf SplineSansMono-Mineral-Regular.ttf wght=400
instance spline-mono.ttf SplineSansMono-Mineral-Semibold.ttf wght=600
instance spline-mono-italic.ttf SplineSansMono-Mineral-Italic.ttf wght=400
instance noto-sans.ttf NotoSans-Mineral-Regular.ttf wght=400 wdth=100
instance noto-sans-italic.ttf NotoSans-Mineral-Italic.ttf wght=400 wdth=100

"$python" - "$work_directory" <<'PY'
from pathlib import Path
import math
import sys
from fontTools.pens.transformPen import TransformPen
from fontTools.pens.ttGlyphPen import TTGlyphPen
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

def oblique(source, target, family, weight_name, weight):
    font = TTFont(source, recalcTimestamp=False)
    shear = math.tan(math.radians(12))
    glyf = font["glyf"]
    glyph_set = font.getGlyphSet()
    for name in font.getGlyphOrder():
        pen = TTGlyphPen(glyph_set)
        glyph_set[name].draw(TransformPen(pen, (1, 0, shear, 1, 0, 0)))
        glyf[name] = pen.glyph()
    font["post"].italicAngle = -12
    font["OS/2"].fsSelection |= 1
    font["OS/2"].fsSelection &= ~(1 << 6)
    font["head"].macStyle |= 2
    font["OS/2"].usWeightClass = weight
    font.save(target, reorderTables=False)
    rename(target, family, f"{weight_name} Oblique")

def set_space_advance(path, em_fraction):
    """Keep the body face readable at UI sizes without changing its outlines."""
    font = TTFont(path, recalcTimestamp=False)
    glyph = font.getBestCmap()[0x20]
    _, left_side_bearing = font["hmtx"].metrics[glyph]
    advance = round(font["head"].unitsPerEm * em_fraction)
    font["hmtx"].metrics[glyph] = (advance, left_side_bearing)
    font.save(path, reorderTables=False)

for level in ("H1", "H2", "H3"):
    rename(root / f"Fraunces-Mineral-{level}-Semibold.ttf", f"Fraunces Mineral {level}", "Semibold")
    rename(root / f"Fraunces-Mineral-{level}-SemiboldItalic.ttf", f"Fraunces Mineral {level}", "Semibold Italic")
rename(root / "SplineSans-Mineral-Regular.ttf", "Spline Sans Mineral", "Regular")
rename(root / "SplineSans-Mineral-Semibold.ttf", "Spline Sans Mineral", "Semibold")
oblique(root / "SplineSans-Mineral-Regular.ttf", root / "SplineSans-Mineral-Oblique.ttf", "Spline Sans Mineral", "Regular", 400)
oblique(root / "SplineSans-Mineral-Semibold.ttf", root / "SplineSans-Mineral-SemiboldOblique.ttf", "Spline Sans Mineral", "Semibold", 600)
rename(root / "SplineSansMono-Mineral-Regular.ttf", "Spline Sans Mono Mineral", "Regular")
rename(root / "SplineSansMono-Mineral-Semibold.ttf", "Spline Sans Mono Mineral", "Semibold")
rename(root / "SplineSansMono-Mineral-Italic.ttf", "Spline Sans Mono Mineral", "Italic")
rename(root / "NotoSans-Mineral-Regular.ttf", "Noto Sans Mineral", "Regular")
rename(root / "NotoSans-Mineral-Italic.ttf", "Noto Sans Mineral", "Italic")
for face in (
    "SplineSans-Mineral-Regular.ttf",
    "SplineSans-Mineral-Semibold.ttf",
    "SplineSans-Mineral-Oblique.ttf",
    "SplineSans-Mineral-SemiboldOblique.ttf",
):
    set_space_advance(root / face, 0.30)
PY

mkdir -p "$output_directory"
find "$output_directory" -maxdepth 1 -type f \( -name '*.ttf' -o -name 'OFL-*.txt' \) -delete
generated=(
    Fraunces-Mineral-H1-Semibold.ttf
    Fraunces-Mineral-H1-SemiboldItalic.ttf
    Fraunces-Mineral-H2-Semibold.ttf
    Fraunces-Mineral-H2-SemiboldItalic.ttf
    Fraunces-Mineral-H3-Semibold.ttf
    Fraunces-Mineral-H3-SemiboldItalic.ttf
    SplineSans-Mineral-Regular.ttf
    SplineSans-Mineral-Semibold.ttf
    SplineSans-Mineral-Oblique.ttf
    SplineSans-Mineral-SemiboldOblique.ttf
    SplineSansMono-Mineral-Regular.ttf
    SplineSansMono-Mineral-Semibold.ttf
    SplineSansMono-Mineral-Italic.ttf
    NotoSans-Mineral-Regular.ttf
    NotoSans-Mineral-Italic.ttf
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
