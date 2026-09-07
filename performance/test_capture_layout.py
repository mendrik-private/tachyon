"""Regression oracles for native layout evidence; no app or display required."""
import copy
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("capture_layout", Path(__file__).with_name("capture-layout.py"))
capture = importlib.util.module_from_spec(spec)
spec.loader.exec_module(capture)


class GeometryEvidenceTests(unittest.TestCase):
    def test_selection_keys_release_navigation_and_shift(self):
        for name, code in capture.SELECTION_KEY_CODES.items():
            self.assertEqual(capture.selection_key_events(name), [(code, 1), (code, 0)])
            self.assertEqual(capture.selection_key_events("shift-" + name),
                             [(42, 1), (code, 1), (code, 0), (42, 0)])
        for name in ("left", "right"):
            code = capture.SELECTION_KEY_CODES[name]
            self.assertEqual(capture.selection_key_events("ctrl-" + name),
                             [(29, 1), (code, 1), (code, 0), (29, 0)])
            self.assertEqual(capture.selection_key_events("ctrl-shift-" + name),
                             [(29, 1), (42, 1), (code, 1), (code, 0), (42, 0), (29, 0)])

    def report(self, published=False):
        return {
            "explicitly_requested": True, "committed": True, "sequence": 1,
            "anchor_displacement_at_commit_px": 0,
            "stages": [{"name": "geometry_and_rendered_extensions", "measurements": {
                "published_geometry_reuses": int(published),
                "geometry_requests": 0 if published else 1522,
                "geometry_cache_hits": 0 if published else 1522,
                "segments_laid_out": 0, "geometry_cache_evictions": 0,
                "wrap_requests": 0, "shaping_calls": 0,
            }}],
        }

    def test_both_reuse_paths_have_positive_evidence(self):
        for published in (False, True):
            capture.check_cached_geometry([self.report(published)], 1)

    def test_malformed_or_incomplete_reuse_is_rejected(self):
        for published in (False, True):
            good = self.report(published)
            for name, value in (
                ("published_geometry_reuses", 2), ("segments_laid_out", 1),
                ("geometry_cache_evictions", 1), ("wrap_requests", 1),
                ("shaping_calls", 1), ("geometry_cache_hits", 42),
            ):
                with self.subTest(published=published, field=name):
                    bad = copy.deepcopy(good)
                    bad["stages"][0]["measurements"][name] = value
                    with self.assertRaises(RuntimeError):
                        capture.check_cached_geometry([bad], 1)
            for name, value in (("committed", False), ("anchor_displacement_at_commit_px", 2)):
                bad = copy.deepcopy(good)
                bad[name] = value
                with self.assertRaises(RuntimeError):
                    capture.check_cached_geometry([bad], 1)
        empty_work = self.report(True)
        empty_work["stages"][0]["measurements"]["published_geometry_reuses"] = 0
        with self.assertRaises(RuntimeError):
            capture.check_cached_geometry([empty_work], 1)
        with self.assertRaises(RuntimeError):
            capture.check_cached_geometry([], 1)


if __name__ == "__main__":
    unittest.main()
