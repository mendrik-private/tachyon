"""Pixel oracle for fixture 108 at 1280x1700 and 200% document zoom.

This deliberately checks native painted content, not only semantic geometry.
The first executable example must stack at this measure and actually paint.
"""

import argparse
import json

from PIL import Image


def evaluate(path):
    image = Image.open(path).convert("RGB")
    if image.size != (1280, 1700):
        raise ValueError("Use fixture 108 at 1280x1700, zoom-steps 10")
    pixels = image.load()
    panel_color = (0x20, 0x2A, 0x2B)
    rows = []
    for y in range(300, 1650):
        xs = [x for x in range(230, 1270) if pixels[x, y] == panel_color]
        if len(xs) >= 200:
            rows.append((y, min(xs), max(xs)))
        elif rows:
            break
    if not rows:
        return {"passes": False, "reason": "executable panel missing"}
    top = rows[0][0]
    left = min(row[1] for row in rows)
    right = max(row[2] for row in rows)
    bottom = top
    misses = 0
    for y in range(top, 1650):
        if pixels[left + 8, y] == panel_color:
            bottom = y
            misses = 0
        else:
            misses += 1
            if misses > 4:
                break
    # Pale syntax/header glyphs; neither the dark panel nor selected green fill
    # contributes. Keep away from surrounding paper and rounded corners.
    ink_pixels = sum(
        min(pixels[x, y]) > 100 and max(pixels[x, y]) > 160
        for y in range(top + 90, bottom - 12)
        for x in range(left + 12, right - 12)
    )
    checks = {
        "readable_stacked_width": right - left >= 800,
        "code_content_painted": ink_pixels >= 200,
    }
    return {
        "panel_bounds": [left, top, right, bottom],
        "ink_pixels": ink_pixels,
        "checks": checks,
        "passes": all(checks.values()),
    }


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("image")
    args = parser.parse_args()
    result = evaluate(args.image)
    print(json.dumps(result))
    raise SystemExit(0 if result["passes"] else 1)
