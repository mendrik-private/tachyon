"""Check native equal columns, vertical tasks, progress spacing and pane bounds."""
import argparse
import json
from pathlib import Path

from PIL import Image


def check(prefix):
    source = json.loads(prefix.with_suffix('.source.json').read_text())
    tree = json.loads(prefix.with_suffix('.active-atspi.json').read_text())
    assert source['passes'] and source['source_unchanged']
    assert source['binary_sha256'] == tree['binary_sha256']
    nodes = tree['nodes']
    zoom = 1 + source['zoom_steps'] / 10

    def bounds(role, starts):
        matches = [n['bounds'] for n in nodes if n['role'] == role and n['name'].startswith(starts)]
        assert len(matches) == 1, (role, starts, matches)
        return matches[0]

    outline = bounds('tree', 'Document outline')
    files = bounds('tree', 'Markdown folders and files')
    assert outline['y'] + outline['height'] < files['y']
    assert files['height'] >= 5 * 28, files
    pairs = []
    for left, right in [('This is one working document', 'The opening itself'),
                        ('Keep the document in source order.', "Keep the reader's position")]:
        a, b = bounds('paragraph', left), bounds('paragraph', right)
        paired = a['x'] != b['x']
        if paired:
            assert abs(a['width'] - b['width']) <= 1, (a, b)
            assert abs(a['y'] - b['y']) <= 1, (a, b)
            assert abs(b['x'] - a['x'] - a['width'] - 24 * zoom) <= 1, (a, b)
        else:
            assert b['y'] >= a['y'] + a['height'] - 1, (a, b)
        pairs.append(dict(paired=paired, widths=[a['width'], b['width']]))
    tasks = [bounds('check box', name) for name in (
        'Sources checked', 'Notes saved', 'Reading reviewed', 'Links followed',
        'Changes compared', 'Draft shared')]
    assert len({task['x'] for task in tasks}) == 1
    assert all(b['y'] >= a['y'] + a['height'] - 1 for a, b in zip(tasks, tasks[1:]))
    image = Image.open(prefix.with_suffix('.png')).convert('RGB')
    first = tasks[0]
    progress_gap = None
    if 80 * zoom < first['y'] < image.height - 32 * zoom:
        x = round(first['x'] + 20 * zoom)
        # Read the progress fill independently of the renderer's geometry.
        rows = [y for y in range(round(first['y'] - 60 * zoom), first['y'])
                if image.getpixel((x, y)) in ((221, 164, 61), (63, 98, 71))]
        assert rows, 'Progress bar is missing above the first task'
        progress_gap = first['y'] - max(rows) - 1
        assert abs(progress_gap - 32 * .7 * zoom) <= 2, (progress_gap, zoom)
    return dict(passes=True, binary_sha256=source['binary_sha256'], zoom=zoom,
                columns=pairs, task_rows=[task['y'] for task in tasks],
                progress_gap=progress_gap, outline=outline, files=files)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('prefix', type=Path, nargs='+')
    args = parser.parse_args()
    result = {str(prefix): check(prefix) for prefix in args.prefix}
    print(json.dumps(result, indent=2))
