import unittest

from layout_space_check import evaluate


def nodes(width=1632, review_x=1364):
    return [
        {"name": "Markdown document editor", "bounds": dict(x=260, y=54, width=width, height=926)},
        {"name": "The most useful documents", "bounds": dict(x=260, y=330, width=528, height=140)},
        {"name": "Our review practice", "bounds": dict(x=review_x, y=494, width=528, height=224)},
    ]


class LayoutSpaceChecks(unittest.TestCase):
    def test_full_width_three_column_capture(self):
        self.assertEqual(evaluate(nodes(), 1920, 3)["columns"], 3)

    def test_rejects_old_canvas_cap(self):
        with self.assertRaises(ValueError):
            evaluate(nodes(width=1280), 1920, 3)

    def test_rejects_missing_third_column(self):
        with self.assertRaises(ValueError):
            evaluate(nodes(review_x=812), 1920, 3)

    def test_rejects_empty_accessibility_tree(self):
        with self.assertRaises(ValueError):
            evaluate([], 1920, 3)

    def test_single_column_must_follow_source_order(self):
        compact = nodes(width=576, review_x=12)
        for node in compact:
            node["bounds"]["x"] = 12
        evaluate(compact, 600, 1)
        compact[2]["bounds"]["y"] = 100
        with self.assertRaises(ValueError):
            evaluate(compact, 600, 1)


if __name__ == "__main__":
    unittest.main()
