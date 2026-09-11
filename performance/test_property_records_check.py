"""Adversarial tests of the property-record evidence oracle, without a display."""
import copy
import unittest

from PIL import Image, ImageDraw

from property_records_check import LABELS, VALUES, PAPER, RULE, evaluate


def specimen(zoom=1):
    source = dict(passes=True, source_unchanged=True, binary_sha256="build",
                  original_sha256="fixture", current_sha256="fixture",
                  width=600*zoom, height=1300*zoom, zoom_steps=(zoom-1)*10)
    tree = {key: source[key] for key in ["binary_sha256", "width", "height", "zoom_steps"]}
    nodes = tree["nodes"] = []
    image = Image.new("RGB", (source["width"], source["height"]), PAPER)
    draw = ImageDraw.Draw(image)

    def node(parent, role, name, x=0, y=0, width=0, height=0):
        nodes.append(dict(parent=parent, role=role, name=name,
                          bounds=dict(x=x*zoom, y=y*zoom, width=width*zoom, height=height*zoom)))
        return len(nodes)-1

    table = node(None, "table", "Delivery settings table")
    header = node(table, "table row", "Row 1")
    node(header, "table cell", "Property", 28, 50, 125, 40)
    node(header, "table cell", "Description", 153, 50, 419, 40)
    top = 120
    for index, (label, value) in enumerate(zip(LABELS, VALUES)):
        row = node(table, "table row", f"Row {index+2}")
        key_y, value_y = top+24, top+24+21+8
        value_height = 42 if index < 3 else 21
        bottom = value_y+value_height+24
        node(row, "table cell", label, 28, key_y, 544, 21)
        node(row, "table cell", value, 28, value_y, 544, value_height)
        draw.rectangle((28*zoom, top*zoom, 572*zoom-1, bottom*zoom-1), outline=RULE, width=zoom)
        for text, y in [(label, key_y), (value, value_y)]:
            if text:
                draw.rectangle((52*zoom, (y+4)*zoom, 72*zoom, (y+14)*zoom), fill=(54, 66, 62))
        top = bottom+16
    node(None, "heading", "Comparison remains aligned", 28, bottom+64, 544, 34)
    for title, y, headings in [
        ("Comparison remains aligned table", bottom+124, ["Criterion", "Option A", "Option B"]),
        ("Compact properties table", bottom+324, ["Key", "Value"]),
    ]:
        table = node(None, "table", title)
        row = node(table, "table row", "Row 1")
        for column, text in enumerate(headings):
            node(row, "table cell", text, 28+column*125, y, 125, 40)
    return source, tree, image


class PropertyRecordEvidenceTests(unittest.TestCase):
    def test_reviewed_geometry_passes_at_both_scales(self):
        for zoom in [1, 2]:
            result = evaluate(*specimen(zoom))
            self.assertTrue(result["passes"])
            self.assertEqual(result["pixel_checked"], 5)

    def test_mismatched_source_build_and_dimensions_are_rejected(self):
        source, tree, image = specimen()
        for field, value in [("current_sha256", "changed"), ("source_unchanged", False),
                             ("passes", False), ("binary_sha256", "different"),
                             ("width", 601), ("zoom_steps", 1)]:
            with self.subTest(field=field), self.assertRaises(AssertionError):
                evaluate(dict(source, **{field: value}), tree, image)

    def test_missing_or_fabricated_values_are_rejected(self):
        for original, changed in [(VALUES[0], ""), ("", "Not supplied")]:
            source, tree, image = specimen()
            value = next(n for n in tree["nodes"] if n["role"] == "table cell" and n["name"] == original)
            value["name"] = changed
            with self.assertRaises(AssertionError):
                evaluate(source, tree, image)

    def test_reordered_labels_and_duplicated_tables_are_rejected(self):
        source, tree, image = specimen()
        labels = [n for n in tree["nodes"] if n["name"] in LABELS]
        labels[0]["name"], labels[1]["name"] = labels[1]["name"], labels[0]["name"]
        with self.assertRaises(AssertionError):
            evaluate(source, tree, image)
        source, tree, image = specimen()
        tree["nodes"].append(copy.deepcopy(tree["nodes"][0]))
        with self.assertRaises(AssertionError):
            evaluate(source, tree, image)

    def test_cramped_label_gap_and_wrong_leading_edge_are_rejected(self):
        for field, amount in [("y", -4), ("x", 12), ("width", -24)]:
            source, tree, image = specimen()
            value = next(n for n in tree["nodes"] if n["name"] == VALUES[0])
            value["bounds"][field] += amount
            with self.subTest(field=field), self.assertRaises(AssertionError):
                evaluate(source, tree, image)

    def test_old_twelve_pixel_text_inset_is_rejected(self):
        source, tree, image = specimen()
        draw = ImageDraw.Draw(image)
        draw.rectangle((52, 148, 72, 158), fill=PAPER)
        draw.rectangle((40, 148, 60, 158), fill=(54, 66, 62))
        with self.assertRaises(AssertionError):
            evaluate(source, tree, image)

    def test_missing_panel_and_tinted_inset_are_rejected(self):
        source, tree, image = specimen()
        for rectangle, color in [((298, 118, 302, 122), PAPER), ((52, 128, 52, 128), RULE)]:
            changed = image.copy()
            ImageDraw.Draw(changed).rectangle(rectangle, fill=color)
            with self.subTest(rectangle=rectangle), self.assertRaises(AssertionError):
                evaluate(source, tree, changed)

    def test_collapsed_comparison_columns_are_rejected(self):
        source, tree, image = specimen()
        option = next(n for n in tree["nodes"] if n["name"] == "Option A")
        option["bounds"]["x"] = 28
        with self.assertRaises(AssertionError):
            evaluate(source, tree, image)


if __name__ == "__main__":
    unittest.main()
