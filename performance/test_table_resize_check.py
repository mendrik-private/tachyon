import unittest

from table_resize_check import check_width_edit


class WidthOracleTests(unittest.TestCase):
    original = b'Lead\n<!-- mineral-table:v1 {"border":"LogicalPixel","widths":[160,320]} -->\n| A | B |\nTail'

    def test_accepts_only_target_width_change(self):
        check_width_edit(self.original, self.original.replace(b"[160,320]", b"[200.0,320.0]"), 200)

    def test_fixture_canonical_form_is_exact_and_does_not_normalize_neighbor_tables(self):
        from pathlib import Path
        original = (Path(__file__).parent / "layout-fixtures/82-authored-table-widths.md").read_bytes()
        edited = original.replace(b"[160,320]", b"[200,320]", 1).replace(
            b"supporting resources together. |", b"supporting resources together\\. |", 1
        ).replace(b"replacing an existing document. |", b"replacing an existing document\\. |", 1)
        check_width_edit(original, edited, 200)
        with self.assertRaises(RuntimeError):
            check_width_edit(original, edited.replace(
                b"supporting resources together. |", b"supporting resources together\\. |"), 200)

    def test_rejects_wrong_width_other_columns_and_content_changes(self):
        for changed in [self.original,
                        self.original.replace(b"[160,320]", b"[200,321]"),
                        self.original.replace(b"[160,320]", b"[200,320]").replace(b"Tail", b"Lost")]:
            with self.assertRaises(RuntimeError):
                check_width_edit(self.original, changed, 200)


if __name__ == "__main__":
    unittest.main()
