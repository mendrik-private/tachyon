"""Fault checks for an explicit validation build on a private native seat."""
import json
import shutil
import subprocess
import time

LABELS = ('Alpha', 'Beta', 'Gamma', 'Delta', 'Epsilon', 'Zeta')


def items(nodes):
    result = []
    for label in LABELS:
        matches = [n for n in nodes if n['name'] == label and n['role'] in ('list item', 'listitem')]
        if len(matches) != 1:
            raise RuntimeError(f'Expected one canonical list item for {label}: {matches}')
        result.append(matches[0])
    return result


def is_stack(nodes):
    rectangles = [item['bounds'] for item in items(nodes)]
    return (all(r['width'] > 0 and r['height'] > 0 for r in rectangles)
            and max(r['x'] for r in rectangles) - min(r['x'] for r in rectangles) <= 1
            and all(b['y'] >= a['y'] + a['height'] - 1 for a, b in zip(rectangles, rectangles[1:])))


def is_grid(nodes):
    rectangles = [item['bounds'] for item in items(nodes)]
    return (all(r['width'] > 0 and r['height'] > 0 for r in rectangles)
            and all(abs(r['y'] - rectangles[0]['y']) <= 1 for r in rectangles[:3])
            and rectangles[0]['x'] < rectangles[1]['x'] < rectangles[2]['x']
            and all(abs(r['y'] - rectangles[3]['y']) <= 1 for r in rectangles[3:])
            and all(abs(rectangles[i]['x'] - rectangles[i - 3]['x']) <= 1 for i in range(3, 6))
            and rectangles[3]['y'] >= max(r['y'] + r['height'] for r in rectangles[:3]) - 1)


def check(kind, env, input_event, source_path, pid, output, probe_path, work, events):
    if not env.get('MINERAL_PRIVATE_ATSPI_BUS') or env.get('DBUS_SESSION_BUS_ADDRESS') != env['MINERAL_PRIVATE_ATSPI_BUS']:
        raise RuntimeError('Recovery checks require the private accessibility bus')
    if source_path.resolve().parent != (work / 'layout-fixtures').resolve():
        raise RuntimeError('Recovery checks require a harness-owned fixture copy')
    original = source_path.read_bytes()

    def key(code, modifiers=()):
        for modifier in modifiers: input_event('key', modifier, 1)
        input_event('key', code, 1)
        input_event('key', code, 0)
        for modifier in reversed(modifiers): input_event('key', modifier, 0)

    def clipboard(text):
        subprocess.run(['wl-copy', '--seat', 'mineral-test', '--type', 'text/plain'],
                       input=text, env=env, text=True, check=True, timeout=5)

    def nodes():
        probe = subprocess.run(['/usr/bin/python3', str(probe_path), str(pid)],
                               env=env, capture_output=True, text=True, timeout=20)
        if probe.returncode:
            raise RuntimeError(f'Recovery accessibility probe failed: {probe.stderr}')
        return json.loads(probe.stdout)['nodes']

    def wait(predicate, label, timeout=8):
        deadline = time.monotonic() + timeout
        while True:
            value = predicate()
            if value: return value
            if time.monotonic() >= deadline: raise RuntimeError(f'Recovery check timed out: {label}')
            time.sleep(0.05)

    def capture(label):
        directory = work / f'recovery-{label}'
        directory.mkdir()
        subprocess.run(['weston-screenshooter'], cwd=directory, env=env, check=True, timeout=10)
        screenshot, = directory.glob('*.png')
        shutil.copyfile(screenshot, output.with_name(f'{output.stem}-{label}.png'))

    key(33, (29,))  # Normal editor blur before the fault; let Find finish resizing.
    time.sleep(0.3)
    before = nodes()
    if not is_grid(before): raise RuntimeError('Fault test must start from a visible three-column grid')
    capture('before')
    key(25 if kind == 'panic' else 20, (29, 56, 42))  # Ctrl+Alt+Shift+P/T: one diagnostic replan
    wait(lambda: f'armed-{kind}' in events(), 'validation action (requires layout-validation build)')
    if kind == 'timeout':
        wait(lambda: 'holding-timeout' in events(), 'owned worker hold')

    after = wait(lambda: (n if is_stack(n := nodes()) else None), 'ready stack publication', 14)
    if [n['path'] for n in items(before)] != [n['path'] for n in items(after)]:
        raise RuntimeError('Recovery replaced canonical accessibility identities')
    if source_path.read_bytes() != original: raise RuntimeError('Recovery changed Markdown')
    if kind == 'timeout' and 'released-timeout' in events():
        raise RuntimeError('Fallback was not observed before actual worker completion')
    capture('fallback')
    if kind == 'timeout':
        wait(lambda: 'discarded-late-result' in events(), 'late result rejection', 12)
        after_late = nodes()
        if not is_stack(after_late): raise RuntimeError('Late result replaced the stack')
        if [n['bounds'] for n in items(after)] != [n['bounds'] for n in items(after_late)]:
            raise RuntimeError('Late result moved recovered items')
        capture('late')

    def find(text):
        key(33, (29,))
        key(30, (29,))
        clipboard(text)
        key(47, (29,))
        wait(lambda: any(n['name'] == '1 / 1' for n in nodes()), 'find result')
        key(1)  # Selected match remains selected when Find closes.

    find('Native recovery fixture')
    key(30, (29,))
    clipboard('recovery-copy-sentinel')
    key(46, (29,))
    def copied():
        result = subprocess.run(['wl-paste', '--no-newline', '--seat', 'mineral-test'],
                                env=env, capture_output=True, text=True, timeout=5)
        return result.stdout if result.returncode == 0 and result.stdout != 'recovery-copy-sentinel' else None
    text = wait(copied, 'source-order copy')
    if any(text.count(label) != 1 for label in LABELS) or [text.index(label) for label in LABELS] != sorted(text.index(label) for label in LABELS):
        raise RuntimeError('Recovered list copy lost, duplicated or reordered content')
    if source_path.read_bytes() != original: raise RuntimeError('Selection/copy changed Markdown')
    find('Recovery edit target')
    key(45)
    expected = original.replace(b'Recovery edit target', b'x')
    wait(lambda: source_path.read_bytes() == expected, 'exact target edit and autosave')
    key(44, (29,))
    wait(lambda: source_path.read_bytes() == original, 'exact undo')
    return dict(passes=True, fault=kind, before=items(before), fallback=items(after),
                source_unchanged=True, native_copy_order=True, exact_edit_and_undo=True,
                fallback_before_worker_completion=kind == 'timeout', events=events(),
                screenshots_require_visual_review=True)
