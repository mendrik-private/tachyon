"""Adversarial, display-independent tests of the entity-record pixel oracle."""
import copy
import unittest

from PIL import Image, ImageDraw

from entity_records_check import CONTROLS, FIXTURE_SHA, HEADERS, PAPER, RECORDS, RULE, evaluate


def specimen():
    source = dict(passes=True, source_unchanged=True, fixture='76-entity-records.md',
                  binary_sha256='build', original_sha256=FIXTURE_SHA, current_sha256=FIXTURE_SHA,
                  width=600, height=2400, zoom_steps=0)
    tree = {key: source[key] for key in ['fixture', 'binary_sha256', 'width', 'height', 'zoom_steps']}
    nodes = tree['nodes'] = []
    image = Image.new('RGB', (600, 2400), PAPER)
    draw = ImageDraw.Draw(image)

    def node(parent, role, name, x=0, y=0, width=0, height=0):
        nodes.append(dict(parent=parent, role=role, name=name,
                          bounds=dict(x=x, y=y, width=width, height=height)))
        return len(nodes)-1

    def cell(parent, role, name, x, y, width, height):
        i = node(parent, role, name, x, y, width, height)
        node(i, 'paragraph', name, x, y, width, height)

    def ink(x, y, width):
        draw.rectangle((x, y+5, x+width-1, y+16), fill=(54, 66, 62))

    table = node(None, 'table', 'Independent services table')
    row = node(table, 'table row', 'Row 1')
    for column, name in enumerate(HEADERS):
        cell(row, 'column header', name, 28+column*100, 60, 100, 21)
    top = 120
    for index, record in enumerate(RECORDS):
        row = node(table, 'table row', f'Row {index+2}')
        y = top+24
        cell(row, 'table cell', record[0], 28, y, 544, 24)
        ink(52, y, 130)
        y += 24+16
        for column, value in enumerate(record[1:]):
            height = 42 if column == 0 else 21
            cell(row, 'table cell', value, 28, y, 544, height)
            ink(53, y, 53 if column < 2 else 41)
            if value:
                ink(125, y, 80)
            y += height+16
        bottom = y-16+24
        draw.rectangle((28, top, 571, bottom-1), outline=RULE)
        top = bottom+16
    node(None, 'heading', 'Comparable capacity', 28, bottom+64, 544, 34)
    y = bottom+120
    for name, values in CONTROLS.items():
        table = node(None, 'table', name)
        for index, names in enumerate(values):
            row = node(table, 'table row', f'Row {index+1}')
            for column, name in enumerate(names):
                cell(row, 'column header' if index == 0 else 'table cell', name,
                     28+column*100, y, 100, 21)
            y += 41
    return source, tree, image


class EntityRecordEvidenceTests(unittest.TestCase):
    def test_reviewed_tokens_and_all_fields_pass(self):
        result = evaluate(*specimen())
        self.assertEqual(result['pixel_checked'], 4)
        self.assertEqual(result['label_ink_checked'], 12)

    def test_source_build_dimensions_and_unknown_surface_fail(self):
        source, tree, image = specimen()
        for key, value in [('current_sha256', 'changed'), ('binary_sha256', 'different'),
                           ('width', 601), ('passes', False), ('source_unchanged', False)]:
            with self.subTest(key=key), self.assertRaises(AssertionError):
                evaluate(dict(source, **{key: value}), tree, image)

    def test_missing_fabricated_or_reordered_cells_fail(self):
        for original, changed in [(RECORDS[0][1], ''), ('', 'Unassigned'),
                                  ('Steward', 'Status'), ('12', '24')]:
            source, tree, image = specimen()
            n = next(n for n in tree['nodes'] if n['name'] == original)
            n['name'] = changed
            with self.subTest(original=original), self.assertRaises(AssertionError):
                evaluate(source, tree, image)

    def test_duplicate_table_or_header_paragraph_fails(self):
        for role in ['table', 'paragraph']:
            source, tree, image = specimen()
            n = next(n for n in tree['nodes'] if n['role'] == role)
            tree['nodes'].append(copy.deepcopy(n))
            with self.subTest(role=role), self.assertRaises(AssertionError):
                evaluate(source, tree, image)

    def test_wrong_geometry_or_dense_gap_fails(self):
        for field, delta in [('x', 12), ('width', -24), ('y', -4), ('height', -4)]:
            source, tree, image = specimen()
            n = next(n for n in tree['nodes'] if n['name'] == RECORDS[0][1])
            n['bounds'][field] += delta
            with self.subTest(field=field), self.assertRaises(AssertionError):
                evaluate(source, tree, image)

    def test_missing_label_ink_and_old_title_inset_fail(self):
        for rectangle in [(52, 184, 110, 205), (52, 144, 183, 167)]:
            source, tree, image = specimen()
            ImageDraw.Draw(image).rectangle(rectangle, fill=PAPER)
            with self.subTest(rectangle=rectangle), self.assertRaises(AssertionError):
                evaluate(source, tree, image)

    def test_missing_edge_or_tinted_inset_fails(self):
        for rectangle, color in [((298, 118, 302, 122), PAPER), ((52, 128, 52, 128), RULE)]:
            source, tree, image = specimen()
            ImageDraw.Draw(image).rectangle(rectangle, fill=color)
            with self.subTest(rectangle=rectangle), self.assertRaises(AssertionError):
                evaluate(source, tree, image)

    def test_empty_value_ink_and_collapsed_comparison_fail(self):
        source, tree, image = specimen()
        empty = next(n for n in tree['nodes'] if n['role'] == 'table cell' and not n['name'])
        y = empty['bounds']['y']
        ImageDraw.Draw(image).rectangle((125, y+5, 150, y+16), fill=(54, 66, 62))
        with self.assertRaises(AssertionError):
            evaluate(source, tree, image)
        source, tree, image = specimen()
        n = next(n for n in tree['nodes'] if n['name'] == 'Concurrent jobs')
        n['bounds']['x'] = 28
        with self.assertRaises(AssertionError):
            evaluate(source, tree, image)


if __name__ == '__main__':
    unittest.main()
