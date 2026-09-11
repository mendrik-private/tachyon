"""Native compact-table rail: compare full semantics, real widths and pixels."""
import argparse
import json
from pathlib import Path

from PIL import Image
from reference_prose_flow_check import capture, semantics
from specification_example_check import bounds, one


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--unchanged", nargs=2, type=Path, action="append", default=[])
    parser.add_argument("--short", type=Path)
    parser.add_argument("--stack", type=Path)
    args = parser.parse_args()
    old_source, old = capture(args.before)
    source, nodes = capture(args.after)
    assert source["original_sha256"] == old_source["original_sha256"]
    assert semantics(nodes) == semantics(old)
    compact = one(nodes, "table", "Configuration table")
    assert compact == one(old, "table", "Configuration table"), "small table unchanged"
    before = one(old, "table", "Comparison across environments table")
    after = one(nodes, "table", "Comparison across environments table")
    assert after["y"] == compact["y"] == before["y"]
    assert after["width"] > before["width"] + 100
    assert after["height"] == before["height"] - 42
    heading = bounds(nodes, "Configuration")
    assert after["x"] - heading["x"] - heading["width"] == 24
    assert 0 <= heading["width"] - compact["width"] < 40
    assert bounds(nodes, "Comparison across environments")["y"] == heading["y"]
    title = bounds(nodes, "Create a document")
    utilization = (after["x"] + after["width"] - title["x"]) / title["width"]
    assert .99 <= utilization <= 1
    saved = bounds(old, "Exact code")["y"] - bounds(nodes, "Exact code")["y"]
    assert saved == 42
    for name in ("Create a document", "Authentication", "Parameters", "Request"):
        assert bounds(nodes, name) == bounds(old, name)
    image = Image.open(args.after.with_suffix(".png")).convert("RGB")
    rule = (218, 221, 213)
    for table in (compact, after):
        y = table["y"] - 10
        xs = range(table["x"] + 3, table["x"] + table["width"] - 3)
        assert any(sum(image.getpixel((x, row)) == rule for x in xs) > len(xs) * .8
                   for row in range(y - 1, y + 2)), "actual aligned table rules"
    unchanged = []
    for baseline, current in args.unchanged:
        before_source, before_nodes = capture(baseline)
        current_source, current_nodes = capture(current)
        assert current_source["binary_sha256"] == source["binary_sha256"]
        assert before_source["original_sha256"] == current_source["original_sha256"]
        assert semantics(before_nodes) == semantics(current_nodes)
        geometry = lambda tree: [(n["path"], n["bounds"]) for n in tree
                                 if n["role"] in ("heading", "paragraph", "table", "static")]
        assert geometry(before_nodes) == geometry(current_nodes), str(current)
        unchanged.append(str(current))
    for prefix, paired in ((args.short, True), (args.stack, False)):
        if prefix is None:
            continue
        other_source, other = capture(prefix)
        assert other_source["binary_sha256"] == source["binary_sha256"]
        assert other_source["original_sha256"] == source["original_sha256"]
        assert semantics(other) == semantics(nodes)
        left, right = bounds(other, "Configuration"), bounds(other, "Comparison across environments")
        assert (left["y"] == right["y"] and left["x"] < right["x"]) == paired
        if not paired:
            assert left["x"] == right["x"] and left["y"] < right["y"]
        reports = json.loads(prefix.with_suffix(".planning.json").read_text())["reports"]
        report = [r for r in reports if r["committed"]][-1]
        for row in report["chosen"]:
            if row["kind"] != "stack":
                assert not row["height_estimated"] and row["rejected"] is None
                assert max(row["heights"]) <= report["viewport_height"] * .7
    result = dict(binary_sha256=source["binary_sha256"], source_sha256=source["original_sha256"],
                  canonical_nodes=len(semantics(nodes)), comparison_width_gain=after["width"] - before["width"],
                  vertical_saving_px=saved, canvas_utilization=utilization,
                  unchanged=unchanged, short=str(args.short), stack=str(args.stack), passes=True)
    args.after.with_suffix(".pixels.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
