import copy
import unittest

from recorded_bugs_check import table_rows


class InsertedTableGeometryTests(unittest.TestCase):
    def setUp(self):
        self.nodes = [dict(parent=None, role="table", name="Reference")]
        for row in range(3):
            parent = len(self.nodes)
            self.nodes.append(dict(parent=0, role="table row", name=f"Row {row}"))
            for column in range(2):
                self.nodes.append(dict(parent=parent, role="table cell", name="",
                                       bounds=dict(x=720 + column * 100, y=300 + row * 40,
                                                   width=100, height=24)))

    def test_aligned_inserted_empty_cells_are_not_skipped(self):
        self.assertEqual([len(row) for row in table_rows(self.nodes, "Reference")], [2, 2, 2])

    def test_origin_snap_is_detected(self):
        nodes = copy.deepcopy(self.nodes)
        nodes[5]["bounds"]["x"] = 0
        with self.assertRaisesRegex(RuntimeError, "column geometry"):
            table_rows(nodes, "Reference")

    def test_row_or_width_mismatch_is_detected(self):
        for axis in ["y", "width"]:
            nodes = copy.deepcopy(self.nodes)
            nodes[5]["bounds"][axis] += 10
            with self.assertRaises(RuntimeError):
                table_rows(nodes, "Reference")

    def test_missing_inserted_cell_is_detected(self):
        with self.assertRaisesRegex(RuntimeError, "ragged table"):
            table_rows(self.nodes[:-1], "Reference")


if __name__ == "__main__":
    unittest.main()
