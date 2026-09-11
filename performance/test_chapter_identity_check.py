"""Unit tests for chapter barrier and duplicate-identity native evidence."""

import copy
import unittest

from chapter_identity_check import TARGET, TARGET_PARAGRAPH, evaluate_snapshot, evaluate_transition


def node(role, name, path, y, height=20):
    return {"role": role, "name": name, "path": path, "parent": None,
            "bounds": {"x": 0, "y": y, "width": 300, "height": height}}


def fixture_nodes(edited=False):
    target = TARGET_PARAGRAPH.replace(TARGET, TARGET + "x") if edited else TARGET_PARAGRAPH
    return [
        node("heading", "Chapter and duplicate identity", "/h0", 0),
        node("heading", "Repeated chapter", "/h1", 50),
        node("paragraph", "ALPHA CHAPTER START.", "/p1", 80),
        node("heading", "Repeated heading", "/h2", 120),
        node("paragraph", "The shared sentence remains deliberately identical.", "/p2", 150),
        node("heading", "Repeated details", "/h3", 200),
        node("paragraph", "ALPHA CHAPTER END.", "/p3", 230, 30),
        node("heading", "Repeated chapter", "/h4", 280),
        node("paragraph", "BETA CHAPTER START.", "/p4", 310),
        node("heading", "Repeated heading", "/h5", 350),
        node("paragraph", target, "/target", 380),
        node("heading", "Repeated details", "/h6", 430),
        node("paragraph", "BETA CHAPTER END.", "/p5", 460),
    ]


ORIGINAL = (
    b"# Chapter and duplicate identity\n\n## Repeated chapter\n\nALPHA CHAPTER END.\n\n"
    b"## Repeated chapter\n\nThe shared sentence remains deliberately identical. "
    b"beta edit target belongs only to the second chapter.\n"
)
EDITED = ORIGINAL.replace(b"beta edit target", b"beta edit targetx")


class ChapterIdentityCheckTests(unittest.TestCase):
    def test_legal_transition_passes(self):
        result = evaluate_transition(
            fixture_nodes(), fixture_nodes(True), fixture_nodes(), ORIGINAL, EDITED, ORIGINAL,
        )
        self.assertTrue(result["passes"])

    def test_chapter_interleaving_is_rejected(self):
        nodes = fixture_nodes()
        second = [node for node in nodes
                  if node["role"] == "heading" and node["name"] == "Repeated chapter"][1]
        second["bounds"]["y"] = 240
        with self.assertRaisesRegex(RuntimeError, "interleaves"):
            evaluate_snapshot(nodes)

    def test_duplicate_heading_path_is_rejected(self):
        nodes = fixture_nodes()
        duplicates = [node for node in nodes
                      if node["role"] == "heading" and node["name"] == "Repeated heading"]
        duplicates[1]["path"] = duplicates[0]["path"]
        with self.assertRaisesRegex(RuntimeError, "share an accessibility identity"):
            evaluate_snapshot(nodes)

    def test_edit_in_first_chapter_is_rejected(self):
        edited_source = EDITED.replace(b"ALPHA CHAPTER END.", b"ALPHA CHAPTER xEND.")
        result = evaluate_transition(
            fixture_nodes(), fixture_nodes(True), fixture_nodes(),
            ORIGINAL, edited_source, ORIGINAL,
        )
        self.assertFalse(result["checks"]["first_chapter_bytes_unchanged"])
        self.assertFalse(result["passes"])

    def test_heading_identity_change_is_rejected(self):
        edited_nodes = copy.deepcopy(fixture_nodes(True))
        edited_nodes[3]["path"] = "/changed"
        result = evaluate_transition(
            fixture_nodes(), edited_nodes, fixture_nodes(), ORIGINAL, EDITED, ORIGINAL,
        )
        self.assertFalse(result["checks"]["heading_identities_stable"])
        self.assertFalse(result["passes"])


if __name__ == "__main__":
    unittest.main()
