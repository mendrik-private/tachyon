"""Compare native opening composition, complete semantics, ink and fallbacks."""
import argparse
import json
from pathlib import Path

from PIL import Image
from prose_flow_check import ink_lines
from reference_prose_flow_check import bounds, capture


MARKERS = ("Tachyon is a native", "The current implementation")


def semantics(nodes):
    included, result = set(), []
    for ordinal, node in enumerate(nodes):
        if node["role"] == "document frame" or node["parent"] in included:
            included.add(ordinal)
            parent = nodes[node["parent"]]["path"] if node["parent"] in included else None
            result.append((node["path"], parent, node["role"], node["name"],
                           node["description"], node["actions"]))
    assert len(result) >= 7, "whole opening specimen, including the next section and code"
    assert any(record[2] == "static" and "sudo apt-get install" in record[3] for record in result)
    return result


def pair_geometry(prefix, nodes):
    # The title and lead share a prefix; select the authored heading exactly.
    title, = [node["bounds"] for node in nodes if node["role"] == "heading" and node["name"] == "Tachyon"]
    lead, overview = [bounds(nodes, marker) for marker in MARKERS]
    assert lead["y"] == overview["y"]
    assert overview["x"] - lead["x"] - lead["width"] == 24
    assert lead["y"] - title["y"] - title["height"] == 24
    assert lead["height"] % 32 == overview["height"] % 28 == 0
    assert lead["height"] >= 96 and overview["height"] >= 112
    assert max(lead["height"], overview["height"]) <= min(lead["height"], overview["height"]) * 1.5
    span = overview["x"] + overview["width"] - lead["x"]
    assert 0.8 <= span / title["width"] <= 1
    bottom = max(p["y"] + p["height"] for p in (lead, overview))
    assert bounds(nodes, "Build and run")["y"] - bottom == 48
    image = Image.open(prefix.with_suffix(".png")).convert("RGB")
    for paragraph, leading in ((lead, 32), (overview, 28)):
        assert paragraph["y"] + paragraph["height"] < image.height
        for top in range(paragraph["y"], paragraph["y"] + paragraph["height"], leading):
            assert len(ink_lines(image, (paragraph["x"], top, paragraph["width"], leading))) == 1
    return dict(canvas_utilization=span / title["width"], column_widths=[lead["width"], overview["width"]],
                column_heights=[lead["height"], overview["height"]], gutter=24)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--pair", type=Path, action="append", default=[])
    parser.add_argument("--stack", type=Path, action="append", default=[])
    args = parser.parse_args()
    before_source, before = capture(args.before)
    after_source, after = capture(args.after)
    canonical = semantics(before)
    assert after_source["original_sha256"] == before_source["original_sha256"]
    assert semantics(after) == canonical
    old = [bounds(before, marker) for marker in MARKERS]
    assert old[0]["x"] == old[1]["x"] and old[1]["y"] > old[0]["y"] + old[0]["height"]
    pairs = {str(args.after): pair_geometry(args.after, after)}
    saved = bounds(before, "Build and run")["y"] - bounds(after, "Build and run")["y"]
    assert saved > 0
    for prefix in args.pair + args.stack:
        source, nodes = capture(prefix)
        assert source["original_sha256"] == after_source["original_sha256"]
        assert source["binary_sha256"] == after_source["binary_sha256"]
        assert semantics(nodes) == canonical
        if prefix in args.pair:
            pairs[str(prefix)] = pair_geometry(prefix, nodes)
        else:
            lead, overview = [bounds(nodes, marker) for marker in MARKERS]
            assert lead["x"] == overview["x"]
            assert overview["y"] > lead["y"] + lead["height"]
    result = dict(binary_sha256=after_source["binary_sha256"],
                  source_sha256=after_source["original_sha256"], canonical_nodes=len(canonical),
                  vertical_saving_px=saved, pairs=pairs, stacks=list(map(str, args.stack)), passes=True)
    args.after.with_suffix(".pixels.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
