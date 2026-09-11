"""Check native reference-font columns, canonical semantics and stacked fallbacks."""
import argparse
import json
from pathlib import Path

from PIL import Image
from prose_flow_check import ink_lines


MARKERS = ("This paragraph keeps", "This second paragraph", "This third paragraph",
           "This fourth paragraph")


def capture(prefix):
    source = json.loads(prefix.with_suffix(".source.json").read_text())
    tree = json.loads(prefix.with_suffix(".active-atspi.json").read_text())
    assert source["passes"] and source["source_unchanged"]
    assert source["binary_sha256"] == tree["binary_sha256"]
    return source, tree["nodes"]


def bounds(nodes, marker):
    found = [node["bounds"] for node in nodes
             if node["role"] in ("paragraph", "heading")
             and node["name"].startswith(marker)]
    assert len(found) == 1, marker
    return found[0]


def semantics(nodes, minimum=41):
    """Exclude changing shell rows; retain every canonical document descendant."""
    included = set()
    result = []
    for ordinal, node in enumerate(nodes):
        if node["role"] == "document frame" or node["parent"] in included:
            included.add(ordinal)
            parent_path = nodes[node["parent"]]["path"] if node["parent"] in included else None
            result.append((node["path"], parent_path, node["role"], node["name"],
                           node["description"], node["actions"]))
    assert len(result) >= minimum, "complete document, not a viewport-only sample"
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--stack", type=Path, action="append", default=[])
    args = parser.parse_args()
    before_source, before_nodes = capture(args.before)
    after_source, after_nodes = capture(args.after)
    assert before_source["original_sha256"] == after_source["original_sha256"]
    canonical = semantics(before_nodes)
    assert semantics(after_nodes) == canonical, "roles, IDs, parents, names and actions survive reflow"
    old = [bounds(before_nodes, marker) for marker in MARKERS]
    new = [bounds(after_nodes, marker) for marker in MARKERS]
    assert len({node["x"] for node in old}) == 1
    assert new[0]["y"] == new[2]["y"], "aligned reading-column tops"
    assert new[0]["x"] == new[1]["x"] and new[2]["x"] == new[3]["x"]
    assert new[2]["x"] - new[0]["x"] - new[0]["width"] == 24
    assert [node["height"] for node in old] == [node["height"] for node in new]
    assert [node["width"] for node in old] == [node["width"] for node in new]
    heading = bounds(after_nodes, "6. Formula")
    span = new[2]["x"] + new[2]["width"] - new[0]["x"]
    assert 0.95 <= span / heading["width"] <= 1.0
    old_height = bounds(before_nodes, "7. Final")["y"] - old[0]["y"]
    new_height = bounds(after_nodes, "7. Final")["y"] - new[0]["y"]
    assert old_height > new_height
    image = Image.open(args.after.with_suffix(".png")).convert("RGB")
    glyphs = []
    for first, last in ((new[0], new[1]), (new[2], new[3])):
        height = last["y"] + last["height"] - first["y"]
        assert first["y"] >= 0 and first["y"] + height <= image.height
        rows = ink_lines(image, (first["x"], first["y"], first["width"], height))
        assert len(rows) >= 6, "actual text in both columns"
        # Different letters have different ascender tops. Check actual ink in
        # each semantic 24px line box, not equal pixel offsets of the first ink.
        for paragraph in (first, last):
            assert paragraph["height"] % 24 == 0
            for top in range(paragraph["y"], paragraph["y"] + paragraph["height"], 24):
                assert len(ink_lines(image, (paragraph["x"], top, paragraph["width"], 24))) == 1
        gap_top = first["y"] + first["height"]
        assert not ink_lines(image, (first["x"], gap_top, first["width"], last["y"] - gap_top))
        glyphs.append(rows)
    assert abs(glyphs[0][0] - glyphs[1][0]) <= 4
    heights = [rows[-1] - rows[0] + 24 for rows in glyphs]
    assert max(heights) <= min(heights) * 1.5
    stacks = []
    for prefix in args.stack:
        source, nodes = capture(prefix)
        assert source["binary_sha256"] == after_source["binary_sha256"]
        assert source["original_sha256"] == after_source["original_sha256"]
        assert semantics(nodes) == canonical
        paragraphs = [bounds(nodes, marker) for marker in MARKERS]
        assert len({node["x"] for node in paragraphs}) == 1
        assert all(b["y"] >= a["y"] + a["height"] for a, b in zip(paragraphs, paragraphs[1:]))
        stacks.append(str(prefix))
    result = dict(binary_sha256=after_source["binary_sha256"],
                  source_sha256=after_source["original_sha256"],
                  canonical_nodes=len(canonical), canvas_utilization=span / heading["width"],
                  saved_height_px=old_height - new_height, glyph_lines=glyphs,
                  column_heights=heights, stacked_captures=stacks, passes=True)
    args.after.with_suffix(".reference-flow.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
