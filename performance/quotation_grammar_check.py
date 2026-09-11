"""Independent native geometry/pixel checks for quotation fixture 70.

The written grammar supplies insets, type roles and ownership; these checks do
not derive a passing golden from the renderer's own private geometry helpers.
"""
import argparse
import json
from pathlib import Path

from PIL import Image
from figure_flow_check import ink_count


def check(prefix):
    source = json.loads(prefix.with_suffix('.source.json').read_text())
    tree = json.loads(prefix.with_suffix('.active-atspi.json').read_text())
    assert source['passes'] and source['source_unchanged']
    assert source['binary_sha256'] == tree['binary_sha256']
    pixels = Image.open(prefix.with_suffix('.png')).convert('RGB')
    nodes = tree['nodes']
    zoom = 1 + source['zoom_steps'] / 10

    def node(role, starts):
        matches = [n for n in nodes if n['role'] == role and n['name'].startswith(starts)]
        assert len(matches) == 1, (role, starts)
        return matches[0]

    def near(actual, expected):
        assert abs(actual - expected) <= 1.1, (actual, expected)

    document = node('document frame', '')['bounds']
    ordinary = node('paragraph', 'A clear page')
    author = node('paragraph', '— Editorial specimen,')
    pull = node('paragraph', 'Give each idea')
    pull_author = [n for n in nodes if n['role'] == 'paragraph' and n['name'] == '— Editorial specimen'][0]
    long_first = node('paragraph', 'A quotation can contain')
    long_last = node('paragraph', 'The second paragraph')
    inspected = []
    for body_node, author_node, leading, is_pull in ((ordinary, author, 28, False), (pull, pull_author, 32, True)):
        body, credit = body_node['bounds'], author_node['bounds']
        assert nodes.index(body_node) < nodes.index(author_node)
        near(credit['y'] - body['y'] - body['height'], 8 * zoom)
        near(credit['height'], 20 * zoom)
        lines = body['height'] / zoom / leading
        assert abs(lines - round(lines)) < .06
        assert body['x'] == credit['x'] and body['width'] == credit['width']
        left = document['x'] if is_pull else body['x']
        right = document['x'] + document['width'] if is_pull else body['x'] + body['width']
        if is_pull:
            near(body['x'], document['x'])
        top = body['y'] - 16 * zoom
        bottom = credit['y'] + credit['height'] + 16 * zoom
        if top > 50 and bottom < pixels.height - 20:
            paper = (232, 238, 226) if is_pull else (242, 241, 236)
            for x, y in ((left + 8 * zoom, top + 8 * zoom), (right - 8 * zoom, bottom - 8 * zoom)):
                assert pixels.getpixel((round(x), round(y))) == paper, (x, y, paper)
            assert ink_count(pixels, (body['x'] + 3 * zoom, body['y'], 19 * zoom, body['height'])) == 0
            assert ink_count(pixels, (body['x'] + 24 * zoom, body['y'], body['width'] - 48 * zoom, body['height'])) > 50
            assert ink_count(pixels, (credit['x'] + 24 * zoom, credit['y'], credit['width'] - 48 * zoom, credit['height'])) > 30
            inspected.append('pull' if is_pull else 'attributed')
    first, last = long_first['bounds'], long_last['bounds']
    near(last['y'] - first['y'] - first['height'], 16 * zoom)
    nested = node('paragraph', 'An observation remains')['bounds']
    rails = []
    for level in range(3):
        x = nested['x'] + 24 * zoom * level
        y = nested['y'] - (3 - level) * 16 * zoom + 8 * zoom
        if 50 < y < pixels.height - 20:
            assert pixels.getpixel((round(x), round(y))) == (63, 98, 71), (x, y)
            rails.append(level)
    assert len([n for n in nodes if n['role'] == 'block quote' and n['name'].startswith('An observation remains')]) == 3
    assert node('paragraph', 'This unquoted paragraph')['bounds']['x'] == document['x']
    return {'binary_sha256': source['binary_sha256'], 'fixture_sha256': source['original_sha256'],
            'zoom': zoom, 'attribution_gap': 8 * zoom, 'panel_padding': 16 * zoom,
            'text_inset': 24 * zoom, 'prose_gap': 16 * zoom, 'pixel_panels': inspected,
            'pixel_nested_rails': rails, 'passes': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path, nargs='+')
    args = parser.parse_args()
    results = {str(prefix): check(prefix) for prefix in args.prefix}
    assert len({r['binary_sha256'] for r in results.values()}) == 1
    assert len({r['fixture_sha256'] for r in results.values()}) == 1
    for prefix, result in results.items():
        Path(prefix).with_suffix('.pixels.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(results, indent=2))


if __name__ == '__main__':
    main()
