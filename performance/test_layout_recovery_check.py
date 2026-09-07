import unittest
from layout_recovery_check import LABELS, is_grid, is_stack


def nodes(columns):
    return [dict(name=label, role='list item', bounds=dict(
        x=(i % columns) * 200, y=(i // columns) * 40, width=180, height=30))
        for i, label in enumerate(LABELS)]


class RecoveryGeometryTests(unittest.TestCase):
    def test_grid_and_stack_are_distinct(self):
        self.assertTrue(is_grid(nodes(3)))
        self.assertFalse(is_stack(nodes(3)))
        self.assertTrue(is_stack(nodes(1)))
        self.assertFalse(is_grid(nodes(1)))
        broken_second_row = nodes(3)
        broken_second_row[4]['bounds']['y'] = 0
        self.assertFalse(is_grid(broken_second_row))

    def test_empty_overlapping_and_reordered_geometry_cannot_pass(self):
        for column_count in (1, 3):
            empty = nodes(column_count)
            for node in empty:
                node['bounds'].update(width=0, height=0)
            self.assertFalse(is_stack(empty))
            self.assertFalse(is_grid(empty))
        overlap = nodes(1)
        overlap[1]['bounds']['y'] = overlap[0]['bounds']['y']
        self.assertFalse(is_stack(overlap))
        reordered = nodes(1)
        reordered[0]['bounds'], reordered[1]['bounds'] = reordered[1]['bounds'], reordered[0]['bounds']
        self.assertFalse(is_stack(reordered))

    def test_missing_or_duplicate_semantics_fail(self):
        for invalid in (nodes(1)[:-1], nodes(1) + [nodes(1)[0]]):
            with self.assertRaises(RuntimeError): is_stack(invalid)


if __name__ == '__main__':
    unittest.main()
