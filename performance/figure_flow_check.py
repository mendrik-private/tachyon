"""Independent native pixels, spacing, semantic order and source checks for fixture 67.

This checks true supporting-image wrap, not caption lanes or the entire grammar.
"""
import argparse
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent / "layout-previews"


def ink_count(image, rect):
    x, y, width, height = map(round, rect)
    assert 0 <= x < x + width <= image.width
    assert 0 <= y < y + height <= image.height
    return sum(max(image.getpixel((col, row))[:3]) < 160
               for row in range(y, y + height) for col in range(x, x + width))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", default="figure-flow-verified")
    args = parser.parse_args()
    evidence, sources = {}, []
    for mode in ("wide", "narrow", "200-top"):
        prefix = ROOT / f"{args.prefix}-{mode}"
        source = json.loads(prefix.with_suffix(".source.json").read_text())
        tree = json.loads(prefix.with_suffix(".active-atspi.json").read_text())
        assert source["passes"] and source["source_unchanged"]
        assert source["binary_sha256"] == tree["binary_sha256"]
        sources.append(source)
        nodes = tree["nodes"]

        def node(role, starts):
            found = [n for n in nodes if n["role"] == role and n["name"].startswith(starts)]
            assert len(found) == 1, (mode, role, starts)
            return found[0]

        figure_node = node("image", "Botanical illustration")
        prose_node = node("paragraph", "A field notebook connects")
        figure, prose = figure_node["bounds"], prose_node["bounds"]
        assert nodes.index(figure_node) < nodes.index(prose_node)
        # Alternative text remains an image name, never a fabricated caption.
        assert not any(n["role"] == "paragraph" and n["name"].startswith("Botanical illustration") for n in nodes)
        if mode in ("wide", "200-top"):
            zoom = 2 if mode == "200-top" else 1
            next_paragraph = node("paragraph", "The following paragraph")["bounds"]
            heading = node("heading", "A separate subject")["bounds"]
            assert figure["x"] == prose["x"] == next_paragraph["x"]
            assert figure["y"] == prose["y"]
            assert 0.25 <= figure["width"] / prose["width"] <= 0.35
            assert abs(next_paragraph["y"] - prose["y"] - prose["height"] - 24 * zoom) <= 1
            assert abs(heading["y"] - next_paragraph["y"] - next_paragraph["height"] - 64 * zoom) <= 1
            image = Image.open(prefix.with_suffix(".png")).convert("RGB")
            leading = 28 * zoom
            narrow_lines = (figure["height"] + leading - 1) // leading
            assert narrow_lines >= 4
            right = figure["x"] + figure["width"] + 24 * zoom + 1  # integer bounds truncate the fractional track
            for row in range(narrow_lines):
                assert ink_count(image, (right, prose["y"] + row * leading, prose["width"] - (right - prose["x"]), leading)) > 50
            # The actual gutter is paper, and the full-width continuation has
            # glyphs under the former image footprint at the next baseline.
            assert ink_count(image, (figure["x"] + figure["width"] + 2, figure["y"], 20 * zoom, figure["height"])) == 0
            full_y = prose["y"] + narrow_lines * leading
            assert ink_count(image, (prose["x"], full_y, figure["width"], leading)) > 50
            evidence[mode] = {"figure_fraction": figure["width"] / prose["width"],
                              "gutter": 24 * zoom, "leading": leading, "useful_lines": narrow_lines,
                              "full_measure_return_y": full_y, "paragraph_gap": 24 * zoom, "section_gap": 64 * zoom}
        else:
            assert prose["y"] >= figure["y"] + figure["height"], (mode, figure, prose)
            assert prose["x"] == figure["x"]
            evidence[mode] = {"stacked": True, "source_order": True}
    assert len({s["binary_sha256"] for s in sources}) == 1
    assert len({s["original_sha256"] for s in sources}) == 1
    result = {"binary_sha256": sources[0]["binary_sha256"], "fixture_sha256": sources[0]["original_sha256"],
              "checks": evidence, "passes": True}
    (ROOT / f"{args.prefix}.pixels.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
