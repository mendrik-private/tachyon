import unittest

from boundary_layout_check import evaluate, peer_layout


def node(role, name, path, x, y, width=100, height=20):
    return {
        "role": role,
        "name": name,
        "path": path,
        "bounds": {"x": x, "y": y, "width": width, "height": height},
    }


def nodes(row):
    result = []
    for ordinal, name in enumerate(("Document core", "Document view", "Application")):
        result.append(node("heading", name, f"/peer/{ordinal}",
                           20 + ordinal * 130 if row else 20,
                           100 if row else 100 + ordinal * 70))
    return result


def sample(width, layout):
    return {
        "width": width,
        "layout": layout,
        "heading_paths": [(f"/peer/{i}", name) for i, name in enumerate(
            ("Document core", "Document view", "Application"))],
        "layout_controls": [],
    }


class BoundaryLayoutCheckTests(unittest.TestCase):
    def test_peer_layout_recognizes_row_and_stack(self):
        self.assertEqual(peer_layout(nodes(True)), "row")
        self.assertEqual(peer_layout(nodes(False)), "stack")

    def test_accepts_one_boundary_then_hysteretic_adjacent_widths(self):
        samples = [sample(900, "row"), sample(892, "row"), sample(884, "stack")]
        samples += [sample(width, "stack") for width in (892, 884, 892, 884, 892, 884, 892, 884)]
        result = evaluate(samples, sample(1100, "row"), b"source", b"source")
        self.assertTrue(result["passes"])

    def test_rejects_topology_thrashing(self):
        samples = [sample(900, "row"), sample(892, "stack")]
        samples += [sample(width, layout) for width, layout in (
            (900, "row"), (892, "stack"), (900, "row"), (892, "stack"),
            (900, "row"), (892, "stack"),
        )]
        result = evaluate(samples, sample(1100, "row"), b"source", b"source")
        self.assertFalse(result["checks"]["hysteresis_prevents_adjacent_thrashing"])
        self.assertFalse(result["passes"])

    def test_rejects_layout_control_or_source_mutation(self):
        samples = [sample(900, "row"), sample(892, "stack")]
        samples += [sample(width, "stack") for width in (900, 892, 900, 892, 900, 892)]
        samples[2]["layout_controls"] = [{"role": "push button", "name": "Layout: Grid"}]
        result = evaluate(samples, sample(1100, "row"), b"source", b"changed")
        self.assertFalse(result["checks"]["automatic_only_no_layout_controls"])
        self.assertFalse(result["checks"]["layout_did_not_change_source_or_undo"])
        self.assertFalse(result["passes"])

    def test_legal_previous_stack_may_survive_a_later_expansion(self):
        samples = [sample(900, "row"), sample(892, "stack")]
        samples += [sample(width, "stack") for width in (900, 892, 900, 892, 900, 892)]
        result = evaluate(samples, sample(1100, "stack"), b"source", b"source")
        self.assertTrue(result["checks"]["expanded_layout_remains_legal"])
        self.assertTrue(result["passes"])


if __name__ == "__main__":
    unittest.main()
