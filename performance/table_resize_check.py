"""Native pointer/source oracle for the fixed-column table in fixture 82."""

import json
import re
import shutil
import subprocess
import time


METADATA = re.compile(rb"<!-- mineral-table:v1 (.*?) -->")


def check_width_edit(original, edited, width):
    before = METADATA.search(original)
    after = METADATA.search(edited)
    if before is None or after is None:
        raise RuntimeError("Missing table width metadata")
    expected = json.loads(before[1])
    if expected["widths"] != [160, 320]:
        raise RuntimeError("Resize oracle requires fixture 82's first fixed table")
    expected["widths"][0] = width
    if json.loads(after[1]) != expected:
        raise RuntimeError(f"Wrong saved table width: {after[1]!r}; expected {expected}")
    # The existing structural table serializer canonically escapes the two
    # terminal periods in fixture 82's edited table. Permit exactly those known
    # spellings, not arbitrary normalization of the document or neighboring tables.
    expected_tail = original[before.end():].replace(
        b"supporting resources together. |", b"supporting resources together\\. |", 1
    ).replace(b"replacing an existing document. |", b"replacing an existing document\\. |", 1)
    if original[:before.start()] + expected_tail != edited[:after.start()] + edited[after.end():]:
        raise RuntimeError("Column resize changed source beyond the target table's exact canonical form")


def check_resize(env, pid, work, output, source, probe, input_event, coordinates, zoom):
    original = source.read_bytes()
    x1, y1, x2, y2 = coordinates
    expected_width = max(32, 160 + (x2 - x1) / zoom)
    if abs(x2 - x1) < 2:
        raise RuntimeError("Resize oracle requires a nonzero drag")

    def screenshot(stage):
        directory = work / f"table-resize-{stage}"
        directory.mkdir()
        subprocess.run(["weston-screenshooter"], cwd=directory, env=env, check=True, timeout=10)
        image, = directory.glob("*.png")
        shutil.copyfile(image, output.with_name(f"{output.stem}-{stage}.png"))

    def key(code):
        input_event("key", code, 1)
        input_event("key", code, 0)

    def start():
        input_event("move", x1, y1)
        input_event("button", 272, 1)
        input_event("move", x2, y2)

    screenshot("before")
    start()
    time.sleep(1.2)  # Longer than autosave debounce: preview must remain clean.
    screenshot("preview")
    if source.read_bytes() != original:
        raise RuntimeError("Provisional drag autosaved before release")
    accessible = json.loads(subprocess.run([
        "/usr/bin/python3", str(probe), str(pid)
    ], env=env, capture_output=True, text=True, check=True, timeout=20).stdout)
    if f"Column width: {expected_width:.0f} px" not in [node["name"] for node in accessible["nodes"]]:
        raise RuntimeError("Native drag did not expose the live logical-width readout")
    key(1)  # Escape while still holding the mouse.
    input_event("button", 272, 0)
    time.sleep(1.2)
    if source.read_bytes() != original:
        raise RuntimeError("Escape followed by release committed the canceled drag")
    screenshot("cancelled")

    start()
    input_event("button", 272, 0)
    deadline = time.monotonic() + 5
    while source.read_bytes() == original and time.monotonic() < deadline:
        time.sleep(0.05)
    edited = source.read_bytes()
    output.with_suffix(".resized.md").write_bytes(edited)
    check_width_edit(original, edited, expected_width)
    time.sleep(1)
    screenshot("resized")
    input_event("key", 29, 1)
    key(44)  # Ctrl+Z: one transaction, regardless of drag duration.
    input_event("key", 29, 0)
    deadline = time.monotonic() + 5
    while source.read_bytes() != original and time.monotonic() < deadline:
        time.sleep(0.05)
    if source.read_bytes() != original:
        raise RuntimeError("One native undo did not restore exact original source")
    return dict(passes=True, screen_delta=x2 - x1, logical_width=expected_width,
                preview_preserves_source=True, live_width_readout=True,
                escape_preserves_source=True, target_table_exact_canonical_source=True,
                unrelated_source_exact=True,
                one_undo_restores_exact_source=True)
