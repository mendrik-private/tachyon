"""Unit tests for the editorial AT-SPI structural oracle."""

import copy
import unittest

from editorial_accessibility_check import HEADINGS, PEER_CONTENT, TABLES, TASKS, check


def fixture_nodes():
    nodes = []

    def add(role, name, parent=None, bounds=None, states=None):
        nodes.append({
            "role": role,
            "name": name,
            "parent": parent,
            "bounds": bounds or {"x": 0, "y": len(nodes) * 10, "width": 100, "height": 10},
            "states": states or [],
        })
        return len(nodes) - 1

    add("entry", "Markdown document editor", states=["focusable", "focused"])
    peer_x = {"Document core": 0, "Document view": 110, "Application": 220}
    for heading in HEADINGS:
        bounds = ({"x": peer_x[heading], "y": 400, "width": 100, "height": 20}
                  if heading in peer_x else None)
        add("heading", heading, bounds=bounds)
        if heading == "Document core":
            add("paragraph", PEER_CONTENT[1][1])
        elif heading == "Document view":
            add("paragraph", PEER_CONTENT[3][1])
        elif heading == "Application":
            add("paragraph", PEER_CONTENT[5][1])
        elif heading == "4. Explanation and example":
            add("paragraph", PEER_CONTENT[7][1])
            add("static", PEER_CONTENT[8][1], states=["selectable"])
        if heading == "1. Product contract":
            list_index = add("list", "List")
            for task in TASKS:
                add("check box", task, parent=list_index)
        for table_name, row_count, column_count in TABLES:
            if heading != table_name.removesuffix(" table"):
                continue
            table_index = add("table", table_name)
            for row in range(row_count):
                row_index = add("table row", f"Row {row + 1}", parent=table_index)
                role = "column header" if row == 0 else "table cell"
                for column in range(column_count):
                    add(role, f"{table_name}-{row}-{column}", parent=row_index)
    return nodes


class EditorialAccessibilityTests(unittest.TestCase):
    def test_complete_tree_passes(self):
        self.assertTrue(check(fixture_nodes())["passes"])

    def test_reordered_peer_content_fails(self):
        nodes = fixture_nodes()
        first = next(node for node in nodes if node["name"] == PEER_CONTENT[1][1])
        second = next(node for node in nodes if node["name"] == PEER_CONTENT[3][1])
        first["name"], second["name"] = second["name"], first["name"]
        with self.assertRaisesRegex(RuntimeError, "reordered"):
            check(nodes)

    def test_broken_visual_row_fails(self):
        nodes = copy.deepcopy(fixture_nodes())
        next(node for node in nodes if node["name"] == "Document view")["bounds"]["y"] += 30
        with self.assertRaisesRegex(RuntimeError, "visual row"):
            check(nodes)

    def test_missing_table_cell_fails(self):
        nodes = fixture_nodes()
        cell = next(node for node in nodes if node["name"] == "Palette table-2-1")
        nodes.remove(cell)
        with self.assertRaisesRegex(RuntimeError, "cell hierarchy"):
            check(nodes)


if __name__ == "__main__":
    unittest.main()
