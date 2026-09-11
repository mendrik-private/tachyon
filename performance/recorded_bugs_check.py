"""Targeted native checks for the four accepted September 8 Crusty bugs.

Uses only the capture harness's private compositor, bus, source copy and seat.
First-frame placement is additionally checked synchronously in recorded_bugs.rs;
screenshots here are the first available capture, not a guaranteed first frame.
"""
import hashlib
import json
from pathlib import Path
import shutil
import subprocess
import time

from adaptive_layout_check import _canonical_list_parent, _content_node, _one


def table_rows(nodes, name):
    table, _ = _one(nodes, "table", name)
    rows = []
    for index, node in enumerate(nodes):
        if node["parent"] == table and node["role"] == "table row":
            rows.append([child for child in nodes if child["parent"] == index])
    if not rows or not all(len(row) == len(rows[0]) for row in rows):
        raise RuntimeError(f"Missing or ragged table: {name}")
    for row in rows:
        if max(cell["bounds"]["y"] for cell in row) != min(cell["bounds"]["y"] for cell in row):
            raise RuntimeError(f"Inserted cells are vertically misaligned: {row}")
    for column in zip(*rows):
        starts = [cell["bounds"]["x"] for cell in column]
        widths = [cell["bounds"]["width"] for cell in column]
        if max(starts) - min(starts) > 1 or max(widths) - min(widths) > 1:
            raise RuntimeError(f"Inserted cells do not share column geometry: {column}")
    return rows


def check(kind, env, input_event, source_path, pid, output, probe_path, work, width, zoom):
    source_path, output, work = map(Path, (source_path, output, work))
    original = source_path.read_bytes()

    def probe():
        result = subprocess.run(["/usr/bin/python3", str(probe_path), str(pid)], env=env,
                                capture_output=True, text=True, check=True, timeout=30)
        return json.loads(result.stdout)["nodes"]

    def capture(label):
        directory = work / label
        directory.mkdir()
        subprocess.run(["weston-screenshooter"], cwd=directory, env=env, check=True, timeout=10)
        screenshot, = directory.glob("*.png")
        destination = output.with_name(f"{output.stem}-{label}.png")
        shutil.copyfile(screenshot, destination)
        return str(destination)

    def key(code, control=False, shift=False):
        modifiers = ([29] if control else []) + ([42] if shift else [])
        for modifier in modifiers:
            input_event("key", modifier, 1)
        input_event("key", code, 1)
        input_event("key", code, 0)
        for modifier in reversed(modifiers):
            input_event("key", modifier, 0)

    def click(bounds):
        input_event("move", bounds["x"] + bounds["width"] // 2,
                    bounds["y"] + bounds["height"] // 2)
        input_event("button", 272, 1)
        input_event("button", 272, 0)

    def wait_source(expected):
        deadline = time.monotonic() + 6
        while source_path.read_bytes() != expected:
            if time.monotonic() > deadline:
                raise RuntimeError("Native edit/undo source did not match expected bytes")
            time.sleep(.05)

    nodes = probe()
    evidence = {"before": capture("before"), "nodes": nodes}
    if kind == "cards":
        names = [node["name"] for node in nodes if node["role"] == "paragraph"
                 and node["name"].startswith(("Document core:", "Document view:", "Application:"))]
        records = [_content_node(nodes, name) for name in names]
        if len(records) != 3:
            raise RuntimeError("Expected the three exact plan.md entity cards")
        _canonical_list_parent(nodes, records)
        bounds = [node["bounds"] for _, node in records]
        if width >= 1500 and zoom == 1:
            if len({bound["y"] for bound in bounds}) != 1 or len({bound["x"] for bound in bounds}) != 3:
                raise RuntimeError(f"Reported plan.md group is not three cards: {bounds}")
        if width <= 600 and len({bound["x"] for bound in bounds}) != 1:
            raise RuntimeError("Narrow list did not return to a source-order stack")
        evidence["cards"] = bounds
        # Find selects exact authored content, then native typing and undo must
        # preserve the original list and every unrelated source byte.
        subprocess.run(["wl-copy", "--seat", "mineral-test", "--type", "text/plain"],
                       input="Application:", text=True, env=env, check=True, timeout=5)
        key(33, control=True)
        key(47, control=True)
        time.sleep(.25)
        key(1)
        key(106)
        key(45)
        deadline = time.monotonic() + 6
        while source_path.read_bytes() == original and time.monotonic() < deadline:
            time.sleep(.05)
        edited = source_path.read_bytes()
        output.with_suffix(".edited.md").write_bytes(edited)
        output.with_suffix(".edited.json").write_text(json.dumps(probe(), indent=2) + "\n")
        capture("edited")
        if edited == original:
            raise RuntimeError("Card editing failed")
        edited_nodes = probe()
        for index, name in enumerate(names):
            changed_name = name.replace("Application:", "Application:x", 1) if index == 2 else name
            after = _content_node(edited_nodes, changed_name)[1]["bounds"]
            if any(after[axis] != bounds[index][axis] for axis in ["x", "y"]):
                raise RuntimeError("Card arrangement moved during native typing")
        key(44, control=True)
        wait_source(original)
        evidence["edit_undo_exact"] = True
        evidence["typing_placement_stable"] = True
    elif kind == "table-flow":
        lead = _content_node(nodes, "Use those families and heading characteristics at document-appropriate sizes:")[1]
        tables = [node for node in nodes if node["role"] == "table"]
        if len(tables) != 2:
            raise RuntimeError("Expected both plan.md tables")
        first = tables[0]["bounds"]
        if abs(first["x"] - lead["bounds"]["x"]) > 1 or first["y"] <= lead["bounds"]["y"]:
            raise RuntimeError("The short table lead-in is still stranded beside its table")
        evidence["table_bounds"] = [table["bounds"] for table in tables]
        click(lead["bounds"])
        key(30, control=True)  # Ctrl+A / Ctrl+C through the actual editor.
        key(46, control=True)
        deadline = time.monotonic() + 3
        while True:
            clipboard = subprocess.run(["wl-paste", "--seat", "mineral-test", "--no-newline"],
                                       env=env, capture_output=True, text=True, timeout=5)
            if not clipboard.returncode:
                break
            if time.monotonic() > deadline or "Nothing is copied" not in clipboard.stderr:
                raise RuntimeError(f"Native clipboard publication failed: {clipboard.stderr}")
            time.sleep(.05)
        copied = clipboard.stdout
        markers = ["Use those families", "Body / lead / table cells", "Bundle fonts locally",
                   "The following light palette", "Hover surface"]
        positions = [copied.find(marker) for marker in markers]
        if min(positions) < 0 or positions != sorted(positions):
            raise RuntimeError("Selection through the tables lost source reading order")
        evidence["selection_copy_order"] = markers
        key(106)
    else:
        evidence["insertions"] = []
        for name in (["Inline reference table"] if kind == "knobs" else ["Inline reference table", "Palette table"]):
            for column in ([False] if kind == "knobs" else [False, True]):
                for placement in (["beginning"] if kind == "knobs" else ["beginning", "middle", "end"]):
                    rows = table_rows(probe(), name)
                    cell = rows[-1][-1] if placement == "end" else rows[0][0]
                    click(cell["bounds"])
                    # A pointer hover reveals all four accessible controls.
                    controls = probe()
                    labels = ["Row above menu", "Column right menu", "Row below menu", "Column left menu"]
                    for label in labels:
                        control = _one(controls, "button", label)[1]
                        if not control["actions"] or "focusable" not in control["states"]:
                            raise RuntimeError(f"Knob lost accessible action: {label}")
                    edge = ("Column left menu" if placement == "beginning" else "Column right menu") if column else (
                        "Row above menu" if placement == "beginning" else "Row below menu")
                    control = _one(controls, "button", edge)[1]
                    if name == "Inline reference table" and not column and placement == "beginning":
                        capture("knobs")
                        output.with_suffix(".controls.json").write_text(json.dumps(controls, indent=2) + "\n")
                    if kind == "knobs":
                        bound = control["bounds"]
                        input_event("move", bound["x"] + bound["width"] // 2,
                                    bound["y"] + bound["height"] // 2)
                        time.sleep(.2)
                        capture("hover")
                        subprocess.run(["/usr/bin/python3", __file__, str(pid), edge],
                                       env=env, check=True, timeout=15)
                        focused = _one(probe(), "button", edge)[1]
                        if "focused" not in focused["states"]:
                            raise RuntimeError("Table knob did not receive native accessibility focus")
                        key(15)  # Tab stays in control navigation, not cell editing.
                        next_control = _one(probe(), "button", "Column right menu")[1]
                        if "focused" not in next_control["states"]:
                            raise RuntimeError("Tab did not advance to the next table control")
                        key(15, shift=True)
                        if "focused" not in _one(probe(), "button", edge)[1]["states"]:
                            raise RuntimeError("Shift+Tab did not restore table control focus")
                        capture("focus")
                        key(57 if zoom == 1.5 else 28)  # Space and Enter both activate.
                    else:
                        click(control["bounds"])
                    menus = probe()
                    output.with_suffix(".menu.json").write_text(json.dumps(menus, indent=2) + "\n")
                    command = "Insert " + edge.replace(" menu", "").lower()
                    menu = _one(menus, "menu item", command)[1]
                    click(menu["bounds"])
                    label = f"{'paired' if name == 'Palette table' else 'inline'}-{'column' if column else 'row'}-{placement}"
                    immediate = table_rows(probe(), name)
                    if len(immediate) != len(rows) + int(not column) or len(immediate[0]) != len(rows[0]) + int(column):
                        raise RuntimeError(f"Insertion menu did not execute: {label}")
                    # First available capture AFTER the inserted state is
                    # published, not a leftover menu frame queued before it.
                    immediate_frame = capture(label + "-immediate")
                    time.sleep(1.2)
                    settled = table_rows(probe(), name)
                    settled_frame = capture(label + "-settled")
                    geometry = lambda group: [[cell["bounds"] for cell in row] for row in group]
                    if geometry(immediate) != geometry(settled):
                        raise RuntimeError(f"Table geometry snapped after insertion: {label}")
                    edited = source_path.read_bytes()
                    inserted_index = 0 if placement == "beginning" else (len(rows[0]) if column else len(rows)) if placement == "end" else 1
                    row, col = (0, inserted_index) if column else (inserted_index, 0)
                    click(settled[row][col]["bounds"])
                    key(45)
                    changed = table_rows(probe(), name)
                    if changed[row][col]["name"] != "x":
                        raise RuntimeError(f"New cell hit/caret does not match its display: {label}")
                    key(44, control=True)
                    wait_source(edited)
                    key(44, control=True)
                    wait_source(original)
                    key(44, control=True, shift=True)
                    wait_source(edited)
                    key(44, control=True)
                    wait_source(original)
                    evidence["insertions"].append(dict(case=label, immediate=immediate_frame,
                                                        settled=settled_frame, geometry=geometry(settled),
                                                        undo_redo_exact=True, inserted_cell_hit_and_typing=True))
    if source_path.read_bytes() != original:
        raise RuntimeError("Bug verification changed source")
    return dict(passes=True, source_unchanged=True, source_sha256=hashlib.sha256(original).hexdigest(),
                zoom=zoom, **evidence)


if __name__ == "__main__":
    # Focus just one advertised button in the harness-owned private AT-SPI
    # application. Never inspect or inject into the physical desktop session.
    import os
    import sys
    import gi
    if not os.environ.get("MINERAL_PRIVATE_ATSPI_BUS") or os.environ.get("MINERAL_PRIVATE_ATSPI_BUS") != os.environ.get("DBUS_SESSION_BUS_ADDRESS"):
        raise RuntimeError("Native focus check requires the private accessibility bus")
    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi
    Atspi.set_timeout(2000, 10000)
    pid, label = int(sys.argv[1]), sys.argv[2]
    desktop = Atspi.get_desktop(0)
    pending = [desktop.get_child_at_index(i) for i in range(desktop.get_child_count())]
    pending = [node for node in pending if node.get_process_id() == pid]
    found = []
    visited = 0
    while pending:
        node = pending.pop()
        visited += 1
        if visited > 2000:
            raise RuntimeError("Synthetic focus traversal exceeded its bound")
        if node.get_role_name() == "button" and node.get_name() == label:
            found.append(node)
        pending.extend(node.get_child_at_index(i) for i in range(node.get_child_count()))
    if len(found) != 1 or not found[0].get_component_iface().grab_focus():
        raise RuntimeError("Expected one focusable native table knob")
