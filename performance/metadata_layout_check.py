"""Independent Board-01 spacing/rule checks for the native metadata specimen.

Typography families and source roles also require the Rust/native semantic
checks; these pixels alone do not approve the complete component family.
"""
import argparse
import json
from pathlib import Path
from PIL import Image

ROOT = Path(__file__).resolve().parent / "layout-previews"
RULE = (218, 221, 213)


def check_strip(path, zoom):
    image = Image.open(path).convert("RGB")
    assert image.size == (1600, 1200)
    # Native origin + 8 title margin + 58 title + 24 gap + 64 lead + 24 gap.
    # This is the written spacing/type grammar, not a candidate-image golden.
    top = 54 + 178 * zoom
    bottom = top + (16 + 20 + 16) * zoom
    rows = []
    for y in range(top - 3, bottom + 3):
        positions = [x for x in range(276, 1530) if image.getpixel((x, y)) == RULE]
        if len(positions) >= 500 * zoom:
            assert positions == list(range(positions[0], positions[-1] + 1))
            rows.append((y, positions[0], positions[-1] + 1))
    assert [row[0] for row in rows] == list(range(top, top + zoom)) + list(range(bottom - zoom, bottom)), rows
    assert all(row[1] == 276 and row[2] == rows[0][2] for row in rows)
    # Require actual text in each property, not an empty outlined strip.
    text_y = range(top + 16 * zoom, top + 36 * zoom)
    chunks = [(276, 430), (467, 582), (610, 820)]
    for start, end in chunks:
        x1, x2 = 276 + (start - 276) * zoom, 276 + (end - 276) * zoom
        assert sum(max(image.getpixel((x, y))) < 150 for y in text_y for x in range(x1, x2)) > 100 * zoom
    return {"zoom": zoom, "top": top, "bottom": bottom, "rule_rows": rows,
            "strip_width_logical": (rows[0][2] - 276) / zoom, "passes": True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--capture-prefix", default="metadata-verified")
    parser.add_argument("--accessible-prefix", default="metadata-verified-accessible")
    parser.add_argument("--output", default="metadata.pixels.json")
    args = parser.parse_args()
    results = [check_strip(ROOT / f"{args.capture_prefix}-wide.png", 1),
               check_strip(ROOT / f"{args.capture_prefix}-200.png", 2)]
    assert abs(results[0]["strip_width_logical"] - results[1]["strip_width_logical"]) <= 1
    source = json.loads((ROOT / f"{args.capture_prefix}-wide.source.json").read_text())
    double_source = json.loads((ROOT / f"{args.capture_prefix}-200.source.json").read_text())
    assert double_source["binary_sha256"] == source["binary_sha256"]
    assert double_source["original_sha256"] == source["original_sha256"]
    assert source["source_unchanged"] and double_source["source_unchanged"]
    accessible = json.loads((ROOT / f"{args.accessible_prefix}.active-atspi.json").read_text())
    assert accessible["binary_sha256"] == source["binary_sha256"]
    nodes = {node["name"]: node["bounds"] for node in accessible["nodes"]
             if node["role"] in ("paragraph", "heading")}
    first = [nodes[name] for name in ("Status: In review", "Version: 2.1", "Owner: Editorial team")]
    assert all(node["y"] == 248 and node["height"] == 20 for node in first)
    assert first[1]["x"] - (first[0]["x"] + first[0]["width"]) == 24
    assert first[2]["x"] - (first[1]["x"] + first[1]["width"]) == 24
    assert nodes["Properties"]["y"] - results[0]["bottom"] == 64
    properties = [node for name, node in nodes.items()
                  if name.startswith(("Source: ", "Review scope: ", "Distribution: "))]
    assert len(properties) == 3
    assert all(node["height"] == 40 and node["x"] == 276 for node in properties)
    assert [properties[i + 1]["y"] - (properties[i]["y"] + properties[i]["height"])
            for i in range(2)] == [12, 12]
    assert nodes["Ordinary features remain ordinary"]["y"] - (properties[-1]["y"] + properties[-1]["height"]) == 64
    result = {"binary_sha256": source["binary_sha256"], "checks": results,
              "native_semantic_gaps": {"strip_gutters": 24, "section_gap": 64, "property_row_gap": 12}, "passes": True}
    (ROOT / args.output).write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
