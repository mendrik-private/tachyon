"""Native fixture-36 overflow actions, confined to the caller's private seat."""
import json
import shutil
import subprocess
import time


def check_html_toolbar(input_event, env, source_path, work, output, position, binary_hash):
    original = source_path.read_bytes()
    expected_html = ('<details open=""><summary>Open cell details</summary>'
                     '<p>Visible cell body with <strong>strong text</strong>.</p></details>')
    expected_text = 'Open cell details\nVisible cell body with strong text.\n\n'
    closed = b'<details><summary>Closed cell details</summary><p>Closed cell body marker.</p></details>'
    if original.count(closed) != 1 or original.count(b'<details open>') != 1:
        raise RuntimeError('HTML toolbar oracle requires unmodified synthetic fixture 36')

    def key(code):
        input_event('key', code, 1)
        input_event('key', code, 0)

    def capture(stage):
        input_event('move', 10, 10)
        time.sleep(0.4)
        directory = work / f'toolbar-{stage}'
        directory.mkdir()
        subprocess.run(['weston-screenshooter'], cwd=directory, env=env, check=True, timeout=10)
        screenshot, = directory.glob('*.png')
        shutil.copyfile(screenshot, output.with_name(f'{output.stem}-{stage}.png'))

    def open_menu():
        input_event('move', *position)
        input_event('button', 272, 1)
        input_event('button', 272, 0)
        time.sleep(0.25)
        if source_path.read_bytes() != original:
            raise RuntimeError('Opening the HTML actions changed authored source')

    for index, expected in enumerate((expected_html, expected_text)):
        subprocess.run(['wl-copy', '--seat', 'mineral-test', '--type', 'text/plain'],
                       input='toolbar sentinel', text=True, env=env, check=True, timeout=5)
        open_menu()
        if index == 0:
            capture('menu')
        for _ in range(index + 1):
            key(108)  # Down, then Enter: exercise native menu focus/actions.
        key(28)
        deadline = time.monotonic() + 5
        while True:
            clipboard = subprocess.run(['wl-paste', '--no-newline', '--seat', 'mineral-test'],
                                       env=env, text=True, capture_output=True, timeout=5)
            if (clipboard.returncode == 0 and clipboard.stdout == expected) or time.monotonic() >= deadline:
                break
            time.sleep(0.05)
        if clipboard.returncode or clipboard.stdout != expected:
            raise RuntimeError(f'HTML toolbar copy {index} mismatch: {clipboard.stdout!r}, expected {expected!r}')
        if source_path.read_bytes() != original:
            raise RuntimeError('Copy from HTML toolbar changed source')

    open_menu()
    for _ in range(3):
        key(108)
    key(28)
    deadline = time.monotonic() + 5
    while source_path.read_bytes() == original and time.monotonic() < deadline:
        time.sleep(0.05)
    edited = source_path.read_bytes()
    if (edited == original or b'<details open' in edited or edited.count(closed) != 1
            or edited.count(b'<table>') != 2
            or edited.count(b'<strong>strong text</strong>') != 1
            or edited.count(b'<p>Open details neighbor.</p>') != 1):
        raise RuntimeError('HTML toolbar conversion lost cells, styling or neighboring disclosure source')
    capture('converted')
    input_event('key', 29, 1)
    key(44)  # Ctrl+Z
    input_event('key', 29, 0)
    deadline = time.monotonic() + 5
    while source_path.read_bytes() != original and time.monotonic() < deadline:
        time.sleep(0.05)
    if source_path.read_bytes() != original:
        raise RuntimeError('One undo did not restore exact fixture source')
    report = dict(binary_sha256=binary_hash, copy_html_exact=True, copy_text_exact=True,
                  keyboard_menu_activation=True, neighboring_cell_source_preserved=True,
                  conversion_undo_exact=True, passes=True)
    output.with_suffix('.toolbar.json').write_text(json.dumps(report, indent=2) + '\n')
    return report
