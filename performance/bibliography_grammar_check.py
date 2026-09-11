"""Check bibliography fixture 71 against written spacing and native pixels.

No renderer-derived golden is accepted: native text bounds and glyph columns
must independently demonstrate hanging indentation and an open paper surface.
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
        found = [n for n in nodes if n['role'] == role and n['name'].startswith(starts)]
        assert len(found) == 1, (role, starts, len(found))
        return found[0]

    def near(actual, expected):
        assert abs(actual - expected) <= 1.1, (actual, expected)

    entries = [node('list item', name) for name in ('Vale,', 'Ibarra,', 'Okafor,')]
    numbered = [node('paragraph', name) for name in ('Rowan,', 'Moreno,', 'Ellis,')]
    bullets = [node('paragraph', name) for name in ('Patel,', 'Kim,')]
    document = node('document frame', '')['bounds']
    inspected = []
    for group in (entries, numbered, bullets):
        for first, second in zip(group, group[1:]):
            a, b = first['bounds'], second['bounds']
            near(b['y'] - a['y'] - a['height'], 12 * zoom)
            assert nodes.index(first) < nodes.index(second)
        for entry in group:
            b = entry['bounds']
            line_count = b['height'] / zoom / 28
            assert abs(line_count - round(line_count)) < .05
            near(b['x'], document['x'])
            assert b['width'] <= document['width']
            if b['y'] < 52 or b['y'] + b['height'] >= pixels.height - 30:
                continue
            left, top, leading = b['x'], b['y'], 28 * zoom
            if group is entries:
                # Actual leading glyphs on the first line; paper in that same
                # 24px rail on every continuation line, then real body glyphs.
                assert ink_count(pixels, (left, top, 22 * zoom, leading)) > 10
                assert ink_count(pixels, (left, top + leading, 22 * zoom, b['height'] - leading)) == 0
                assert ink_count(pixels, (left + 24 * zoom, top + leading, 24 * zoom, leading)) > 10
            else:
                indent = (32 if group is numbered else 24) * zoom
                # List markers belong only to the first line; wrapped body
                # aligns to the citation, not to the reference label.
                assert ink_count(pixels, (left, top + leading, indent - 2 * zoom, b['height'] - leading)) == 0
                assert ink_count(pixels, (left + indent, top + leading, 24 * zoom, leading)) > 10
            # Plain paper, no feature card tint or stepper enclosure.
            assert pixels.getpixel((round(left + b['width'] - 3 * zoom), round(top + 2 * zoom))) == (250, 249, 246)
            inspected.append(entry['name'].split('.')[0])
    assert inspected, 'No complete visible citation was pixel-verified'
    assert nodes.index(entries[-1]) < nodes.index(numbered[0]) < nodes.index(bullets[0])
    assert not any(n['role'] == 'check box' for n in nodes)
    final = node('paragraph', 'Ordinary paragraphs return')['bounds']
    near(final['x'], document['x'])
    for label, last in [('References · numbered', entries[-1]), ('Works cited', numbered[-1]), ('Back to the document', bullets[-1])]:
        heading = node('heading', label)['bounds']
        near(heading['y'] - last['bounds']['y'] - last['bounds']['height'], 64 * zoom)
    return {'binary_sha256': source['binary_sha256'], 'fixture_sha256': source['original_sha256'],
            'zoom': zoom, 'entry_gap': 12 * zoom, 'hanging_indent': 24 * zoom,
            'section_gap': 64 * zoom, 'native_citations': 8, 'pixel_entries': inspected, 'passes': True}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path, nargs='+')
    results = {str(prefix): check(prefix) for prefix in parser.parse_args().prefix}
    assert len({r['binary_sha256'] for r in results.values()}) == 1
    assert len({r['fixture_sha256'] for r in results.values()}) == 1
    for prefix, result in results.items():
        Path(prefix).with_suffix('.pixels.json').write_text(json.dumps(result, indent=2) + '\n')
    print(json.dumps(results, indent=2))


if __name__ == '__main__':
    main()
