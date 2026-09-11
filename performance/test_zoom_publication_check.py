"""Verify that the pixel oracle rejects both narrow tracks and absent glyphs."""

import unittest
from unittest.mock import patch

from zoom_publication_check import evaluate


class Pixels:
    def __init__(self, right, body):
        self.right, self.body = right, body

    def __getitem__(self, point):
        x, y = point
        if 254 <= x <= self.right and 624 <= y <= 959:
            if (280 <= x <= 330 and 644 <= y <= 656) or (
                self.body and 300 <= x <= 340 and 735 <= y <= 760
            ):
                return (200, 210, 220)
            return (32, 42, 43)
        return (250, 249, 246)


class NativeImage:
    size = (1280, 1700)

    def __init__(self, right, body):
        self.pixels = Pixels(right, body)

    def convert(self, mode):
        return self

    def load(self):
        return self.pixels


class ZoomPublicationTests(unittest.TestCase):
    def test_only_readable_stacked_code_passes(self):
        for right, body, expected in [(1257, True, True), (570, True, False),
                                      (1257, False, False)]:
            with self.subTest(right=right, body=body):
                with patch("zoom_publication_check.Image.open", return_value=NativeImage(right, body)):
                    result = evaluate("unused.png")
                self.assertEqual(result["passes"], expected)
                self.assertEqual(result["checks"]["code_content_painted"], body)


if __name__ == "__main__":
    unittest.main()
