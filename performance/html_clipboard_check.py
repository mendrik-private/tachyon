"""Inspect actual Wayland clipboard formats on the private harness seat."""
import json
import subprocess
import time


def check(env, input_event, source, pid, probe):
    if not env.get('MINERAL_PRIVATE_ATSPI_BUS') or env.get('DBUS_SESSION_BUS_ADDRESS') != env['MINERAL_PRIVATE_ATSPI_BUS']:
        raise RuntimeError('HTML clipboard check requires the private accessibility bus')
    original = source.read_bytes()
    expected_html = '<div><ul><li>One <strong>bold</strong> <a href="https://example.test">link</a><ul><li>Nested</li></ul></li><li>Other</li></ul></div>\n'

    def key(code, control=False, shift=False):
        if control: input_event('key', 29, 1)
        if shift: input_event('key', 42, 1)
        input_event('key', code, 1)
        input_event('key', code, 0)
        if shift: input_event('key', 42, 0)
        if control: input_event('key', 29, 0)

    def nodes():
        result = subprocess.run(['/usr/bin/python3', str(probe), str(pid)], env=env,
                                capture_output=True, text=True, timeout=20, check=True)
        return json.loads(result.stdout)['nodes']

    def wait_for(test, message):
        deadline = time.monotonic() + 8
        while not test():
            if time.monotonic() >= deadline: raise RuntimeError(message)
            time.sleep(0.05)

    def read(mime):
        result = subprocess.run(['wl-paste', '--no-newline', '--seat', 'mineral-test', '--type', mime],
                                env=env, capture_output=True, text=True, timeout=5)
        return result.stdout if result.returncode == 0 else None

    def select(query):
        key(33, control=True)
        key(30, control=True)
        subprocess.run(['wl-copy', '--seat', 'mineral-test', '--type', 'text/plain'],
                       input='clipboard-query-sentinel', text=True, env=env, check=True, timeout=5)
        key(47, control=True)
        wait_for(lambda: any(n['name'] == 'No results' and 'status' in n['role'] for n in nodes()), 'Search did not clear the previous result')
        key(30, control=True)
        subprocess.run(['wl-copy', '--seat', 'mineral-test', '--type', 'text/plain'],
                       input=query, text=True, env=env, check=True, timeout=5)
        key(47, control=True)
        wait_for(lambda: any(n['name'] == '1 / 1' and 'status' in n['role'] for n in nodes()), 'Search did not select HTML text')
        if any(n['role'] == 'button' and n['name'] in ('HTML', 'Edit text', 'Copy fragment text') for n in nodes()):
            raise RuntimeError('Obsolete HTML action buttons remain')
        key(1)

    if source.name == '44-cross-preview-selection.md':
        root_a = '<div><p>Alpha <strong>café</strong></p></div>\n\n'
        root_b = '<div><p>Beta <em>tail</em></p></div>\n\n'
        roots = root_a + '<p>Middle</p>\n' + root_b
        cases = [
            ('café', 105, 106, 3, '**café**\n\nMiddle\n\nBeta', roots),
            ('Beta', 106, 105, 3, '**café**\n\nMiddle\n\nBeta', roots),
            ('café', 105, 106, 5, '**café**\n\nMiddle\n\nBeta *tail*\n\nAfter', roots + '<p>After</p>\n'),
            ('After', 106, 105, 10, '# Cross preview selection\n\nBefore\n\nAlpha **café**\n\nMiddle\n\nBeta *tail*\n\nAfter', '<h1>Cross preview selection</h1>\n<p>Before</p>\n' + roots + '<p>After</p>\n'),
        ]
        def extend(case):
            query, collapse, direction, count, _, _ = case
            select(query)
            key(collapse)
            for _ in range(count): key(direction, control=True, shift=True)
        results = []
        for case in cases:
            extend(case)
            plain, html = case[-2:]
            key(46, control=True)
            wait_for(lambda: read('text/plain') == plain, f'Cross-owner Markdown mismatch: expected {plain!r}')
            wait_for(lambda: read('text/html') == html, 'Cross-owner roots or surrounding Markdown mismatch')
            if source.read_bytes() != original: raise RuntimeError('Cross-owner copy changed source')
            key(44, control=True)
            time.sleep(0.3)
            if source.read_bytes() != original: raise RuntimeError('Cross-owner copy entered undo history')
            results.append(dict(query=case[0], reverse=case[2] == 105, plain=plain, html=html))
        extend(cases[0])
        key(45)
        expected_edit = '# Cross preview selection\n\nBefore\n\nAlpha x *tail*\n\nAfter\n'
        wait_for(lambda: source.read_text() == expected_edit, 'Cross-owner first edit did not match complete save golden')
        key(44, control=True)
        wait_for(lambda: source.read_bytes() == original, 'Cross-owner edit did not restore both exact HTML roots with one Undo')
        return dict(passes=True, native_mime_payloads=results, source_unchanged_on_copy=True,
                    first_edit_converted=True, one_undo_restores_exact_source=True)

    results = []
    for reverse in (False, True):
        for query, plain in [('ol', '**ol**'), ('link', '[link](https://example.test)'),
                             ('bold link', '**bold** [link](https://example.test)'), ('Nested', 'Nested'), ('Other', 'Other')]:
            select(query)
            if reverse:
                key(106)  # Collapse at the selected range's end.
                for _ in query: key(105, shift=True)
            key(46, control=True)
            wait_for(lambda: read('text/plain') == plain, f'Selected Markdown mismatch for {query}: {read("text/plain")!r}')
            wait_for(lambda: read('text/html') == expected_html,
                     f'Canonical root MIME did not arrive for {query} (reverse={reverse})')
            if source.read_bytes() != original: raise RuntimeError('Copy changed source')
            # Copy must add no content history entry. This Undo must leave bytes intact.
            key(44, control=True)
            time.sleep(0.3)
            if source.read_bytes() != original: raise RuntimeError('Copy entered undo history')
            results.append(dict(query=query, reverse=reverse, plain=plain, html=expected_html))
    select('Other')
    # First typing replaces the formerly clipped final item and converts once.
    key(45)
    wait_for(lambda: source.read_bytes() != original, 'Typing did not edit HTML')
    edited = source.read_text()
    expected_edit = '# HTML clipboard\n\n- One **bold** [link](https://example.test)\n    - Nested\n- x\n\nAfter the HTML.\n'
    if edited != expected_edit:
        raise RuntimeError(f'First edit target/conversion mismatch: {edited!r}')
    key(44, control=True)
    wait_for(lambda: source.read_bytes() == original, 'One Undo did not restore original HTML')
    return dict(passes=True, native_mime_payloads=results, source_unchanged_on_copy=True,
                first_edit_converted=True, one_undo_restores_exact_source=True)
