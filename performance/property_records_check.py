"""Board 04 property records: independent native token and semantic checks."""
import argparse
import json
from pathlib import Path

from PIL import Image

LABELS = ["Evidence retention", "Review assignment", "Escalation policy", "Status", "Optional comment"]
VALUES = [
    "Keep original messages and attachments alongside each decision. Removal requires an explicit action.",
    "Route unresolved observations to the person who owns the affected work, with the original evidence attached.",
    "Notify the project coordinator when a review remains unanswered after two working days.",
    "Draft", "",
]
RULE = (218, 221, 213)
PAPER = (250, 249, 246)


def check(prefix):
    source = json.loads(prefix.with_suffix('.source.json').read_text())
    tree = json.loads(prefix.with_suffix('.active-atspi.json').read_text())
    image = Image.open(prefix.with_suffix('.png')).convert('RGB')
    return evaluate(source, tree, image)


def evaluate(source, tree, image):
    """Check the reviewed fixture against the grammar, not renderer constants.

    This oracle covers the recorded wide/narrow/200%/short specimen sizes;
    it does not claim a general content-fit classifier or all corner variants.
    """
    assert source['passes'] and source['source_unchanged']
    assert source['original_sha256'] == source['current_sha256']
    assert source['binary_sha256'] == tree['binary_sha256']
    assert image.size == (source['width'], source['height'])
    assert all(source[key] == tree[key] for key in ['width', 'height', 'zoom_steps'])
    zoom = 1 + source['zoom_steps'] / 10
    nodes = tree['nodes']

    def children(index):
        return [(i, n) for i, n in enumerate(nodes) if n['parent'] == index]

    def find(name, role):
        matches = [(i, n) for i, n in enumerate(nodes) if n['name'] == name and n['role'] == role]
        assert len(matches) == 1, (name, role, len(matches))
        return matches[0]

    def near(a, b):
        assert abs(a-b) <= 1.5, (a, b)

    table, _ = find('Delivery settings table', 'table')
    rows = children(table)
    assert len(rows) == 6 and all(n['role'] == 'table row' for _, n in rows)
    assert [n['name'] for _, n in children(rows[0][0])] == ['Property', 'Description']
    expected_records = source['width'] / zoom < 800
    previous_bottom = None
    checked = 0
    for label, expected_value, (row, _) in zip(LABELS, VALUES, rows[1:]):
        cells = children(row)
        assert len(cells) == 2 and all(n['role'] == 'table cell' for _, n in cells)
        assert cells[0][1]['name'] == label
        assert cells[1][1]['name'] == expected_value
        key, value = [n['bounds'] for _, n in cells]
        if not expected_records:
            near(key['y'], value['y'])
            assert value['x'] > key['x']
            continue
        near(key['x'], value['x'])
        near(key['width'], value['width'])
        near(value['y'] - key['y'] - key['height'], 8*zoom)
        top = key['y'] - 24*zoom
        bottom = value['y'] + value['height'] + 24*zoom
        if previous_bottom is not None:
            near(top - previous_bottom, 16*zoom)
        previous_bottom = bottom
        left, right = key['x'], key['x'] + key['width']
        if top < 40 or bottom >= image.height-30:
            continue
        # Actual outer boundaries and empty inset areas. These samples do not
        # by themselves prove border thickness or corner curvature.
        for x, y in [(left+(right-left)/2, top), (left+(right-left)/2, bottom-zoom),
                     (left, (top+bottom)/2), (right-zoom, (top+bottom)/2)]:
            nearby = [image.getpixel((round(x)+dx, round(y)+dy)) for dx in (-1, 0, 1) for dy in (-1, 0, 1)]
            assert RULE in nearby, (label, x, y, nearby)
        for y in [top+8*zoom, bottom-8*zoom]:
            assert image.getpixel((round(left+24*zoom), round(y))) == PAPER
        # Source text starts at the measured 24px inset, not the old 12px cell
        # inset. The AT-SPI bounds intentionally describe the full source lane.
        for cell, name in [(key, label), (value, cells[1][1]['name'])]:
            if not name:
                continue
            ink = image.crop((round(left+4*zoom), round(cell['y']), round(right-4*zoom), round(cell['y']+cell['height'])))
            bbox = ink.convert('L').point(lambda p: 255 if p < 160 else 0).getbbox()
            assert bbox, label
            assert 19*zoom <= bbox[0] <= 24*zoom, (label, bbox)
        checked += 1
    if expected_records:
        assert checked, 'No complete record was pixel-checked'
        _, heading = find('Comparison remains aligned', 'heading')
        near(heading['bounds']['y'] - previous_bottom, 64*zoom)
    # Comparisons and already compact property tables preserve columns.
    for name in ['Comparison remains aligned table', 'Compact properties table']:
        index, _ = find(name, 'table')
        for row, _ in children(index):
            cells = [n['bounds'] for _, n in children(row)]
            assert len(cells) >= 2
            assert all(cells[i]['x'] < cells[i+1]['x'] for i in range(len(cells)-1))
            assert max(c['y'] for c in cells)-min(c['y'] for c in cells) <= 1.5
    return dict(passes=True, binary_sha256=source['binary_sha256'], fixture_sha256=source['original_sha256'],
                records=expected_records, pixel_checked=checked, source_rows=6, source_columns=2,
                inset=24*zoom, label_gap=8*zoom, record_gap=16*zoom)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', nargs='+', type=Path)
    results = {str(p): check(p) for p in parser.parse_args().prefix}
    assert len({v['binary_sha256'] for v in results.values()}) == 1
    for prefix, result in results.items():
        Path(prefix).with_suffix('.pixels.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(results, indent=2))
