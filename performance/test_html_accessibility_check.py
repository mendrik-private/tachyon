import copy
import unittest

from html_accessibility_check import html_state


class HtmlAccessibilityCheckTests(unittest.TestCase):
    def test_gate_rejects_leaks_missing_content_duplicates_and_missing_actions(self):
        text = ("Visible HTML body marker with strong emphasis and ordinary rich text.\n"
                "Unicode marker: 東京 café 🦀.\nClosed HTML disclosure\n"
                "Open HTML disclosure\nOpen HTML body marker.")
        nodes = [dict(role="paragraph", name=text),
                 dict(role="button", name="Closed HTML disclosure", actions=["click"], states=["expandable"]),
                 dict(role="paragraph", name="<custom-widget>Unknown original HTML source marker.</custom-widget>"),
                 dict(role="paragraph", name="Following canonical paragraph marker."),
                 dict(role="button", name="Open HTML disclosure", actions=["click"], states=["expandable", "expanded"])]
        self.assertTrue(html_state(nodes, False))
        self.assertFalse(html_state(nodes, True))
        for bad in [text + "\nDisplay-none body marker.", text + "\nClosed HTML body marker.",
                    text.replace("東京", ""), text + "\nOpen HTML body marker."]:
            with self.subTest(bad=bad):
                corrupted = copy.deepcopy(nodes)
                corrupted[0]["name"] = bad
                self.assertFalse(html_state(corrupted, False))
        corrupted = copy.deepcopy(nodes)
        corrupted[1]["actions"] = []
        self.assertFalse(html_state(corrupted, False))
        self.assertFalse(html_state(nodes + [nodes[0]], False))
        for states in [[], ["expanded"], ["expandable", "expanded"]]:
            corrupted = copy.deepcopy(nodes)
            corrupted[1]["states"] = states
            self.assertFalse(html_state(corrupted, False))
        opened = copy.deepcopy(nodes)
        opened[0]["name"] = text.replace("Open HTML disclosure", "Closed HTML body marker.\nNested HTML disclosure\nNested hidden body marker.\nOpen HTML disclosure")
        opened[1]["states"].append("expanded")
        opened.append(dict(role="button", name="Nested HTML disclosure", actions=["click"], states=["expandable", "expanded"]))
        self.assertTrue(html_state(opened, True))
        self.assertFalse(html_state(opened, False))
        for index in [-1, -2]:
            corrupted = copy.deepcopy(opened)
            corrupted[index]["states"] = ["expandable"]
            self.assertFalse(html_state(corrupted, True))
        self.assertFalse(html_state(opened[:-1], True))
        corrupted = copy.deepcopy(opened)
        corrupted[1]["states"] = ["expandable"]
        self.assertFalse(html_state(corrupted, True))


if __name__ == "__main__":
    unittest.main()
