"""Native keyboard checks for the private recovery/conflict status row."""
import hashlib
import json
import shutil
import subprocess
import time


def recovered(source):
    return source.replace('# Find without editing', '# Recovered draft', 1)


def seed_recovery(source, work):
    key = hashlib.sha256(b'mineral-recovery-v1\0' + bytes(source)).hexdigest()
    directory = work / 'state/tachyon/recovery'
    directory.mkdir(parents=True)
    record = dict(source_path=str(source), revision=1, markdown=recovered(source.read_text()),
                  base_identity=None, written_at_unix_ms=int(time.time() * 1000))
    (directory / f'v1-{key}.json').write_text(json.dumps(record))


def check(choice, env, input_event, pid, source, probe, output, work):
    if not env.get('MINERAL_PRIVATE_ATSPI_BUS') or env.get('DBUS_SESSION_BUS_ADDRESS') != env['MINERAL_PRIVATE_ATSPI_BUS']:
        raise RuntimeError('Status inspection requires the private accessibility bus')
    original = source.read_text()
    draft = recovered(original)

    def nodes():
        result = subprocess.run(['/usr/bin/python3', str(probe), str(pid)], env=env,
                                capture_output=True, text=True, timeout=20)
        if result.returncode: raise RuntimeError(result.stderr)
        return json.loads(result.stdout)['nodes']

    def key(code, modifier=None):
        if modifier is not None: input_event('key', modifier, 1)
        input_event('key', code, 1)
        input_event('key', code, 0)
        if modifier is not None: input_event('key', modifier, 0)

    def capture(label):
        directory = work / ('status-' + label)
        directory.mkdir()
        subprocess.run(['weston-screenshooter'], cwd=directory, env=env, check=True, timeout=10)
        image, = directory.glob('*.png')
        shutil.copyfile(image, output.with_name(output.stem + '-' + label + '.png'))

    def focus(name):
        key(33, 29)
        seen = []
        for _ in range(24):
            key(15, 42)
            current = nodes()
            seen.append([(n['role'], n['name']) for n in current if 'focused' in n['states']])
            if any(n['role'] == 'button' and n['name'] == name and 'focused' in n['states'] for n in current):
                return
        raise RuntimeError(f'{name} is not keyboard reachable: {seen}')

    def wait_heading(name):
        deadline = time.monotonic() + 5
        while True:
            current = nodes()
            if any(n['role'] == 'heading' and n['name'] == name for n in current): return current
            if time.monotonic() > deadline: raise RuntimeError(f'Missing heading after status action: {name}')
            time.sleep(.1)

    focus('Restore')
    capture('restore-focus')
    key(57)
    current = wait_heading('Recovered draft')
    if source.read_text() != original: raise RuntimeError('Restore overwrote disk before confirmation')
    for name in ('Reload', 'Save copy', 'Overwrite'):
        button, = (n for n in current if n['role'] == 'button' and n['name'] == name)
        if 'click' not in button['actions']: raise RuntimeError(f'{name} has no accessible action')
        focus(name)
    capture('conflict-focus')
    focus('Reload' if choice == 'reload' else 'Overwrite')
    key(28)
    expected = original if choice == 'reload' else draft
    deadline = time.monotonic() + 5
    while time.monotonic() < deadline:
        current = nodes()
        if source.read_text() == expected and not any(n['role'] == 'button' and n['name'] == 'Overwrite' for n in current): break
        time.sleep(.1)
    else: raise RuntimeError(f'{choice} did not resolve the conflict with exact source')
    wait_heading('Find without editing' if choice == 'reload' else 'Recovered draft')
    capture('resolved')
    return dict(restore_keyboard=True, conflict_buttons_keyboard=True,
                choice=choice, exact_source=True, passes=True)
