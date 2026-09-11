"""Pure regression oracles for the native A07 matrix."""

from pathlib import Path
import subprocess
import sys
import unittest

sys.path.insert(0, str(Path(__file__).parent))
from a07_geometry_check import (
    EDIT_MARKER,
    evaluate,
    exact_single_insertion_within,
    semantic_single_insertion_within,
)


def sample_nodes(visible=True, scale=150):
    factor = scale / 120
    nodes = [
        {"role": "application", "name": "mineral-markdown", "parent": None},
        {"role": "frame", "name": "", "parent": 0},
        {"role": "button", "name": "Close window", "parent": 1,
         "bounds": {"x": 560, "y": 3, "width": round(28 * factor),
                    "height": round(28 * factor)}},
    ]
    if visible:
        landmark = len(nodes)
        nodes.append({"role": "landmark", "name": "Files and document outline", "parent": 1})
        files = len(nodes)
        nodes.append({"role": "tree", "name": "Markdown folders and files", "parent": landmark})
        nodes.append({"role": "tree item", "name": "Open fixture.md", "parent": files})
        outline = len(nodes)
        nodes.append({"role": "tree", "name": "Document outline", "parent": landmark})
        nodes.append({"role": "tree item", "name": "Heading", "parent": outline})
    editor = len(nodes)
    nodes.append({
        "role": "entry", "name": "Markdown document editor", "parent": 1,
        "interfaces": ["Accessible", "Component", "Text"],
        "character_count": 20, "selection_count": 0,
        "bounds": {"x": 15, "y": 67, "width": 570, "height": 908},
    })
    document = len(nodes)
    nodes.append({
        "role": "document frame", "name": "", "parent": editor,
        "bounds": {"x": 15, "y": 66, "width": 570, "height": 1065},
    })
    nodes.extend([
        {"role": "heading", "name": "Heading", "parent": document,
         "bounds": {"x": 15, "y": 76, "width": 570, "height": 62}},
        {"role": "paragraph", "name": "Text", "parent": document,
         "bounds": {"x": 15, "y": 150, "width": 500, "height": 30}},
    ])
    return nodes


def sample_reports():
    return [{
        "committed": True,
        "scope": "whole_document",
        "planner_failed": False,
        "unresolved_image_dimensions": 0,
        "text_zoom": 1.0,
        "canvas_width": 456.0,
        "viewport_height": 726.4,
        "anchor_displacement_at_commit_px": 0.8,
    }]


class A07GeometryTests(unittest.TestCase):
    def test_exact_edit_oracle_accepts_only_one_insertion_inside_unique_target(self):
        original = b"prefix\n" + EDIT_MARKER + b"\nsuffix"
        marker_start = original.index(EDIT_MARKER)
        for offset in (marker_start, marker_start + 7, marker_start + len(EDIT_MARKER)):
            edited = original[:offset] + b"x" + original[offset:]
            self.assertEqual(
                exact_single_insertion_within(original, edited, EDIT_MARKER), offset
            )

        rejected = (
            original,
            b"x" + original,
            original + b"x",
            original.replace(b"prefix", b"changed") + b"x",
            original.replace(EDIT_MARKER, EDIT_MARKER + b"xx"),
        )
        for candidate in rejected:
            with self.subTest(candidate=candidate):
                self.assertIsNone(
                    exact_single_insertion_within(original, candidate, EDIT_MARKER)
                )

    def test_semantic_edit_oracle_allows_only_lossless_punctuation_escaping(self):
        original = b"prefix\n" + EDIT_MARKER + b"\nsuffix"
        offset = original.index(b"ordinary") + 4
        inserted = original[:offset] + b"x" + original[offset:]
        escaped = inserted.replace(b".", b"\\.")
        self.assertEqual(
            semantic_single_insertion_within(original, escaped, EDIT_MARKER), offset
        )
        self.assertIsNone(
            semantic_single_insertion_within(
                original, escaped.replace(b"suffix", b"changed"), EDIT_MARKER
            )
        )

    def test_complete_visible_navigation_snapshot_passes(self):
        result = evaluate(sample_nodes(), sample_reports(), 480, 800, 150, True)
        self.assertTrue(result["passes"])
        self.assertEqual(result["document_headings"], result["outline_headings"])
        self.assertLessEqual(result["max_anchor_displacement_physical_px"], 1)

    def test_complete_collapsed_navigation_snapshot_passes(self):
        result = evaluate(sample_nodes(False), sample_reports(), 480, 800, 150, False)
        self.assertTrue(result["passes"])
        self.assertEqual(result["outline_headings"], [])

    def test_wrong_scale_clipping_outline_and_anchor_are_independent_failures(self):
        mutations = []
        wrong_scale = sample_nodes()
        wrong_scale[2]["bounds"]["width"] = 28
        mutations.append((wrong_scale, sample_reports(), "effective_fractional_scale"))
        clipped = sample_nodes()
        clipped[-1]["bounds"]["width"] = 600
        mutations.append((clipped, sample_reports(), "semantic_content_inside_document_horizontally"))
        wrong_outline = sample_nodes()
        next(node for node in wrong_outline if node.get("parent") == 6)["name"] = "Other"
        mutations.append((wrong_outline, sample_reports(), "outline_matches_document_headings"))
        anchor = sample_reports()
        anchor[0]["anchor_displacement_at_commit_px"] = 0.81
        mutations.append((sample_nodes(), anchor, "anchor_within_one_physical_pixel"))
        for nodes, reports, failed_check in mutations:
            with self.subTest(check=failed_check):
                result = evaluate(nodes, reports, 480, 800, 150, True)
                self.assertFalse(result["passes"])
                self.assertFalse(result["checks"][failed_check])

    def test_capture_option_rejects_incomplete_evidence_contract(self):
        script = Path(__file__).with_name("capture-layout.py")
        result = subprocess.run(
            [sys.executable, str(script), "--a07-geometry-check"],
            capture_output=True, text=True, timeout=5,
        )
        self.assertEqual(result.returncode, 2)
        self.assertIn("requires fixture41, active AT, layout trace", result.stderr)


if __name__ == "__main__":
    unittest.main()
