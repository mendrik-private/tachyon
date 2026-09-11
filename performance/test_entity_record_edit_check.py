"""The native edit oracle permits movement, not changed or duplicate glyphs."""
import unittest
from PIL import Image, ImageDraw
from entity_record_edit_check import matching_title_rows


class EntityRecordEditEvidenceTests(unittest.TestCase):
    def specimen(self):
        template = Image.new('RGB', (80, 24), (250, 249, 246))
        draw = ImageDraw.Draw(template)
        draw.rectangle((2, 5, 5, 20), fill=(54, 66, 62))
        draw.rectangle((2, 5, 45, 8), fill=(54, 66, 62))
        edited = Image.new('RGB', (600, 400), (250, 249, 246))
        edited.paste(template, (52, 120))
        return template, edited

    def test_row_growth_keeps_identical_title(self):
        template, edited = self.specimen()
        self.assertEqual(matching_title_rows(template, edited, 52), [120])

    def test_changed_weight_fails_even_if_content_location_is_unchanged(self):
        template, edited = self.specimen()
        ImageDraw.Draw(edited).rectangle((57, 125, 57, 140), fill=(250, 249, 246))
        self.assertEqual(matching_title_rows(template, edited, 52), [])

    def test_duplicate_title_is_detected(self):
        template, edited = self.specimen()
        edited.paste(template, (52, 200))
        self.assertEqual(matching_title_rows(template, edited, 52), [120, 200])

    def test_blank_reference_cannot_pass(self):
        template, edited = self.specimen()
        ImageDraw.Draw(template).rectangle((0, 0, 79, 23), fill=(250, 249, 246))
        with self.assertRaises(AssertionError):
            matching_title_rows(template, edited, 52)


if __name__ == '__main__':
    unittest.main()
