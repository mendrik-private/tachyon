"""Unchanged record-title typography must survive native header edit/reflow."""
import argparse
import json
from pathlib import Path

from PIL import Image


def matching_title_rows(template, edited, left):
    """Compare actual glyph masks; tolerate unrelated vertical row growth only."""
    mask = lambda im: im.convert('L').point(lambda p: 255 if p < 160 else 0)
    expected = mask(template)
    assert expected.getbbox(), 'The title reference must contain real glyphs'
    width, height = expected.size
    expected_bytes = expected.tobytes()
    edited = mask(edited)
    return [y for y in range(40, edited.height-height-30)
            if edited.crop((left, y, left+width, y+height)).tobytes() == expected_bytes]


def check(before, after):
    from entity_records_check import FIXTURE_SHA, RECORDS
    source = json.loads(before.with_suffix('.source.json').read_text())
    restored = json.loads(after.with_suffix('.source.json').read_text())
    result = json.loads(after.with_suffix('.edit.json').read_text())
    tree = json.loads(before.with_suffix('.active-atspi.json').read_text())
    assert source['binary_sha256'] == restored['binary_sha256'] == result['binary_sha256'] == tree['binary_sha256']
    assert source['current_sha256'] == restored['current_sha256'] == FIXTURE_SHA
    assert all(source[k] == restored[k] for k in ['width', 'height', 'zoom_steps'])
    assert source['width'] == 600 and source['zoom_steps'] == 0
    assert result['expected_edited_source_exact'] and result['undo_restores_exact_bytes']
    original = Image.open(before.with_suffix('.png')).convert('RGB')
    edited = Image.open(after.with_name(after.name+'-idle').with_suffix('.png')).convert('RGB')
    assert original.size == edited.size
    matches = []
    for name, *_ in RECORDS:
        cells = [n for n in tree['nodes'] if n['role'] == 'table cell' and n['name'] == name]
        assert len(cells) == 1
        bounds = cells[0]['bounds']
        x, y = round(bounds['x']+24), round(bounds['y'])
        template = original.crop((x, y, x+200, y+24))
        rows = matching_title_rows(template, edited, x)
        assert len(rows) == 1, (name, 'untouched title glyphs changed or duplicated', rows)
        matches.append(rows[0])
    assert matches == sorted(set(matches)), 'Record source order changed'
    return dict(passes=True, unchanged_titles=len(matches), edited_title_rows=matches,
                binary_sha256=source['binary_sha256'], focused_idle_seconds=result['edit_idle_seconds'])


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    args = parser.parse_args()
    result = check(args.before, args.after)
    args.after.with_suffix('.titles.json').write_text(json.dumps(result, indent=2)+'\n')
    print(json.dumps(result, indent=2))
