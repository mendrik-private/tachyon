import unittest

from font_reflow_check import evaluate


HEADINGS = (
    "Typography, direction, and bounded overflow",
    "1. Measured body copy",
    "2. Reading anchor",
    "Stable identities",
    "3. Mixed direction and scoped overflow",
    "4. Closing section",
)


def snapshot(offsets, anchor_screen_y=80):
    anchor_offset = offsets["2. Reading anchor"]
    nodes = [{
        "role": "entry", "name": "Markdown document editor", "path": "/editor",
        "bounds": {"x": 0, "y": 64, "width": 900, "height": 700},
    }]
    for index, name in enumerate(HEADINGS):
        nodes.append({
            "role": "heading", "name": name, "path": f"/h/{index}",
            "bounds": {
                "x": 20, "y": anchor_screen_y + offsets[name] - anchor_offset,
                "width": 200, "height": 30,
            },
        })
    return {"nodes": nodes}


def trace(sequence, shaping=4, committed=True):
    return {
        "sequence": sequence, "geometry_generation": sequence, "committed": committed,
        "stages": [{"measurements": {"shaping_calls": shaping, "wrap_cache_misses": shaping}}],
    }


class FontReflowCheckTests(unittest.TestCase):
    def setUp(self):
        self.base = {name: index * 100 for index, name in enumerate(HEADINGS)}
        self.alternate = dict(self.base)
        self.alternate["1. Measured body copy"] -= 18

    def test_requires_measured_reflow_anchor_identity_and_exact_restore(self):
        source = b"# fixture\n"
        result = evaluate(
            snapshot(self.base), snapshot(self.alternate), snapshot(self.alternate),
            snapshot(self.base), trace(2), trace(3), source, source,
        )
        self.assertTrue(result["passes"])

    def test_rejects_cached_geometry_reused_across_font_change(self):
        source = b"# fixture\n"
        result = evaluate(
            snapshot(self.base), snapshot(self.base), snapshot(self.base),
            snapshot(self.base), trace(2, shaping=0), trace(3), source, source,
        )
        self.assertFalse(result["checks"]["font_change_measured_reflow"])
        self.assertFalse(result["visible_geometry_changed"])

    def test_rejects_anchor_jump_or_unstable_repeat(self):
        source = b"# fixture\n"
        repeated = dict(self.alternate)
        repeated["4. Closing section"] += 8
        result = evaluate(
            snapshot(self.base), snapshot(self.alternate), snapshot(repeated),
            snapshot(self.base, anchor_screen_y=90), trace(2), trace(3), source, source,
        )
        self.assertFalse(result["checks"]["reading_anchor_stable"])
        self.assertFalse(result["checks"]["same_font_is_stable"])

    def test_rejects_missing_shaping_or_source_change(self):
        result = evaluate(
            snapshot(self.base), snapshot(self.alternate), snapshot(self.alternate),
            snapshot(self.base), trace(2, shaping=0), trace(3), b"before", b"after",
        )
        self.assertFalse(result["checks"]["alternate_font_reshaped"])
        self.assertFalse(result["checks"]["source_unchanged"])
        self.assertFalse(result["passes"])


if __name__ == "__main__":
    unittest.main()
