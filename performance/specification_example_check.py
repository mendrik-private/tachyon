"""Native specification/example composition: pixels, source and full semantics."""
import argparse
import json
from pathlib import Path

from PIL import Image
from reference_prose_flow_check import capture, semantics


def one(nodes, role, starts):
    result = [n["bounds"] for n in nodes if n["role"] == role and n["name"].startswith(starts)]
    assert len(result) == 1, (role, starts)
    return result[0]


def bounds(nodes, name):
    return one(nodes, "heading", name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--stack", type=Path, action="append", default=[])
    args = parser.parse_args()
    old_source, old = capture(args.before)
    source, nodes = capture(args.after)
    assert old_source["original_sha256"] == source["original_sha256"]
    canonical = semantics(old)
    assert semantics(nodes) == canonical
    parameters = bounds(nodes, "Parameters")
    request = bounds(nodes, "Request")
    old_request = bounds(old, "Request")
    assert old_request["x"] == bounds(old, "Parameters")["x"]
    assert request["x"] - parameters["x"] - parameters["width"] == 24
    assert request["y"] - parameters["y"] == 24, "authored example's own top inset"
    title = bounds(nodes, "Create a document")
    span = request["x"] + request["width"] - parameters["x"]
    assert .99 <= span / title["width"] <= 1
    table, old_table = [one(tree, "table", "Parameters table") for tree in (nodes, old)]
    code, old_code = [one(tree, "static", "{\n") for tree in (nodes, old)]
    assert table["width"] == old_table["width"] and table["height"] == old_table["height"]
    assert code["height"] == old_code["height"] == 126
    assert code["width"] < old_code["width"]
    assert parameters["y"] == bounds(old, "Parameters")["y"]
    saved = bounds(old, "Configuration")["y"] - bounds(nodes, "Configuration")["y"]
    assert saved > 200
    image = Image.open(args.after.with_suffix(".png")).convert("RGB")
    # Native table bounds enclose cell text, 10px below its painted top edge.
    table_top = table["y"] - 10
    code_top = request["y"] + request["height"] + 8
    assert abs(table_top - code_top) <= 1
    rule = (218, 221, 213)
    for x, width, y in ((table["x"] + 4, table["width"] - 8, table_top),
                        (request["x"] + 28, request["width"] - 56, code_top)):
        assert any(sum(image.getpixel((col, row)) == rule for col in range(x, x + width)) > width * .8
                   for row in range(y - 1, y + 2)), "actual aligned panel borders"
    # Every code line is painted, including the short brace-only lines.
    for y in range(code["y"], code["y"] + code["height"], 21):
        assert sum(max(image.getpixel((x, row))) < 160
                   for row in range(y, y + 21)
                   for x in range(code["x"] + 72, code["x"] + code["width"] - 24)) > 4
    for prefix in args.stack:
        other_source, other = capture(prefix)
        assert other_source["binary_sha256"] == source["binary_sha256"]
        assert other_source["original_sha256"] == source["original_sha256"]
        assert semantics(other) == canonical
        table = one(other, "table", "Parameters table")
        request = bounds(other, "Request")
        assert request["x"] == table["x"]
        assert request["y"] > table["y"] + table["height"]
    result = dict(binary_sha256=source["binary_sha256"], source_sha256=source["original_sha256"],
                  canonical_nodes=len(canonical), vertical_saving_px=saved,
                  canvas_utilization=span / title["width"], stacks=list(map(str, args.stack)), passes=True)
    args.after.with_suffix(".pixels.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
