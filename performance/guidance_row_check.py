"""Native optional guidance rows: full semantics, unchanged measures and ink."""
import argparse
import json
from pathlib import Path

from PIL import Image
from reference_prose_flow_check import capture, semantics
from specification_example_check import one
from prose_flow_check import ink_lines


MARKERS = ("A presentation choice", "Choose List")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("before", type=Path)
    parser.add_argument("after", type=Path)
    parser.add_argument("--stack", type=Path, action="append", default=[])
    parser.add_argument("--pair", type=Path, action="append", default=[])
    args = parser.parse_args()
    old_source, old = capture(args.before)
    source, nodes = capture(args.after)
    canonical = semantics(old, minimum=29)
    assert len(canonical) == 29
    assert semantics(nodes, minimum=29) == canonical
    assert source["original_sha256"] == old_source["original_sha256"]
    old_boxes = [one(old, "notification", marker) for marker in MARKERS]
    new_boxes = [one(nodes, "notification", marker) for marker in MARKERS]
    assert old_boxes[0]["x"] == old_boxes[1]["x"]
    assert old_boxes[1]["y"] > old_boxes[0]["y"] + old_boxes[0]["height"]
    assert new_boxes[0] == old_boxes[0], "first notice keeps exact geometry"
    for old_box, new_box in zip(old_boxes, new_boxes):
        assert (old_box["width"], old_box["height"]) == (new_box["width"], new_box["height"])
    title = one(nodes, "heading", "Details worth keeping")
    assert title == one(old, "heading", "Details worth keeping")
    saved = one(old, "heading", "Languages and inline detail")["y"] - one(nodes, "heading", "Languages and inline detail")["y"]
    assert saved == 108
    for prefix in [args.after] + args.pair + args.stack:
        other_source, other = capture(prefix)
        assert other_source["binary_sha256"] == source["binary_sha256"]
        assert other_source["original_sha256"] == source["original_sha256"]
        assert semantics(other, minimum=29) == canonical
        left, right = [one(other, "notification", marker) for marker in MARKERS]
        if prefix in args.stack:
            assert left["x"] == right["x"] and right["y"] > left["y"] + left["height"]
            continue
        assert left["y"] == right["y"] and right["x"] - left["x"] - left["width"] == 24
        assert left["width"] == right["width"] == 639
        assert left["height"] == right["height"] == 48
        image = Image.open(prefix.with_suffix(".png")).convert("RGB")
        for box, fill in zip((left, right), ((237, 244, 249), (232, 238, 226))):
            # Existing compact labels remain on the leading rail, body at160px.
            assert image.getpixel((box["x"] + 120, box["y"] + 25)) == fill
            for y in range(box["y"], box["y"] + box["height"], 24):
                assert len(ink_lines(image, (box["x"] + 160, y, box["width"] - 176, 24))) == 1
    span = new_boxes[1]["x"] + new_boxes[1]["width"] - new_boxes[0]["x"]
    assert span / title["width"] > .99
    result = dict(binary_sha256=source["binary_sha256"], source_sha256=source["original_sha256"],
                  canonical_nodes=len(canonical), vertical_saving_px=saved,
                  canvas_utilization=span / title["width"], stacks=list(map(str, args.stack)),
                  pairs=list(map(str, args.pair)), passes=True)
    args.after.with_suffix(".pixels.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result))


if __name__ == "__main__":
    main()
