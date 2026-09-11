"""Check enclosed timelines against canonical identity and native geometry."""
import argparse
import json
from pathlib import Path

from PIL import Image
from reference_prose_flow_check import capture, semantics
from rich_timeline_check import one, rail_coverage, ACCENT


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--variant", action="append", type=Path, default=[])
    args = parser.parse_args()
    baseline, old = capture(args.before)
    final, nodes = capture(args.after)
    canonical = semantics(old, 40)
    assert baseline["original_sha256"] == final["original_sha256"]
    assert semantics(nodes, 40) == canonical
    dates = ("2024:", "2025:", "2026-09-08:", "2026-09-10:")
    prefixes = [args.after, *args.variant]
    for prefix in prefixes:
        source, variant = capture(prefix)
        assert source["binary_sha256"] == final["binary_sha256"]
        assert source["original_sha256"] == baseline["original_sha256"]
        assert semantics(variant, 40) == canonical
        headers = [one(variant, "paragraph", date)["bounds"] for date in dates]
        assert all(a["y"] + a["height"] <= b["y"] for a, b in zip(headers, headers[1:]))
        for header in headers:
            assert header["x"] >= 0 and header["width"] > 0
            assert header["x"] + header["width"] <= source["width"]
        for start, end in (("2024:", "Supporting evidence"),
                           ("2025:", "Keep the original"),
                           ("Record the unanswered", "This paragraph follows"),
                           ("2026-09-10:", "The review remains")):
            a, b = [one(variant, "paragraph", p)["bounds"] for p in (start, end)]
            assert a["y"] + a["height"] <= b["y"], (start, end)

    # Readable summaries gain width without compressing technical evidence.
    before = one(old, "static", "tachyon")["bounds"]
    after = one(nodes, "static", "tachyon")["bounds"]
    assert all(before[key] == after[key] for key in ("x", "width", "height"))
    first = one(nodes, "paragraph", "2024:")["bounds"]
    assert first["height"] == 24, "a short summary fits without premature wrapping"
    assert first["width"] > one(old, "paragraph", "2024:")["bounds"]["width"]
    assert one(nodes, "heading", "Outside the containers")["bounds"]["width"] == 1314
    # Top-level compact milestones retain their horizontal composition.
    a, b = [one(nodes, "paragraph", date)["bounds"] for date in ("2022:", "2023:")]
    assert a["y"] == b["y"] and b["x"] >= a["x"] + a["width"]
    pixels = Image.open(args.after.with_suffix(".png")).convert("RGB")
    coverage = []
    for first, last, inset in (("2024:", "2025:", 32),
                               ("2026-09-08:", "2026-09-10:", 56)):
        a, b = [one(nodes, "paragraph", date)["bounds"] for date in (first, last)]
        x, y = a["x"] + inset, a["y"] + 12
        assert sum(pixels.getpixel((xx, yy)) == ACCENT
                   for xx in range(x - 4, x + 5) for yy in range(y - 4, y + 5)) >= 30
        coverage.append(rail_coverage(pixels, x, y + 8, b["y"] + 4))
    result = dict(passes=True, canonical_nodes=len(canonical),
                  binary_sha256=final["binary_sha256"], rail_coverage=coverage,
                  summary_bounds=one(nodes, "paragraph", "2024:")["bounds"],
                  variants=[str(p) for p in args.variant])
    args.after.with_suffix(".enclosed.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
