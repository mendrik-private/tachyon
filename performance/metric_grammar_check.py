"""Independent Board03 metric geometry and glyph checks for fixture 73.

Checks actual native text bounds and screenshot pixels against the written
grammar. Does not use a golden image produced by the candidate renderer.
"""
import argparse
import json
from pathlib import Path

from PIL import Image


METRICS = [
    ('Metric: Review completion', '75%', ['9 of 12 documents reviewed · September 2026.']),
    ('Metric: Median response', '128 ms', ['240 requests · September 2026.']),
    ('Metric: Monthly cost', '€1.234,50', ['Per workspace · September 2026. Taxes included.']),
    ('Metric: Response change', '−2.4%', [
        'Compared with 125 ms in August 2026; September median is 122 ms.',
        'This is an authored comparison, not a trend inferred by the layout engine.']),
    ('Metric: Review count', '9', [
        'Documents reviewed out of 12 submitted. Partial reviews are not counted.']),
]


def check(prefix):
    source = json.loads(prefix.with_suffix('.source.json').read_text())
    tree = json.loads(prefix.with_suffix('.active-atspi.json').read_text())
    assert source['passes'] and source['source_unchanged']
    assert source['binary_sha256'] == tree['binary_sha256']
    pixels = Image.open(prefix.with_suffix('.png')).convert('RGB')
    zoom = 1 + source['zoom_steps'] / 10
    nodes = tree['nodes']
    document = next(i for i, n in enumerate(nodes) if n['role'] == 'document frame')
    roots = [n for n in nodes if n['parent'] == document]

    def find(name):
        return next(n for n in roots if n['name'] == name)

    def near(actual, expected, tolerance=1.2):
        assert abs(actual - expected) <= tolerance, (actual, expected)

    order = []
    inspected = []
    boxes = []
    for label, value, context in METRICS:
        members = [find(t) for t in [label, value, *context]]
        order.extend(roots.index(n) for n in members)
        h, v, *body = [n['bounds'] for n in members]
        near(v['height'], 44 * zoom)
        near(h['height'] % (24 * zoom), 0, 1.2)
        for previous, following in zip([h, v, *body], [v, *body]):
            near(following['y'] - previous['y'] - previous['height'], 8 * zoom)
            near(following['x'], h['x'])
            near(following['width'], h['width'])
        top = h['y'] - 24 * zoom
        bottom = body[-1]['y'] + body[-1]['height'] + 24 * zoom
        boxes.append((h['x'], top, h['width'], bottom))
        if top < 40 or bottom >= pixels.height - 30:
            continue
        x, right = h['x'], h['x'] + h['width']
        leading = 72 * zoom if h['width'] / zoom >= 419 else 0
        # Borders are single logical-pixel hairlines. Sample well away from
        # corners/text; allow one row of fractional rasterization uncertainty.
        for edge in (top, bottom - zoom):
            line = [pixels.getpixel((round(x + h['width'] / 2), y))
                    for y in range(round(edge) - 1, round(edge + zoom) + 2)]
            assert (218, 221, 213) in line, (label, edge, line)
            assert sum(c == (218, 221, 213) for c in line) <= round(zoom) + 1
        # Actual value glyphs are large, legible, source-positioned and inside
        # the same 24px inset used by the label and contextual text.
        value_ink = pixels.crop((round(x + 24 * zoom + leading), round(v['y']),
                                 round(right - 24 * zoom), round(v['y'] + v['height'])))
        ink = value_ink.convert('L').point(lambda p: 255 if p < 160 else 0).getbbox()
        assert ink and ink[3] - ink[1] >= 24 * zoom, (label, ink)
        assert ink[0] <= 5 * zoom, (label, ink)
        assert pixels.getpixel((round(x + 12 * zoom), round(top + 12 * zoom))) == (250, 249, 246)
        inspected.append(label)
    assert order == sorted(order), 'Metric reading order changed'
    assert inspected, 'No complete visible metric was pixel checked'
    first = boxes[:3]
    if len({round(b[1]) for b in first}) == 1:
        for left, right in zip(first, first[1:]):
            near(right[0] - left[0] - left[2], 24 * zoom)
    else:
        for above, below in zip(first, first[1:]):
            near(below[1] - above[3], 24 * zoom)
    near(first[0][1] - find('Review snapshot')['bounds']['y'] - find('Review snapshot')['bounds']['height'], 24 * zoom)
    near(find('Change needs a baseline')['bounds']['y'] - max(b[3] for b in first), 64 * zoom)
    near(find('Ordinary numbers stay in prose')['bounds']['y'] - max(b[3] for b in boxes[3:]), 64 * zoom)
    # Negative semantic controls must not receive metric display typography.
    near(find('42%')['bounds']['height'], 24 * zoom)
    near(find('120 ms')['bounds']['height'], 24 * zoom)
    return dict(passes=True, binary_sha256=source['binary_sha256'],
                fixture_sha256=source['original_sha256'], zoom=zoom,
                metric_count=5, pixel_checked=inspected,
                value_leading=44 * zoom, inner_gap=8 * zoom,
                inset=24 * zoom, row_gutter=24 * zoom)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path, nargs='+')
    results = {str(p): check(p) for p in parser.parse_args().prefix}
    assert len({r['binary_sha256'] for r in results.values()}) == 1
    for prefix, result in results.items():
        Path(prefix).with_suffix('.pixels.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(results, indent=2))


if __name__ == '__main__':
    main()
