"""Verify complete rich-event semantics, native rails and offscreen ownership."""
import argparse
import json
from pathlib import Path

from PIL import Image
from reference_prose_flow_check import capture, semantics


DATES = ("2026-09-10:", "2026-09-08:", "2026-09-06:")
RULE = (218, 221, 213)
ACCENT = (63, 98, 71)


def one(nodes, role, prefix):
    found = [n for n in nodes if n["role"] == role and n["name"].startswith(prefix)]
    assert len(found) == 1, (role, prefix, len(found))
    return found[0]


def rail_coverage(pixels, x, top, bottom):
    assert 0 <= top < bottom <= pixels.height
    counts = [sum(pixels.getpixel((column, y)) == RULE for y in range(top, bottom))
              for column in range(x - 2, x + 3)]
    # Exactly one native pixel column owns the rule at display/text scale1.
    assert sum(count > (bottom - top) * 0.95 for count in counts) == 1, counts
    return max(counts) / (bottom - top)


def event_support_geometry(nodes):
    """Every supporting leaf stays between its authored date and the next event.

    Use canonical ancestry rather than accepting only the currently visible
    code/table. Nested children and table-cell paragraphs are included.
    """
    headers = [one(nodes, "paragraph", date) for date in DATES]
    end = one(nodes, "heading", "Ordinary requirements")["bounds"]["y"]
    events = []
    for index, header in enumerate(headers):
        item_index = header["parent"]
        assert nodes[item_index]["role"] == "list item"
        descendants = {item_index}
        support = []
        for ordinal, node in enumerate(nodes):
            if node["parent"] in descendants:
                descendants.add(ordinal)
                if node is not header and node["role"] in ("paragraph", "static"):
                    support.append(node["bounds"])
        assert support, "the rich fixture must include every event's supporting leaves"
        bounds = header["bounds"]
        bottom = headers[index + 1]["bounds"]["y"] if index + 1 < len(headers) else end
        events.append({
            "date": DATES[index], "support_count": len(support),
            "passes": all(b["width"] > 0 and b["height"] > 0
                          and b["y"] >= bounds["y"] + bounds["height"] - 1
                          and b["y"] + b["height"] <= bottom + 1 for b in support),
        })
    return {"events": events, "passes": all(event["passes"] for event in events)}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--variant", type=Path, action="append", default=[])
    parser.add_argument("--offscreen", type=Path, required=True)
    args = parser.parse_args()
    original, old = capture(args.before)
    current, new = capture(args.after)
    canonical = semantics(old, 68)
    assert len(canonical) == 68
    assert original["original_sha256"] == current["original_sha256"]
    assert semantics(new, 68) == canonical
    for variant in args.variant:
        source, nodes = capture(variant)
        assert source["binary_sha256"] == current["binary_sha256"]
        assert source["original_sha256"] == original["original_sha256"]
        assert semantics(nodes, 68) == canonical

    # The technical evidence retains its actual dimensions, not a squeezed
    # rendition selected to make the new timeline fit.
    for role, prefix in (("static", "mineral-markdown field-notes.md"), ("table", "Table")):
        before = one(old, role, prefix)["bounds"]
        after = one(new, role, prefix)["bounds"]
        assert (after["x"], after["width"], after["height"]) == (before["x"], before["width"], before["height"])
    headers = [one(new, "paragraph", date)["bounds"] for date in DATES]
    assert len({h["x"] for h in headers}) == 1
    assert all(a["y"] + a["height"] < b["y"] for a, b in zip(headers, headers[1:]))
    pixels = Image.open(args.after.with_suffix(".png")).convert("RGB")
    coverage = []
    for header, following in zip(headers, headers[1:]):
        x, y = header["x"] + 8, header["y"] + 12
        assert sum(pixels.getpixel((xx, yy)) == ACCENT
                   for xx in range(x - 4, x + 5) for yy in range(y - 4, y + 5)) >= 30
        coverage.append(rail_coverage(pixels, x, y + 8, following["y"] + 4))

    stress_source, stress = capture(args.offscreen)
    assert stress_source["binary_sha256"] == current["binary_sha256"]
    assert len(semantics(stress, 68)) == 68
    code = one(stress, "static", "# Retained event evidence")["name"]
    assert code == "# Retained event evidence\n" * 60 + "mineral-markdown field-notes.md\n"
    viewport = one(stress, "entry", "Markdown document editor")["bounds"]
    header = one(stress, "paragraph", DATES[0])["bounds"]
    following = one(stress, "paragraph", DATES[1])["bounds"]
    assert header["y"] + header["height"] < viewport["y"] - viewport["height"], "header must be outside the one-viewport overscan"
    assert following["y"] > viewport["y"] + viewport["height"]
    stress_pixels = Image.open(args.offscreen.with_suffix(".png")).convert("RGB")
    offscreen_coverage = rail_coverage(stress_pixels, header["x"] + 8,
                                      viewport["y"], viewport["y"] + viewport["height"])
    result = dict(passes=True, canonical_nodes=68, rail_coverage=coverage,
                  offscreen_rail_coverage=offscreen_coverage,
                  offscreen_header=header, viewport=viewport,
                  variants=[str(p) for p in args.variant], binary_sha256=current["binary_sha256"])
    args.after.with_suffix(".pixels.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
