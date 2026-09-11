"""Inspect title-bar states using only the capture harness's private native seat."""
import json
import shutil
import subprocess
import time


def window_controls(env, input_event, app, source_path, probe_path, output, work):
    """Exercise the app's actual keyboard buttons under a desktop compositor."""
    if not env.get('TACHYON_PRIVATE_ATSPI_BUS') or env.get('DBUS_SESSION_BUS_ADDRESS') != env['TACHYON_PRIVATE_ATSPI_BUS']:
        raise RuntimeError('Window-control inspection requires the private accessibility bus')
    original = source_path.read_bytes()
    bar_origins = {}

    def capture(label):
        from PIL import Image
        folder = work / ('window-controls-' + label)
        folder.mkdir()
        subprocess.run(['weston-screenshooter'], cwd=folder, env=env, check=True, timeout=10)
        image, = folder.glob('*.png')
        shutil.copyfile(image, output.with_name(output.stem + '-' + label + '.png'))
        with Image.open(image) as shot:
            rgb = shot.convert('RGB')
            pixels = rgb.load()
            rows = [(y, [x for x in range(shot.width) if pixels[x, y] == (39, 43, 46)])
                    for y in range(shot.height)]
            title_rows = [(y, xs) for y, xs in rows if len(xs) > 500]
            if title_rows:
                bar_origins[label] = (min(min(xs) for _, xs in title_rows), title_rows[0][0])
            return sum(count for count, color in rgb.getcolors(shot.width * shot.height)
                       if color == (39, 43, 46))

    def nodes():
        result = subprocess.run(['/usr/bin/python3', str(probe_path), str(app.pid)],
                                env=env, capture_output=True, text=True, timeout=20)
        if result.returncode:
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout)['nodes']

    def key(code, modifier=None):
        if modifier is not None: input_event('key', modifier, 1)
        input_event('key', code, 1)
        input_event('key', code, 0)
        if modifier is not None: input_event('key', modifier, 0)

    def wait_for(predicate, description):
        deadline = time.monotonic() + 5
        while True:
            current = nodes()
            if predicate(current): return current
            if time.monotonic() > deadline:
                raise RuntimeError(description)
            time.sleep(.1)

    def focus(name):
        key(33, 29)  # Ctrl+F gives a known focus anchor.
        for _ in range(16):
            key(15, 42)
            if any(n['name'] == name and 'focused' in n.get('states', []) for n in nodes()):
                return
        raise RuntimeError(f'Cannot reach {name} with keyboard')

    def close_bounds(current):
        node, = (n for n in current if n['role'] == 'button' and n['name'] == 'Close window')
        return node['screen_bounds']

    def pointer_button(name, capture_label):
        current = nodes()
        menu, = (n['screen_bounds'] for n in current if n['role'] == 'button' and n['name'] == 'Application menu')
        target, = (n['screen_bounds'] for n in current if n['role'] == 'button' and n['name'] == name)
        x, y = bar_origins[capture_label]
        # Wayland AT-SPI coordinates are window-local. Anchor them to the
        # screenshot's title-bar origin, including the client decoration inset.
        input_event('move', round(x + 8 - menu['x'] + target['x'] + target['width'] / 2),
                    round(y + 2 - menu['y'] + target['y'] + target['height'] / 2))
        input_event('button', 272, 1)
        input_event('button', 272, 0)

    startup = nodes()
    if not any(n['name'] == 'Restore window' for n in startup):
        raise RuntimeError('App did not start maximized')
    startup_bounds = close_bounds(startup)
    capture('startup-maximized')
    focus('Restore window')
    key(57)
    initial = close_bounds(wait_for(
        lambda ns: any(n['name'] == 'Maximize window' for n in ns),
        'Startup restore did not publish Maximize window'))
    if initial['x'] >= startup_bounds['x']:
        raise RuntimeError('Startup restore did not recover the normal window width')
    focus('Maximize window')
    key(28)  # Enter activates the actual title-bar button.
    maximized = wait_for(lambda ns: any(n['name'] == 'Restore window' for n in ns),
                         'Maximize did not publish Restore window')
    expanded = close_bounds(maximized)
    capture('maximized')
    if expanded['x'] <= initial['x']:
        raise RuntimeError(f'Maximize did not expand window geometry: {initial} -> {expanded}')
    focus('Restore window')
    key(57)
    restored = wait_for(lambda ns: any(n['name'] == 'Maximize window' for n in ns)
                       and close_bounds(ns) == initial, 'Restore did not restore original geometry')
    visible_pixels = capture('restored')
    x, y = bar_origins['restored']
    input_event('move', x + 500, y + 16)
    input_event('button', 272, 1)
    input_event('move', x + 498, y + 16)
    time.sleep(.1)
    # A window restored after maximized startup may be placed near the top-left
    # by the compositor. Drag into the output, away from its clamped edges.
    input_event('move', x + 660, y + 96)
    input_event('button', 272, 0)
    time.sleep(.3)
    capture('moved')
    moved_x, moved_y = bar_origins['moved']
    if not (150 <= moved_x - x <= 165 and 75 <= moved_y - y <= 85):
        raise RuntimeError(f'Blank title-bar drag did not move the native window: {(x,y)} -> {(moved_x,moved_y)}')
    focus('Minimize window')
    key(28)
    time.sleep(.5)
    hidden_pixels = capture('minimized')
    if visible_pixels < 500 or hidden_pixels >= visible_pixels / 10:
        raise RuntimeError(f'Minimize did not remove the visible title bar: {visible_pixels} -> {hidden_pixels}')
    key(15, 125)  # Weston's default binding-modifier is Super.
    time.sleep(.5)
    returned_pixels = capture('reactivated')
    if returned_pixels < visible_pixels * .9:
        raise RuntimeError('Desktop task switching did not reactivate the minimized window')
    pointer_button('Maximize window', 'reactivated')
    wait_for(lambda ns: any(n['name'] == 'Restore window' for n in ns)
             and close_bounds(ns) == expanded, 'Pointer Maximize failed')
    capture('pointer-maximized')
    pointer_button('Restore window', 'pointer-maximized')
    wait_for(lambda ns: any(n['name'] == 'Maximize window' for n in ns)
             and close_bounds(ns) == initial, 'Pointer Restore failed')
    capture('pointer-restored')
    pointer_button('Minimize window', 'pointer-restored')
    time.sleep(.5)
    if capture('pointer-minimized') >= visible_pixels / 10:
        raise RuntimeError('Pointer Minimize failed')
    key(15, 125)
    time.sleep(.5)
    if capture('pointer-reactivated') < visible_pixels * .9:
        raise RuntimeError('Pointer-minimized window did not reactivate')
    focus('Close window')
    key(28)
    deadline = time.monotonic() + 5
    while app.poll() is None and time.monotonic() < deadline:
        time.sleep(.05)
    if app.poll() != 0 or source_path.read_bytes() != original:
        raise RuntimeError('Keyboard Close did not exit cleanly with source intact')
    return dict(startup_maximized=True, startup_restore=True,
                maximize_enter=True, restore_space=True, minimize_enter=True,
                pointer_maximize=True, pointer_restore=True, pointer_minimize=True,
                blank_title_bar_drag=True, title_bar_origins=bar_origins,
                task_switch_reactivation=True, keyboard_close=True,
                title_bar_pixels=[visible_pixels, hidden_pixels, returned_pixels],
                initial_close_bounds=initial, maximized_close_bounds=expanded,
                restored_close_bounds=close_bounds(restored), source_unchanged=True, passes=True)


def inspect(env, input_event, source_path, pid, output, probe_path, work, expected_scale):
    if not env.get('TACHYON_PRIVATE_ATSPI_BUS') or env.get('DBUS_SESSION_BUS_ADDRESS') != env['TACHYON_PRIVATE_ATSPI_BUS']:
        raise RuntimeError('Title-bar inspection requires the private accessibility bus')
    original = source_path.read_bytes()

    def nodes():
        result = subprocess.run(['/usr/bin/python3', str(probe_path), str(pid)],
                                env=env, capture_output=True, text=True, timeout=20)
        if result.returncode:
            raise RuntimeError(result.stderr)
        return json.loads(result.stdout)['nodes']

    def key(code, control=False, shift=False):
        if control: input_event('key', 29, 1)
        if shift: input_event('key', 42, 1)
        input_event('key', code, 1)
        input_event('key', code, 0)
        if shift: input_event('key', 42, 0)
        if control: input_event('key', 29, 0)

    def capture(label):
        folder = work / ('titlebar-' + label)
        folder.mkdir()
        subprocess.run(['weston-screenshooter'], cwd=folder, env=env, check=True, timeout=10)
        image, = folder.glob('*.png')
        shutil.copyfile(image, output.with_name(output.stem + '-' + label + '.png'))

    initial = nodes()
    names = ['Application menu', 'Search document', 'Zoom out', 'Reset zoom', 'Zoom in',
             'Minimize window', 'Maximize window', 'Close window']
    controls = {}
    for name in names:
        matches = [n for n in initial if n['role'] == 'button' and n['name'] == name]
        if len(matches) != 1:
            raise RuntimeError(f'Expected one named title-bar button: {name}')
        controls[name] = matches[0]
    # AT-SPI reports device coordinates; this harness's input client uses logical
    # compositor coordinates. Derive their ratio from the fixed 28px hit target.
    scale = controls['Search document']['screen_bounds']['width'] / 28
    if abs(scale - expected_scale / 120) > .02:
        raise RuntimeError(f'Effective title-bar scale {scale} differs from requested {expected_scale}/120')
    zoom = int(controls['Reset zoom']['description'].split(': ')[1].rstrip('%'))
    disabled_names = []
    for name, expected_disabled in [('Zoom out', zoom == 75), ('Zoom in', zoom == 200)]:
        disabled = 'enabled' not in controls[name]['states']
        if disabled != expected_disabled:
            raise RuntimeError(f'{name} availability is wrong at {zoom}%: {controls[name]["states"]}')
        if disabled:
            disabled_names.append(name)
            b = controls[name]['screen_bounds']
            input_event('move', round((b['x'] + b['width'] / 2) / scale),
                        round((b['y'] + b['height'] / 2) / scale))
            input_event('button', 272, 1)
            input_event('button', 272, 0)
            time.sleep(.1)
            reset, = (n for n in nodes() if n['role'] == 'button' and n['name'] == 'Reset zoom')
            if reset['description'] != controls['Reset zoom']['description']:
                raise RuntimeError('Disabled zoom control changed the document scale')
    bounds = controls['Search document']['screen_bounds']
    center = ((bounds['x'] + bounds['width'] / 2) / scale,
              (bounds['y'] + bounds['height'] / 2) / scale)
    input_event('move', 3, 60)
    capture('normal')
    input_event('move', round(center[0]), round(center[1]))
    time.sleep(.15)
    capture('hover')
    input_event('button', 272, 1)
    time.sleep(.15)
    capture('pressed')
    from PIL import Image
    # Compare the button interior, excluding the glyph and pointer. This catches
    # event interception that leaves a held button painted only as hovered.
    samples = {}
    for label in ('normal', 'hover', 'pressed'):
        with Image.open(output.with_name(output.stem + '-' + label + '.png')) as shot:
            samples[label] = shot.convert('RGB').getpixel(
                (round(center[0] - 7), round(center[1] + 7)))
    if len(set(samples.values())) != 3:
        raise RuntimeError(f'Title-bar normal/hover/pressed feedback is not distinct: {samples}')
    input_event('move', 3, 60)
    input_event('button', 272, 0)
    key(33, control=True)
    key(15, shift=True)
    deadline = time.monotonic() + 3
    while True:
        focused = [n for n in nodes() if 'focused' in n.get('states', [])]
        if any(n['name'] == 'Close window' for n in focused):
            break
        if time.monotonic() > deadline:
            raise RuntimeError(f'Shift+Tab did not reach Close window: {focused}')
        time.sleep(.05)
    capture('focus')
    for _ in range(16):
        if any(n['name'] == 'Application menu' and 'focused' in n.get('states', []) for n in nodes()):
            break
        key(15, shift=True)
    else:
        raise RuntimeError('Application menu is not reachable by keyboard')
    key(57)
    deadline = time.monotonic() + 3
    while True:
        if any(n['name'] == 'New document' for n in nodes()):
            break
        if time.monotonic() > deadline:
            raise RuntimeError('Space did not open the application menu')
        time.sleep(.05)
    capture('menu')
    key(1)
    deadline = time.monotonic() + 3
    while any(n['name'] == 'New document' for n in nodes()):
        if time.monotonic() > deadline:
            raise RuntimeError('Escape did not dismiss the application menu')
        time.sleep(.05)
    menu_bounds = controls['Application menu']['screen_bounds']
    input_event('move', round((menu_bounds['x'] + menu_bounds['width'] / 2) / scale),
                round((menu_bounds['y'] + menu_bounds['height'] / 2) / scale))
    input_event('button', 272, 1)
    input_event('button', 272, 0)
    deadline = time.monotonic() + 3
    while not any(n['name'] == 'New document' for n in nodes()):
        if time.monotonic() > deadline:
            raise RuntimeError('Pointer did not open the application menu')
        time.sleep(.05)
    key(1)
    key(33, control=True)
    key(1)
    def pointer_title(name):
        target, = (n for n in nodes() if n['role'] == 'button' and n['name'] == name)
        b = target['screen_bounds']
        input_event('move', round((b['x'] + b['width'] / 2) / scale),
                    round((b['y'] + b['height'] / 2) / scale))
        input_event('button', 272, 1)
        input_event('button', 272, 0)

    def keyboard_title(name, code):
        key(33, control=True)
        key(1)
        for _ in range(24):
            key(15, shift=True)
            if any(n['name'] == name and 'focused' in n['states'] for n in nodes()):
                key(code)
                return
        raise RuntimeError(f'{name} cannot be activated from keyboard')

    def require_zoom(percent):
        deadline = time.monotonic() + 3
        while True:
            reset, = (n for n in nodes() if n['role'] == 'button' and n['name'] == 'Reset zoom')
            if reset['description'] == f'Current document zoom: {percent}%': return
            if time.monotonic() > deadline: raise RuntimeError(f'Zoom did not become {percent}%')
            time.sleep(.05)

    pointer_title('Reset zoom')
    require_zoom(100)
    keyboard_title('Zoom in', 28)
    require_zoom(110)
    keyboard_title('Zoom out', 57)
    require_zoom(100)
    pointer_title('Zoom out')
    require_zoom(90)
    pointer_title('Zoom in')
    require_zoom(100)
    pointer_title('Zoom in')
    require_zoom(110)
    keyboard_title('Reset zoom', 57)
    require_zoom(100)
    keyboard_title('Search document', 57)
    if not any(n['name'] == 'Close find' for n in nodes()):
        raise RuntimeError('Keyboard Search did not open find')
    key(1)
    for _ in range(3 if zoom == 75 else abs(zoom - 100) // 10):
        key(12 if zoom < 100 else 13, control=True)
    require_zoom(zoom)
    if source_path.read_bytes() != original:
        raise RuntimeError('Title-bar interaction changed the document')
    return dict(named_controls=names, control_bounds={k: v['screen_bounds'] for k, v in controls.items()},
                observed_coordinate_scale=scale, keyboard_close_focus=True,
                keyboard_menu=True, pointer_menu=True, feedback_samples=samples, disabled_controls=disabled_names,
                pointer_and_keyboard_zoom=True, keyboard_search=True,
                source_unchanged=True)


def unsaved_close(choice, env, input_event, app, source_path, probe_path, output, work):
    """Keep a private edit unsaved, cancel Close, then save or discard it."""
    if not env.get('TACHYON_PRIVATE_ATSPI_BUS') or env.get('DBUS_SESSION_BUS_ADDRESS') != env['TACHYON_PRIVATE_ATSPI_BUS']:
        raise RuntimeError('Unsaved-close inspection requires the private accessibility bus')
    original = source_path.read_bytes()
    expected = original.replace(b'# Find without editing', b'# xFind without editing', 1)
    if expected == original:
        raise RuntimeError('Unexpected unsaved-close fixture')
    parent_mode = source_path.parent.stat().st_mode & 0o777

    def nodes():
        result = subprocess.run(['/usr/bin/python3', str(probe_path), str(app.pid)],
                                env=env, capture_output=True, text=True, timeout=20)
        if result.returncode: raise RuntimeError(result.stderr)
        return json.loads(result.stdout)['nodes']

    def key(code, modifier=None):
        if modifier is not None: input_event('key', modifier, 1)
        input_event('key', code, 1)
        input_event('key', code, 0)
        if modifier is not None: input_event('key', modifier, 0)

    def capture(label):
        folder = work / ('unsaved-close-' + label)
        folder.mkdir()
        subprocess.run(['weston-screenshooter'], cwd=folder, env=env, check=True, timeout=10)
        image, = folder.glob('*.png')
        shutil.copyfile(image, output.with_name(output.stem + '-' + label + '.png'))

    def close():
        key(33, 29)
        for _ in range(20):
            key(15, 42)
            if any(n['name'] == 'Close window' and 'focused' in n.get('states', []) for n in nodes()):
                key(28)
                time.sleep(.3)
                return
        raise RuntimeError('Cannot focus title-bar Close')

    def select(name):
        for _ in range(15):
            current = nodes()
            if any(n['name'] == name and n['role'] == 'button' and 'focused' in n.get('states', []) for n in current):
                capture('focus-' + name.lower())
                key(57)
                time.sleep(.2)
                return
            key(15)
        raise RuntimeError(f'Close prompt has no keyboard-reachable {name} button: {[(n["role"],n["name"]) for n in current if n["name"] in ("Save","Discard","Cancel")]}')

    try:
        # Prevent atomic autosave only in this harness-owned fixture directory.
        source_path.parent.chmod(0o555)
        key(33, 29)
        key(1)
        key(102, 29)
        key(45)
        time.sleep(1.5)
        if source_path.read_bytes() != original:
            raise RuntimeError('Private autosave prevention failed')
        capture('save-error')
        close()
        capture('prompt')
        key(45)  # Typing while modal must not edit the document behind it.
        select('Cancel')
        if app.poll() is not None or source_path.read_bytes() != original:
            raise RuntimeError('Cancel closed the app or changed source')
        close()
        key(1)
        time.sleep(.3)
        if app.poll() is not None or any(n['name'] == 'Cancel' and n['role'] == 'button' for n in nodes()):
            raise RuntimeError('Escape did not cancel the close dialog')
        close()
        if choice == 'save': source_path.parent.chmod(parent_mode)
        select('Save' if choice == 'save' else 'Discard')
        deadline = time.monotonic() + 8
        while app.poll() is None and time.monotonic() < deadline: time.sleep(.05)
        if app.poll() != 0: raise RuntimeError('Confirmed close did not finish')
        if source_path.read_bytes() != (expected if choice == 'save' else original):
            raise RuntimeError('Close save/discard changed unexpected source bytes')
        return dict(cancel_preserves_unsaved=True, escape_cancels=True,
                    choice=choice, exact_source=True, passes=True)
    finally:
        source_path.parent.chmod(parent_mode)
