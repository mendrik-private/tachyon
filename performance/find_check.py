"""Native find checks on the harness-owned seat and accessibility bus only."""
import json
import shutil
import subprocess
import time


def validate_controls(nodes):
    obsolete = {'HTML', 'Edit text', 'Copy fragment text', 'Copy original HTML'}
    if any(n['role'] == 'button' and n['name'] in obsolete for n in nodes):
        raise RuntimeError('Rendered HTML must use ordinary selection and typing, without an action toolbar')
    for name in ('Previous result', 'Next result', 'Close find'):
        controls = [n for n in nodes if n['role'] == 'button' and n['name'] == name]
        if len(controls) != 1 or 'click' not in controls[0].get('actions', []):
            raise RuntimeError(f'Find command needs one named, actionable native button: {name}')


def check(env, input_event, source_path, pid, output, probe_path, work, viewport):
    if env.get('DBUS_SESSION_BUS_ADDRESS') != env.get('MINERAL_PRIVATE_ATSPI_BUS') or not env.get('MINERAL_PRIVATE_ATSPI_BUS'):
        raise RuntimeError('Find checks require the private accessibility bus')
    original = source_path.read_bytes()

    def key(code, control=False, shift=False):
        if control: input_event('key', 29, 1)
        if shift: input_event('key', 42, 1)
        input_event('key', code, 1)
        input_event('key', code, 0)
        if shift: input_event('key', 42, 0)
        if control: input_event('key', 29, 0)

    def clipboard(text):
        subprocess.run(['wl-copy', '--seat', 'mineral-test', '--type', 'text/plain'],
                       input=text, env=env, text=True, check=True, timeout=5)

    def probe_nodes():
        probe = subprocess.run(['/usr/bin/python3', str(probe_path), str(pid)],
                               env=env, capture_output=True, text=True, timeout=20)
        if probe.returncode:
            raise RuntimeError(f'Find accessibility probe failed ({probe.returncode}): {probe.stderr}')
        return json.loads(probe.stdout)['nodes']

    def pointer_button(name, nodes):
        matches = [n for n in nodes if n['role'] == 'button' and n['name'] == name]
        if len(matches) != 1 or 'click' not in matches[0].get('actions', []):
            raise RuntimeError(f'Expected one actionable button: {name}')
        bounds = matches[0]['screen_bounds']
        if bounds['width'] < 20 or bounds['height'] < 20:
            raise RuntimeError(f'Button target too small: {name}: {bounds}')
        # The adapter's frame node has unknown (-1) extents. The private kiosk
        # window occupies the known compositor output at origin (0, 0).
        if not (bounds['x'] >= 0 and bounds['y'] >= 0
                and bounds['x'] + bounds['width'] <= viewport[0]
                and bounds['y'] + bounds['height'] <= viewport[1]):
            raise RuntimeError(f'Button lies outside native output: {name}: {bounds}')
        input_event('move', bounds['x'] + bounds['width'] // 2,
                    bounds['y'] + bounds['height'] // 2)
        input_event('button', 272, 1)
        input_event('button', 272, 0)

    def status(expected):
        deadline = time.monotonic() + 8
        while True:
            nodes = probe_nodes()
            if any(n['name'] == expected and n['role'] in ('statusbar', 'status bar', 'status') for n in nodes):
                if source_path.read_bytes() != original:
                    raise RuntimeError('Finding text changed document bytes')
                return nodes
            if time.monotonic() >= deadline:
                raise RuntimeError(f'Find status missing: {expected}; statuses: {[n for n in nodes if "status" in n["role"]]}')
            time.sleep(0.05)

    def query(text):
        key(33, control=True)  # Ctrl+F
        key(30, control=True)  # Ctrl+A in find input
        clipboard(text)
        key(47, control=True)  # Ctrl+V

    def copied(expected):
        clipboard('find-test-sentinel')
        key(46, control=True)  # Ctrl+C
        deadline = time.monotonic() + 3
        while True:
            result = subprocess.run(['wl-paste', '--no-newline', '--seat', 'mineral-test'],
                                    env=env, text=True, capture_output=True, timeout=5)
            if result.returncode == 0 and result.stdout == expected:
                return
            if time.monotonic() > deadline:
                raise RuntimeError(f'Find selection copy mismatch: expected {expected!r}, got {result.stdout!r}')
            time.sleep(0.05)

    def capture_match(label):
        capture = work / f'find-{label}-capture'
        capture.mkdir()
        subprocess.run(['weston-screenshooter'], cwd=capture, env=env, check=True, timeout=10)
        screenshot, = capture.glob('*.png')
        shutil.copyfile(screenshot, output.with_name(f'{output.stem}-{label}.png'))

    if source_path.name == '42-find-overflow.md':
        targets = [('table-right', 'right table target'), ('table-left', 'left table target'),
                   ('code', 'right_code_target'), ('html', 'right html target')]
        for label, target in targets:
            query(target)
            validate_controls(status('1 / 1'))
            capture_match(label)
            key(1)
            copied(target)
        key(45)  # Replace selected HTML target, not the prior code/table caret.
        deadline = time.monotonic() + 5
        while source_path.read_bytes() == original and time.monotonic() < deadline:
            time.sleep(0.05)
        edited = source_path.read_bytes()
        if edited == original or b'right html target' in edited:
            raise RuntimeError('Scrolled HTML selection did not receive the edit')
        prefix = original.split(b'<details open>', 1)[0]
        suffix = original[original.index(b'\n\n## After the wide blocks'):]
        if not edited.startswith(prefix) or not edited.endswith(suffix):
            raise RuntimeError('Scrolled HTML edit changed neighboring content')
        key(44, control=True)
        deadline = time.monotonic() + 5
        while source_path.read_bytes() != original and time.monotonic() < deadline:
            time.sleep(0.05)
        if source_path.read_bytes() != original:
            raise RuntimeError('Scrolled HTML edit did not undo exactly')
        return dict(passes=True, exact_native_copy=True, html_first_edit_and_exact_undo=True,
                    source_unchanged=True, captured_targets=[label for label, _ in targets],
                    visibility_requires_visual_review=True)

    # Open through the title bar, without Ctrl+F: pasting must reach its input.
    pointer_button('Search document', probe_nodes())
    clipboard('needle')
    key(47, control=True)
    controls = status('1 / 7')
    pointer_button('Next result', controls)
    controls = status('2 / 7')
    pointer_button('Previous result', controls)
    controls = status('1 / 7')
    capture_match('title-search')
    input_event('move', 5, 5)
    key(15)  # Tab from query to Previous result.
    deadline = time.monotonic() + 3
    while True:
        focused = [n for n in probe_nodes() if 'focused' in n.get('states', [])]
        if any(n['role'] == 'button' and n['name'] == 'Previous result' for n in focused):
            break
        if time.monotonic() > deadline:
            raise RuntimeError(f'Tab did not focus Previous result: {focused}')
        time.sleep(0.05)
    capture_match('keyboard-focus')
    pointer_button('Close find', controls)
    copied('Needle')
    query('needle')
    controls = status('1 / 7')
    validate_controls(controls)
    key(28)  # Enter
    status('2 / 7')
    key(28, shift=True)
    status('1 / 7')
    key(28, shift=True)
    status('7 / 7')
    key(28)
    status('1 / 7')
    key(28, shift=True)
    status('7 / 7')
    key(1)  # Escape back to document
    copied('NEEDLE')
    query('deep needle')
    nodes = status('1 / 1')
    for name in ('Closed outer', 'Closed inner'):
        matches = [n for n in nodes if n['name'] == name and 'expandable' in n['states']]
        if not matches or not all('expanded' in n['states'] for n in matches):
            raise RuntimeError(f'Find failed to reveal {name}')
    capture_match('html')
    key(1)
    copied('Deep needle')
    key(45)  # Replace actual selected text with x
    deadline = time.monotonic() + 5
    while source_path.read_bytes() == original and time.monotonic() < deadline:
        time.sleep(0.05)
    edited = source_path.read_bytes()
    if edited == original or b'Deep needle' in edited or b'formatted words' not in edited:
        raise RuntimeError('Find-selected HTML did not receive the exact edit')
    prefix, _ = original.split(b'<details>', 1)
    suffix = original[original.index(b'\n\n## Last location'):]
    if not edited.startswith(prefix) or not edited.endswith(suffix):
        raise RuntimeError('Find-selected edit changed neighboring Markdown')
    key(44, control=True)
    deadline = time.monotonic() + 5
    while source_path.read_bytes() != original and time.monotonic() < deadline:
        time.sleep(0.05)
    if source_path.read_bytes() != original:
        raise RuntimeError('One undo failed to restore original HTML and Markdown')
    query('a phrase that does not occur')
    status('No results')
    key(1)
    query('needle')
    status('1 / 7')
    return dict(passes=True, title_bar_pointer_search=True, pointer_match_navigation=True,
                pointer_close_restores_document_focus=True, keyboard_match_focus=True,
                source_order_navigation=True, backward_wrap=True, forward_wrap=True,
                exact_native_copy=True, nested_disclosure_reveal=True,
                html_first_edit_and_exact_undo=True, no_results=True, source_unchanged=True)
