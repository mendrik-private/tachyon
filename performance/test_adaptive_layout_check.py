"""Unit tests for the native adaptive-layout geometry oracle."""

import copy
import unittest

from adaptive_layout_check import (
    LIST_GROUPS, ROW_CODE, ROW_HEADINGS, ROW_PEERS, TABLES,
    evaluate_lists, evaluate_rows, evaluate_tables,
)


def node(role, name, x, y, width=100, height=20, parent=None):
    return {"role": role, "name": name, "parent": parent,
            "bounds": {"x": x, "y": y, "width": width, "height": height}}


def list_nodes(columns=3):
    nodes = []
    y = 0
    for group, names in LIST_GROUPS.items():
        list_index = len(nodes)
        nodes.append(node("list", "List", 0, y, 400, 200))
        stack = group in {"uneven", "references", "ten"} or columns == 1
        for index, name in enumerate(names):
            column = 0 if stack else index % columns
            row = index if stack else index // columns
            item_index = len(nodes)
            nodes.append(node("list item", name, column * 120, y + row * 30,
                              parent=list_index))
            nodes.append(node("paragraph", name, column * 120, y + row * 30,
                              parent=item_index))
        y += (len(names) if stack else (len(names) + columns - 1) // columns) * 30 + 30
    for index, name in enumerate((
        "Open the workspace.", "Keep the original source file available.",
        "Verify the document title.", "Run the verification command.",
        "cargo test --locked -p document-view\n", "Review the outcome.",
    )):
        nodes.append(node("static" if name.endswith("\n") else "paragraph", name, 0, y + index * 30))
    return nodes


def row_nodes(wide=True):
    nodes = []
    ordinary_y = {
        "Measured document rows": 0,
        "Starting a worker": 50,
        "Review checks": 250,
        "Another example": 620,
        "A separate section": 820,
    }
    for name in ROW_HEADINGS:
        if name in ROW_PEERS:
            position = ROW_PEERS.index(name)
            x, heading_y = ((position * 130, 300) if wide else (0, 300 + position * 100))
        else:
            x, heading_y = 0, ordinary_y[name]
        nodes.append(node("heading", name, x, heading_y, 120))
        if name in ROW_CODE:
            paragraph, code, following = ROW_CODE[name]
            base = heading_y + 30
            nodes.append(node("paragraph", paragraph, 0, base, 180, 50))
            paired = wide and name == "Another example"
            nodes.append(node("static", code, 200 if paired else 0,
                              base if paired else base + 60, 180, 50))
            if following:
                nodes.append(node("paragraph", following, 0, base + 120, 380, 30))
    return nodes


def table_nodes(wide=True):
    nodes = []
    for table_ordinal, (name, expected) in enumerate(TABLES.items()):
        if table_ordinal == 0:
            x, y, width = 0, 0, 180
        elif table_ordinal == 1:
            x, y, width = (200, 0, 360) if wide else (0, 160, 360)
        else:
            x, y, width = 0, 320 + table_ordinal * 40, 360
        table_index = len(nodes)
        nodes.append(node("table", name, x, y, width, 120))
        for row_ordinal, names in enumerate(expected):
            row_index = len(nodes)
            nodes.append(node("table row", f"row {row_ordinal}", x, y + row_ordinal * 25,
                              width, 25, table_index))
            role = "column header" if row_ordinal == 0 else "table cell"
            for column, cell_name in enumerate(names):
                nodes.append(node(role, cell_name, x + column * 50,
                                  y + row_ordinal * 25, 50, 25, row_index))
    return nodes


class AdaptiveLayoutOracleTests(unittest.TestCase):
    def test_wide_legal_layouts_pass(self):
        self.assertTrue(evaluate_lists(list_nodes(3), "wide")["passes"])
        self.assertTrue(evaluate_rows(row_nodes(True), "wide")["passes"])
        self.assertTrue(evaluate_tables(table_nodes(True), "wide")["passes"])

    def test_narrow_stacks_pass(self):
        self.assertTrue(evaluate_lists(list_nodes(1), "narrow")["passes"])
        self.assertTrue(evaluate_rows(row_nodes(False), "narrow")["passes"])
        self.assertTrue(evaluate_tables(table_nodes(False), "narrow")["passes"])

    def test_reordered_list_is_rejected(self):
        nodes = list_nodes(3)
        first = next(node for node in nodes if node["role"] == "paragraph" and node["name"] == "North")
        second = next(node for node in nodes if node["role"] == "paragraph" and node["name"] == "South")
        first["name"], second["name"] = second["name"], first["name"]
        with self.assertRaisesRegex(RuntimeError, "source order"):
            evaluate_lists(nodes, "wide")

    def test_list_item_without_list_parent_is_rejected(self):
        nodes = list_nodes(3)
        paragraph = next(node for node in nodes
                         if node["role"] == "paragraph" and node["name"] == "North")
        nodes[paragraph["parent"]]["parent"] = None
        with self.assertRaisesRegex(RuntimeError, "lost its list parent"):
            evaluate_lists(nodes, "wide")

    def test_overlapping_peer_row_is_rejected(self):
        nodes = row_nodes(True)
        second = next(node for node in nodes if node["name"] == "Readable measures")
        second["bounds"]["x"] = 100
        with self.assertRaisesRegex(RuntimeError, "legal row"):
            evaluate_rows(nodes, "wide")

    def test_missing_table_cell_is_rejected(self):
        nodes = table_nodes(True)
        nodes.remove(next(node for node in nodes if node["role"] == "table cell"
                          and node["name"] == "30 s"))
        with self.assertRaisesRegex(RuntimeError, "exact cells"):
            evaluate_tables(nodes, "wide")


if __name__ == "__main__":
    unittest.main()
