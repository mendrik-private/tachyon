"""Fixture 77: large-text record labels stack with 24/16/8 grammar spacing."""
import argparse
import json
from pathlib import Path
from PIL import Image

from entity_records_check import PAPER, RULE


def check(prefix):
    source = json.loads(prefix.with_suffix('.source.json').read_text())
    tree = json.loads(prefix.with_suffix('.active-atspi.json').read_text())
    image = Image.open(prefix.with_suffix('.png')).convert('RGB')
    assert source['passes'] and source['source_unchanged']
    assert source['fixture'] == tree['fixture'] == '77-stacked-entity-labels.md'
    assert source['original_sha256'] == source['current_sha256'] == 'bfd9e5b950d59cee0b4c143d3aeaed1eddf7f54f717891e1eaa466bce710962c'
    assert source['binary_sha256'] == tree['binary_sha256']
    assert tuple(source[k] for k in ['width', 'height', 'zoom_steps']) == (600, 3000, 10)
    assert all(source[k] == tree[k] for k in ['width', 'height', 'zoom_steps'])
    assert image.size == (600, 3000)
    nodes = tree['nodes']
    children = lambda i: [(j, n) for j, n in enumerate(nodes) if n['parent'] == i]
    tables = [(i, n) for i, n in enumerate(nodes) if n['role'] == 'table']
    assert len(tables) == 1 and tables[0][1]['name'] == 'Independent services table'
    rows = children(tables[0][0])
    expected = [
        ['Name', 'Description', 'Owner'],
        ['Atlas', 'Keep the source with each review so a new reader can trace what changed and why.', 'Research'],
        ['Reed', 'Hold open items for the next review and keep their full context in one place.', 'Delivery'],
    ]
    assert len(rows) == 3
    previous_bottom = None
    for ordinal, ((i, row), names) in enumerate(zip(rows, expected)):
        assert row['role'] == 'table row'
        cells = children(i)
        assert [n['name'] for _, n in cells] == names
        assert all(n['role'] == ('column header' if ordinal == 0 else 'table cell') for _, n in cells)
        if ordinal == 0:
            continue
        bounds = [n['bounds'] for _, n in cells]
        title = bounds[0]
        assert title['height'] == 48
        for a, b in zip(bounds, bounds[1:]):
            assert a['x'] == b['x'] and a['width'] == b['width']
            assert abs(b['y']-a['y']-a['height']-90) <= 1.5  # 16+21+8, at 200%.
        left, right = title['x'], title['x']+title['width']
        top, bottom = title['y']-48, bounds[-1]['y']+bounds[-1]['height']+48
        if previous_bottom is not None:
            assert abs(top-previous_bottom-32) <= 1.5
        previous_bottom = bottom
        for x, y in [(left, (top+bottom)/2), (right-2, (top+bottom)/2),
                     ((left+right)/2, top), ((left+right)/2, bottom-2)]:
            assert RULE in [image.getpixel((round(x)+dx, round(y)+dy))
                            for dx in [-1, 0, 1] for dy in [-1, 0, 1]]
        for y in [top+16, bottom-16]:
            assert image.getpixel((round(left+48), round(y))) == PAPER
        # Separate label/value ink at one leading edge, not narrow side rails.
        for field in bounds[1:]:
            for y, height in [(field['y']-58, 42), (field['y'], field['height'])]:
                crop = image.crop((round(left+8), round(y), round(right-8), round(y+height)))
                ink = crop.convert('L').point(lambda p: 255 if p < 160 else 0).getbbox()
                assert ink and 38 <= ink[0] <= 48
    return dict(passes=True, binary_sha256=source['binary_sha256'], pixel_checked_records=2,
                stacked_labels=4, inset=48, field_gap=32, label_value_gap=16, record_gap=32)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path)
    prefix = parser.parse_args().prefix
    result = check(prefix)
    prefix.with_suffix('.pixels.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result, indent=2))
