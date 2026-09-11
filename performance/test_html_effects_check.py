"""Unit tests for inert HTML request and disclosure evidence."""

import copy
import unittest

from html_effects_check import evaluate


def snapshot(expanded):
    states = ["expandable"] + (["expanded"] if expanded else [])
    nodes = [{
        "role": "button", "name": "Safe authored details", "path": "/details",
        "states": states, "actions": ["click"],
    }, {
        "role": "static", "name": "Unsupported or preserved HTML", "path": "/fallback",
        "states": [], "actions": [],
    }, {
        "role": "paragraph",
        "name": "Following canonical paragraph remains visible and editable.",
        "path": "/following", "states": [], "actions": [],
    }]
    if expanded:
        nodes.append({
            "role": "paragraph", "name": "Safe disclosure body marker.",
            "path": "/body", "states": [], "actions": [],
        })
    return nodes


class HtmlEffectsCheckTests(unittest.TestCase):
    def test_zero_effect_transition_passes(self):
        result = evaluate(snapshot(False), snapshot(True), snapshot(False),
                          [[], [], []], b"source", b"source")
        self.assertTrue(result["passes"])

    def test_any_request_is_rejected(self):
        result = evaluate(snapshot(False), snapshot(True), snapshot(False),
                          [[], [("GET", "/image.png")], []], b"source", b"source")
        self.assertFalse(result["checks"]["zero_open_requests"])
        self.assertFalse(result["passes"])

    def test_identity_change_is_rejected(self):
        opened = copy.deepcopy(snapshot(True))
        opened[0]["path"] = "/replacement"
        result = evaluate(snapshot(False), opened, snapshot(False),
                          [[], [], []], b"source", b"source")
        self.assertFalse(result["checks"]["disclosure_identity_stable"])

    def test_unverified_monitor_is_rejected(self):
        result = evaluate(snapshot(False), snapshot(True), snapshot(False),
                          [[], [], []], b"source", b"source", False)
        self.assertFalse(result["checks"]["monitor_control_verified"])
        self.assertFalse(result["passes"])

    def test_missing_conservative_fallback_is_rejected(self):
        initial = snapshot(False)
        initial.pop(1)
        with self.assertRaisesRegex(RuntimeError, "conservative visible fallback"):
            evaluate(initial, snapshot(True), snapshot(False), [[], [], []], b"source", b"source")


if __name__ == "__main__":
    unittest.main()
