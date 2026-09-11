"""Exact-source native pointer check for one explicitly authored task."""

import re
import difflib
import shutil
import subprocess
import time


def toggled_source(original, label):
    pattern = re.compile(rb"(?m)^[ \t]*(?:[-+*]|[0-9]+[.)])[ \t]+\[([ xX])\][ \t]+"
                         + re.escape(label.encode()) + rb"[ \t]*\r?$")
    matches = list(pattern.finditer(original))
    if len(matches) != 1:
        raise RuntimeError("Task label must identify exactly one complete authored task line")
    marker = matches[0].span(1)
    replacement = b"x" if matches[0][1] == b" " else b" "
    return original[:marker[0]] + replacement + original[marker[1]:]


def check_toggle(env, work, output, source, input_event, coordinates, label):
    original = source.read_bytes()
    expected = toggled_source(original, label)
    x, y, end_x, end_y = coordinates
    if (x, y) != (end_x, end_y):
        raise RuntimeError("Task toggle requires a click, not a drag")

    def capture_frame(stage):
        capture = work / f"task-{stage}"
        capture.mkdir()
        subprocess.run(["weston-screenshooter"], cwd=capture, env=env, check=True, timeout=10)
        screenshot, = capture.glob("*.png")
        shutil.copyfile(screenshot, output.with_name(f"{output.stem}-{stage}.png"))

    capture_frame("before")
    input_event("move", x, y)
    time.sleep(0.1)
    input_event("button", 272, 1)
    input_event("button", 272, 0)
    deadline = time.monotonic() + 5
    while source.read_bytes() == original and time.monotonic() < deadline:
        time.sleep(0.05)
    # Capture a settled frame after the completed autosave, not during debounce.
    time.sleep(0.3)
    capture_frame("toggled")
    if source.read_bytes() != expected:
        print("".join(difflib.unified_diff(expected.decode().splitlines(keepends=True),
                                          source.read_text().splitlines(keepends=True),
                                          fromfile="expected", tofile="saved")), flush=True)
        raise RuntimeError("Native checkbox click did not autosave exactly the target task marker"
                           f" (source unchanged: {source.read_bytes() == original})")

    input_event("key", 29, 1)
    input_event("key", 44, 1)
    input_event("key", 44, 0)
    input_event("key", 29, 0)
    deadline = time.monotonic() + 5
    while source.read_bytes() != original and time.monotonic() < deadline:
        time.sleep(0.05)
    if source.read_bytes() != original:
        raise RuntimeError("One native undo did not restore exact original task source")
    return dict(passes=True, label=label, native_checkbox_click=True,
                autosave_changes_only_target_marker=True, unrelated_source_exact=True,
                one_undo_restores_exact_source=True)
