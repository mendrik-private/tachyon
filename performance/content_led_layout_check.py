"""Independent native geometry/source checks for content-first reference layout."""
import json
from pathlib import Path

from PIL import Image


def main():
    root = Path(__file__).resolve().parent / "layout-previews"

    def record(mode):
        prefix = root / f"content-led-{mode}"
        source = json.loads(prefix.with_suffix(".source.json").read_text())
        tree = json.loads(prefix.with_suffix(".active-atspi.json").read_text())
        assert source["passes"] and source["source_unchanged"]
        assert source["binary_sha256"] == tree["binary_sha256"]
        return prefix, source, tree["nodes"]

    def one(nodes, role, starts):
        matches = [n for n in nodes if n["role"] == role and n["name"].startswith(starts)]
        assert len(matches) == 1, (role, starts, len(matches))
        return matches[0]["bounds"]

    before, old_source, old = record("before")
    prefix, source, nodes = record("fitted-wide")
    assert old_source["original_sha256"] == source["original_sha256"]
    table = one(nodes, "table", "Keyboard commands table")
    old_table = one(old, "table", "Keyboard commands table")
    prose = one(nodes, "paragraph", "The divider")
    heading = one(nodes, "heading", "Keyboard commands")
    following = one(nodes, "heading", "After the reference")
    old_following = one(old, "heading", "After the reference")
    assert table == old_table, "do not shrink the table to claim a space saving"
    assert table["x"] == heading["x"] == following["x"]
    assert abs(prose["x"] - table["x"] - table["width"] - 24) <= 1
    # AT-SPI table bounds start at its first content row, ten pixels below
    # the actual outer border; the prose starts at the shared component top.
    assert table["y"] - prose["y"] == 10
    assert prose["width"] >= 400
    assert (prose["x"] + prose["width"] - heading["x"]) / heading["width"] >= .78
    saved = old_following["y"] - following["y"]
    assert saved >= 300, saved
    image = Image.open(prefix.with_suffix(".png")).convert("RGB")
    # Actual glyph pixels must occupy the explanation's newly used column.
    ink = sum(max(image.getpixel((x, y))) < 160
              for y in range(prose["y"], prose["y"] + prose["height"])
              for x in range(prose["x"], prose["x"] + prose["width"]))
    assert ink > 300
    for mode in ("narrow", "short", "dark200"):
        _, other_source, other = record(mode)
        assert other_source["binary_sha256"] == source["binary_sha256"]
        assert other_source["original_sha256"] == source["original_sha256"]
        body = one(other, "paragraph", "The divider")
        grid = one(other, "table", "Keyboard commands table")
        assert body["x"] == grid["x"], mode
        assert body["y"] > grid["y"] + grid["height"], mode
    edit = json.loads((root / "content-led-prose-edit.edit.json").read_text())
    assert edit["binary_sha256"] == source["binary_sha256"]
    assert all(edit[key] for key in ("native_type_autosaved", "expected_edited_source_exact",
                                    "undo_restores_exact_bytes"))
    copy = json.loads((root / "content-led-copy.copy.json").read_text())
    assert copy["binary_sha256"] == source["binary_sha256"] and copy["passes"]
    print(json.dumps(dict(passes=True, saved_vertical_pixels=saved,
                          table=table, prose=prose, explanation_ink_pixels=ink,
                          binary_sha256=source["binary_sha256"])))


if __name__ == "__main__":
    main()
