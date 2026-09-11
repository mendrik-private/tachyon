"""Native caption/credit attachment, source order, scaled gaps and actual wrap ink.

Fixture 69 only. This is not an approval of every board-05 media family.
"""
import argparse
import json
import math
from pathlib import Path

from PIL import Image

from figure_flow_check import ink_count


def check(prefix, mode):
    source = json.loads(prefix.with_suffix('.source.json').read_text())
    tree = json.loads(prefix.with_suffix('.active-atspi.json').read_text())
    assert source['passes'] and source['source_unchanged']
    assert source['binary_sha256'] == tree['binary_sha256']
    nodes = tree['nodes']

    def node(role, starts):
        matches = [n for n in nodes if n['role'] == role and n['name'].startswith(starts)]
        assert len(matches) == 1, (mode, role, starts)
        return matches[0]

    figure = node('image', 'Botanical illustration')
    caption = node('caption', 'Figure 1.')
    credit = node('paragraph', 'Credit: Field notebook')
    prose = node('paragraph', 'A field notebook connects')
    assert caption['name'] in figure['description']
    assert credit['name'] in figure['description']
    assert [nodes.index(n) for n in (figure, caption, credit, prose)] == sorted(nodes.index(n) for n in (figure, caption, credit, prose))
    assert not any(n['role'] == 'paragraph' and n['name'].startswith('Botanical illustration') for n in nodes)
    f, c, r, p = [n['bounds'] for n in (figure, caption, credit, prose)]
    zoom = 2 if mode.startswith('200-') else 1
    assert abs(c['y'] - f['y'] - f['height'] - 8 * zoom) <= 1
    assert abs(r['y'] - c['y'] - c['height'] - 4 * zoom) <= 1
    assert f['x'] == c['x'] == r['x']
    assert f['width'] == c['width'] == r['width']
    for text in (c, r):
        assert abs(text['height'] / (18 * zoom) - round(text['height'] / (18 * zoom))) < .05
    pixels = Image.open(prefix.with_suffix('.png')).convert('RGB')
    result = {'caption_gap': 8 * zoom, 'credit_gap': 4 * zoom, 'caption_leading': 18 * zoom, 'canonical_order': True}
    if mode not in ('narrow', '200-stack'):
        assert f['y'] == p['y']
        assert .25 <= f['width'] / p['width'] <= .35
        leading = 28 * zoom
        media_height = r['y'] + r['height'] - f['y']
        rows = math.ceil(media_height / leading)
        assert rows >= 4
        right = f['x'] + f['width'] + 24 * zoom + 1
        for row in range(rows):
            assert ink_count(pixels, (right, p['y'] + row * leading, p['width'] - right + p['x'], leading)) > 50
        full_y = p['y'] + rows * leading
        assert ink_count(pixels, (p['x'], full_y, f['width'], leading)) > 50
        assert ink_count(pixels, (f['x'] + f['width'] + 2, f['y'], 20 * zoom, media_height)) == 0
        result.update({'useful_lines': rows, 'full_width_return_y': full_y, 'gutter': 24 * zoom})
    else:
        assert abs(p['y'] - r['y'] - r['height'] - 24 * zoom) <= 1
        assert p['x'] == f['x']
        for text in (c, r):
            assert ink_count(pixels, (text['x'], text['y'], text['width'], text['height'])) > 50
        result['stacked'] = True
    # The image's visible pixels start at its semantic leading edge: no old
    # eight-pixel underlay or centered image inside an unrelated wide rectangle.
    if mode != '200-stack':
        assert pixels.getpixel((f['x'] + 2, f['y'] + f['height'] // 2)) == (232, 238, 226)
    return source, result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--prefix', default='figure-caption-final')
    args = parser.parse_args()
    root = Path(__file__).resolve().parent / 'layout-previews'
    sources, results = [], {}
    for mode in ('wide', 'narrow', '200-top', '200-stack'):
        source, results[mode] = check(root / f'{args.prefix}-{mode}', mode)
        sources.append(source)
    assert len({s['binary_sha256'] for s in sources}) == 1
    assert len({s['original_sha256'] for s in sources}) == 1
    gallery = Image.open(root / f'{args.prefix}-gallery-weston.png').convert('RGB')
    # Final native outline navigation, not a separately laid-out mockup. These
    # exact x anchors come from the companion Weston accessibility snapshot.
    tree = json.loads((root / f'{args.prefix}.weston.json').read_text())['tree']['nodes']
    figures = [n['bounds'] for n in tree if n['role'] == 'image' and n['name'].startswith('Photograph')]
    assert len(figures) == 2
    left, right = figures
    assert left['width'] == right['width'] == 600
    assert right['x'] - left['x'] - left['width'] == 24
    y = next(y for y in range(50, gallery.height) if gallery.getpixel((left['x'] + 24, y)) == (232, 238, 226))
    assert ink_count(gallery, (left['x'] + 602, y, 20, 600)) == 0
    for figure in figures:
        assert gallery.getpixel((figure['x'] + 24, y)) == (232, 238, 226)
        assert ink_count(gallery, (figure['x'], y + 608, 600, 18)) > 50
        assert ink_count(gallery, (figure['x'], y + 630, 600, 18)) > 50
    results['gallery'] = {'gutter': 24, 'caption_gap': 8, 'credit_gap': 4, 'caption_ink': True}
    result = {'binary_sha256': sources[0]['binary_sha256'], 'fixture_sha256': sources[0]['original_sha256'], 'checks': results, 'passes': True}
    (root / f'{args.prefix}.pixels.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(result))


if __name__ == '__main__':
    main()
