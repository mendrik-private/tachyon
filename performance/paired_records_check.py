"""Native paired records retain all fields, usable width and comparison columns."""
import argparse
import json
from pathlib import Path

from PIL import Image
from reference_prose_flow_check import capture, semantics
from rich_timeline_check import one, RULE


def rows(nodes, name):
    table = one(nodes, "table", name)
    index = nodes.index(table)
    result = []
    for ordinal, node in enumerate(nodes):
        if node["parent"] == index and node["role"] == "table row":
            result.append([n for n in nodes if n["parent"] == ordinal])
    return result


def panel(cells, zoom=1):
    first, last = cells[0]["bounds"], cells[-1]["bounds"]
    return (first["x"], first["y"] - 24 * zoom, first["width"],
            last["y"] + last["height"] + 24 * zoom)


def record_geometry(records):
    """Every complete row is a labeled record or an ordinary table row."""
    if len(records) != 3 or any(len(record) != 6 for record in records):
        return False
    previous = None
    band_bottom = None
    for record in records:
        bounds = [cell["bounds"] for cell in record]
        stacked = all(a["x"] == b["x"] and a["width"] == b["width"]
                      and a["y"] + a["height"] <= b["y"]
                      for a, b in zip(bounds, bounds[1:]))
        columns = all(a["x"] + a["width"] <= b["x"] and a["y"] == b["y"]
                      for a, b in zip(bounds, bounds[1:]))
        if not (stacked or columns):
            return False
        first = bounds[0]
        if previous is not None:
            if first["y"] == previous["y"]:
                if not stacked or previous["x"] + previous["width"] > first["x"]:
                    return False
            elif first["y"] < band_bottom:
                return False
        bottom = max(b["y"] + b["height"] for b in bounds)
        band_bottom = max(band_bottom, bottom) if previous and first["y"] == previous["y"] else bottom
        previous = first
    return records[-1][-1]["name"] == ""


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--stack", type=Path, action="append", default=[])
    args = parser.parse_args()
    before, old = capture(args.before)
    after, nodes = capture(args.after)
    canonical = semantics(old, 88)
    assert len(canonical) == 88
    assert before["original_sha256"] == after["original_sha256"]
    assert semantics(nodes, 88) == canonical
    records = rows(nodes, "Package directory")[1:]
    assert record_geometry(records)
    assert len(records) == 3 and all(len(cells) == 6 for cells in records)
    panels = [panel(cells) for cells in records]
    a, b, c = panels
    assert a[1] == b[1] and b[0] - a[0] - a[2] == 16
    assert a[0] == c[0] and a[2] == b[2] == c[2]
    assert c[1] - max(a[3], b[3]) == 16
    canvas = one(nodes, "heading", "Package directory")["bounds"]
    utilization = (b[0] + b[2] - a[0]) / canvas["width"]
    assert 0.99 <= utilization <= 1.001
    for cells in records:
        for first, second in zip(cells, cells[1:]):
            assert first["bounds"]["x"] == second["bounds"]["x"]
            assert first["bounds"]["y"] + first["bounds"]["height"] <= second["bounds"]["y"]
    assert records[-1][-1]["name"] == "", "an empty steward must remain empty"
    assert records[1][1]["bounds"]["height"] < rows(old, "Package directory")[2][1]["bounds"]["height"]
    for first, second in zip(rows(old, "Comparable capacity"), rows(nodes, "Comparable capacity")):
        for a_cell, b_cell in zip(first, second):
            assert all(a_cell["bounds"][key] == b_cell["bounds"][key] for key in ("x", "width", "height"))
    pixels = Image.open(args.after.with_suffix(".png")).convert("RGB")
    for x, top, width, bottom in panels:
        for point in ((x + width // 2, top), (x, (top + bottom) // 2),
                      (x + width - 1, (top + bottom) // 2), (x + width // 2, bottom - 1)):
            assert pixels.getpixel(point) == RULE, (point, pixels.getpixel(point))
    for prefix in args.stack:
        source, stack = capture(prefix)
        assert source["original_sha256"] == after["original_sha256"]
        assert source["binary_sha256"] == after["binary_sha256"]
        assert semantics(stack, 88) == canonical
        zoom = 1 + source["zoom_steps"] * 0.1
        stack_panels = [panel(cells, zoom) for cells in rows(stack, "Package directory")[1:]]
        assert len({p[0] for p in stack_panels}) == 1
        assert all(a[3] <= b[1] for a, b in zip(stack_panels, stack_panels[1:]))
        assert all(p[0] >= 0 and p[0] + p[2] <= source["width"] for p in stack_panels)
    saved = one(old, "heading", "Following work")["bounds"]["y"] - one(nodes, "heading", "Following work")["bounds"]["y"]
    assert saved > 0
    result = dict(passes=True, canonical_nodes=88, canvas_utilization=utilization,
                  saved_height_px=saved, panels=panels, binary_sha256=after["binary_sha256"],
                  stacks=[str(p) for p in args.stack])
    args.after.with_suffix(".records.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
