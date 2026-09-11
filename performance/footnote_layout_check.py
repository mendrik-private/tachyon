#!/usr/bin/env python3
"""Independent checks over native AT-SPI bounds and captured note glyphs.

This checks fixture 66, not the entire grammar. In particular it rejects the
observed ellipsis-only return markers and punctuation-only reference lines.
"""
import argparse
import hashlib
import json
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent / "layout-previews"


def ink_rows(image, bounds):
    x, y, w, h = (bounds[key] for key in ("x", "y", "width", "height"))
    x0, y0 = max(0, int(x)), max(0, int(y))
    x1, y1 = min(image.width, int(x + w)), min(image.height, int(y + h))
    return [row for row in range(y0, y1)
            if any(max(image.getpixel((col, row))) < 185 for col in range(x0, x1))]


def check(prefix, profile, expected_hash):
    stem = ROOT / f"{prefix}-{profile}"
    evidence = json.loads(stem.with_suffix(".active-atspi.json").read_text())
    source = json.loads(stem.with_suffix(".source.json").read_text())
    assert source["source_unchanged"] and source["passes"]
    fixture = ROOT.parent / "layout-fixtures" / "66-footnotes.md"
    assert source["fixture"] == evidence["fixture"] == fixture.name
    assert source["current_sha256"] == source["original_sha256"] == hashlib.sha256(fixture.read_bytes()).hexdigest()
    assert source["binary_sha256"] == evidence["binary_sha256"]
    assert expected_hash in (None, source["binary_sha256"]), "mixed builds"
    image = Image.open(stem.with_suffix(".png")).convert("RGB")
    assert image.size == (evidence["width"], evidence["height"])
    zoom = 1 + evidence["zoom_steps"] * 0.1
    nodes = evidence["nodes"]
    refs = [node for node in nodes if node["role"] == "link" and node["name"].startswith("Footnote ")]
    notes = [node for node in nodes if node["role"] == "comment" and node["name"].startswith("Footnote ")]
    assert [node["name"] for node in refs] == ["Footnote 1", "Footnote 2", "Footnote 1", "Footnote 3", "Footnote 3"]
    assert [node["name"].split(".")[0] for node in notes] == ["Footnote 2", "Footnote 1", "Footnote 3"]
    for node in refs:
        b = node["bounds"]
        parent = nodes[node["parent"]]["bounds"]
        assert "click" in node["actions"]
        assert 4 * zoom <= b["width"] <= 14 * zoom, b
        assert b["x"] - parent["x"] > 20 * zoom, "reference stranded at start of a line"
    back = [node for node in nodes if node["role"] == "link" and node["name"].startswith("Return to first reference")]
    assert len(back) == 3 and all("click" in node["actions"] for node in back)
    assert all(abs(node["bounds"]["width"] - 40 * zoom) <= 1 for node in back)
    for control in back:
        rail = control["bounds"]
        assert not any(node["role"] == "button" and node.get("bounds")
                       and abs(node["bounds"]["x"] - rail["x"]) <= 1
                       and abs(node["bounds"]["y"] - rail["y"]) <= 4 * zoom
                       for node in nodes), "duplicate button outside the source-owned footnote link"
    # Do not accept an ellipsis-shaped marker as a numbered note. Only check
    # visible markers; scrolled 200% evidence covers the offscreen note rows.
    visible_markers = 0
    for node in back:
        b = node["bounds"]
        if 54 <= b["y"] and b["y"] + b["height"] < image.height - 20:
            rows = ink_rows(image, b)
            assert len(rows) >= 6 * zoom, (node["name"], rows)
            visible_markers += 1
    if profile in ("wide", "narrow-notes", "200-notes"):
        assert visible_markers == 3
    # Native source containers retain every paragraph, with no fake labels in
    # the copyable text. Internal paragraph and inter-note gaps use separate roles.
    first, second, third = [node["bounds"] for node in notes]
    assert abs(second["y"] - first["y"] - first["height"] - 16 * zoom) <= 1
    assert abs(third["y"] - second["y"] - second["height"] - 16 * zoom) <= 1
    second_index = nodes.index(notes[1])
    paragraphs = [node["bounds"] for node in nodes if node.get("parent") == second_index and node["role"] == "paragraph"]
    assert len(paragraphs) == 2
    assert abs(paragraphs[1]["y"] - paragraphs[0]["y"] - paragraphs[0]["height"] - 12 * zoom) <= 1
    notes_heading = next(node["bounds"] for node in nodes if node["role"] == "heading" and node["name"] == "Notes")
    assert abs(first["y"] - notes_heading["y"] - notes_heading["height"] - 24 * zoom) <= 1
    for note in notes:
        note_index = nodes.index(note)
        for paragraph in (node for node in nodes
                          if node.get("parent") == note_index and node["role"] == "paragraph"):
            b = paragraph["bounds"]
            # Paragraph semantic boxes include the marker gutter. Validate the
            # text's inset from pixels, not by pretending that box is glyph ink.
            assert abs(b["x"] - note["bounds"]["x"]) <= 1, b
            line_count = round(b["height"] / (20 * zoom))
            assert abs(b["height"] - line_count * 20 * zoom) <= 1
            # Positive raster evidence in every visible note line, independent
            # of the layout's claimed boxes. Empty boxes cannot satisfy leading.
            for line in range(line_count):
                band = dict(b, x=b["x"] + 40 * zoom, width=b["width"] - 40 * zoom,
                            y=b["y"] + line * 20 * zoom, height=20 * zoom)
                if 54 <= band["y"] and band["y"] + band["height"] < image.height - 20:
                    assert len(ink_rows(image, band)) >= 7 * zoom, band
                    if line == 0:
                        assert ink_rows(image, dict(band, width=14 * zoom)), band
                        assert not ink_rows(image, dict(band, x=b["x"] + 36 * zoom, width=4 * zoom)), band
    return {"profile": profile, "visible_numbered_markers": visible_markers,
            "references": len(refs), "note_gap": 16 * zoom,
            "paragraph_gap": 12 * zoom, "heading_gap": 24 * zoom,
            "binary_sha256": source["binary_sha256"], "passes": True}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--prefix", default="footnotes-final")
    parser.add_argument("--profiles", nargs="+", default=["wide", "narrow", "narrow-notes", "200", "200-notes"])
    args = parser.parse_args()
    reports, binary_hash = [], None
    for profile in args.profiles:
        report = check(args.prefix, profile, binary_hash)
        binary_hash = report["binary_sha256"]
        reports.append(report)
    output = {"profiles": reports, "passes": True}
    (ROOT / f"{args.prefix}.pixels.json").write_text(json.dumps(output, indent=2) + "\n")
    print(json.dumps(output, indent=2))


if __name__ == "__main__":
    main()
