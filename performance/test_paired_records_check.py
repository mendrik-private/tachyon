import copy
import unittest

from paired_records_check import record_geometry


def records(paired):
    result = []
    for row in range(3):
        x = 700 if paired and row == 1 else 20
        y = 80 + (row // 2 if paired else row) * 300
        result.append([dict(name="" if (row, col) == (2, 5) else f"{row}/{col}",
                            bounds=dict(x=x, y=y + col * 40, width=650, height=24))
                       for col in range(6)])
    return result


class PairedRecordChecks(unittest.TestCase):
    def test_complete_source_ordered_pairs_and_stacks_pass(self):
        self.assertTrue(record_geometry(records(True)))
        self.assertTrue(record_geometry(records(False)))

    def test_field_and_following_band_overlap_fail(self):
        for row, col, key, value in ((0, 2, "y", 100), (1, 0, "x", 600),
                                     (2, 0, "y", 150), (1, 3, "width", 800)):
            broken = records(True)
            broken[row][col]["bounds"][key] = value
            self.assertFalse(record_geometry(broken))

    def test_missing_records_fields_or_fabricated_empty_values_fail(self):
        original = records(True)
        for broken in (original[:-1], [original[0][:-1], *original[1:]]):
            self.assertFalse(record_geometry(broken))
        broken = copy.deepcopy(original)
        broken[-1][-1]["name"] = "Unassigned"
        self.assertFalse(record_geometry(broken))


if __name__ == "__main__":
    unittest.main()
