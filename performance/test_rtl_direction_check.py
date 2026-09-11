import unittest

from rtl_direction_check import evaluate


def state(offset, x):
    return {
        "selection_start": offset,
        "selection_end": offset,
        "caret_bounds": [x, 100, x + 1.5, 124],
    }


class RtlDirectionCheckTests(unittest.TestCase):
    def test_accepts_reversed_hits_and_visual_arrow_motion(self):
        source = b"rtl"
        result = evaluate(
            state(10, 300), state(80, 100), state(44, 200),
            state(46, 188), state(42, 212),
            {"x": 100, "y": 100, "width": 200, "height": 24},
            source, source,
        )
        self.assertTrue(result["passes"])

    def test_rejects_logical_arrow_motion_or_source_change(self):
        result = evaluate(
            state(10, 300), state(80, 100), state(44, 200),
            state(42, 212), state(46, 188),
            {"x": 100, "y": 100, "width": 200, "height": 24},
            b"before", b"after",
        )
        self.assertFalse(result["checks"]["left_arrow_moves_visually_left"])
        self.assertFalse(result["checks"]["right_arrow_moves_visually_right"])
        self.assertFalse(result["checks"]["source_unchanged"])
        self.assertFalse(result["passes"])


if __name__ == "__main__":
    unittest.main()
