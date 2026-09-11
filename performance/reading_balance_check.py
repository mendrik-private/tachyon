"""Measure actual glyph columns in the unchanged native fixture113 capture."""
import argparse
import json
from pathlib import Path

from PIL import Image
from prose_flow_check import ink_lines


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("prefix", type=Path)
    args = parser.parse_args()
    source = json.loads(args.prefix.with_suffix(".source.json").read_text())
    tree = json.loads(args.prefix.with_suffix(".active-atspi.json").read_text())
    assert source["passes"] and source["source_unchanged"]
    assert source["binary_sha256"] == tree["binary_sha256"]

    def bounds(name):
        matches = [node["bounds"] for node in tree["nodes"]
                   if node["role"] in ("paragraph", "heading")
                   and node["name"].startswith(name)]
        assert len(matches) == 1, name
        return matches[0]

    first = bounds("The three-crate foundation")
    following = bounds("Constraints to preserve")
    image = Image.open(args.prefix.with_suffix(".png")).convert("RGB")
    lines = [ink_lines(image, (first["x"] + column * (first["width"] + 24),
                              first["y"], first["width"], following["y"] - first["y"]))
             for column in range(2)]
    assert all(lines), "both columns must contain actual text"
    heights = [rows[-1] - rows[0] + 28 for rows in lines]
    balanced = max(heights) <= min(heights) * 1.5
    result = dict(binary_sha256=source["binary_sha256"],
                  fixture_sha256=source["original_sha256"],
                  glyph_lines=lines, heights=heights, balanced=balanced,
                  following_heading_y=following["y"])
    print(json.dumps(result))
    assert abs(lines[0][0] - lines[1][0]) <= 4, "aligned column tops"
    assert balanced, "large accidental column void"


if __name__ == "__main__":
    main()
