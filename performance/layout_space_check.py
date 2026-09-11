"""Check native document-space captures, reading order, and edit/undo evidence.

Run after the layout-space-{wide,medium,narrow,zoomed} capture matrix.
These are light-theme, 100% display-scale checks, not release qualification.
"""
import argparse
import json
from pathlib import Path

from PIL import Image
from prose_flow_check import ink_lines


def one_bounds(nodes, prefix):
    matches = [node["bounds"] for node in nodes if node["name"].startswith(prefix)]
    if len(matches) != 1:
        raise ValueError(f"Expected one accessible node beginning {prefix!r}")
    return matches[0]


def evaluate(nodes, window_width, columns):
    editor = one_bounds(nodes, "Markdown document editor")
    first = one_bounds(nodes, "The most useful documents")
    review = one_bounds(nodes, "Our review practice")
    # The captured wide/medium windows have the default 224px navigation
    # pane and 8px divider; compact navigation is an overlay, not a reservation.
    pane = window_width - (232 if window_width >= 800 else 0)
    if editor["width"] < pane * 0.95 or editor["x"] + editor["width"] > window_width:
        raise ValueError("The document must fill at least 95% of its pane without overflow")
    if first["x"] != editor["x"]:
        raise ValueError("Prose must share the document leading edge")
    if columns > 1:
        if abs(columns * first["width"] + (columns - 1) * 24 - editor["width"]) > 2:
            raise ValueError("Reading columns and gutters must fill the document canvas")
        if abs(review["x"] - first["x"] - (columns - 1) * (first["width"] + 24)) > 2:
            raise ValueError("The concluding paragraph must reach the final reading column")
    elif review["x"] != first["x"] or review["y"] <= first["y"]:
        raise ValueError("Narrow/zoomed prose must retain a single source-ordered column")
    return {"document_width": editor["width"], "columns": columns,
            "pane_utilization": editor["width"] / pane}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--prefix", type=Path,
                        default=Path(__file__).parent / "layout-previews/layout-space")
    args = parser.parse_args()
    results = {}
    hashes = set()
    for mode, columns in [("wide", 3), ("medium", 2), ("narrow", 1), ("zoomed", 1)]:
        prefix = args.prefix.with_name(f"{args.prefix.name}-{mode}")
        source = json.loads(prefix.with_suffix(".source.json").read_text())
        accessible = json.loads(prefix.with_suffix(".active-atspi.json").read_text())
        assert source["passes"] and source["source_unchanged"]
        assert source["original_sha256"] == source["current_sha256"]
        assert source["binary_sha256"] == accessible["binary_sha256"]
        assert (source["width"], source["height"], source["zoom_steps"]) == (
            accessible["width"], accessible["height"], accessible["zoom_steps"])
        hashes.add(source["binary_sha256"])
        results[mode] = evaluate(accessible["nodes"], source["width"], columns)
        with Image.open(prefix.with_suffix(".png")) as raw:
            image = raw.convert("RGB")
        assert image.size == (source["width"], source["height"])
        if columns > 1:
            first = one_bounds(accessible["nodes"], "The most useful documents")
            for column in range(columns):
                rect = (first["x"] + column * (first["width"] + 24),
                        first["y"], first["width"], 112)
                assert len(ink_lines(image, rect)) == 4, "Each column needs real rendered text"
    assert len(hashes) == 1, "Capture every mode from the same build"
    wide = args.prefix.with_name(f"{args.prefix.name}-wide")
    edit = json.loads(wide.with_suffix(".edit.json").read_text())
    copied = json.loads(wide.with_suffix(".copy.json").read_text())
    assert edit["binary_sha256"] in hashes and copied["binary_sha256"] in hashes
    assert edit["native_type_autosaved"] and edit["undo_restores_exact_bytes"]
    assert edit["edit_within"] == "while someone is writing"
    assert copied["passes"] and copied["source_order"]
    print(json.dumps({"passes": True, "layouts": results,
                      "binary_sha256": hashes.pop()}, indent=2))


if __name__ == "__main__":
    main()
