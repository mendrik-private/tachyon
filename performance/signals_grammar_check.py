"""Board 03 literal signals: native geometry and independent token pixels."""
import argparse
import json
from pathlib import Path

from PIL import Image

COLORS = [
    ('Color: Moss Green', '#3F6247', 'Emphasis and controls; not an automatic claim of completion.', (63, 98, 71)),
    ('Color: Paper', '#FAF9F6', 'The document canvas. A fine outline keeps the swatch visible.', (250, 249, 246)),
    ('Color: Pale blue', '#EDF4F9', 'Neutral information with a readable foreground.', (237, 244, 249)),
    ('Colour token: Short notation', '#0aB', 'The original shorthand and letter case remain unchanged.', (0, 170, 187)),
    ('Color token: Transparent green', '#3f624780', 'Alpha is previewed over Paper, not converted in the source.', (156, 173, 158)),
]


def check(prefix):
    source = json.loads(prefix.with_suffix('.source.json').read_text())
    tree = json.loads(prefix.with_suffix('.active-atspi.json').read_text())
    assert source['passes'] and source['source_unchanged']
    assert source['binary_sha256'] == tree['binary_sha256']
    zoom = 1 + source['zoom_steps'] / 10
    image = Image.open(prefix.with_suffix('.png')).convert('RGB')
    nodes = tree['nodes']
    doc = next(i for i, n in enumerate(nodes) if n['role'] == 'document frame')
    roots = [n for n in nodes if n['parent'] == doc]

    def find(name):
        return next(n for n in roots if n['name'] == name)

    def near(actual, expected, tolerance=1.3):
        assert abs(actual - expected) <= tolerance, (actual, expected)

    checked = []
    boxes = []
    order = []
    for title, value, context, rgb in COLORS:
        h = find(title)
        # Duplicate literal in the ordinary negative control is intentional.
        i = roots.index(h)
        v, body = roots[i+1:i+3]
        assert (v['name'], body['name']) == (value, context)
        order.extend([i, i+1, i+2])
        h, v, body = [n['bounds'] for n in (h, v, body)]
        near(h['height'] % (24 * zoom), 0)
        near(v['height'], 20 * zoom)
        near(v['y'] - h['y'] - h['height'], 8 * zoom)
        near(body['y'] - v['y'] - v['height'], 8 * zoom)
        near(v['x'], h['x'])
        near(body['x'], h['x'])
        top = h['y'] - 24 * zoom
        bottom = body['y'] + body['height'] + 24 * zoom
        boxes.append((h['x'], top, h['width'], bottom))
        if top < 40 or bottom >= image.height - 30:
            continue
        edge = (48 if h['width'] / zoom >= 359 else 24) * zoom
        x, y = h['x'] + 24 * zoom, top + 24 * zoom
        actual = image.getpixel((round(x + edge/2), round(y + edge/2)))
        assert max(abs(a-b) for a, b in zip(actual, rgb)) <= 2, (title, actual, rgb)
        # A square swatch, not a stretched rectangle. Sample all four edges.
        for px, py in [(x+edge/2, y), (x+edge/2, y+edge-zoom), (x,y+edge/2), (x+edge-zoom,y+edge/2)]:
            neighborhood = [image.getpixel((round(px)+dx, round(py)+dy)) for dx in (-1,0,1) for dy in (-1,0,1)]
            assert (218,221,213) in neighborhood, (title, px, py, neighborhood)
        # The text begins after a 24px swatch gutter, and the actual literal
        # remains visible in dark monospace glyphs, outside the color sample.
        text_x = x + edge + 24 * zoom
        ink = image.crop((round(text_x), round(v['y']), round(h['x']+h['width']-24*zoom), round(v['y']+v['height'])))
        glyphs = ink.convert('L').point(lambda p: 255 if p < 170 else 0).getbbox()
        assert glyphs and glyphs[2] - glyphs[0] >= 30 * zoom, (title, glyphs)
        assert glyphs[0] <= 4 * zoom, (title, glyphs)
        for border_y in (top, bottom-zoom):
            pixels = [image.getpixel((round(h['x']+h['width']/2), y)) for y in range(round(border_y)-1, round(border_y+zoom)+2)]
            assert (218,221,213) in pixels, (title, pixels)
            assert pixels.count((218,221,213)) <= round(zoom)+1
        checked.append(title)
    assert order == sorted(order)
    assert checked, 'No complete visible swatch was verified'
    for group in (boxes[:3], boxes[3:]):
        if len({round(b[1]) for b in group}) == 1:
            for a, b in zip(group, group[1:]):
                near(b[0] - a[0] - a[2], 24*zoom)
        elif len({round(b[0]) for b in group}) == 1:
            for a, b in zip(group, group[1:]):
                near(b[1] - a[3], 24*zoom)
    near(boxes[0][1] - find('Color tokens')['bounds']['y'] - find('Color tokens')['bounds']['height'], 24*zoom)
    near(find('Literal formats')['bounds']['y'] - max(b[3] for b in boxes[:3]), 64*zoom)
    near(find('Status registry')['bounds']['y'] - max(b[3] for b in boxes[3:]), 64*zoom)
    negative = roots.index(find('Color discussion'))
    near(roots[negative+1]['bounds']['height'], 24*zoom)
    assert any(n['name'] == '#GGHHII' for n in roots)
    properties = [next(n['bounds'] for n in nodes if n['role']=='paragraph' and n['name']==name)
                  for name in ('Status: Accepted','Classification: Internal','Owner: Design systems')]
    if source['width'] >= 1600 and zoom == 1:
        assert len({b['y'] for b in properties}) == 1, 'Metadata unnecessarily stacked despite usable width'
        assert properties[0]['x'] < properties[1]['x'] < properties[2]['x']
        assert properties[-1]['x']+properties[-1]['width']-properties[0]['x'] < 800, 'Metadata was needlessly stretched'
    badge_checked = []
    for label, paper in [('Draft',(242,241,236)),('Accepted',(232,238,226)),('Beta',(237,244,249)),
                         ('Deprecated',(251,237,234)),('In review',(255,244,223)),
                         ('Custom state',(242,241,236)),('Mandatory',(242,241,236)),
                         ('Confidential',(242,241,236)),('Pending',(255,244,223))]:
        b = next(n['bounds'] for n in nodes if n['role']=='paragraph' and n['name']==label)
        if b['y'] < 40 or b['y']+b['height']+5*zoom >= image.height-30:
            continue
        # The 4px vertical inset must be painted, not clipped at the text
        # line. These two points deliberately lie outside the glyph bounds.
        for y in (b['y']-2*zoom, b['y']+b['height']+2*zoom):
            actual = image.getpixel((round(b['x']+24*zoom),round(y)))
            assert max(abs(a-c) for a,c in zip(actual,paper)) <= 2, (label,y,actual,paper)
        badge_checked.append(label)
    return dict(passes=True, binary_sha256=source['binary_sha256'], fixture_sha256=source['original_sha256'],
                zoom=zoom, pixel_checked=checked, badges_checked=badge_checked, swatch_count=5,
                inset=24*zoom, inner_gap=8*zoom, module_gap=24*zoom)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', nargs='+', type=Path)
    results = {str(p): check(p) for p in parser.parse_args().prefix}
    assert len({v['binary_sha256'] for v in results.values()}) == 1
    for prefix, result in results.items():
        Path(prefix).with_suffix('.pixels.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(results, indent=2))
