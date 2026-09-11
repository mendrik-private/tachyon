"""Independent fixture114 native map geometry, actions and label-pixel checks."""
import json
from pathlib import Path

from PIL import Image


def main():
    root = Path(__file__).resolve().parent / "layout-previews"
    records = []
    for mode, zoom in (("wide", 1), ("narrow", 1), ("200", 2)):
        prefix = root / f"map-previews-readable-{mode}"
        source = json.loads(prefix.with_suffix(".source.json").read_text())
        tree = json.loads(prefix.with_suffix(".active-atspi.json").read_text())
        assert source["passes"] and source["source_unchanged"]
        assert source["binary_sha256"] == tree["binary_sha256"]
        nodes = tree["nodes"]

        def one(role, starts):
            matches = [node for node in nodes if node["role"] == role
                       and node["name"].startswith(starts)]
            assert len(matches) == 1, (mode, role, starts)
            return matches[0]

        picture = one("image", "Map: Workshop")["bounds"]
        caption = one("caption", "Caption: Workshop")["bounds"]
        assert len([n for n in nodes if n["role"] == "button"
                    and n["name"].startswith("Open map:")]) == 2
        action = one("button", "Open map: Workshop")
        control = action["bounds"]
        assert "click" in action["actions"]
        body_height = picture["height"] - 48 * zoom
        assert abs(body_height - picture["width"] * 0.4) <= 2, "complete 1000×400 map aspect"
        assert control["x"] == picture["x"]
        assert control["height"] == 32 * zoom
        assert control["width"] <= picture["width"]
        assert abs(control["y"] - picture["y"] - body_height - 8 * zoom) <= 2
        assert abs(caption["y"] - picture["y"] - picture["height"] - 8 * zoom) <= 2
        assert caption["x"] == picture["x"]
        if mode == "wide":
            assert picture["x"] > 700, "use a readable source-related pair"
            assert picture["width"] >= 720
        else:
            title = one("heading", "Workshop site guide")["bounds"]
            assert picture["x"] == title["x"], "stack when narrow or large text"
            assert abs(picture["width"] - title["width"]) <= 2
        image = Image.open(prefix.with_suffix(".png")).convert("RGB")
        scale = picture["width"] / 1000
        # Each separately labelled source room must contain actual dark glyphs.
        # These boxes exclude borders and the dashed path; a blank rectangle
        # or a cropped map cannot satisfy the three independent label checks.
        ink = []
        for left in (52, 388, 722):
            x0 = round(picture["x"] + left * scale)
            x1 = round(x0 + 224 * scale)
            y0 = round(picture["y"] + 140 * scale)
            y1 = round(picture["y"] + 173 * scale)
            count = sum(max(image.getpixel((x, y))) < 160
                        for y in range(y0, y1) for x in range(x0, x1))
            assert count >= 20, (mode, left, count)
            ink.append(count)
        records.append(dict(mode=mode, source=source["original_sha256"],
                            binary=source["binary_sha256"], image=picture,
                            control=control, room_label_ink=ink))
    assert len({r["source"] for r in records}) == len({r["binary"] for r in records}) == 1
    print(json.dumps(dict(passes=True, checks=records)))


if __name__ == "__main__":
    main()
