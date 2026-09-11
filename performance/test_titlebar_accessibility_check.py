"""Negative controls for the native title-bar acceptance oracle."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location("titlebar", Path(__file__).with_name("titlebar_accessibility_check.py"))
titlebar = importlib.util.module_from_spec(spec)
spec.loader.exec_module(titlebar)


class TitlebarEvidenceTests(unittest.TestCase):
    def test_real_names_are_unique_and_zoom_controls_have_actions(self):
        nodes = [dict(role="button", name=name, actions=["click"])
                 for name in ("Application menu", "Zoom out", "Reset zoom", "Zoom in")]
        self.assertTrue(titlebar.check(nodes)["passes"])
        self.assertFalse(titlebar.check(nodes[:-1])["passes"])
        self.assertFalse(titlebar.check(nodes + [nodes[-1]])["passes"])
        self.assertFalse(titlebar.check(nodes[:-1] + [dict(nodes[-1], actions=[])])["passes"])
        self.assertFalse(titlebar.check(nodes[:-1] + [dict(nodes[-1], role="label")])["passes"])
        self.assertFalse(titlebar.check([dict(node, name="") for node in nodes])["passes"])


if __name__ == "__main__":
    unittest.main()
