"""Private Wayland table menus, surviving caret, and exact structural history."""
import hashlib
import json
from pathlib import Path
import subprocess
import time

from adaptive_layout_check import _one
from paired_records_check import rows
from resize_layout_check import _capture, caret_visible, prefixed_records


def check(kind, env, input_event, source_path, pid, output, probe_path, work, width, zoom):
    source_path, output, work = map(Path, (source_path, output, work))
    original = source_path.read_bytes()
    log_path = work / "weston.log"

    def key(code, control=False, shift=False):
        modifiers = ([29] if control else []) + ([42] if shift else [])
        for modifier in modifiers:
            input_event("key", modifier, 1)
        input_event("key", code, 1)
        input_event("key", code, 0)
        for modifier in reversed(modifiers):
            input_event("key", modifier, 0)

    def probe():
        result = subprocess.run(["/usr/bin/python3", str(probe_path), str(pid)], env=env,
                                capture_output=True, text=True, check=True, timeout=30)
        return json.loads(result.stdout)["nodes"]

    def state():
        prefix = "MINERAL_RESIZE_STATE "
        count = len(prefixed_records(log_path.read_bytes(), prefix))
        key(66)
        deadline = time.monotonic() + 5
        while time.monotonic() < deadline:
            reports = prefixed_records(log_path.read_bytes(), prefix)
            if len(reports) > count:
                return reports[-1]
            time.sleep(.02)
        raise RuntimeError("Native editor-state report did not arrive")

    def visible():
        result = state()
        if not (result["focused"] and result["selection_start"] == result["selection_end"]
                and caret_visible(result)):
            raise RuntimeError(f"Structural history lost its focused visible caret: {result}")
        return result

    def click(x, y, button=272):
        input_event("move", round(x), round(y))
        input_event("button", button, 1)
        input_event("button", button, 0)

    def wait_source(expected):
        deadline = time.monotonic() + 6
        while source_path.read_bytes() != expected:
            if time.monotonic() >= deadline:
                raise RuntimeError("Structural Undo/Redo did not restore exact source bytes")
            time.sleep(.05)

    def names(nodes):
        return [[cell["name"] for cell in row] for row in rows(nodes, "Package directory")]

    baseline = names(probe())
    evidence = []
    for command in ("Insert row above", "Delete row"):
        slug = command.lower().replace(" ", "-")
        marker = "Collects unresolved questions"
        subprocess.run(["wl-copy", "--seat", "mineral-test", "--type", "text/plain"],
                       input=marker, text=True, env=env, check=True, timeout=5)
        key(33, control=True)
        time.sleep(.1)
        key(47, control=True)
        time.sleep(.2)
        key(1)
        key(106)
        time.sleep(.3)
        before = visible()
        nodes = probe()
        viewport = _one(nodes, "entry", "Markdown document editor")[1]["bounds"]
        caret = before["caret_bounds"]
        offset_x = viewport["x"] - before["viewport_bounds"][0]
        offset_y = viewport["y"] - before["viewport_bounds"][1]
        click(caret[0] + offset_x, (caret[1] + caret[3]) / 2 + offset_y, 273)
        time.sleep(.2)
        menu_nodes = probe()
        # The same popup also has an Insert block > Table action. The table
        # operations entry is the submenu, with no direct click action.
        table_entries = [node for node in menu_nodes if node["role"] == "menu item"
                         and node["name"] == "Table" and not node["actions"]]
        if len(table_entries) != 1:
            raise RuntimeError(f"Ambiguous Table menus: {table_entries}")
        table = table_entries[0]["bounds"]
        input_event("move", round(table["x"] + table["width"] / 2), round(table["y"] + table["height"] / 2))
        time.sleep(.4)
        menu = _one(probe(), "menu item", command)[1]["bounds"]
        _capture(env, work, output, slug + "-menu")
        # The current popup can extend beyond the output's trailing edge.
        # Click its exposed native hit area; keep popup placement unqualified.
        menu_x = min(menu["x"] + menu["width"] / 2, width - 5)
        if menu_x < menu["x"]:
            raise RuntimeError("Table submenu is wholly outside the native output")
        click(menu_x, menu["y"] + menu["height"] / 2)
        time.sleep(1.2)
        _capture(env, work, output, slug + "-after")
        after = visible()
        changed = source_path.read_bytes()
        expected = baseline[:2] + ([[''] * 6] + baseline[2:] if command.startswith("Insert") else baseline[3:])
        if names(probe()) != expected or changed == original:
            raise RuntimeError("Table menu changed the wrong rows or cells")
        # Structural serialization may regenerate the table. Every byte
        # outside its authored region must stay untouched.
        prefix, rest = original.split(b"| Service |", 1)
        suffix = rest.split(b"\n\n", 1)[1]
        if not changed.startswith(prefix) or not changed.endswith(b"\n\n" + suffix):
            raise RuntimeError("Structural table edit changed unrelated source")
        _capture(env, work, output, slug)
        key(44, control=True)
        wait_source(original)
        undone = visible()
        if names(probe()) != baseline or undone["selection_start"] != before["selection_start"]:
            raise RuntimeError("Undo did not restore the original table and caret")
        key(44, control=True, shift=True)
        wait_source(changed)
        redone = visible()
        if names(probe()) != expected or redone["selection_start"] != after["selection_start"]:
            raise RuntimeError("Redo did not restore the changed table and caret")
        key(44, control=True)
        wait_source(original)
        visible()
        evidence.append(dict(command=command, before=before, after=after, undone=undone,
                             redone=redone, exact_undo_redo=True,
                             changed_sha256=hashlib.sha256(changed).hexdigest()))
    return dict(passes=True, original_sha256=hashlib.sha256(original).hexdigest(),
                source_unchanged=source_path.read_bytes() == original, operations=evidence)
