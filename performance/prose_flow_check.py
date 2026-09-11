"""Independent native reading-band spacing and responsive-flow checks.

Consumes real screenshots, native AT-SPI bounds and source hashes. These
checks do not approve pagination, figures, footnotes or other grammar families.
"""
import argparse
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent / "layout-previews"


def ink_lines(image, rect):
    x, y, width, height = rect
    rows = [row for row in range(y, y + height)
            if sum(max(image.getpixel((col, row))[:3]) < 160
                   for col in range(x, x + width)) >= 20]
    starts = []
    for index, row in enumerate(rows):
        # Dots/accents can be disconnected from the body of the same glyph.
        if index == 0 or row > rows[index - 1] + 5:
            starts.append(row)
    return starts


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", default="prose-flow-verified")
    args = parser.parse_args()
    evidence = {}
    sources = []
    for mode in ("wide", "narrow", "short", "200"):
        prefix = ROOT / f"{args.prefix}-{mode}"
        source = json.loads(prefix.with_suffix(".source.json").read_text())
        accessible = json.loads(prefix.with_suffix(".active-atspi.json").read_text())
        assert source["source_unchanged"] and source["passes"]
        assert source["binary_sha256"] == accessible["binary_sha256"]
        sources.append(source)
        nodes = [node for node in accessible["nodes"]
                 if node["role"] in ("paragraph", "heading")]

        def bounds(starts):
            found = [node["bounds"] for node in nodes if node["name"].startswith(starts)]
            assert len(found) == 1, starts
            return found[0]

        intro = bounds("A field guide")
        heading = bounds("1. A place")
        first = bounds("The most useful")
        continued = bounds("Attention is")
        review = bounds("Our review practice")
        next_heading = bounds("2. Keep")
        if mode == "wide":
            assert review["x"] - first["x"] - first["width"] == 24
            assert first["height"] % 28 == review["height"] % 28 == 0
            assert heading["y"] - intro["y"] - intro["height"] == 64
            assert first["y"] - heading["y"] - heading["height"] == 24
            assert next_heading["y"] - continued["y"] - continued["height"] == 64
            assert continued["width"] > first["width"] * 2
            assert continued["y"] == first["y"], "same source paragraph reaches the right-column top"
            image = Image.open(prefix.with_suffix(".png")).convert("RGB")
            # Actual glyph rows, not an empty semantic rectangle. Paragraph
            # and band transitions add 24 px to the 28 px narrative leading.
            left = ink_lines(image, (first["x"], first["y"], first["width"], continued["height"]))
            assert len(left) >= 20
            paragraph_top = first["y"] + first["height"] + 24
            band_top = review["y"]
            # Whitespace remains paper, and actual body glyphs occupy every
            # 28 px line box. Glyph ascenders need not share one ink top.
            line_boxes = list(range(first["y"], first["y"] + first["height"], 28))
            line_boxes += list(range(paragraph_top, band_top - 24, 28))
            line_boxes += list(range(band_top, continued["y"] + continued["height"], 28))
            for top in line_boxes:
                assert len(ink_lines(image, (first["x"], top, first["width"], 28))) == 1
            for top in (paragraph_top - 24, band_top - 24):
                assert not ink_lines(image, (first["x"], top, first["width"], 24))
            right = ink_lines(image, (review["x"], first["y"], review["width"], review["y"] - first["y"]))
            assert len(right) >= 8 and abs(right[0] - left[0]) <= 4
            assert right[-1] - right[0] < 560
            evidence[mode] = {"gutter": 24, "paragraph_and_band_gap": 24,
                              "section_gap": 64, "narrative_leading": 28,
                              "left_glyph_line_starts": left, "right_glyph_line_starts": right}
        else:
            assert first["x"] == continued["x"] == review["x"]
            assert continued["y"] >= first["y"] + first["height"]
            assert review["y"] >= continued["y"] + continued["height"]
            evidence[mode] = {"single_column": True, "source_order": True}
    assert len({s["binary_sha256"] for s in sources}) == 1
    assert len({s["original_sha256"] for s in sources}) == 1
    result = {"binary_sha256": sources[0]["binary_sha256"],
              "fixture_sha256": sources[0]["original_sha256"],
              "checks": evidence, "passes": True}
    (ROOT / f"{args.prefix}.pixels.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
