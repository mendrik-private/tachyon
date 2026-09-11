"""Verify nested timeline ownership, native geometry and offscreen ancestor rails."""
import argparse
import json
from pathlib import Path

from PIL import Image
from reference_prose_flow_check import capture, semantics
from rich_timeline_check import one, rail_coverage, ACCENT


PAIRS = (("2024:", "2025: Review", 32),
         ("2022:", "2023:", 32),
         ("2026:", "2025: Earlier", 8),
         ("2026-09-08:", "2026-09-10:", 32))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--variant", type=Path, action="append", default=[])
    parser.add_argument("--offscreen", type=Path, required=True)
    args = parser.parse_args()
    before, old = capture(args.before)
    after, nodes = capture(args.after)
    canonical = semantics(old, 38)
    assert len(canonical) == 38
    for prefix in (args.after, *args.variant):
        source, variant = capture(prefix)
        assert source["binary_sha256"] == after["binary_sha256"]
        assert source["original_sha256"] == before["original_sha256"]
        assert semantics(variant, 38) == canonical
        for start, end, _ in PAIRS:
            a, b = [one(variant, "paragraph", marker)["bounds"] for marker in (start, end)]
            assert a["y"] + a["height"] <= b["y"]
            assert a["x"] == b["x"] and a["width"] == b["width"]
        for start, end in (("2024:", "Supporting evidence"),
                           ("2025: Review", "The research branch"),
                           ("2026-09-10:", "The release summary"),
                           ("The release summary", "2025: Earlier")):
            a, b = [one(variant, "paragraph", marker)["bounds"] for marker in (start, end)]
            assert a["y"] + a["height"] <= b["y"]
    for code in ("mineral-markdown research", "mineral-markdown release"):
        a, b = [one(tree, "static", code)["bounds"] for tree in (old, nodes)]
        assert all(a[key] == b[key] for key in ("x", "width", "height"))

    pixels = Image.open(args.after.with_suffix(".png")).convert("RGB")
    coverage = []
    for start, end, inset in PAIRS:
        a, b = [one(nodes, "paragraph", marker)["bounds"] for marker in (start, end)]
        x, y = a["x"] + inset, a["y"] + 12
        assert sum(pixels.getpixel((xx, yy)) == ACCENT
                   for xx in range(x - 4, x + 5) for yy in range(y - 4, y + 5)) >= 30
        coverage.append(rail_coverage(pixels, x, y + 8, b["y"] + 4))
        if inset == 32:
            # No redundant ordinary-tree rule beside a dated list's own rail.
            duplicate_x = a["x"] + 24
            assert sum(pixels.getpixel((duplicate_x, yy)) == (218, 221, 213)
                       for yy in range(y + 8, b["y"] + 4)) == 0

    stress_source, stress = capture(args.offscreen)
    assert stress_source["binary_sha256"] == after["binary_sha256"]
    assert len(semantics(stress, 38)) == 38
    code = one(stress, "static", "# Retained nested event")["name"]
    assert code == "# Retained nested event\n" * 60 + "mineral-markdown release.md\n"
    viewport = one(stress, "entry", "Markdown document editor")["bounds"]
    stress_pixels = Image.open(args.offscreen.with_suffix(".png")).convert("RGB")
    offscreen_coverage = []
    for start, end, inset in PAIRS[2:]:
        a, b = [one(stress, "paragraph", marker)["bounds"] for marker in (start, end)]
        assert a["y"] + a["height"] < viewport["y"] - viewport["height"]
        assert b["y"] > viewport["y"] + viewport["height"]
        offscreen_coverage.append(rail_coverage(stress_pixels, a["x"] + inset,
                                  viewport["y"], viewport["y"] + viewport["height"]))
    result = dict(passes=True, canonical_nodes=38, binary_sha256=after["binary_sha256"],
                  rail_coverage=coverage, offscreen_rail_coverage=offscreen_coverage,
                  variants=[str(p) for p in args.variant])
    args.after.with_suffix(".nested.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
