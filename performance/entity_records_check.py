"""Board 04 entity records: source topology, native geometry and sampled pixels.

This is a fixture-specific acceptance oracle, not a general layout classifier.
Native font tests verify shaping; this checks independently specified grammar
tokens and visible ink. It does not OCR labels or approve all corner variants.
"""
import argparse
import json
from pathlib import Path

from PIL import Image

FIXTURE_SHA = '5c607ea0536a188cd6591478010394890b1cc75612408eb1ace34ab19c7395c9'
HEADERS = ['Service', 'Purpose', 'Steward', 'Status']
RECORDS = [
    ['Evidence library', 'Retains original messages and attachments so reviewers can trace each decision to its source.', 'Research operations', 'Ready'],
    ['Review queue', 'Routes unresolved observations to the person responsible for the affected work, with context attached.', 'Delivery team', 'In review'],
    ['Change journal', 'Records explicit changes without replacing the original evidence or guessing an outcome.', 'Platform team', 'Draft'],
    ['Archive export', 'Produces a portable record for a completed review, including the material used to reach it.', '', 'Planned'],
]
CONTROLS = {
    'Comparable capacity table': [
        ['Service', 'Concurrent jobs', 'Retention (days)', 'Storage (GB)'],
        ['Standard', '12', '30', '48'], ['Extended', '24', '90', '96'],
    ],
    'Compact directory table': [
        ['Name', 'Owner', 'Status'], ['Atlas', 'Mira', 'Ready'], ['Reed', 'Leon', 'Draft'],
    ],
}
PAPER = (250, 249, 246)
RULE = (218, 221, 213)
SURFACES = {(600, 2400, 0): True, (600, 600, 0): True,
            (1150, 4000, 10): True, (1600, 1600, 0): False,
            (550, 2400, 0): True}
# New 12px outer margins make the old 600px specimen a 576px table canvas.
# The newly reviewed 550px surface has 526px for records and the current
# 32px section gap. Historical surfaces retain their original expectations.
SECTION_GAPS = {(550, 2400, 0): 32}


def evaluate(source, tree, image):
    assert source['passes'] and source['source_unchanged']
    assert source['fixture'] == tree['fixture'] == '76-entity-records.md'
    assert source['original_sha256'] == source['current_sha256'] == FIXTURE_SHA
    assert source['binary_sha256'] == tree['binary_sha256']
    surface = tuple(source[key] for key in ['width', 'height', 'zoom_steps'])
    assert surface == tuple(tree[key] for key in ['width', 'height', 'zoom_steps'])
    assert image.size == surface[:2]
    assert surface in SURFACES, 'This specimen size has not been reviewed'
    records = SURFACES[surface]
    zoom = 1 + surface[2] / 10
    nodes = tree['nodes']

    def near(a, b):
        assert abs(a-b) <= 1.5, (a, b)

    def children(index):
        return [(i, n) for i, n in enumerate(nodes) if n['parent'] == index]

    def find(name, role):
        matches = [(i, n) for i, n in enumerate(nodes) if n['name'] == name and n['role'] == role]
        assert len(matches) == 1, (name, role, len(matches))
        return matches[0]

    def table_rows(name, expected):
        index, _ = find(name, 'table')
        rows = children(index)
        assert len(rows) == len(expected)
        result = []
        for row_number, ((row, node), values) in enumerate(zip(rows, expected)):
            assert node['role'] == 'table row'
            cells = children(row)
            assert [n['name'] for _, n in cells] == values
            role = 'column header' if row_number == 0 else 'table cell'
            assert all(n['role'] == role for _, n in cells)
            for (cell, n), value in zip(cells, values):
                paragraphs = children(cell)
                assert len(paragraphs) == 1
                assert paragraphs[0][1]['role'] == 'paragraph'
                assert paragraphs[0][1]['name'] == value
            result.append([n['bounds'] for _, n in cells])
        return result

    def ink(box):
        crop = image.crop(tuple(round(v) for v in box)).convert('L')
        return crop.point(lambda p: 255 if p < 160 else 0).getbbox()

    def aligned_columns(cells):
        assert all(a['x'] < b['x'] for a, b in zip(cells, cells[1:]))
        assert max(c['y'] for c in cells) - min(c['y'] for c in cells) <= 1.5

    rows = table_rows('Independent services table', [HEADERS, *RECORDS])
    aligned_columns(rows[0])  # Original schema headers remain editable.
    pixel_checked = 0
    labels_checked = 0
    previous_bottom = None
    for expected, cells in zip(RECORDS, rows[1:]):
        if not records:
            aligned_columns(cells)
            continue
        title, *fields = cells
        near(title['height'], 24*zoom)
        for a, b in zip(cells, cells[1:]):
            near(a['x'], b['x'])
            near(a['width'], b['width'])
            near(b['y'] - a['y'] - a['height'], 16*zoom)
        for field in fields:
            near(field['height'] % (21*zoom), 0)
            assert field['height'] >= 21*zoom  # Empty fields retain a caret line.
        left, right = title['x'], title['x'] + title['width']
        top = title['y'] - 24*zoom
        bottom = fields[-1]['y'] + fields[-1]['height'] + 24*zoom
        if previous_bottom is not None:
            near(top-previous_bottom, 16*zoom)
        previous_bottom = bottom
        if top < 40 or bottom >= image.height-30:
            continue
        # Four actual edges, independent of semantic bounds. Inspecting these
        # samples is deliberately not a claim about every rounded-corner pixel.
        for x, y in [(left+(right-left)/2, top), (left+(right-left)/2, bottom-zoom),
                     (left, (top+bottom)/2), (right-zoom, (top+bottom)/2)]:
            pixels = [image.getpixel((round(x)+dx, round(y)+dy))
                      for dx in (-1, 0, 1) for dy in (-1, 0, 1)]
            assert RULE in pixels, (expected[0], x, y)
        for y in [top+8*zoom, bottom-8*zoom]:
            assert image.getpixel((round(left+24*zoom), round(y))) == PAPER
        title_ink = ink((left+4*zoom, title['y'], right-4*zoom, title['y']+title['height']))
        assert title_ink and 19*zoom <= title_ink[0] <= 24*zoom
        label_rights, value_lefts = [], []
        # The fixture's short labels fit a rail below 70 px at the prescribed
        # 14 px font. The gap oracle compares actual glyphs, allowing bearings.
        for field, value in zip(fields, expected[1:]):
            y = field['y']
            label = ink((left+24*zoom, y, left+90*zoom, y+21*zoom))
            assert label and 0 <= label[0] <= 3*zoom
            label_rights.append(left+24*zoom+label[2])
            labels_checked += 1
            value_ink = ink((left+90*zoom, y, right-24*zoom, y+field['height']))
            if value:
                assert value_ink, value
                value_lefts.append(left+90*zoom+value_ink[0])
            else:
                assert value_ink is None, 'Empty steward must not gain fabricated content'
        near(max(value_lefts)-min(value_lefts), 0)
        # 16 px geometric gap plus the two shaped glyph side bearings.
        assert 15*zoom <= min(value_lefts)-max(label_rights) <= 22*zoom
        pixel_checked += 1
    if records:
        assert pixel_checked, 'No complete record was pixel-checked'
        _, heading = find('Comparable capacity', 'heading')
        near(heading['bounds']['y']-previous_bottom, SECTION_GAPS.get(surface, 64)*zoom)
    for name, expected in CONTROLS.items():
        for cells in table_rows(name, expected):
            aligned_columns(cells)
    return dict(passes=True, binary_sha256=source['binary_sha256'], fixture_sha256=FIXTURE_SHA,
                records=records, pixel_checked=pixel_checked, label_ink_checked=labels_checked,
                canonical_rows=5, canonical_columns=4, inset=24*zoom,
                field_gap=16*zoom, record_gap=16*zoom)


def check(prefix):
    source = json.loads(prefix.with_suffix('.source.json').read_text())
    tree = json.loads(prefix.with_suffix('.active-atspi.json').read_text())
    image = Image.open(prefix.with_suffix('.png')).convert('RGB')
    return evaluate(source, tree, image)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', nargs='+', type=Path)
    results = {str(p): check(p) for p in parser.parse_args().prefix}
    assert len({v['binary_sha256'] for v in results.values()}) == 1
    for prefix, result in results.items():
        Path(prefix).with_suffix('.pixels.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(results, indent=2))
